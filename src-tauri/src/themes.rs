//! Theme download and update through HTTP — no git required. Themes are
//! public GitHub repositories; a `theme.json` at the root declares
//! metadata and asset paths, and `theme.css` is the entire visual theme.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Minimal theme manifest — identity, asset paths and nothing else.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Brand icon in the sidebar (png/jpg/webp; SVG is unreliable in the
    /// WebView asset protocol).
    #[serde(default)]
    pub brand: Option<String>,
    /// Font files to inject as @font-face (paths to .ttf/.woff2 files).
    #[serde(default)]
    pub fonts: Vec<String>,
}

/// A theme with every asset resolved to an absolute path and the theme.css
/// content ready to inject.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    /// The full theme.css content — CSS variables and custom rules.
    pub css: String,
    /// Absolute path to the theme directory.
    pub root: String,
    /// Total bytes of the theme directory on disk.
    pub size: u64,
    pub brand: Option<String>,
    pub fonts: Vec<ResolvedFont>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFont {
    pub family: String,
    pub path: String,
}

/// The commit SHA of the downloaded snapshot, stored in `.source-sha`
/// inside the theme directory for update checks.
const SOURCE_SHA_FILE: &str = ".source-sha";

pub fn themes_dir() -> PathBuf {
    rusty_cleaner::activity_log::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-cleaner")
        .join("themes")
}

/// Extracts `owner/repo` from a GitHub URL. Accepts full HTTPS URLs and
/// the short `owner/repo` form.
fn parse_repo(repo: &str) -> Result<String, String> {
    let trimmed = repo.trim().trim_end_matches('/');
    if let Some(rest) = trimmed.strip_suffix(".git") {
        let _ = rest;
    }
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let rest = rest.trim_matches('/');
        if rest.split('/').count() == 2 {
            return Ok(rest.to_owned());
        }
    }
    if trimmed.split('/').count() == 2 {
        return Ok(trimmed.to_owned());
    }
    Err(format!("not a GitHub repository: {repo}"))
}

/// Downloads the tarball for the repo's default branch and extracts it.
/// The GitHub API returns the tarball with the latest commit SHA in the
/// `X-GitHub-Commit-Sha`-equivalent way; we also fetch the commit SHA
/// separately for the update check.
pub fn download(repo: &str) -> Result<ResolvedTheme, String> {
    let owner_repo = parse_repo(repo)?;
    let sha = latest_commit_sha(&owner_repo)?;

    let root = themes_dir();
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let staging = root.join(format!(".staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;

    download_and_extract(&owner_repo, &staging)?;
    std::fs::write(staging.join(SOURCE_SHA_FILE), &sha).map_err(|error| error.to_string())?;
    std::fs::write(staging.join(".source-repo"), &owner_repo).map_err(|error| error.to_string())?;

    let manifest_path = find_manifest(&staging)?;
    let manifest = read_manifest(&manifest_path)?;
    if manifest.id.trim().is_empty()
        || manifest.id.contains(['/', '\\'])
        || manifest.id.contains("..")
    {
        return Err(format!("invalid theme id: {}", manifest.id));
    }

    let target = root.join(&manifest.id);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&staging, &target).map_err(|error| error.to_string())?;
    resolve(&target, manifest)
}

/// Lists every downloaded theme.
pub fn installed() -> Vec<ResolvedTheme> {
    let Ok(entries) = std::fs::read_dir(themes_dir()) else {
        return Vec::new();
    };
    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("theme.json");
        if !path.is_file() {
            continue;
        }
        if let Ok(manifest) = read_manifest(&path) {
            if let Ok(resolved) = resolve(&entry.path(), manifest) {
                themes.push(resolved);
            }
        }
    }
    themes.sort_by(|left, right| left.name.cmp(&right.name));
    themes
}

/// Checks which installed themes have a newer commit on GitHub. Returns
/// (theme_id, has_update) pairs. Themes without a stored SHA or network
/// access report no update.
pub fn check_updates() -> Vec<(String, bool)> {
    installed()
        .into_iter()
        .map(|theme| {
            let dir = themes_dir().join(&theme.id);
            let has_update = std::fs::read_to_string(dir.join(SOURCE_SHA_FILE))
                .ok()
                .and_then(|stored| {
                    let repo = read_repo_url(&dir);
                    repo.and_then(|repo| latest_commit_sha(&repo).ok())
                        .map(|latest| latest != stored.trim())
                })
                .unwrap_or(false);
            (theme.id, has_update)
        })
        .collect()
}

/// The repository URL is stored in `.source-repo` so update checks work
/// without a git clone.
fn read_repo_url(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(".source-repo"))
        .ok()
        .map(|url| url.trim().to_owned())
}

pub fn delete(id: &str) -> Result<(), String> {
    if id.trim().is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        return Err(format!("invalid theme id: {id}"));
    }
    let dir = themes_dir().join(id);
    if !dir.is_dir() {
        return Err("theme is not installed".to_owned());
    }
    std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())
}

/// Fetches the latest commit SHA from the GitHub API. One HTTP request.
fn latest_commit_sha(owner_repo: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{owner_repo}/commits?per_page=1");
    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "rusty-cleaner")
        .call()
        .map_err(|error| format!("GitHub API unreachable: {error}"))?;
    let text = response
        .into_string()
        .map_err(|error| format!("GitHub API read failed: {error}"))?;
    let commits: Vec<serde_json::Value> =
        serde_json::from_str(&text).map_err(|error| format!("GitHub API response: {error}"))?;
    let sha = commits
        .first()
        .and_then(|commit| commit.get("sha"))
        .and_then(|sha| sha.as_str())
        .ok_or("GitHub API returned no commits")?;
    Ok(sha.to_owned())
}

/// Downloads the tarball and extracts it into `destination`. The tarball
/// root directory (e.g. `tokyo-afterburn-main/`) is stripped.
fn download_and_extract(owner_repo: &str, destination: &Path) -> Result<(), String> {
    let url = format!("https://github.com/{owner_repo}/archive/refs/heads/main.tar.gz");
    let response = ureq::get(&url)
        .set("User-Agent", "rusty-cleaner")
        .call()
        .map_err(|error| format!("download failed: {error}"))?;

    let mut gz = flate2::read::GzDecoder::new(response.into_reader());
    let mut tar = tar::Archive::new(&mut gz);
    tar.set_overwrite(true);
    tar.unpack(destination)
        .map_err(|error| format!("extract failed: {error}"))?;

    // GitHub tarballs have a top-level directory like `repo-main/`.
    // Move its contents up to `destination` and remove the wrapper.
    let entries = std::fs::read_dir(destination).map_err(|error| error.to_string())?;
    let wrapper = entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .ok_or("tarball has no root directory")?;
    for entry in std::fs::read_dir(&wrapper).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let name = path.file_name().unwrap_or_default();
        std::fs::rename(&path, destination.join(name)).map_err(|error| error.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&wrapper);
    Ok(())
}

/// Finds theme.json inside the extracted directory (it might be nested
/// one level deeper in some tarball layouts).
fn find_manifest(dir: &Path) -> Result<PathBuf, String> {
    let direct = dir.join("theme.json");
    if direct.is_file() {
        return Ok(direct);
    }
    for entry in std::fs::read_dir(dir)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let candidate = entry.path().join("theme.json");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("theme.json not found in the repository".to_owned())
}

fn read_manifest(path: &Path) -> Result<ThemeManifest, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("theme.json: {error}"))?;
    let manifest: ThemeManifest =
        serde_json::from_str(&text).map_err(|error| format!("theme.json: {error}"))?;
    Ok(manifest)
}

/// Strips CSS constructs that could load external resources or execute code.
fn sanitize_css(css: &str) -> String {
    css.lines()
        .filter(|line| {
            let trimmed = line.trim().to_lowercase();
            !(trimmed.starts_with("@import")
                || trimmed.contains("javascript:")
                || trimmed.contains("expression(")
                || trimmed.contains("data:text/html")
                || trimmed.starts_with("<script"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Total size of a directory tree.
fn dir_size(path: &Path, depth: usize) -> u64 {
    if depth > 8 {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            total += dir_size(&entry.path(), depth + 1);
        }
    }
    total
}

fn resolve(root: &Path, manifest: ThemeManifest) -> Result<ResolvedTheme, String> {
    let absolute = |relative: &Option<String>| -> Option<String> {
        let relative = relative.as_ref()?;
        if relative.is_empty() {
            return None;
        }
        let path = if Path::new(relative).is_absolute() {
            PathBuf::from(relative)
        } else {
            root.join(relative)
        };
        path.is_file().then(|| path.to_string_lossy().into_owned())
    };

    let css_path = root.join("theme.css");
    let css = std::fs::read_to_string(&css_path).map_err(|error| format!("theme.css: {error}"))?;
    let css = sanitize_css(&css);

    let fonts = manifest
        .fonts
        .iter()
        .filter_map(|path| {
            let full = root.join(path);
            if !full.is_file() {
                return None;
            }
            let family = full
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "ThemeFont".to_owned());
            Some(ResolvedFont {
                family,
                path: full.to_string_lossy().into_owned(),
            })
        })
        .collect();

    // Store the repo URL for future update checks.
    // .source-repo is written by download() with the GitHub owner/repo;
    // resolve() only writes a fallback for legacy themes without it.
    if !root.join(".source-repo").is_file() {
        let _ = std::fs::write(root.join(".source-repo"), &manifest.id);
    }

    Ok(ResolvedTheme {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        author: manifest.author,
        description: manifest.description,
        css,
        root: root.to_string_lossy().into_owned(),
        size: dir_size(root, 0),
        brand: absolute(&manifest.brand),
        fonts,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_repo, sanitize_css, ThemeManifest};

    #[test]
    fn parses_full_and_short_github_urls() {
        assert_eq!(
            parse_repo("https://github.com/leandroruel/rusty-cleaner-theme-tokyo-afterburn")
                .unwrap(),
            "leandroruel/rusty-cleaner-theme-tokyo-afterburn"
        );
        assert_eq!(
            parse_repo("https://github.com/user/repo.git").unwrap(),
            "user/repo"
        );
        assert_eq!(parse_repo("user/repo").unwrap(), "user/repo");
        assert!(parse_repo("not-a-url").is_err());
        assert!(parse_repo("https://example.com/x").is_err());
    }

    #[test]
    fn sanitizes_dangerous_css() {
        let dirty = ":root { --bg: red; }\n@import url('http://evil.com');\n.content { color: expression(alert(1)); }";
        let clean = sanitize_css(dirty);
        assert!(!clean.contains("@import"));
        assert!(!clean.contains("expression("));
        assert!(clean.contains("--bg: red"));
    }

    #[test]
    fn parses_the_minimal_manifest() {
        let text = r#"{"id": "test", "name": "Test", "version": "1.0", "brand": "icons/x.png"}"#;
        let manifest: ThemeManifest = serde_json::from_str(text).unwrap();
        assert_eq!(manifest.id, "test");
        assert_eq!(manifest.brand.as_deref(), Some("icons/x.png"));
    }
}
