use crate::{
    platform,
    scanner::{self, Feature, Finding},
};
use std::collections::HashSet;

const MIN_AGE_DAYS: u64 = 180;

/// Finds application data that likely belongs to uninstalled programs.
///
/// Strategy: for each top-level directory inside the platform's app-data
/// roots, compare its name against the installed package database
/// (pacman/dpkg/flatpak on Linux, Homebrew on macOS). A directory whose name
/// matches no installed package and whose contents are old is a strong orphan
/// candidate. When no package database is available, fall back to the
/// age-only heuristic — results remain candidates for user review either way.
pub fn scan() -> Vec<Finding> {
    let installed = platform::installed_packages();
    let mut findings = Vec::new();

    for root in platform::app_data_dirs() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !installed.is_empty() && is_installed(name, &installed) {
                continue;
            }
            let files = scanner::walk_files(std::slice::from_ref(&path), 5, |_, metadata| {
                scanner::age_days(metadata).is_some_and(|days| days >= MIN_AGE_DAYS)
            });
            findings.extend(
                files
                    .into_iter()
                    .map(|(file, metadata)| scanner::finding(Feature::Orphan, file, metadata)),
            );
        }
    }
    findings
}

fn is_installed(dir_name: &str, installed: &HashSet<String>) -> bool {
    let normalized = dir_name.to_lowercase();
    let dashed = normalized.replace(['_', ' '], "-");
    installed.contains(&normalized)
        || installed.contains(&dashed)
        || installed
            .iter()
            .any(|package| package == &normalized || package.replace(['_', ' '], "-") == dashed)
}

#[cfg(test)]
mod tests {
    use super::is_installed;
    use std::collections::HashSet;

    #[test]
    fn matches_installed_packages_with_normalized_names() {
        let installed: HashSet<String> = ["google-chrome", "telegram-desktop"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(is_installed("google-chrome", &installed));
        assert!(is_installed("Telegram Desktop", &installed));
        assert!(!is_installed("deleted-app", &installed));
    }
}
