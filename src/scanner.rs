use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Optional observer notified when the walker enters a directory. Used by the
/// desktop app to show which folder is being scanned right now.
type ProgressCallback = Box<dyn Fn(&Path) + Send>;

static PROGRESS_CALLBACK: Mutex<Option<ProgressCallback>> = Mutex::new(None);

pub fn set_progress_callback(callback: Option<ProgressCallback>) {
    if let Ok(mut slot) = PROGRESS_CALLBACK.lock() {
        *slot = callback;
    }
}

fn report_progress(path: &Path) {
    if let Ok(slot) = PROGRESS_CALLBACK.lock() {
        if let Some(callback) = slot.as_ref() {
            callback(path);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Orphan,
    Temporary,
    ChatMedia,
    Trash,
    Browser,
    Duplicates,
    LargeOld,
}

impl Feature {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orphan" => Some(Self::Orphan),
            "temp" => Some(Self::Temporary),
            "chat-media" => Some(Self::ChatMedia),
            "trash" => Some(Self::Trash),
            "browser" => Some(Self::Browser),
            "duplicates" => Some(Self::Duplicates),
            "large-old" => Some(Self::LargeOld),
            _ => None,
        }
    }
}

impl std::fmt::Display for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Orphan => "orphan",
            Self::Temporary => "temp",
            Self::ChatMedia => "chat-media",
            Self::Trash => "trash",
            Self::Browser => "browser",
            Self::Duplicates => "duplicates",
            Self::LargeOld => "large-old",
        };
        f.write_str(label)
    }
}

/// Strategy pattern: every cleaning feature implements this trait and is
/// registered once in `scanners()`. Adding a feature means implementing
/// `Scanner` in a new module and pushing it into the registry — no `match`
/// statements to keep in sync.
pub trait Scanner: Sync {
    fn feature(&self) -> Feature;
    fn scan(&self) -> Vec<Finding>;
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub feature: Feature,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub fn walk_files(
    roots: &[PathBuf],
    max_depth: usize,
    accept: impl FnMut(&Path, &Metadata) -> bool,
) -> Vec<PathBuf> {
    walk_with_dir_exclusions(roots, max_depth, accept, true)
}

/// Same as [`walk_files`], but ignores the global heavy-directory exclusion
/// list (`.cache`, `flatpak`, `snap`, ...). Used by the browser scanner, whose
/// roots are explicit narrow cache paths that live inside those directories —
/// a global exclusion of `.cache` must never hide browser cache files from it.
/// User-configured exclusions from the settings screen are still respected.
pub fn walk_with_dir_exclusions(
    roots: &[PathBuf],
    max_depth: usize,
    mut accept: impl FnMut(&Path, &Metadata) -> bool,
    respect_dir_exclusions: bool,
) -> Vec<PathBuf> {
    let user_excluded = crate::settings::load_excluded_dirs();
    let mut found = Vec::new();
    for root in roots {
        if user_excluded
            .iter()
            .any(|excluded| root.starts_with(excluded))
        {
            continue;
        }
        walk_one(
            root,
            0,
            max_depth,
            &user_excluded,
            respect_dir_exclusions,
            &mut accept,
            &mut found,
        );
    }
    found
}

/// Runtime binaries and native libraries shipped by applications. Identical
/// copies across apps are required dependencies (e.g. WebView2, Electron,
/// Azure Speech SDK), not removable duplicates — deleting one breaks the app
/// that owns it. A disk cleaner should only ever flag user data.
pub fn is_runtime_binary(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "dll"
                    | "exe"
                    | "sys"
                    | "drv"
                    | "ocx"
                    | "cpl"
                    | "so"
                    | "dylib"
                    | "node"
                    | "pyd"
                    | "framework"
            )
        })
}

fn walk_one(
    path: &Path,
    depth: usize,
    max_depth: usize,
    user_excluded: &[PathBuf],
    respect_dir_exclusions: bool,
    accept: &mut impl FnMut(&Path, &Metadata) -> bool,
    found: &mut Vec<PathBuf>,
) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_file() {
        if accept(path, &metadata) {
            found.push(path.to_path_buf());
        }
        return;
    }
    if !metadata.is_dir() || depth >= max_depth {
        return;
    }
    if respect_dir_exclusions && depth > 0 && is_excluded_dir(path) {
        return;
    }
    if user_excluded
        .iter()
        .any(|excluded| path.starts_with(excluded))
    {
        return;
    }
    report_progress(path);
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        walk_one(
            &entry.path(),
            depth + 1,
            max_depth,
            user_excluded,
            respect_dir_exclusions,
            accept,
            found,
        );
    }
}

/// Directories that are never worth scanning for cleanup candidates: they are
/// either build artifacts/dependency caches managed by tools, or huge trees
/// that would make a full-home scan slow and memory-hungry.
fn is_excluded_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    matches!(
        name,
        "node_modules"
            | "target"
            | ".git"
            | ".svn"
            | ".hg"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".rustup"
            | ".cargo"
            | ".npm"
            | ".pnpm-store"
            | ".yarn"
            | ".gradle"
            | ".m2"
            | "go"
            | ".dotnet"
            | ".vscode-server"
            | ".vscode"
            | ".idea"
            | "Library"
            | "flatpak"
            | "snap"
            | ".cache"
            | "pip"
            | ".pip"
            | "site-packages"
            | "vendor"
            | "Pods"
            | ".pub-cache"
            | ".composer"
            | "node-gyp"
            | ".bun"
            | "deno"
    )
}

pub fn finding(feature: Feature, path: PathBuf) -> Finding {
    let metadata = fs::metadata(&path).ok();
    Finding {
        feature,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size: metadata.as_ref().map_or(0, Metadata::len),
        modified: metadata.and_then(|item| item.modified().ok()),
        path,
    }
}

pub fn age_days(metadata: &Metadata) -> Option<u64> {
    let modified = metadata.modified().ok()?;
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|age| age.as_secs() / 86_400)
}

pub fn unix_epoch() -> SystemTime {
    UNIX_EPOCH
}

#[cfg(test)]
mod tests {
    use super::{is_excluded_dir, is_runtime_binary, walk_files, walk_with_dir_exclusions};
    use std::path::Path;

    #[test]
    fn excludes_package_manager_and_toolchain_directories() {
        for name in [
            "node_modules",
            "target",
            ".cargo",
            ".rustup",
            ".npm",
            ".pnpm-store",
            "flatpak",
            "snap",
            "site-packages",
            "vendor",
        ] {
            assert!(
                is_excluded_dir(Path::new("/home/user").join(name).as_path()),
                "{name}"
            );
        }
        assert!(!is_excluded_dir(Path::new("/home/user/Downloads")));
        assert!(!is_excluded_dir(Path::new("/home/user/Documents")));
    }

    #[test]
    fn browser_scan_finds_cache_files_inside_a_dot_cache_directory() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-cleaner-browser-{}-{nonce}",
            std::process::id()
        ));
        // Mirror a snap browser layout: the cache tree is nested inside a
        // directory literally named ".cache", which the generic walker skips.
        let cache_file = root.join(".cache/mozilla/firefox/profile/cache2/entries/f_0");
        std::fs::create_dir_all(cache_file.parent().unwrap()).unwrap();
        std::fs::write(&cache_file, b"entry").unwrap();

        let accept = |path: &Path, _: &std::fs::Metadata| {
            path.to_string_lossy().to_lowercase().contains("cache")
        };
        let generic = walk_files(std::slice::from_ref(&root), 8, accept);
        let browser = walk_with_dir_exclusions(std::slice::from_ref(&root), 8, accept, false);

        std::fs::remove_dir_all(&root).unwrap();

        assert!(
            generic.is_empty(),
            "generic walker must skip .cache trees, found {generic:?}"
        );
        assert_eq!(browser, vec![cache_file]);
    }

    #[test]
    fn detects_runtime_binaries_by_extension() {
        for path in [
            "C:/Apps/App/Microsoft.CognitiveServices.Speech.core.dll",
            "C:/Apps/App/app.exe",
            "C:/Windows/System32/driver.sys",
            "/usr/lib/libwebkit2gtk-4.1.so",
            "/usr/lib/libtauri.dylib",
            "/home/user/.config/app/resources.pak.node",
        ] {
            assert!(is_runtime_binary(Path::new(path)), "{path}");
        }
        assert!(!is_runtime_binary(Path::new(
            "/home/user/Downloads/photo.jpg"
        )));
        assert!(!is_runtime_binary(Path::new("/home/user/notes.txt")));
        assert!(!is_runtime_binary(Path::new("/home/user/no-extension")));
    }
}
