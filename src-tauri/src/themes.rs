use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Minimal theme manifest — identity, asset paths and nothing else. All
/// styling lives in the theme's `theme.css`, which is injected verbatim into
/// the webview. The CSS IS the theme.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Main window background image (tinted by the theme.css itself).
    #[serde(default)]
    pub background: Option<String>,
    /// Sidebar background image.
    #[serde(default)]
    pub sidebar: Option<String>,
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
    /// Absolute path to the theme directory, used by the frontend to
    /// resolve {{THEME_ROOT}} placeholders in the CSS.
    pub root: String,
    pub background: Option<String>,
    pub sidebar: Option<String>,
    pub brand: Option<String>,
    /// Font families declared by @font-face in the theme.css.
    pub fonts: Vec<ResolvedFont>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFont {
    /// The CSS font-family name to use.
    pub family: String,
    /// Absolute path to the font file.
    pub path: String,
}

pub fn themes_dir() -> PathBuf {
    rusty_cleaner::activity_log::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-cleaner")
        .join("themes")
}

/// Clones (or refreshes) a theme repository and returns the resolved theme
/// with the theme.css content. The repository must be public.
pub fn download(repo: &str) -> Result<ResolvedTheme, String> {
    let root = themes_dir();
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;

    // Clone into a temporary directory first: the final folder name comes
    // from the manifest id, which is only known after parsing.
    let temp = root.join(format!(".staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1"])
        .arg(repo)
        .arg(&temp)
        .output()
        .map_err(|error| format!("failed to launch git: {error}"))?;
    if !status.status.success() {
        let message = String::from_utf8_lossy(&status.stderr).trim().to_owned();
        return Err(format!("git clone failed: {message}"));
    }

    let manifest = read_manifest(&temp.join("theme.json"))?;
    if manifest.id.trim().is_empty()
        || manifest.id.contains(['/', '\\'])
        || manifest.id.contains("..")
    {
        return Err(format!("invalid theme id: {}", manifest.id));
    }
    let target = root.join(&manifest.id);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&temp, &target).map_err(|error| error.to_string())?;
    resolve(&target, manifest)
}

/// Lists every downloaded theme with resolved assets and CSS content.
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

fn read_manifest(path: &Path) -> Result<ThemeManifest, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("theme.json: {error}"))?;
    let manifest: ThemeManifest =
        serde_json::from_str(&text).map_err(|error| format!("theme.json: {error}"))?;
    Ok(manifest)
}

/// Strips CSS constructs that could load external resources or execute code.
/// The CSS comes from a git repository the user deliberately downloaded, but
/// defense in depth still applies inside the WebView.
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

    // Resolve font files declared in the manifest.
    let fonts = manifest
        .fonts
        .iter()
        .filter_map(|path| {
            let full = root.join(path);
            if !full.is_file() {
                return None;
            }
            // Derive a font-family name from the file stem.
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

    Ok(ResolvedTheme {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        author: manifest.author,
        description: manifest.description,
        css,
        root: root.to_string_lossy().into_owned(),
        background: absolute(&manifest.background),
        sidebar: absolute(&manifest.sidebar),
        brand: absolute(&manifest.brand),
        fonts,
    })
}

#[cfg(test)]
mod tests {
    use super::{sanitize_css, ThemeManifest};

    const MANIFEST: &str = r##"{
        "id": "tokyo-afterburn",
        "name": "Tokyo Afterburn",
        "version": "1.0.0",
        "author": "leandroruel",
        "background": "src/assets/images/background.png",
        "brand": "src/assets/icons/brand.png"
    }"##;

    #[test]
    fn parses_the_minimal_manifest() {
        let manifest: ThemeManifest = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest.id, "tokyo-afterburn");
        assert_eq!(
            manifest.background.as_deref(),
            Some("src/assets/images/background.png")
        );
        assert_eq!(
            manifest.brand.as_deref(),
            Some("src/assets/icons/brand.png")
        );
        assert!(manifest.sidebar.is_none());
        assert!(manifest.fonts.is_empty());
    }

    #[test]
    fn sanitizes_dangerous_css() {
        let dirty = ":root { --bg: red; }\n@import url('http://evil.com');\n.content { color: expression(alert(1)); }";
        let clean = sanitize_css(dirty);
        assert!(!clean.contains("@import"));
        assert!(!clean.contains("expression("));
        assert!(clean.contains("--bg: red"));
    }
}
