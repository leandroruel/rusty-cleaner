pub mod browser;
pub mod chat_media;
pub mod duplicates;
pub mod large_old;
pub mod orphan;
pub mod platform;
pub mod scanner;
pub mod temp;
pub mod trash;

use scanner::{Feature, Finding};

pub fn scan(feature: Option<Feature>) -> Vec<Finding> {
    match feature {
        Some(Feature::Orphan) => orphan::scan(),
        Some(Feature::Temporary) => temp::scan(),
        Some(Feature::ChatMedia) => chat_media::scan(),
        Some(Feature::Trash) => trash::scan(),
        Some(Feature::Browser) => browser::scan(),
        Some(Feature::Duplicates) => duplicates::scan(),
        Some(Feature::LargeOld) => large_old::scan(),
        None => {
            let mut findings = Vec::new();
            findings.extend(orphan::scan());
            findings.extend(temp::scan());
            findings.extend(chat_media::scan());
            findings.extend(trash::scan());
            findings.extend(browser::scan());
            findings.extend(duplicates::scan());
            findings.extend(large_old::scan());
            findings
        }
    }
}
