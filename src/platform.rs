use std::env;
#[cfg(any(target_os = "linux", windows))]
use std::path::Path;
use std::path::PathBuf;

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

pub fn temp_dirs() -> Vec<PathBuf> {
    vec![env::temp_dir()]
}

pub fn trash_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home_dir() {
        #[cfg(target_os = "linux")]
        paths.push(home.join(".local/share/Trash/files"));
        #[cfg(target_os = "macos")]
        paths.push(home.join(".Trash"));
        #[cfg(windows)]
        paths.push(home.join("$Recycle.Bin"));
    }
    paths
}

pub fn app_data_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(home) = home_dir() {
        paths.push(home.join(".config"));
        paths.push(home.join(".local/share"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Application Support"));
    }
    #[cfg(windows)]
    if let Some(appdata) = env::var_os("APPDATA") {
        paths.push(PathBuf::from(appdata));
    }
    paths
}

pub fn browser_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(home) = home_dir() {
        // Native package installs (pacman/dpkg/deb/rpm).
        paths.push(home.join(".cache/mozilla"));
        paths.push(home.join(".cache/google-chrome"));
        paths.push(home.join(".cache/chromium"));
        paths.push(home.join(".cache/BraveSoftware"));
        paths.push(home.join(".cache/microsoft-edge"));
        paths.push(home.join(".config/google-chrome"));
        paths.push(home.join(".config/chromium"));
        paths.push(home.join(".config/BraveSoftware"));
        paths.push(home.join(".config/microsoft-edge"));
        // Flatpak installs keep cache and config under ~/.var/app/<app-id>.
        paths.push(home.join(".var/app/org.mozilla.firefox/cache"));
        paths.push(home.join(".var/app/org.chromium.Chromium/cache"));
        paths.push(home.join(".var/app/com.brave.Browser/cache"));
        paths.push(home.join(".var/app/com.brave.Browser/config/BraveSoftware"));
        paths.push(home.join(".var/app/com.microsoft.Edge/cache"));
        paths.push(home.join(".var/app/com.microsoft.Edge/config/microsoft-edge"));
        // Snap installs keep cache under ~/snap/<name>/common/.cache.
        paths.push(home.join("snap/firefox/common/.cache"));
        paths.push(home.join("snap/chromium/common/.cache"));
        paths.push(home.join("snap/brave/common/.cache"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Caches/Google/Chrome"));
        paths.push(home.join("Library/Caches/Firefox"));
        paths.push(home.join("Library/Caches/Chromium"));
        paths.push(home.join("Library/Caches/BraveSoftware"));
        paths.push(home.join("Library/Caches/microsoft-edge"));
        paths.push(home.join("Library/Application Support/Google/Chrome"));
        paths.push(home.join("Library/Application Support/Firefox"));
        paths.push(home.join("Library/Application Support/BraveSoftware"));
        paths.push(home.join("Library/Application Support/Microsoft Edge"));
    }
    #[cfg(windows)]
    if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        paths.push(local.join("Google/Chrome/User Data"));
        paths.push(local.join("Chromium/User Data"));
        paths.push(local.join("BraveSoftware/Brave-Browser/User Data"));
        paths.push(local.join("Microsoft/Edge/User Data"));
        paths.push(local.join("Mozilla/Firefox/Profiles"));
    }
    paths
}

/// Windows Store WhatsApp installs per-user under
/// `AppData/Local/Packages/5319275A.WhatsAppDesktop_<hash>`.
#[cfg(any(target_os = "linux", windows))]
fn whatsapp_store_packages(packages_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(packages_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("5319275A.WhatsAppDesktop"))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|version| version.to_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn windows_profiles_on_mount() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/mnt/c/Users") else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path().join("AppData"))
        .filter(|appdata| appdata.is_dir())
        .map(|appdata| appdata.parent().map(Path::to_path_buf).unwrap_or_default())
        .filter(|profile| !profile.as_os_str().is_empty())
        .collect()
}

pub fn chat_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(home) = home_dir() {
        paths.push(home.join(".local/share/TelegramDesktop"));
        paths.push(home.join(".var/app/org.telegram.desktop"));
        paths.push(home.join(".config/discord"));
        paths.push(home.join(".var/app/com.rtosta.zapzap"));
        paths.push(home.join(".config/whatsdesk"));
        paths.push(home.join(".config/whatsapp-for-linux"));
        paths.push(home.join(".config/Signal"));
        paths.push(home.join(".config/Slack"));
        paths.push(home.join(".config/Element"));
        // WSL runs the Linux build next to Windows messengers; their app
        // data is reachable through /mnt/c.
        if is_wsl() {
            for profile in windows_profiles_on_mount() {
                paths.push(profile.join("AppData/Roaming/Telegram Desktop"));
                paths.push(profile.join("AppData/Roaming/discord"));
                paths.extend(whatsapp_store_packages(
                    &profile.join("AppData/Local/Packages"),
                ));
            }
        }
        // Telegram Desktop can run from any folder (e.g. Downloads/Telegram-Desktop).
        if let Ok(downloads) = home.join("Downloads").read_dir() {
            for entry in downloads.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.to_lowercase().contains("telegram"))
                {
                    paths.push(path);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Application Support/Telegram Desktop"));
        paths.push(home.join("Library/Application Support/discord"));
        paths.push(home.join("Library/Application Support/WhatsApp"));
        paths.push(home.join("Library/Application Support/Signal"));
        paths.push(home.join("Library/Application Support/Slack"));
        paths.push(home.join("Library/Application Support/Element"));
    }
    #[cfg(windows)]
    {
        if let Some(roaming) = env::var_os("APPDATA").map(PathBuf::from) {
            paths.push(roaming.join("Telegram Desktop"));
            paths.push(roaming.join("discord"));
            paths.push(roaming.join("WhatsApp"));
            paths.push(roaming.join("Signal"));
            paths.push(roaming.join("Slack"));
            paths.push(roaming.join("Element"));
        }
        if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            paths.extend(whatsapp_store_packages(&local.join("Packages")));
        }
        // Telegram Desktop can run from any folder (e.g. Downloads\Telegram-Desktop).
        if let Some(home) = home_dir() {
            if let Ok(downloads) = home.join("Downloads").read_dir() {
                for entry in downloads.flatten() {
                    let path = entry.path();
                    if path.is_dir()
                        && path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.to_lowercase().contains("telegram"))
                    {
                        paths.push(path);
                    }
                }
            }
        }
    }
    paths
}

/// Names of packages installed on the system, lowercased. Used to detect
/// application data left behind by uninstalled programs. Returns an empty set
/// on platforms or setups where no package database is available.
pub fn installed_packages() -> std::collections::HashSet<String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut packages = std::collections::HashSet::new();
        #[cfg(target_os = "linux")]
        {
            if let Some(output) = run_quiet("pacman", &["-Qq"]) {
                packages.extend(output.lines().map(|line| line.trim().to_lowercase()));
            }
            if let Some(output) = run_quiet("dpkg-query", &["-W", "-f=${Package}\n"]) {
                packages.extend(output.lines().map(|line| line.trim().to_lowercase()));
            }
            if let Some(output) = run_quiet("flatpak", &["list", "--app", "--columns=application"])
            {
                packages.extend(output.lines().map(|line| line.trim().to_lowercase()));
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(output) = run_quiet("brew", &["list", "--versions", "-1"]) {
                packages.extend(output.lines().map(|line| line.trim().to_lowercase()));
            }
        }
        packages
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        std::collections::HashSet::new()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn run_quiet(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn temporary_roots_only_include_the_os_temp_directory() {
        assert_eq!(super::temp_dirs(), vec![std::env::temp_dir()]);
    }

    #[test]
    fn browser_roots_cover_the_cache_paths_of_all_supported_browsers() {
        let roots = super::browser_dirs();
        let joined = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        // macOS identifies Firefox by name rather than vendor; the other
        // platforms use the "mozilla" directories.
        #[cfg(target_os = "macos")]
        let markers = ["firefox", "chrome", "chromium", "brave", "edge"];
        #[cfg(not(target_os = "macos"))]
        let markers = ["mozilla", "chrome", "chromium", "brave", "edge"];
        for marker in markers {
            assert!(joined.contains(marker), "browser root missing: {marker}");
        }
    }
}
