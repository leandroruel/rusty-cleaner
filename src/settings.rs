use crate::activity_log;
use std::fs;
use std::path::{Path, PathBuf};

/// User-managed settings persisted next to the activity log. Currently only
/// the list of user-excluded directories (one absolute path per line).
fn settings_path() -> Option<PathBuf> {
    activity_log::data_dir().map(|base| base.join("rusty-cleaner").join("excluded-dirs.txt"))
}

/// System directories that must never be offered as user exclusions.
fn is_system_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with("/proc")
        || text.starts_with("/sys")
        || text.starts_with("/dev")
        || text.starts_with("/run")
        || text.starts_with("/boot")
        || text == "/"
        || text == "/usr"
        || text.starts_with("/usr/")
        || text == "/etc"
        || text.starts_with("/etc/")
        || text == "/var"
        || text.starts_with("/var/")
        || text == "/bin"
        || text == "/sbin"
        || text == "/lib"
        || text.starts_with("/lib/")
        || text.starts_with("C:\\Windows")
        || text.starts_with("C:\\Program Files")
}

pub fn load_excluded_dirs() -> Vec<PathBuf> {
    let Some(path) = settings_path() else {
        return Vec::new();
    };
    fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Adds a user exclusion. Returns false for system paths or duplicates.
pub fn add_excluded_dir(path: &Path) -> bool {
    if is_system_path(path) {
        return false;
    }
    let mut dirs = load_excluded_dirs();
    if dirs.iter().any(|existing| existing == path) {
        return false;
    }
    dirs.push(path.to_path_buf());
    save(&dirs)
}

pub fn remove_excluded_dir(path: &Path) -> bool {
    let mut dirs = load_excluded_dirs();
    let before = dirs.len();
    dirs.retain(|existing| existing != path);
    dirs.len() != before && save(&dirs)
}

fn save(dirs: &[PathBuf]) -> bool {
    let Some(path) = settings_path() else {
        return false;
    };
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let content = dirs
        .iter()
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).is_ok()
}

#[cfg(test)]
mod tests {
    use super::is_system_path;
    use std::path::Path;

    #[test]
    fn rejects_system_directories() {
        assert!(is_system_path(Path::new("/usr/bin")));
        assert!(is_system_path(Path::new("/etc")));
        assert!(is_system_path(Path::new("/proc/1")));
        assert!(is_system_path(Path::new("/")));
        assert!(!is_system_path(Path::new("/home/user/Downloads")));
        assert!(!is_system_path(Path::new("/home/user/Videos")));
    }
}
