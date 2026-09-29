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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheCleanResult {
    removed: Vec<String>,
    failed: Vec<TrashFailure>,
}

/// Permanently removes browser cache directories through the browser
/// scanner's purge guard. Caches are regenerable and can hold hundreds of
/// thousands of files; routing them through the OS trash has exhausted
/// system memory on Windows in the field.
#[tauri::command]
async fn delete_browser_caches(paths: Vec<String>) -> Result<CacheCleanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            match rusty_cleaner::browser::purge(std::path::Path::new(&path)) {
                Ok(()) => {
                    rusty_cleaner::activity_log::record("browser-cache-clean", &path);
                    removed.push(path);
                }
                Err(error) => {
                    rusty_cleaner::activity_log::record(
                        "browser-cache-clean-failed",
                        &format!("{path} ({error})"),
                    );
                    failed.push(TrashFailure { path, error });
                }
            }
        }
        rusty_cleaner::activity_log::record(
            "browser-cache-clean",
            &format!("summary removed={} failed={}", removed.len(), failed.len()),
        );
        Ok(CacheCleanResult { removed, failed })
    })
    .await
    .map_err(|error| error.to_string())?
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
async fn fix_registry_issues(items: Vec<RegistryFixItem>) -> Result<RegistryFixResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let targets: Vec<(String, Option<String>)> = items
            .into_iter()
            .map(|item| (item.key, item.value))
            .collect();
        let outcome = rusty_cleaner::registry::fix(&targets);
        for (key, error) in &outcome.failed {
            rusty_cleaner::activity_log::record("registry-fix-failed", &format!("{key} ({error})"));
        }
        rusty_cleaner::activity_log::record(
            "registry-fix",
            &format!(
                "fixed={} failed={}",
                outcome.fixed.len(),
                outcome.failed.len()
            ),
        );
        Ok(RegistryFixResult {
            fixed: outcome.fixed,
            failed: outcome
                .failed
                .into_iter()
                .map(|(key, error)| TrashFailure { path: key, error })
                .collect(),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn trash_candidates(paths: Vec<String>) -> Result<TrashResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut trashed = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            let path_buf = PathBuf::from(&path);
            if !path_buf.exists() {
                failed.push(TrashFailure {
                    path,
                    error: "O arquivo não existe mais".to_owned(),
                });
                continue;
            }
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
            delete_browser_caches,
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
