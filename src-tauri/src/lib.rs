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
    let findings = tauri::async_runtime::spawn_blocking(move || scan(selected_feature))
        .await
        .map_err(|error| error.to_string())?;
    rusty_cleaner::scanner::set_progress_callback(None);
    let elapsed_ms = started.elapsed().as_millis();

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
                Ok(()) => trashed.push(path),
                Err(error) => failed.push(TrashFailure {
                    path,
                    error: error.to_string(),
                }),
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
        Ok(EmptyTrashResult { removed, failed })
    })
    .await
    .map_err(|error| error.to_string())?
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

    SystemMetrics {
        cpu_percent: system.global_cpu_usage(),
        memory_used: system.used_memory(),
        memory_total: system.total_memory(),
        disk_used,
        disk_total,
    }
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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_candidates,
            detect_theme,
            trash_candidates,
            empty_trash,
            system_metrics
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
