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
async fn scan_candidates(feature: Option<String>) -> Result<ScanResult, String> {
    let selected_feature = feature
        .map(|value| {
            Feature::parse(&value).ok_or_else(|| format!("Unsupported scan feature: {value}"))
        })
        .transpose()?;
    let started = Instant::now();
    let findings = tauri::async_runtime::spawn_blocking(move || scan(selected_feature))
        .await
        .map_err(|error| error.to_string())?;
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
        .invoke_handler(tauri::generate_handler![scan_candidates, detect_theme])
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
