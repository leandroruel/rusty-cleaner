use rusty_cleaner::{
    scan,
    scanner::{Feature, Finding},
};
use serde::Serialize;
use std::{env, path::PathBuf, time::Instant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingView {
    feature: String,
    name: String,
    path: String,
    size: u64,
    age_days: Option<u64>,
    meta: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    findings: Vec<FindingView>,
    elapsed_ms: u128,
    platform: &'static str,
}

#[tauri::command]
async fn scan_candidates(
    app: tauri::AppHandle,
    feature: Option<String>,
) -> Result<ScanResult, String> {
    use tauri::Emitter;

    let selected_feature = feature
        .map(|value| {
            Feature::parse(&value).ok_or_else(|| format!("Unsupported scan feature: {value}"))
        })
        .transpose()?;
    let started = Instant::now();
    rusty_cleaner::scanner::set_progress_callback(Some(Box::new(move |path| {
        let _ = app.emit("scan-progress", path.to_string_lossy().into_owned());
    })));
    let feature_label = selected_feature.map(|f| f.to_string());
    let findings = tauri::async_runtime::spawn_blocking(move || scan(selected_feature))
        .await
        .map_err(|error| error.to_string())?;
    rusty_cleaner::scanner::set_progress_callback(None);
    let elapsed_ms = started.elapsed().as_millis();

    rusty_cleaner::activity_log::record(
        "scan",
        &format!(
            "feature={} items={} elapsed_ms={}",
            feature_label.as_deref().unwrap_or("all"),
            findings.len(),
            elapsed_ms
        ),
    );

    Ok(ScanResult {
        findings: findings.into_iter().map(to_view).collect(),
        elapsed_ms,
        platform: current_platform(),
    })
}

#[tauri::command]
async fn scan_registry_issues(
    app: tauri::AppHandle,
    rules: Vec<String>,
) -> Result<ScanResult, String> {
    use tauri::Emitter;

    let selected: Vec<rusty_cleaner::registry::Rule> = rules
        .iter()
        .map(|value| {
            rusty_cleaner::registry::Rule::parse(value)
                .ok_or_else(|| format!("Unsupported registry rule: {value}"))
        })
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    rusty_cleaner::scanner::set_progress_callback(Some(Box::new(move |path| {
        let _ = app.emit("scan-progress", path.to_string_lossy().into_owned());
    })));
    let rule_count = selected.len();
    let findings = tauri::async_runtime::spawn_blocking(move || {
        rusty_cleaner::registry::scan_rules(&selected)
    })
    .await
    .map_err(|error| error.to_string())?;
    rusty_cleaner::scanner::set_progress_callback(None);
    let elapsed_ms = started.elapsed().as_millis();

    rusty_cleaner::activity_log::record(
        "scan",
        &format!(
            "feature=registry rules={} items={} elapsed_ms={}",
            rule_count,
            findings.len(),
            elapsed_ms
        ),
    );

    Ok(ScanResult {
        findings: findings.into_iter().map(to_view).collect(),
        elapsed_ms,
        platform: current_platform(),
    })
}

#[tauri::command]
fn detect_theme() -> &'static str {
    if cfg!(target_os = "linux") && has_omarchy_marker(omarchy_marker_paths()) {
        "omarchy"
    } else {
        "rusty"
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashFailure {
    path: String,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashResult {
    trashed: Vec<String>,
    failed: Vec<TrashFailure>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFixItem {
    key: String,
    value: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFixResult {
    fixed: Vec<String>,
    failed: Vec<TrashFailure>,
    backup_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheCleanResult {
    removed: Vec<String>,
    failed: Vec<TrashFailure>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CleanProgress {
    current: u32,
    total: u32,
}

/// Reports cleaning progress to the frontend, throttled to ~1% steps so a
/// hundred-thousand-item trash never becomes an IPC storm.
fn emit_clean_progress(app: &tauri::AppHandle, current: u32, total: u32) {
    use tauri::Emitter;
    let _ = app.emit("clean-progress", CleanProgress { current, total });
}

fn clean_progress_step(total: u32) -> u32 {
    (total / 100).max(1)
}

/// Permanently removes cache directories (browser or messenger) through the
/// purge guard. Caches are regenerable and can hold hundreds of thousands of
/// files; routing them through the OS trash has exhausted system memory on
/// Windows in the field.
#[tauri::command]
async fn purge_cache_dirs(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<CacheCleanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let total = paths.len() as u32;
        let step = clean_progress_step(total);
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let current = (index + 1) as u32;
            match rusty_cleaner::browser::purge(std::path::Path::new(&path)) {
                Ok(()) => {
                    rusty_cleaner::activity_log::record("cache-clean", &path);
                    removed.push(path);
                }
                Err(error) => {
                    rusty_cleaner::activity_log::record(
                        "cache-clean-failed",
                        &format!("{path} ({error})"),
                    );
                    failed.push(TrashFailure { path, error });
                }
            }
            if current % step == 0 {
                emit_clean_progress(&app, current, total);
            }
        }
        rusty_cleaner::activity_log::record(
            "cache-clean",
            &format!("summary removed={} failed={}", removed.len(), failed.len()),
        );
        Ok(CacheCleanResult { removed, failed })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(windows)]
fn backup_registry_keys(keys: &[String]) -> Result<String, String> {
    use std::collections::BTreeSet;
    use std::process::Command;

    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|age| age.as_secs())
        .unwrap_or(0);
    let base = rusty_cleaner::activity_log::data_dir()
        .ok_or("backup folder unavailable")?
        .join("rusty-cleaner")
        .join("registry-backups")
        .join(format!("backup-{started}"));
    std::fs::create_dir_all(&base).map_err(|error| error.to_string())?;

    let unique: BTreeSet<&String> = keys.iter().collect();
    let mut manifest = String::new();
    for (index, key) in unique.iter().enumerate() {
        let file = base.join(format!("{index:02}.reg"));
        let output = Command::new("reg")
            .arg("export")
            .arg(key)
            .arg(&file)
            .arg("/y")
            .output()
            .map_err(|error| format!("failed to launch reg.exe: {error}"))?;
        if !output.status.success() {
            return Err(format!("registry backup failed for {key}"));
        }
        manifest.push_str(&format!("{index:02}.reg\t{key}\n"));
    }
    std::fs::write(base.join("keys.txt"), manifest).map_err(|error| error.to_string())?;
    Ok(base.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn create_restore_point() -> Result<(), String> {
    use std::process::Command;

    // Both lifting the once-per-24h restore-point throttle and creating the
    // checkpoint need elevation; a single UAC prompt covers them.
    let inner = r"$ErrorActionPreference = 'Stop'; Set-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' -Name SystemRestorePointCreationFrequency -Value 0 -Type DWord; Checkpoint-Computer -Description 'Rusty Cleaner registry fix' -RestorePointType MODIFY_SETTINGS; exit 0";
    let encoded = base64_utf16(inner);
    let outer = format!(
        "try {{ $p = Start-Process -Verb RunAs -Wait -PassThru -FilePath powershell.exe \
-ArgumentList @('-NoProfile','-EncodedCommand','{encoded}'); exit $p.ExitCode }} catch {{ exit 2 }}"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &outer])
        .output()
        .map_err(|error| format!("failed to launch powershell: {error}"))?;
    match output.status.code() {
        Some(0) => Ok(()),
        Some(2) => {
            Err("the restore point needs elevation and the UAC prompt was declined".to_owned())
        }
        _ => {
            Err("system restore point could not be created (is System Restore enabled?)".to_owned())
        }
    }
}

/// PowerShell's -EncodedCommand expects UTF-16LE bytes in standard base64.
#[cfg(any(windows, test))]
fn base64_utf16(text: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let triple = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let bits =
            (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);
        encoded.push(ALPHABET[(bits >> 18 & 63) as usize] as char);
        encoded.push(ALPHABET[(bits >> 12 & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(bits >> 6 & 63) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClosedBrowserView {
    name: String,
    was_running: bool,
}

#[tauri::command]
async fn close_browsers() -> Result<Vec<ClosedBrowserView>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rusty_cleaner::browser::quit_running_browsers()
            .into_iter()
            .map(|browser| ClosedBrowserView {
                name: browser.name,
                was_running: browser.was_running,
            })
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn detect_platform() -> &'static str {
    current_platform()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppView {
    name: String,
    path: String,
    size: Option<u64>,
    last_used: Option<u64>,
    uninstall_kind: Option<String>,
    uninstall_arg: Option<String>,
}

fn to_app_view(app: rusty_cleaner::applications::AppEntry) -> AppView {
    let (kind, arg) = match &app.uninstall {
        Some(action) => (
            Some(action.kind().to_owned()),
            Some(match action {
                rusty_cleaner::applications::Uninstall::MacBundle(path) => {
                    path.to_string_lossy().into_owned()
                }
                rusty_cleaner::applications::Uninstall::Pacman(arg)
                | rusty_cleaner::applications::Uninstall::Dpkg(arg)
                | rusty_cleaner::applications::Uninstall::Rpm(arg)
                | rusty_cleaner::applications::Uninstall::Flatpak(arg)
                | rusty_cleaner::applications::Uninstall::Snap(arg)
                | rusty_cleaner::applications::Uninstall::Windows(arg) => arg.clone(),
            }),
        ),
        None => (None, None),
    };
    AppView {
        name: app.name,
        path: app.path.to_string_lossy().into_owned(),
        size: app.size,
        last_used: app
            .last_used
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|age| age.as_secs()),
        uninstall_kind: kind,
        uninstall_arg: arg,
    }
}

#[tauri::command]
async fn list_applications() -> Result<Vec<AppView>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let started = std::time::Instant::now();
        let apps = rusty_cleaner::applications::scan();
        rusty_cleaner::activity_log::record(
            "applications-scan",
            &format!(
                "apps={} elapsed_ms={}",
                apps.len(),
                started.elapsed().as_millis()
            ),
        );
        Ok(apps.into_iter().map(to_app_view).collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UninstallRequest {
    kind: String,
    arg: String,
    name: String,
}

#[tauri::command]
async fn uninstall_application(request: UninstallRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let action = rusty_cleaner::applications::Uninstall::parse(&request.kind, &request.arg)
            .ok_or_else(|| format!("Unsupported uninstall kind: {}", request.kind))?;
        rusty_cleaner::applications::uninstall(&action).map_err(|error| error.to_string())?;
        rusty_cleaner::activity_log::record(
            "uninstall",
            &format!("{} ({})", request.name, request.kind),
        );
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn fix_registry_issues(
    app: tauri::AppHandle,
    items: Vec<RegistryFixItem>,
) -> Result<RegistryFixResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let total = items.len() as u32;
        let step = clean_progress_step(total);
        let targets: Vec<(String, Option<String>)> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let current = (index + 1) as u32;
                if current % step == 0 {
                    emit_clean_progress(&app, current, total);
                }
                (item.key, item.value)
            })
            .collect();
        // Mandatory safety net: every key about to be modified is exported
        // to a .reg backup and a system restore point is taken — the fix
        // refuses to run when either step fails.
        #[cfg(windows)]
        let backup_path = {
            let keys: Vec<String> = targets.iter().map(|(key, _)| key.clone()).collect();
            let backup_dir = backup_registry_keys(&keys)?;
            create_restore_point()?;
            Some(backup_dir)
        };
        #[cfg(not(windows))]
        let backup_path = None;

        let outcome = rusty_cleaner::registry::fix(&targets);
        for (key, error) in &outcome.failed {
            rusty_cleaner::activity_log::record("registry-fix-failed", &format!("{key} ({error})"));
        }
        rusty_cleaner::activity_log::record(
            "registry-fix",
            &format!(
                "fixed={} failed={} backup={}",
                outcome.fixed.len(),
                outcome.failed.len(),
                backup_path.is_some()
            ),
        );
        Ok(RegistryFixResult {
            fixed: outcome.fixed,
            failed: outcome
                .failed
                .into_iter()
                .map(|(key, error)| TrashFailure { path: key, error })
                .collect(),
            backup_path,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn trash_candidates(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<TrashResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let total = paths.len() as u32;
        let step = clean_progress_step(total);
        let mut trashed = Vec::new();
        let mut failed = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let current = (index + 1) as u32;
            let path_buf = PathBuf::from(&path);
            if !path_buf.exists() {
                failed.push(TrashFailure {
                    path,
                    error: "O arquivo não existe mais".to_owned(),
                });
            } else {
                match trash::delete(&path_buf) {
                    Ok(()) => {
                        rusty_cleaner::activity_log::record("trash", &path);
                        trashed.push(path);
                    }
                    Err(error) => {
                        rusty_cleaner::activity_log::record(
                            "trash-failed",
                            &format!("{path} ({error})"),
                        );
                        failed.push(TrashFailure {
                            path,
                            error: error.to_string(),
                        });
                    }
                }
            }
            if current % step == 0 {
                emit_clean_progress(&app, current, total);
            }
        }
        Ok(TrashResult { trashed, failed })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmptyTrashResult {
    removed: u64,
    failed: u64,
}

#[tauri::command]
async fn empty_trash() -> Result<EmptyTrashResult, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut removed = 0_u64;
        let mut failed = 0_u64;
        for root in rusty_cleaner::platform::trash_dirs() {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let result = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                match result {
                    Ok(()) => removed += 1,
                    Err(_) => failed += 1,
                }
            }
        }
        rusty_cleaner::activity_log::record(
            "empty-trash",
            &format!("removed={removed} failed={failed}"),
        );
        Ok(EmptyTrashResult { removed, failed })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashItemView {
    name: String,
    original_path: String,
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn list_trash_items() -> Result<Vec<TrashItemView>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let items = trash::os_limited::list().map_err(|error| error.to_string())?;
        Ok(items
            .into_iter()
            .map(|item| TrashItemView {
                name: item.name.to_string_lossy().into_owned(),
                original_path: item.original_path().to_string_lossy().into_owned(),
            })
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(target_os = "macos")]
#[tauri::command]
async fn list_trash_items() -> Result<Vec<TrashItemView>, String> {
    Err("Restoring from trash is not supported on macOS yet".to_owned())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn restore_trash_items(names: Vec<String>) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let items = trash::os_limited::list().map_err(|error| error.to_string())?;
        let selected: Vec<_> = items
            .into_iter()
            .filter(|item| names.contains(&item.name.to_string_lossy().into_owned()))
            .collect();
        let count = selected.len() as u64;
        trash::os_limited::restore_all(selected).map_err(|error| error.to_string())?;
        rusty_cleaner::activity_log::record("restore-trash", &format!("restored={count}"));
        Ok(count)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(target_os = "macos")]
#[tauri::command]
async fn restore_trash_items(_names: Vec<String>) -> Result<u64, String> {
    Err("Restoring from trash is not supported on macOS yet".to_owned())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemMetrics {
    cpu_percent: f32,
    memory_used: u64,
    memory_total: u64,
    disk_used: u64,
    disk_total: u64,
}

#[tauri::command]
fn system_metrics() -> SystemMetrics {
    use sysinfo::{Disks, System};

    let mut system = System::new();
    system.refresh_cpu_usage();
    system.refresh_memory();

    let disks = Disks::new_with_refreshed_list();
    let (disk_used, disk_total) = disks
        .iter()
        .find(|disk| disk.mount_point() == std::path::Path::new("/"))
        .or_else(|| disks.iter().next())
        .map(|disk| {
            (
                disk.total_space() - disk.available_space(),
                disk.total_space(),
            )
        })
        .unwrap_or((0, 0));

    // Match the OS task manager: "in use" is total minus available, not the
    // kernel's used figure (which excludes cache/buffers differently per OS).
    let memory_total = system.total_memory();
    let memory_used = memory_total.saturating_sub(system.available_memory());

    SystemMetrics {
        cpu_percent: system.global_cpu_usage(),
        memory_used,
        memory_total,
        disk_used,
        disk_total,
    }
}

#[tauri::command]
fn log_file_path() -> Option<String> {
    rusty_cleaner::activity_log::log_path().map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_log_folder() -> Result<(), String> {
    let path = rusty_cleaner::activity_log::log_path().ok_or("Pasta de log indisponível")?;
    let folder = path.parent().ok_or("Pasta de log indisponível")?;
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    open::that(folder).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_excluded_dirs() -> Vec<String> {
    rusty_cleaner::settings::load_excluded_dirs()
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
fn add_excluded_dir(path: String) -> Result<Vec<String>, String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        return Err("O caminho não é uma pasta existente".to_owned());
    }
    if !rusty_cleaner::settings::add_excluded_dir(&path_buf) {
        return Err("Pasta do sistema ou já excluída".to_owned());
    }
    Ok(list_excluded_dirs())
}

#[tauri::command]
fn remove_excluded_dir(path: String) -> Vec<String> {
    rusty_cleaner::settings::remove_excluded_dir(&PathBuf::from(path));
    list_excluded_dirs()
}

fn omarchy_marker_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/etc/omarchy"),
        PathBuf::from("/usr/share/omarchy"),
    ];
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        paths.push(home.join(".config/omarchy"));
        paths.push(home.join(".local/share/omarchy"));
    }
    if let Some(search_path) = env::var_os("PATH") {
        paths.extend(env::split_paths(&search_path).map(|directory| directory.join("omarchy")));
    }
    paths
}

fn has_omarchy_marker(paths: impl IntoIterator<Item = PathBuf>) -> bool {
    paths.into_iter().any(|path| path.exists())
}

fn to_view(finding: Finding) -> FindingView {
    FindingView {
        feature: finding.feature.to_string(),
        name: finding.name,
        path: finding.path.to_string_lossy().into_owned(),
        size: finding.size,
        age_days: finding
            .modified
            .and_then(|modified| modified.elapsed().ok())
            .map(|age| age.as_secs() / 86_400),
        meta: finding.meta,
    }
}

fn current_platform() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "unknown"
    }
}

#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|error| error.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(update.version.to_string())),
        Ok(None) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|error| error.to_string())?;
    if let Some(update) = updater.check().await.map_err(|error| error.to_string())? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|error| error.to_string())?;
        app.restart();
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            scan_candidates,
            scan_registry_issues,
            list_applications,
            uninstall_application,
            purge_cache_dirs,
            close_browsers,
            detect_platform,
            detect_theme,
            trash_candidates,
            fix_registry_issues,
            empty_trash,
            list_trash_items,
            restore_trash_items,
            system_metrics,
            log_file_path,
            open_log_folder,
            list_excluded_dirs,
            add_excluded_dir,
            remove_excluded_dir,
            check_for_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Rusty Cleaner desktop application");
}

#[cfg(test)]
mod tests {
    use super::has_omarchy_marker;
    use std::{fs, path::PathBuf, time::SystemTime};

    #[test]
    fn encodes_powershell_commands_as_utf16_base64() {
        // "ABC" in UTF-16LE is 41 00 42 00 43 00, which base64s to QQBCAEMA.
        assert_eq!(super::base64_utf16("ABC"), "QQBCAEMA");
        assert_eq!(super::base64_utf16(""), "");
        assert_eq!(super::base64_utf16("A"), "QQA=");
    }

    #[test]
    fn detects_an_omarchy_marker_without_using_the_arch_distro_id() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-cleaner-omarchy-{}-{nonce}",
            std::process::id()
        ));
        let marker = root.join(".config/omarchy");
        assert!(!has_omarchy_marker([marker.clone()]));

        fs::create_dir_all(&marker).unwrap();
        assert!(has_omarchy_marker([PathBuf::from(&marker)]));

        fs::remove_dir_all(root).unwrap();
    }
}
