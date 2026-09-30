use crate::{
    browser, parallel, platform,
    scanner::{self, Feature, Finding},
    settings,
};
use std::path::Path;

/// Modern messengers store media as hash-named, extensionless cache blobs —
/// per-file extension matching finds nothing in them. The scan therefore has
/// two modes:
///
/// 1. **Cache directories** (regenerable): one finding per cache-named
///    directory, sized over its whole tree and purged in place, exactly like
///    browser caches.
/// 2. **Saved media files** (jpg/png/mp4/…): per-file findings that go to
///    the trash — the user chose to keep these.
pub fn scan() -> Vec<Finding> {
    let excluded = settings::load_excluded_dirs();
    let roots = platform::chat_dirs();

    let mut cache_dirs = Vec::new();
    for root in &roots {
        if browser::user_excludes(root, &excluded) {
            continue;
        }
        browser::discover(root, 0, &excluded, &mut cache_dirs);
    }
    cache_dirs.sort();
    cache_dirs.dedup();

    // Saved media: files with media extensions that do NOT live inside a
    // cache directory (those are counted as part of their cache finding).
    let media_files: Vec<_> = roots
        .iter()
        .flat_map(|root| {
            scanner::walk_with_dir_exclusions(
                std::slice::from_ref(root),
                10,
                |path, _| is_media_file(path) && !inside_any(path, &cache_dirs),
                false,
            )
        })
        .map(|(path, metadata)| scanner::finding(Feature::ChatMedia, path, metadata))
        .collect();

    let cache_findings = parallel::parallel_map(cache_dirs, |dir| {
        browser::cache_dir_finding(&dir, Feature::ChatMedia)
    })
    .into_iter()
    .flatten();

    cache_findings.chain(media_files).collect()
}

fn inside_any(path: &Path, dirs: &[std::path::PathBuf]) -> bool {
    dirs.iter().any(|dir| path.starts_with(dir))
}

fn is_media_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "webp"
            | "gif"
            | "mp4"
            | "mov"
            | "mkv"
            | "webm"
            | "3gp"
            | "opus"
            | "ogg"
            | "mp3"
            | "m4a"
            | "pdf"
    )
}

#[cfg(test)]
mod tests {
    use super::is_media_file;
    use crate::scanner::Feature;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recognizes_media_extensions_only() {
        for name in [
            "photo.jpg",
            "photo.JPEG",
            "video.mp4",
            "voice.opus",
            "doc.PDF",
            "animation.webp",
        ] {
            assert!(is_media_file(Path::new(name)), "{name}");
        }
        assert!(!is_media_file(Path::new("data_0")));
        assert!(!is_media_file(Path::new("f_000123")));
        assert!(!is_media_file(Path::new("cache")));
    }

    #[test]
    fn aggregates_extensionless_cache_blobs_into_one_finding() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rusty-cleaner-chat-{}-{nonce}", std::process::id()));
        // Telegram-like layout: hash-named blobs without extensions.
        let cache = root.join("tdata/user_data/cache/1/00");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("5CA0108B034D"), [1_u8; 1024]).unwrap();
        fs::write(cache.join("9054A322375E"), [1_u8; 2048]).unwrap();
        // A saved media file outside any cache.
        fs::create_dir_all(root.join("media")).unwrap();
        fs::write(root.join("media/photo.jpg"), [1_u8; 512]).unwrap();
        // A stray image inside the cache must not duplicate the cache finding.
        fs::write(cache.join("thumb.jpg"), [1_u8; 64]).unwrap();

        let excluded = crate::settings::load_excluded_dirs();
        let mut dirs = Vec::new();
        crate::browser::discover(&root, 0, &excluded, &mut dirs);
        let findings: Vec<_> = dirs
            .iter()
            .filter_map(|dir| crate::browser::cache_dir_finding(dir, Feature::ChatMedia))
            .collect();

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(findings.len(), 1, "one finding for the cache tree");
        assert_eq!(findings[0].size, 1024 + 2048 + 64);
        assert_eq!(findings[0].meta.as_deref(), Some("cache-dir"));
        assert!(findings[0].path.ends_with("user_data/cache"));
    }
}
