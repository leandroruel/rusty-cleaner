use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Optional observer notified when the walker enters a directory. Used by the
/// desktop app to show which folder is being scanned right now.
type ProgressCallback = Box<dyn Fn(&Path) + Send>;

struct ProgressState {
    callback: Option<ProgressCallback>,
    last_emit: std::time::Instant,
}

static PROGRESS: Mutex<Option<ProgressState>> = Mutex::new(None);

/// Minimum interval between progress reports. Emitting per directory floods
/// the UI with IPC events on large scans; the walker walks far faster than
/// any human can read the current path.
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

pub fn set_progress_callback(callback: Option<ProgressCallback>) {
    if let Ok(mut slot) = PROGRESS.lock() {
        *slot = callback.map(|callback| ProgressState {
            callback: Some(callback),
            last_emit: std::time::Instant::now(),
        });
    }
}

fn report_progress(path: &Path) {
    let Ok(mut state) = PROGRESS.lock() else {
        return;
    };
    let Some(state) = state.as_mut() else {
        return;
    };
    if state.last_emit.elapsed() < PROGRESS_INTERVAL {
        return;
    }
    state.last_emit = std::time::Instant::now();
    if let Some(callback) = state.callback.as_ref() {
        callback(path);
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
    Registry,
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
            "registry" => Some(Self::Registry),
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
            Self::Registry => "registry",
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
    /// Feature-specific extra data. For registry findings, the registry value
    /// name to remove (`None` means the whole key must be removed).
    pub meta: Option<String>,
}

/// Walks `roots` and returns every accepted file together with the metadata
/// already obtained during the walk, so callers never pay a second stat.
pub fn walk_files(
    roots: &[PathBuf],
    max_depth: usize,
    accept: impl Fn(&Path, &Metadata) -> bool + Sync,
) -> Vec<(PathBuf, Metadata)> {
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
    accept: impl Fn(&Path, &Metadata) -> bool + Sync,
    respect_dir_exclusions: bool,
) -> Vec<(PathBuf, Metadata)> {
    let user_excluded = crate::settings::load_excluded_dirs();
    walk_parallel(
        roots,
        max_depth,
        &user_excluded,
        respect_dir_exclusions,
        &accept,
    )
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

/// Work queue for the parallel walk. `pending` counts jobs sitting in the
/// queue plus jobs being processed; it reaches zero exactly when the whole
/// tree is done, which is the workers' termination signal.
struct WalkerShared {
    queue: Vec<(PathBuf, usize)>,
    pending: usize,
}

/// Walks every root on all available threads, preserving the sequential
/// walker's rules exactly: symlinks are never followed, hidden files are
/// included, user exclusions and the heavy-directory list are honored and
/// `max_depth` is respected. Directory I/O is syscall-bound, so plain worker
/// threads over a shared queue turn the walk into a 2-4x speedup on NVMe.
fn walk_parallel(
    roots: &[PathBuf],
    max_depth: usize,
    user_excluded: &[PathBuf],
    respect_dir_exclusions: bool,
    accept: &(impl Fn(&Path, &Metadata) -> bool + Sync),
) -> Vec<(PathBuf, Metadata)> {
    let mut shared = WalkerShared {
        queue: Vec::new(),
        pending: 0,
    };
    for root in roots {
        if user_excluded
            .iter()
            .any(|excluded| root.starts_with(excluded))
        {
            continue;
        }
        shared.pending += 1;
        shared.queue.push((root.clone(), 0));
    }
    if shared.pending == 0 {
        return Vec::new();
    }

    let pair = (Mutex::new(shared), Condvar::new());
    let found: Mutex<Vec<(PathBuf, Metadata)>> = Mutex::new(Vec::new());
    let config = WalkConfig {
        lock: &pair.0,
        wake: &pair.1,
        found: &found,
        max_depth,
        user_excluded,
        respect_dir_exclusions,
        accept: accept as &(dyn Fn(&Path, &Metadata) -> bool + Sync),
    };
    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4);

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let (path, depth) = {
                    let Ok(mut guard) = config.lock.lock() else {
                        return;
                    };
                    loop {
                        match guard.queue.pop() {
                            Some(job) => break job,
                            None if guard.pending == 0 => return,
                            None => {
                                let Ok(next) = config.wake.wait(guard) else {
                                    return;
                                };
                                guard = next;
                            }
                        }
                    }
                };
                walk_directory(&config, &path, depth);
                if let Ok(mut guard) = config.lock.lock() {
                    guard.pending -= 1;
                    if guard.pending == 0 {
                        config.wake.notify_all();
                    }
                }
            });
        }
    });

    found.into_inner().unwrap()
}

/// Everything the walk workers share: the work queue, the accepted-files
/// sink and the walk rules.
struct WalkConfig<'a> {
    lock: &'a Mutex<WalkerShared>,
    wake: &'a Condvar,
    found: &'a Mutex<Vec<(PathBuf, Metadata)>>,
    max_depth: usize,
    user_excluded: &'a [PathBuf],
    respect_dir_exclusions: bool,
    accept: &'a (dyn Fn(&Path, &Metadata) -> bool + Sync),
}

/// Processes one directory: accepted files are flushed to `found` in a single
/// lock per directory, and subdirectories are enqueued for other workers.
fn walk_directory(config: &WalkConfig<'_>, path: &Path, depth: usize) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_file() {
        if (config.accept)(path, &metadata) {
            config
                .found
                .lock()
                .unwrap()
                .push((path.to_path_buf(), metadata));
        }
        return;
    }
    if !metadata.is_dir() || depth >= config.max_depth {
        return;
    }
    if config.respect_dir_exclusions && depth > 0 && is_excluded_dir(path) {
        return;
    }
    if config
        .user_excluded
        .iter()
        .any(|excluded| path.starts_with(excluded))
    {
        return;
    }
    report_progress(path);
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    let mut accepted = Vec::new();
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        // DirEntry::metadata stats through the parent directory handle and,
        // like symlink_metadata, never follows symlinks.
        let Ok(child_metadata) = entry.metadata() else {
            continue;
        };
        let child_path = entry.path();
        if child_metadata.file_type().is_symlink() {
            continue;
        }
        if child_metadata.is_file() {
            if (config.accept)(&child_path, &child_metadata) {
                accepted.push((child_path, child_metadata));
            }
        } else if child_metadata.is_dir() {
            subdirs.push((child_path, depth + 1));
        }
    }
    if !accepted.is_empty() {
        config.found.lock().unwrap().extend(accepted);
    }
    if !subdirs.is_empty() {
        if let Ok(mut guard) = config.lock.lock() {
            guard.pending += subdirs.len();
            guard.queue.append(&mut subdirs);
            config.wake.notify_all();
        }
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

/// Builds a finding from a path and the metadata already collected by the
/// walker. Callers that lack fresh metadata can `fs::metadata` themselves,
/// but the walker's output must never be stat-ted again.
pub fn finding(feature: Feature, path: PathBuf, metadata: Metadata) -> Finding {
    Finding {
        feature,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
        path,
        meta: None,
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
    use std::path::{Path, PathBuf};

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

        let browser_paths: Vec<PathBuf> = browser.into_iter().map(|(path, _)| path).collect();
        assert!(
            generic.is_empty(),
            "generic walker must skip .cache trees, found {generic:?}"
        );
        assert_eq!(browser_paths, vec![cache_file]);
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
