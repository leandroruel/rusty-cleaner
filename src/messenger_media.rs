use crate::scanner;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How many bytes are read to classify a file by content. Every signature we
/// match lives within the first 64 bytes.
const SNIFF_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Document,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
        }
    }
}

pub struct MediaItem {
    pub path: PathBuf,
    pub kind: MediaKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Sniffs the media kind from the first bytes of a file. Telegram and Discord
/// keep real photos and videos behind hash names with no extension — the
/// only way to see them is to look at the content.
pub fn sniff_media(bytes: &[u8]) -> Option<MediaKind> {
    if bytes.starts_with(b"%PDF") {
        return Some(MediaKind::Document);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(MediaKind::Image);
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some(MediaKind::Image);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(MediaKind::Image);
    }
    if bytes.starts_with(b"fLaC") {
        return Some(MediaKind::Audio);
    }
    if bytes.starts_with(b"OggS") {
        return Some(MediaKind::Audio);
    }
    if bytes.starts_with(b"ID3") {
        return Some(MediaKind::Audio);
    }
    // Raw MPEG audio sync word.
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Some(MediaKind::Audio);
    }
    if bytes.starts_with(b"BM") && bytes.len() > 14 {
        return Some(MediaKind::Image);
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 {
        return match &bytes[8..12] {
            b"WEBP" => Some(MediaKind::Image),
            b"WAVE" => Some(MediaKind::Audio),
            b"AVI " => Some(MediaKind::Video),
            _ => None,
        };
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return Some(match &bytes[8..12] {
            b"M4A " | b"M4B " | b"M4P " => MediaKind::Audio,
            b"heic" | b"heix" | b"hevc" | b"hevx" => MediaKind::Image,
            // isom, mp41, mp42, qt, avc1, dash, msnv… all video containers.
            _ => MediaKind::Video,
        });
    }
    // EBML: matroska and webm.
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(MediaKind::Video);
    }
    None
}

/// Media kind of a file: by extension when it has one, by content otherwise.
pub fn media_kind_of(path: &Path) -> Option<MediaKind> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "heic" | "heif" => {
            return Some(MediaKind::Image)
        }
        "mp4" | "mov" | "mkv" | "webm" | "3gp" | "avi" | "m4v" => return Some(MediaKind::Video),
        "opus" | "ogg" | "oga" | "mp3" | "m4a" | "wav" | "flac" => return Some(MediaKind::Audio),
        "pdf" => return Some(MediaKind::Document),
        _ => {}
    }
    let mut file = File::open(path).ok()?;
    let mut buffer = [0_u8; SNIFF_LEN];
    let read = file.read(&mut buffer).ok()?;
    sniff_media(&buffer[..read])
}

/// Lists every media file under `roots`: files with media extensions plus
/// extensionless blobs that sniff as media (Telegram/Discord cache blobs).
/// Cache directories are descended into — their blobs are real media.
pub fn scan_roots(roots: &[PathBuf], _excluded: &[PathBuf]) -> Vec<MediaItem> {
    let mut items = Vec::new();
    for root in roots {
        let found = scanner::walk_with_dir_exclusions(
            std::slice::from_ref(root),
            14,
            |path, metadata| metadata.len() > 0 && media_kind_of(path).is_some(),
            false,
        );
        for (path, metadata) in found {
            let Some(kind) = media_kind_of(&path) else {
                continue;
            };
            items.push(MediaItem {
                kind,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                path,
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::{media_kind_of, sniff_media, MediaKind};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sniffs_media_signatures() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
        assert_eq!(sniff_media(&jpeg), Some(MediaKind::Image));
        let mut png = vec![0x89, 0x50, 0x4E, 0x47];
        png.extend_from_slice(b"rest");
        assert_eq!(sniff_media(&png), Some(MediaKind::Image));
        let gif = b"GIF89a.....".to_vec();
        assert_eq!(sniff_media(&gif), Some(MediaKind::Image));
        let webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert_eq!(sniff_media(&webp), Some(MediaKind::Image));
        let wav = b"RIFF\x00\x00\x00\x00WAVEfmt ".to_vec();
        assert_eq!(sniff_media(&wav), Some(MediaKind::Audio));
        let avi = b"RIFF\x00\x00\x00\x00AVI LIST".to_vec();
        assert_eq!(sniff_media(&avi), Some(MediaKind::Video));
        let mp4 = b"\x00\x00\x00\x20ftypisom\x00\x00\x02\x00".to_vec();
        assert_eq!(sniff_media(&mp4), Some(MediaKind::Video));
        let heic = b"\x00\x00\x00\x20ftypheic\x00\x00\x02\x00".to_vec();
        assert_eq!(sniff_media(&heic), Some(MediaKind::Image));
        let m4a = b"\x00\x00\x00\x20ftypM4A \x00\x00\x02\x00".to_vec();
        assert_eq!(sniff_media(&m4a), Some(MediaKind::Audio));
        let mkv = [0x1A, 0x45, 0xDF, 0xA3, 0x93, 0x42, 0x82];
        assert_eq!(sniff_media(&mkv), Some(MediaKind::Video));
        assert_eq!(sniff_media(b"ID3\x04\x00tagdata"), Some(MediaKind::Audio));
        assert_eq!(sniff_media(b"OggS\x00\x02opus"), Some(MediaKind::Audio));
        assert_eq!(sniff_media(b"fLaCstream"), Some(MediaKind::Audio));
        assert_eq!(sniff_media(b"%PDF-1.7 doc"), Some(MediaKind::Document));
        assert_eq!(
            sniff_media(b"BM\x36\x00\x00\x00\x00\x00\x00\x00bitmapdata"),
            Some(MediaKind::Image)
        );
        // Raw binary cache index files are not media.
        assert_eq!(sniff_media(b"\x00\x01\x02\x03random-data"), None);
        assert_eq!(sniff_media(b""), None);
    }

    #[test]
    fn classifies_by_extension_and_by_content() {
        assert_eq!(
            media_kind_of(Path::new("photo.JPG")),
            Some(MediaKind::Image)
        );
        assert_eq!(media_kind_of(Path::new("clip.mp4")), Some(MediaKind::Video));
        assert_eq!(
            media_kind_of(Path::new("voice.opus")),
            Some(MediaKind::Audio)
        );
        assert_eq!(
            media_kind_of(Path::new("doc.pdf")),
            Some(MediaKind::Document)
        );

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-cleaner-sniff-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        // Telegram-style blob: a real JPEG with a hash name, no extension.
        let mut blob = vec![0xFF, 0xD8, 0xFF, 0xE0];
        blob.extend(std::iter::repeat(1_u8).take(512));
        let blob_path: PathBuf = root.join("5CA0108B034D");
        fs::write(&blob_path, &blob).unwrap();
        assert_eq!(media_kind_of(&blob_path), Some(MediaKind::Image));

        // A non-media blob stays out.
        fs::write(root.join("data_0"), [0_u8; 128]).unwrap();
        assert_eq!(media_kind_of(&root.join("data_0")), None);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_roots_finds_blobs_and_saved_media() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-cleaner-media-scan-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("user_data/media")).unwrap();
        let mut blob = vec![0xFF, 0xD8, 0xFF, 0xE0];
        blob.extend(std::iter::repeat(2_u8).take(1024));
        fs::write(root.join("user_data/media/AABBCC"), &blob).unwrap();
        fs::write(root.join("photo.jpg"), [3_u8; 256]).unwrap();
        fs::write(root.join("notes.txt"), b"not media").unwrap();
        fs::write(root.join("user_data/media/index-db"), [0_u8; 64]).unwrap();

        let items = super::scan_roots(std::slice::from_ref(&root), &[]);

        fs::remove_dir_all(&root).unwrap();
        assert_eq!(items.len(), 2, "blob by content + file by extension");
        assert!(items.iter().any(|item| item.path.ends_with("AABBCC")));
        assert!(items.iter().any(|item| item.path.ends_with("photo.jpg")));
    }
}
