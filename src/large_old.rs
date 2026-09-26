use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

const MIN_SIZE: u64 = 100 * 1024 * 1024;
const MIN_AGE_DAYS: u64 = 180;

pub fn scan() -> Vec<Finding> {
    let roots = platform::home_dir().into_iter().collect::<Vec<_>>();
    scanner::walk_files(&roots, 12, |_, metadata| {
        metadata.len() >= MIN_SIZE
            && scanner::age_days(metadata).is_some_and(|days| days >= MIN_AGE_DAYS)
    })
    .into_iter()
    .map(|path| scanner::finding(Feature::LargeOld, path))
    .collect()
}
