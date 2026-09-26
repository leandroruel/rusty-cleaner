use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

pub fn scan() -> Vec<Finding> {
    scanner::walk_files(&platform::trash_dirs(), 12, |_, _| true)
        .into_iter()
        .map(|path| scanner::finding(Feature::Trash, path))
        .collect()
}
