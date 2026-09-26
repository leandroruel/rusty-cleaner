use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

pub fn scan() -> Vec<Finding> {
    scanner::walk_files(&platform::browser_dirs(), 8, |path, _| {
        let text = path.to_string_lossy().to_lowercase();
        text.contains("cache") || text.contains("cache2") || text.contains("code cache")
    })
    .into_iter()
    .map(|path| scanner::finding(Feature::Browser, path))
    .collect()
}
