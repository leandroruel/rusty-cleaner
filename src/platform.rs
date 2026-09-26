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
    if let Some(home) = home_dir() {
        #[cfg(target_os = "linux")]
        {
            paths.push(home.join(".config"));
            paths.push(home.join(".local/share"));
        }
        #[cfg(target_os = "macos")]
        paths.push(home.join("Library/Application Support"));
        #[cfg(windows)]
        if let Some(appdata) = env::var_os("APPDATA") {
            paths.push(PathBuf::from(appdata));
        }
    }
    paths
}

pub fn browser_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home_dir() {
        #[cfg(target_os = "linux")]
        {
            paths.push(home.join(".cache/mozilla"));
            paths.push(home.join(".cache/google-chrome"));
            paths.push(home.join(".config/google-chrome"));
            paths.push(home.join(".config/chromium"));
        }
        #[cfg(target_os = "macos")]
        {
            paths.push(home.join("Library/Caches"));
            paths.push(home.join("Library/Application Support/Google/Chrome"));
            paths.push(home.join("Library/Application Support/Firefox"));
        }
        #[cfg(windows)]
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            paths.push(PathBuf::from(local).join("Google/Chrome/User Data"));
            paths.push(PathBuf::from(local).join("Mozilla/Firefox/Profiles"));
        }
    }
    paths
}

pub fn chat_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home_dir() {
        #[cfg(target_os = "linux")]
        {
            paths.push(home.join(".local/share/TelegramDesktop"));
            paths.push(home.join(".var/app/org.telegram.desktop"));
            paths.push(home.join(".config/discord"));
        }
        #[cfg(target_os = "macos")]
        {
            paths.push(home.join("Library/Application Support/Telegram Desktop"));
            paths.push(home.join("Library/Application Support/discord"));
        }
        #[cfg(windows)]
        if let Some(roaming) = env::var_os("APPDATA") {
            paths.push(PathBuf::from(roaming).join("Telegram Desktop"));
            paths.push(PathBuf::from(roaming).join("discord"));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    #[test]
    fn temporary_roots_only_include_the_os_temp_directory() {
        assert_eq!(super::temp_dirs(), vec![std::env::temp_dir()]);
    }
}
