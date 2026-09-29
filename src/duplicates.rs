use crate::{
    parallel, platform,
    scanner::{self, Feature, Finding},
};
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};

type FileEntry = (PathBuf, Metadata);

/// Shared component directories (WebView2, Electron, etc.) where identical
/// files belong to different apps and must not be treated as removable
/// duplicates. Deleting one copy breaks the app that owns it.
fn is_shared_component(path: &Path) -> bool {
    let text = path.to_string_lossy().to_lowercase();
    text.contains("ebwebview")
        || text.contains("webview2")
        || text.contains("speech recognition")
        || text.contains("widevinecdm")
        || text.contains("subresource filter")
}

/// Pipeline: walk (metadata reused, no second stat) → group by exact size →
/// same-size groups are pre-filtered with a 64 KiB head hash before any full
/// read → full-content hash on the survivors → byte-exact confirmation
/// against each group's leader. Every heavy stage runs on all cores. The
/// exact comparison is what makes a group safe to delete — a 64-bit hash
/// alone is not proof.
pub fn scan() -> Vec<Finding> {
    let roots = platform::home_dir().into_iter().collect::<Vec<_>>();
    let files = scanner::walk_files(&roots, 12, |path, metadata| {
        metadata.len() >= 1_048_576
            && !is_shared_component(path)
            && !scanner::is_runtime_binary(path)
    });

    let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for entry in files {
        by_size.entry(entry.1.len()).or_default().push(entry);
    }

    // Same size is not same content: most groups die here without a single
    // full-file read. Candidates carry a subgroup id so files from different
    // size groups can never mix.
    let mut head_jobs: Vec<(usize, FileEntry)> = Vec::new();
    for (subgroup, entries) in by_size
        .into_values()
        .filter(|entries| entries.len() > 1)
        .enumerate()
    {
        head_jobs.extend(entries.into_iter().map(|entry| (subgroup, entry)));
    }
    let mut by_head: HashMap<(usize, u64), Vec<FileEntry>> = HashMap::new();
    for (subgroup, head, entry) in
        parallel::parallel_map(head_jobs, |(subgroup, (path, metadata))| {
            (subgroup, head_hash(&path), (path, metadata))
        })
    {
        if let Some(head) = head {
            by_head.entry((subgroup, head)).or_default().push(entry);
        }
    }

    // Full-content hash, in parallel, over the same-head survivors only.
    let mut hash_jobs: Vec<(usize, FileEntry)> = Vec::new();
    for (subgroup, entries) in by_head
        .into_values()
        .filter(|entries| entries.len() > 1)
        .enumerate()
    {
        hash_jobs.extend(entries.into_iter().map(|entry| (subgroup, entry)));
    }
    let mut by_hash: HashMap<(usize, u64), Vec<FileEntry>> = HashMap::new();
    for (subgroup, hash, entry) in
        parallel::parallel_map(hash_jobs, |(subgroup, (path, metadata))| {
            (subgroup, hash_file(&path), (path, metadata))
        })
    {
        if let Some(hash) = hash {
            by_hash.entry((subgroup, hash)).or_default().push(entry);
        }
    }

    // Byte-exact confirmation of each member against its group leader. A
    // member that differs is dropped; groups left with a single file are
    // discarded below — a file with no twin is not a duplicate.
    let mut confirm_jobs: Vec<(usize, ConfirmJob)> = Vec::new();
    for (group, entries) in by_hash
        .into_values()
        .filter(|entries| entries.len() > 1)
        .enumerate()
    {
        let mut iter = entries.into_iter();
        let Some(leader) = iter.next() else {
            continue;
        };
        let leader_path = leader.0.clone();
        confirm_jobs.push((group, ConfirmJob::Leader(leader)));
        confirm_jobs.extend(iter.map(|entry| {
            (
                group,
                ConfirmJob::Member {
                    leader: leader_path.clone(),
                    entry,
                },
            )
        }));
    }

    let mut confirmed: HashMap<usize, Vec<Finding>> = HashMap::new();
    for (group, finding) in parallel::parallel_map(confirm_jobs, |(group, job)| {
        let finding = match job {
            ConfirmJob::Leader(entry) => {
                Some(scanner::finding(Feature::Duplicates, entry.0, entry.1))
            }
            ConfirmJob::Member { leader, entry } => {
                if same_contents(&leader, &entry.0) {
                    Some(scanner::finding(Feature::Duplicates, entry.0, entry.1))
                } else {
                    None
                }
            }
        };
        (group, finding)
    }) {
        if let Some(finding) = finding {
            confirmed.entry(group).or_default().push(finding);
        }
    }

    let mut findings: Vec<Finding> = confirmed
        .into_values()
        .filter(|group| group.len() > 1)
        .flatten()
        .collect();
    findings.sort_by(|left, right| left.path.cmp(&right.path));
    findings
}

enum ConfirmJob {
    Leader(FileEntry),
    Member { leader: PathBuf, entry: FileEntry },
}

/// Hash of the first 64 KiB. Files in this scanner are at least 1 MiB, so a
/// differing head rules out a full read of multi-gigabyte same-size pairs.
fn head_hash(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut buffer = [0_u8; 64 * 1024];
    let read = file.read(&mut buffer).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(&buffer[..read]);
    Some(hasher.finish())
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
    use super::{is_shared_component, same_contents};
    use std::{fs, path::Path, time::SystemTime};

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

    #[test]
    fn excludes_shared_component_duplicates() {
        assert!(is_shared_component(Path::new(
            "C:/Users/App/EBWebView/Speech Recognition/1.0/speech.dll"
        )));
        assert!(is_shared_component(Path::new(
            "/home/user/.config/app/EBWebView/WidevineCdm/widevine.so"
        )));
        assert!(!is_shared_component(Path::new(
            "/home/user/Downloads/photo.jpg"
        )));
    }

    #[test]
    fn excludes_runtime_binaries_from_duplicate_scan() {
        assert!(crate::scanner::is_runtime_binary(Path::new(
            "C:/Users/App/Microsoft.CognitiveServices.Speech.core.dll"
        )));
        assert!(crate::scanner::is_runtime_binary(Path::new(
            "/home/user/.config/app/libffmpeg.so"
        )));
        assert!(!crate::scanner::is_runtime_binary(Path::new(
            "/home/user/Downloads/photo.jpg"
        )));
        assert!(!crate::scanner::is_runtime_binary(Path::new(
            "/home/user/Downloads/installer.exe.txt"
        )));
    }
}
