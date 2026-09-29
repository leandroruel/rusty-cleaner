use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

pub fn scan() -> Vec<Finding> {
    scanner::walk_with_dir_exclusions(
        &platform::browser_dirs(),
        8,
        |path, _| {
            let text = path.to_string_lossy().to_lowercase();
            text.contains("cache") || text.contains("cache2") || text.contains("code cache")
        },
        false,
    )
    .into_iter()
    .map(|(path, metadata)| scanner::finding(Feature::Browser, path, metadata))
    .collect()
}
