use rusty_cleaner::{
    scan,
    scanner::{Feature, Finding},
};
use serde::Serialize;
use std::time::Instant;

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
        .invoke_handler(tauri::generate_handler![scan_candidates])
        .run(tauri::generate_context!())
        .expect("failed to run Rusty Cleaner desktop application");
}
