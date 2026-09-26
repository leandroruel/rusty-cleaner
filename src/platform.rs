use std::env;
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
        paths.push(home.join(".cache/mozilla"));
        paths.push(home.join(".cache/google-chrome"));
        paths.push(home.join(".config/google-chrome"));
        paths.push(home.join(".config/chromium"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Caches"));
        paths.push(home.join("Library/Application Support/Google/Chrome"));
        paths.push(home.join("Library/Application Support/Firefox"));
    }
    #[cfg(windows)]
    if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        paths.push(local.join("Google/Chrome/User Data"));
        paths.push(local.join("Mozilla/Firefox/Profiles"));
    }
    paths
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
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        paths.push(home.join("Library/Application Support/Telegram Desktop"));
        paths.push(home.join("Library/Application Support/discord"));
        paths.push(home.join("Library/Application Support/WhatsApp"));
    }
    #[cfg(windows)]
    if let Some(roaming) = env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(roaming.join("Telegram Desktop"));
        paths.push(roaming.join("discord"));
        paths.push(roaming.join("WhatsApp"));
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
fn run_quiet(program: &str, args: &[&str]) -> Option<String> {
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
}
