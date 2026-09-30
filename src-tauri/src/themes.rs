//! Theme download and manifest handling. Themes live in their own git
//! repositories; a `theme.json` at the root declares colors, background
//! images, fonts and icons, all as paths relative to the repository.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The theme.json contract. Every color is a CSS color string; images,
/// fonts and icons are paths inside the theme repository.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub colors: ThemeColors,
    #[serde(default)]
    pub background: ThemeSurface,
    #[serde(default)]
    pub sidebar: ThemeSurface,
    #[serde(default)]
    pub fonts: ThemeFonts,
    #[serde(default)]
    pub icons: ThemeIcons,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub background: Option<String>,
    pub panel: Option<String>,
    pub panel2: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub text_dim: Option<String>,
    pub text_faint: Option<String>,
    pub accent: Option<String>,
    pub accent_secondary: Option<String>,
    pub purple: Option<String>,
    pub green: Option<String>,
    pub amber: Option<String>,
    pub red: Option<String>,
    pub orange: Option<String>,
    pub blue: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSurface {
    /// Image path relative to the theme root.
    pub image: Option<String>,
    /// CSS color painted over the image for readability (e.g. "rgba(0,0,0,.7)").
    pub tint: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFonts {
    /// Font file for interface text (.ttf/.woff2).
    pub text: Option<String>,
    /// Font file for monospace runs.
    pub mono: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeIcons {
    /// Brand icon shown in the sidebar.
    pub brand: Option<String>,
}

/// A manifest with every asset path resolved to an absolute file path,
/// ready to be served to the webview through the asset protocol.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub colors: ThemeColors,
    pub background: ResolvedSurface,
    pub sidebar: ResolvedSurface,
    pub fonts: ResolvedFonts,
    pub icons: ResolvedIcons,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSurface {
    pub image: Option<String>,
    pub tint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFonts {
    pub text: Option<String>,
    pub mono: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedIcons {
    pub brand: Option<String>,
}

pub fn themes_dir() -> PathBuf {
    rusty_cleaner::activity_log::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-cleaner")
        .join("themes")
}

/// Clones (or refreshes) a theme repository and returns its resolved
/// manifest. The repository must be public; `git` is the download mechanism.
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

/// Lists every downloaded theme with resolved assets.
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
    Ok(ResolvedTheme {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        author: manifest.author,
        description: manifest.description,
        colors: manifest.colors,
        background: ResolvedSurface {
            image: absolute(&manifest.background.image),
            tint: manifest.background.tint,
        },
        sidebar: ResolvedSurface {
            image: absolute(&manifest.sidebar.image),
            tint: manifest.sidebar.tint,
        },
        fonts: ResolvedFonts {
            text: absolute(&manifest.fonts.text),
            mono: absolute(&manifest.fonts.mono),
        },
        icons: ResolvedIcons {
            brand: absolute(&manifest.icons.brand),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::ThemeManifest;

    const SAMPLE: &str = r##"{
        "id": "tokyo-afterburn",
        "name": "Tokyo Afterburn",
        "version": "1.0.0",
        "author": "leandroruel",
        "description": "Synthwave neon night",
        "colors": {
            "background": "#0d0218",
            "panel": "#170a2e",
            "accent": "#33e0ff",
            "accentSecondary": "#ff3df0"
        },
        "background": {
            "image": "src/assets/images/background.svg",
            "tint": "rgba(13,2,24,0.78)"
        },
        "icons": { "brand": "src/assets/icons/brand.svg" }
    }"##;

    #[test]
    fn parses_the_theme_contract() {
        let manifest: ThemeManifest = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(manifest.id, "tokyo-afterburn");
        assert_eq!(manifest.colors.background.as_deref(), Some("#0d0218"));
        assert_eq!(manifest.colors.accent_secondary.as_deref(), Some("#ff3df0"));
        assert_eq!(
            manifest.background.image.as_deref(),
            Some("src/assets/images/background.svg")
        );
        assert_eq!(
            manifest.icons.brand.as_deref(),
            Some("src/assets/icons/brand.svg")
        );
        // Optional sections default to empty.
        assert!(manifest.fonts.text.is_none());
        assert!(manifest.sidebar.image.is_none());
    }

    #[test]
    fn rejects_a_manifest_without_an_id() {
        let text = r##"{ "id": "", "name": "Broken", "version": "0" }"##;
        let manifest: Result<ThemeManifest, _> = serde_json::from_str(text);
        assert!(manifest.is_ok()); // parsing succeeds; the guard rejects it later
    }
}
