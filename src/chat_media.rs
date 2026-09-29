use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

pub fn scan() -> Vec<Finding> {
    scanner::walk_files(&platform::chat_dirs(), 10, |path, _| {
        matches!(
            path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_lowercase()
                .as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "mp4" | "mov" | "opus" | "ogg" | "pdf"
        )
    })
    .into_iter()
    .map(|(path, metadata)| scanner::finding(Feature::ChatMedia, path, metadata))
    .collect()
}
