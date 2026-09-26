use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append-only activity log (JSON Lines) recording scans, cleanups and trash
/// operations. Stored in the platform data directory so it survives updates
/// and stays out of scanned locations.
pub fn log_path() -> Option<PathBuf> {
    let base = data_dir()?;
    Some(base.join("rusty-cleaner").join("activity.log"))
}

fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

/// Appends one event to the activity log. `kind` examples: "scan", "trash",
/// "empty-trash". `detail` is a free-form JSON-serializable summary.
/// Failures are ignored on purpose: logging must never break a scan.
pub fn record(kind: &str, detail: &str) {
    let Some(path) = log_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let escaped = detail.replace('\\', "\\\\").replace('"', "\\\"");
    let _ = writeln!(
        file,
        r#"{{"ts":{timestamp},"kind":"{kind}","detail":"{escaped}"}}"#
    );
}

#[cfg(test)]
mod tests {
    use super::{log_path, record};
    use std::fs;

    #[test]
    fn appends_json_lines_to_the_log() {
        let Some(path) = log_path() else { return };
        let before = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .count();
        record("test", "hello");
        let after = fs::read_to_string(&path).unwrap_or_default();
        let lines: Vec<&str> = after.lines().collect();
        assert_eq!(lines.len(), before + 1);
        let last = lines.last().unwrap();
        assert!(last.contains(r#""kind":"test""#));
        assert!(last.contains(r#""detail":"hello""#));
    }
}
