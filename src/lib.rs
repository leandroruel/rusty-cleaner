pub mod browser;
pub mod chat_media;
pub mod duplicates;
pub mod large_old;
pub mod orphan;
pub mod platform;
pub mod scanner;
pub mod temp;
pub mod trash;

use scanner::{Feature, Finding, Scanner};

struct FnScanner {
    feature: Feature,
    scan_fn: fn() -> Vec<Finding>,
}

impl Scanner for FnScanner {
    fn feature(&self) -> Feature {
        self.feature
    }

    fn scan(&self) -> Vec<Finding> {
        (self.scan_fn)()
    }
}

/// Registry of all cleaning strategies. Order defines the default scan order.
pub fn scanners() -> Vec<Box<dyn Scanner>> {
    vec![
        Box::new(FnScanner {
            feature: Feature::Trash,
            scan_fn: trash::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::Temporary,
            scan_fn: temp::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::Browser,
            scan_fn: browser::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::ChatMedia,
            scan_fn: chat_media::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::Orphan,
            scan_fn: orphan::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::LargeOld,
            scan_fn: large_old::scan,
        }),
        Box::new(FnScanner {
            feature: Feature::Duplicates,
            scan_fn: duplicates::scan,
        }),
    ]
}

pub fn scan(feature: Option<Feature>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for scanner in scanners() {
        if feature.is_none() || feature == Some(scanner.feature()) {
            findings.extend(scanner.scan());
        }
    }
    findings
}
