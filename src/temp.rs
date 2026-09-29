use crate::{
    platform,
    scanner::{self, Feature, Finding},
};

const MIN_AGE_DAYS: u64 = 7;

pub fn scan() -> Vec<Finding> {
    scanner::walk_files(&platform::temp_dirs(), 6, |_, metadata| is_stale(metadata))
        .into_iter()
        .map(|(path, metadata)| scanner::finding(Feature::Temporary, path, metadata))
        .collect()
}

fn is_stale(metadata: &std::fs::Metadata) -> bool {
    scanner::age_days(metadata).is_some_and(|days| days >= MIN_AGE_DAYS)
}

#[cfg(test)]
mod tests {
    use super::is_stale;
    use std::{fs, time::SystemTime};

    #[test]
    fn does_not_flag_a_recent_temporary_file() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("rusty-cleaner-temp-{}-{nonce}", std::process::id()));
        fs::write(&path, b"recent").unwrap();
        let metadata = fs::metadata(&path).unwrap();

        assert!(!is_stale(&metadata));

        fs::remove_file(path).unwrap();
    }
}
