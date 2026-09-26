use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    mut accept: impl FnMut(&Path, &Metadata) -> bool,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        walk_one(root, 0, max_depth, &mut accept, &mut found);
    }
    found
}

fn walk_one(
    path: &Path,
    depth: usize,
    max_depth: usize,
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
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        walk_one(&entry.path(), depth + 1, max_depth, accept, found);
    }
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
