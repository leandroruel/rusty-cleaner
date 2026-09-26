use crate::{
    platform,
    scanner::{self, Feature, Finding},
};
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn scan() -> Vec<Finding> {
    let roots = platform::home_dir().into_iter().collect::<Vec<_>>();
    let files = scanner::walk_files(&roots, 12, |_, metadata| metadata.len() >= 1_048_576);
    let mut by_size: HashMap<u64, Vec<_>> = HashMap::new();
    for path in files {
        if let Ok(metadata) = std::fs::metadata(&path) {
            by_size.entry(metadata.len()).or_default().push(path);
        }
    }
    let mut findings = Vec::new();
    for paths in by_size.into_values().filter(|paths| paths.len() > 1) {
        let mut by_hash: HashMap<u64, Vec<_>> = HashMap::new();
        for path in paths {
            if let Some(hash) = hash_file(&path) {
                by_hash.entry(hash).or_default().push(path);
            }
        }
        for group in by_hash.into_values().filter(|group| group.len() > 1) {
            let mut exact_groups: Vec<Vec<PathBuf>> = Vec::new();
            for path in group {
                if let Some(matching) = exact_groups
                    .iter_mut()
                    .find(|candidate| same_contents(&candidate[0], &path))
                {
                    matching.push(path);
                } else {
                    exact_groups.push(vec![path]);
                }
            }
            for exact_group in exact_groups.into_iter().filter(|group| group.len() > 1) {
                findings.extend(
                    exact_group
                        .into_iter()
                        .map(|path| scanner::finding(Feature::Duplicates, path)),
                );
            }
        }
    }
    findings
}

fn hash_file(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.write(&buffer[..read]);
    }
    Some(hasher.finish())
}

fn same_contents(left_path: &Path, right_path: &Path) -> bool {
    let (Ok(mut left), Ok(mut right)) = (File::open(left_path), File::open(right_path)) else {
        return false;
    };
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let (Ok(left_read), Ok(right_read)) =
            (left.read(&mut left_buffer), right.read(&mut right_buffer))
        else {
            return false;
        };
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return false;
        }
        if left_read == 0 {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::same_contents;
    use std::{fs, time::SystemTime};

    #[test]
    fn confirms_bytes_after_hash_candidates_match() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_dir = std::env::temp_dir().join(format!(
            "rusty-cleaner-duplicates-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&test_dir).unwrap();
        let first = test_dir.join("first.bin");
        let identical = test_dir.join("identical.bin");
        let different = test_dir.join("different.bin");
        fs::write(&first, b"same bytes").unwrap();
        fs::write(&identical, b"same bytes").unwrap();
        fs::write(&different, b"other bytes").unwrap();

        assert!(same_contents(&first, &identical));
        assert!(!same_contents(&identical, &different));

        fs::remove_dir_all(test_dir).unwrap();
    }
}
