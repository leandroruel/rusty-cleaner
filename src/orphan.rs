use crate::{
    platform,
    scanner::{self, Feature, Finding},
};
use std::time::Duration;

pub fn scan() -> Vec<Finding> {
    // Old app-data files are only candidates; age alone cannot prove an app was uninstalled.
    scanner::walk_files(&platform::app_data_dirs(), 5, |_, metadata| {
        scanner::age_days(metadata)
            .is_some_and(|days| days >= Duration::from_secs(180 * 86_400).as_secs() / 86_400)
    })
    .into_iter()
    .map(|path| scanner::finding(Feature::Orphan, path))
    .collect()
}
