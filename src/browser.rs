use crate::{
    parallel, platform,
    scanner::{self, Feature, Finding},
    settings,
};
use std::path::{Path, PathBuf};

/// How deep the discovery walks browser roots looking for cache directories,
/// and how deep the size walk goes inside a cache tree.
const DISCOVER_DEPTH: usize = 6;
const SIZE_DEPTH: usize = 10;

/// A cache directory is one cleanup candidate, never its individual files.
/// Browser caches hold hundreds of thousands of small files; emitting a
/// finding per file has previously exhausted RAM during the scan, the IPC
/// round trip and — worst — the OS trash flow, which on Windows creates
/// recycle-bin metadata per item. The browsers rebuild these trees from
/// scratch, so the whole directory is the unit of work.
pub fn scan() -> Vec<Finding> {
    let excluded = settings::load_excluded_dirs();
    let mut dirs = Vec::new();
    for root in platform::browser_dirs() {
        if user_excludes(&root, &excluded) {
            continue;
        }
        if is_cache_directory(&root) {
            // Roots like ~/snap/firefox/common/.cache are caches themselves.
            dirs.push(root);
        } else {
            discover(&root, 0, &excluded, &mut dirs);
        }
    }
    dirs.sort();
    dirs.dedup();

    parallel::parallel_map(dirs, |dir| cache_dir_finding(&dir))
        .into_iter()
        .flatten()
        .collect()
}

/// Any directory with "cache" in its name is a cache directory. This covers
/// the Chromium family (Cache, Cache_Data, Code Cache, GPUCache, ShaderCache,
/// DawnCache, CacheStorage, ScriptCache), Firefox (cache2, startupCache,
/// jumpListCache) and future variants, without hardcoding per-browser lists.
/// "Service Worker" is deliberately not matched: it also holds registrations
/// in Database/, while its CacheStorage and ScriptCache children match here.
fn is_cache_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.to_ascii_lowercase().contains("cache")
}

/// A path is excluded when it sits inside a user-excluded folder — or when it
/// is an ancestor of one, since purging it would take the excluded folder
/// down with it.
fn user_excludes(path: &Path, excluded: &[PathBuf]) -> bool {
    excluded
        .iter()
        .any(|entry| path.starts_with(entry) || entry.starts_with(path))
}

fn discover(path: &Path, depth: usize, excluded: &[PathBuf], out: &mut Vec<PathBuf>) {
    if depth > DISCOVER_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        // DirEntry::metadata never follows symlinks, matching the walker.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        if is_cache_directory(&child) {
            // The cache directory is the candidate; do not descend into it.
            if !user_excludes(&child, excluded) {
                out.push(child);
            }
        } else {
            discover(&child, depth + 1, excluded, out);
        }
    }
}

fn cache_dir_finding(dir: &Path) -> Option<Finding> {
    let metadata = std::fs::symlink_metadata(dir).ok()?;
    let size = scanner::walk_with_dir_exclusions(
        std::slice::from_ref(&dir.to_path_buf()),
        SIZE_DEPTH,
        |_, _| true,
        false,
    )
    .into_iter()
    .map(|(_, metadata)| metadata.len())
    .sum();
    // Empty caches free nothing and would only add noise.
    if size == 0 {
        return None;
    }
    Some(Finding {
        feature: Feature::Browser,
        name: dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size,
        modified: metadata.modified().ok(),
        path: dir.to_path_buf(),
        // Marks the finding for the cleanup routing: cache directories are
        // purged in place, never sent to the trash.
        meta: Some("cache-dir".to_owned()),
    })
}

/// Permanently removes a regenerable browser cache directory (or file) and
/// recreates the directory empty, so the browser finds its cache root on
/// the next run. Nothing is sent to the OS trash: cache trees can hold
/// hundreds of thousands of files, and per-item recycle-bin metadata has
/// exhausted system memory on Windows in the field.
pub fn purge(path: &Path) -> Result<(), String> {
    if !is_safe_to_purge(path) {
        return Err(format!(
            "refusing to purge a path that does not look like browser cache: {}",
            path.display()
        ));
    }
    purge_unchecked(path)
}

/// A path may only be purged when it lives inside a known browser root and
/// carries a cache-like component. Defense in depth: the frontend only sends
/// browser findings here, but a bug (or a crafted invoke) must never turn
/// this into an arbitrary-delete command.
pub fn is_safe_to_purge(path: &Path) -> bool {
    let under_browser_root = platform::browser_dirs()
        .iter()
        .any(|root| path.starts_with(root));
    let cache_like = path
        .components()
        .any(|component| is_cache_directory(component.as_ref()));
    under_browser_root && cache_like
}

fn purge_unchecked(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("refusing to follow a symlink".to_owned());
    }
    if metadata.is_dir() {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path).map_err(|error| error.to_string())
    } else {
        std::fs::remove_file(path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{discover, is_safe_to_purge, purge_unchecked};
    use crate::platform;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rusty-cleaner-browsers-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn recognizes_cache_directory_names() {
        for name in [
            "Cache",
            "cache2",
            "Code Cache",
            "GPUCache",
            "CacheStorage",
            "DawnWebGPUCache",
            "startupCache",
            "jumpListCache",
        ] {
            assert!(cache_name_checks(name), "{name}");
        }
        assert!(!cache_name_checks("Local Storage"));
        assert!(!cache_name_checks("IndexedDB"));
        assert!(!cache_name_checks("Default"));
        assert!(!cache_name_checks("Service Worker"));
    }

    #[test]
    fn collects_one_finding_per_cache_directory_with_the_whole_tree_size() {
        let root = temp_root("aggregate");
        fs::create_dir_all(root.join("Default/Cache/nested")).unwrap();
        fs::create_dir_all(root.join("Default/Local Storage")).unwrap();
        fs::create_dir_all(root.join("Guest Profile/Code Cache")).unwrap();
        fs::write(root.join("Default/Cache/f_0001"), [1_u8; 1024]).unwrap();
        fs::write(root.join("Default/Cache/f_0002"), [1_u8; 2048]).unwrap();
        fs::write(root.join("Default/Cache/nested/f_0003"), [1_u8; 512]).unwrap();
        fs::write(root.join("Default/Local Storage/leveldb"), [1_u8; 4096]).unwrap();
        fs::write(root.join("Guest Profile/Code Cache/js_0001"), [1_u8; 256]).unwrap();

        let mut dirs = Vec::new();
        discover(&root, 0, &[], &mut dirs);
        dirs.sort();

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(dirs.len(), 2, "one candidate per cache directory");
        assert_eq!(dirs[0], root.join("Default/Cache"));
        assert_eq!(dirs[1], root.join("Guest Profile/Code Cache"));
    }

    #[test]
    fn honors_user_exclusions_in_both_directions() {
        let root = temp_root("exclusions");
        fs::create_dir_all(root.join("Default/Cache")).unwrap();
        fs::create_dir_all(root.join("Guest Profile/Code Cache")).unwrap();
        fs::write(root.join("Default/Cache/f_0001"), b"data").unwrap();
        fs::write(root.join("Guest Profile/Code Cache/js_0001"), b"data").unwrap();

        // A cache directory inside an excluded path is skipped...
        let mut dirs = Vec::new();
        let excluded_inside = vec![root.join("Default").to_path_buf()];
        discover(&root, 0, &excluded_inside, &mut dirs);
        assert_eq!(dirs, vec![root.join("Guest Profile/Code Cache")]);

        // ...and a cache directory that CONTAINS an excluded path is skipped
        // too, since purging it would delete the excluded folder.
        let mut dirs = Vec::new();
        let excluded_within = vec![root.join("Default/Cache/nested").to_path_buf()];
        discover(&root, 0, &excluded_within, &mut dirs);
        assert!(
            !dirs.contains(&root.join("Default/Cache")),
            "cache containing an exclusion must be skipped"
        );
        assert!(
            dirs.contains(&root.join("Guest Profile/Code Cache")),
            "unrelated caches are still discovered"
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn skips_empty_caches_and_cache_named_files() {
        let root = temp_root("empty");
        fs::create_dir_all(root.join("Default/Empty Cache")).unwrap();
        fs::write(root.join("Default/cache.dat"), b"not a directory").unwrap();

        let mut dirs = Vec::new();
        discover(&root, 0, &[], &mut dirs);

        // The cache-named directory is a candidate, but the file never is;
        // the empty cache then produces no finding.
        assert_eq!(dirs, vec![root.join("Default/Empty Cache")]);
        let finding = super::cache_dir_finding(&dirs[0]);

        fs::remove_dir_all(&root).unwrap();
        assert!(finding.is_none(), "empty caches produce no finding");
    }

    #[test]
    fn refuses_to_purge_paths_outside_browser_roots_or_without_a_cache_component() {
        // Outside every browser root — with or without a cache-like name.
        assert!(!is_safe_to_purge(Path::new(
            "/tmp/rusty-cleaner-innocent/Cache"
        )));
        assert!(!is_safe_to_purge(Path::new(
            "/tmp/rusty-cleaner-innocent/file.txt"
        )));
        // Inside a browser root: cache-like paths pass, anything else is
        // refused. Roots whose own path already contains a cache component
        // (like ~/.cache/mozilla) cannot express the negative case, so a
        // neutral root is used.
        let root = platform::browser_dirs()
            .into_iter()
            .find(|root| {
                !root
                    .components()
                    .any(|component| super::is_cache_directory(component.as_ref()))
            })
            .expect("at least one browser root without a cache component");
        assert!(!is_safe_to_purge(&root.join("Default")));
        assert!(is_safe_to_purge(&root.join("Default/Cache")));
    }

    #[test]
    fn purge_removes_the_tree_and_recreates_the_directory_empty() {
        let root = temp_root("purge");
        fs::create_dir_all(root.join("Cache/nested")).unwrap();
        fs::write(root.join("Cache/nested/f"), b"data").unwrap();
        let cache = root.join("Cache");

        purge_unchecked(&cache).unwrap();

        assert!(cache.is_dir(), "cache directory must be recreated empty");
        assert!(fs::read_dir(&cache).unwrap().next().is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn purge_refuses_a_symlink() {
        let root = temp_root("symlink");
        fs::create_dir_all(root.join("real")).unwrap();
        let link = root.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("real"), &link).unwrap();

        let result = purge_unchecked(&link);
        fs::remove_dir_all(&root).unwrap();
        #[cfg(unix)]
        assert!(result.is_err(), "symlinks must never be followed");
        #[cfg(not(unix))]
        let _ = result;
    }

    /// Checks the same predicate `is_cache_directory` applies to a directory
    /// name, without requiring a real path on disk.
    fn cache_name_checks(name: &str) -> bool {
        super::is_cache_directory(Path::new(name))
    }
}
