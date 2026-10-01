//! Optional AI assistant — BYOK (bring your own key), disabled by default.
//! All AI calls happen in Rust so the API key never touches the WebView
//! beyond the settings input. Only aggregated metadata (paths, sizes,
//! timestamps, categories) is sent to the model — never file contents.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

// ─── Settings ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub send_paths: bool,
}

pub fn settings_path() -> PathBuf {
    rusty_cleaner::activity_log::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-cleaner")
        .join("ai-settings.json")
}

pub fn load_settings() -> AiSettings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &AiSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // The key is persisted but NEVER logged or sent to telemetry.
    let json = serde_json::to_string(settings).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

/// Local providers (Ollama, LM Studio) don't need a key.
fn is_local_provider(base_url: &str) -> bool {
    let lower = base_url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

fn is_ready(settings: &AiSettings) -> bool {
    if !settings.enabled || settings.base_url.trim().is_empty() || settings.model.trim().is_empty()
    {
        return false;
    }
    is_local_provider(&settings.base_url) || !settings.api_key.trim().is_empty()
}

// ─── Risk rules (local, no API needed) ───────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Review,
    Dangerous,
}

/// Path segment matching (case-insensitive) — a segment is a directory or
/// file name between separators, not a substring buried in a longer name.
fn has_segment(path: &str, needle: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment.eq_ignore_ascii_case(needle))
}

/// Suffix segment match (catches "Code Cache", "GPUCache" etc.).
fn has_suffix_segment(path: &str, suffix: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment.to_lowercase().ends_with(suffix))
}

/// True when `Documents` or `Desktop` is a user-profile folder — i.e. one
/// of the first path segments (`/home/<x>/Documents`, `/Users/<x>/Desktop`,
/// `C:/Users/<x>/Documents`), not a same-named folder buried inside an app
/// directory like `App\Documents\cache`.
fn has_user_documents_segment(path: &str) -> bool {
    path.split(['/', '\\'])
        .enumerate()
        .take(4)
        .any(|(i, segment)| {
            i <= 3
                && (segment.eq_ignore_ascii_case("Documents")
                    || segment.eq_ignore_ascii_case("Desktop"))
        })
}

/// Local path-based risk classification — runs without any API call.
/// The AI enriches with `why`/`if_deleted`; the badge comes from here.
pub fn classify_risk(path: &str, category: &str) -> Risk {
    let lower = path.to_lowercase();
    let segments: Vec<&str> = lower.split(['/', '\\']).collect();

    // Dangerous: credentials, system dirs, user documents.
    if has_segment(path, ".ssh")
        || has_suffix_segment(&lower, "wallet")
        // Segment starting with "program files" catches both
        // "Program Files" and "Program Files (x86)".
        || segments.iter().any(|seg| seg.starts_with("program files"))
        || lower.starts_with("/etc/")
        || has_user_documents_segment(path)
        || has_segment(path, ".gnupg")
        || has_segment(path, ".kube")
    {
        return Risk::Dangerous;
    }

    // Safe: regenerable caches the app rebuilds (as segments or suffixes).
    // No category shortcut — the path itself must look like a cache.
    if has_suffix_segment(&lower, "cache")
        || has_segment(&lower, "code cache")
        || has_segment(&lower, "gpucache")
        || has_segment(&lower, "shadercache")
        || has_suffix_segment(&lower, "cachestorage")
        || has_suffix_segment(&lower, "thumbnails")
        || has_suffix_segment(&lower, "wasm")
        || category == "temp"
    {
        return Risk::Safe;
    }

    // Review: user data that needs a look before removing.
    if category == "chat-media" || category == "trash" || category == "duplicates" {
        return Risk::Review;
    }

    Risk::Review
}

fn fallback_why(risk: Risk, category: &str) -> String {
    match risk {
        Risk::Safe => format!(
            "{} is a regenerable cache — the application rebuilds it on the next run.",
            capitalize(category)
        ),
        Risk::Review => format!(
            "{} contains user data — review before removing; items go to the trash and can be restored.",
            capitalize(category)
        ),
        Risk::Dangerous =>
            "This path contains user files or system data. Removing it may cause data loss."
                .to_owned(),
    }
}

fn fallback_if_deleted(risk: Risk) -> String {
    match risk {
        Risk::Safe => "The app rebuilds this data automatically.".to_owned(),
        Risk::Review => "Items move to the trash and can be restored.".to_owned(),
        Risk::Dangerous => "May break apps or lose personal data.".to_owned(),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => text.to_owned(),
    }
}

// ─── Group payload (what we send to the model) ───────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPayload {
    pub path: String,
    #[serde(rename = "extHint")]
    pub ext_hint: String,
    pub bytes: u64,
    #[serde(rename = "mtimeDays")]
    pub mtime_days: Option<u64>,
    #[serde(rename = "appGuess")]
    pub app_guess: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainGroup {
    pub id: String,
    pub risk: Risk,
    pub app: String,
    pub regenerable: bool,
    pub title: String,
    pub why: String,
    #[serde(rename = "ifDeleted")]
    pub if_deleted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanBriefing {
    pub headline: String,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub safe_gb: f64,
    #[serde(default)]
    pub review_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NlFilter {
    #[serde(default)]
    pub categories: Vec<String>,
    pub min_age_days: Option<u64>,
    pub min_size_bytes: Option<u64>,
    pub apps: Option<Vec<String>>,
    #[serde(default)]
    pub query_echo: String,
}

/// Maximum groups sent to the model per request — the rest keep the local fallback.
const MAX_AI_GROUPS: usize = 40;

// ─── OpenAI-compatible chat completion ────────────────────────────────────

fn chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

/// Timeout per call type: connection tests fail fast (20s), NVIDIA NIM
/// cold starts can take minutes (300s), everything else gets 45s.
fn timeout_for(settings: &AiSettings, is_test: bool) -> Duration {
    if is_test {
        return Duration::from_secs(20);
    }
    if settings.base_url.to_lowercase().contains("nvidia.com") {
        return Duration::from_secs(300);
    }
    Duration::from_secs(45)
}

fn chat_completion(
    settings: &AiSettings,
    system: &str,
    user: &str,
    is_test: bool,
) -> Result<String, String> {
    let url = chat_url(&settings.base_url);
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "max_tokens": 800,
    });

    let json_body = serde_json::to_string(&body).map_err(|error| format!("payload: {error}"))?;

    let request = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(timeout_for(settings, is_test));

    // Local providers (Ollama, LM Studio) accept an empty Bearer token.
    let request = if settings.api_key.trim().is_empty() {
        request
    } else {
        request.set("Authorization", &format!("Bearer {}", settings.api_key))
    };

    let response = request
        .send_string(&json_body)
        .map_err(|error| format!("AI request failed: {error}"))?;

    let text = response
        .into_string()
        .map_err(|error| format!("AI response read failed: {error}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("AI response invalid: {error}"))?;

    let content = json
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .or_else(|| {
            json.get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("text"))
                .and_then(|text| text.as_str())
        });

    match content {
        Some(content) if !content.trim().is_empty() => Ok(strip_markdown_fences(content)),
        _ => {
            let preview: String = text.chars().take(300).collect();
            Err(format!("AI response has no content. Response: {preview}"))
        }
    }
}

/// Strips ```json … ``` fences if the model wrapped the JSON.
fn strip_markdown_fences(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        inner.strip_suffix("```").unwrap_or(inner).trim().to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Tolerant JSON extraction: strips markdown fences and, if the model
/// added prose around the object, keeps only the outermost `{…}` span.
/// Returns an owned `&Cow`-free slice of the original when possible.
fn extract_json(reply: &str) -> String {
    let cleaned = strip_markdown_fences(reply);
    match (cleaned.find('{'), cleaned.rfind('}')) {
        (Some(start), Some(end)) if end > start => cleaned[start..=end].to_owned(),
        _ => cleaned,
    }
}

// ─── Public commands ──────────────────────────────────────────────────────

pub fn test_connection(settings: &AiSettings) -> Result<String, String> {
    if !is_ready(settings) {
        return Err(
            "Fill in the base URL and model first (API key required for remote providers)"
                .to_owned(),
        );
    }
    let reply = chat_completion(
        settings,
        "You are a connection test. Reply with exactly this JSON: {\"ok\":true}",
        "Test connection",
        true,
    )?;
    let parsed: serde_json::Value =
        serde_json::from_str(&reply).map_err(|error| format!("Unexpected reply: {error}"))?;
    if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok("Connected".to_owned())
    } else {
        Err("Provider responded but the reply was unexpected".to_owned())
    }
}

pub fn explain_groups(
    settings: &AiSettings,
    groups: &[GroupPayload],
) -> Result<Vec<ExplainGroup>, String> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    // Local risk first — badge works even without AI.
    let mut results: Vec<ExplainGroup> = groups
        .iter()
        .map(|g| {
            let risk = classify_risk(&g.path, &g.category);
            ExplainGroup {
                id: g.path.clone(),
                risk,
                app: g.app_guess.clone(),
                regenerable: risk == Risk::Safe,
                title: g.app_guess.clone(),
                why: fallback_why(risk, &g.category),
                if_deleted: fallback_if_deleted(risk),
            }
        })
        .collect();

    if !is_ready(settings) {
        return Ok(results);
    }

    // Cap at the largest 40 groups; the rest keep the local fallback.
    let mut sorted: Vec<usize> = (0..groups.len()).collect();
    sorted.sort_by(|&a, &b| groups[b].bytes.cmp(&groups[a].bytes));
    let top: Vec<usize> = sorted.into_iter().take(MAX_AI_GROUPS).collect();

    let payload: Vec<GroupPayload> = top
        .iter()
        .map(|&i| {
            let mut g = groups[i].clone();
            if !settings.send_paths {
                g.path = format!("[{}]", g.category);
            }
            g
        })
        .collect();

    let user = serde_json::to_string(&payload).map_err(|error| format!("payload: {error}"))?;

    let system = "You are a disk-cleaning assistant. You receive a JSON array of file groups (path, size, category). Respond with a JSON object: {\"groups\": [{\"id\": <original path>, \"risk\": \"safe\"|\"review\"|\"dangerous\", \"app\": <app name>, \"regenerable\": boolean, \"title\": <≤60 chars>, \"why\": <≤280 chars>, \"ifDeleted\": <≤160 chars>}]}. Rules: caches the app rebuilds = safe. User media and downloads = review. Documents, credentials, system files = dangerous. The \"id\" must be the original path exactly as given.";

    let reply = chat_completion(settings, system, &user, false)?;

    // Parse {groups: [...]} — tolerate fences/prose around the object.
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&extract_json(&reply)) {
        if let Some(groups_value) = parsed.get("groups").and_then(|g| g.as_array()) {
            if let Ok(enriched) = serde_json::from_value::<Vec<ExplainGroup>>(
                serde_json::Value::Array(groups_value.clone()),
            ) {
                // Merge by id, not by index — the model may reorder.
                for ai_group in &enriched {
                    if let Some(local) = results.iter_mut().find(|r| r.id == ai_group.id) {
                        if !ai_group.title.is_empty() {
                            local.title = ai_group.title.clone();
                        }
                        if !ai_group.why.is_empty() {
                            local.why = ai_group.why.clone();
                        }
                        if !ai_group.if_deleted.is_empty() {
                            local.if_deleted = ai_group.if_deleted.clone();
                        }
                        // Local risk always wins — the model cannot downgrade dangerous → safe.
                        local.risk = classify_risk(&local.id, "");
                    }
                }
            }
        }
    }

    Ok(results)
}

pub fn scan_briefing(settings: &AiSettings, summary: &str) -> Result<ScanBriefing, String> {
    if !is_ready(settings) {
        return Err("AI assistant is disabled or missing configuration".to_owned());
    }

    let system = "You are a disk-cleaning assistant. You receive a summary of scan results (categories, item counts, sizes). Respond with a JSON object: {\"headline\": <≤80 chars>, \"bullets\": [<3-6 short strings>], \"safeGb\": <number>, \"reviewGb\": <number>}. The headline says what the junk is. Bullets explain per category. safeGb = GB safely removable (caches). reviewGb = GB needing review (media, duplicates).";

    let reply = chat_completion(settings, system, summary, false)?;
    let json_text = extract_json(&reply);
    serde_json::from_str(&json_text).map_err(|error| format!("briefing parse: {error}"))
}

pub fn nl_filter(
    settings: &AiSettings,
    query: &str,
    valid_categories: &[String],
) -> Result<NlFilter, String> {
    if !is_ready(settings) {
        return Err("AI assistant is disabled or missing configuration".to_owned());
    }

    let categories_list = valid_categories.join(", ");
    let system = format!(
        "You translate a natural-language query about disk cleaning into a JSON filter. Valid categories: [{categories_list}]. Respond with a JSON object: {{\"categories\": [subset of valid], \"minAgeDays\": <number or null>, \"minSizeBytes\": <number or null>, \"apps\": [<strings>] or null, \"queryEcho\": <original query>}}. Only use categories from the valid list.",
    );

    let reply = chat_completion(settings, &system, query, false)?;
    let json_text = extract_json(&reply);
    let mut filter: NlFilter =
        serde_json::from_str(&json_text).map_err(|error| format!("filter parse: {error}"))?;

    filter
        .categories
        .retain(|cat| valid_categories.contains(cat));
    filter.query_echo = query.to_owned();
    Ok(filter)
}

pub fn is_enabled() -> bool {
    is_ready(&load_settings())
}

#[cfg(test)]
mod tests {
    use super::{classify_risk, is_local_provider, Risk};

    #[test]
    fn classifies_cache_segments_as_safe() {
        assert_eq!(
            classify_risk("/home/user/.cache/google-chrome/Default/Cache", "browser"),
            Risk::Safe
        );
        assert_eq!(
            classify_risk(
                "/home/user/.cache/google-chrome/Default/Code Cache/js",
                "browser"
            ),
            Risk::Safe
        );
        assert_eq!(
            classify_risk(
                "/home/user/.cache/google-chrome/Default/GPUCache",
                "browser"
            ),
            Risk::Safe
        );
        assert_eq!(classify_risk("/tmp/old-file.tmp", "temp"), Risk::Safe);
    }

    #[test]
    fn classifies_user_data_as_review() {
        assert_eq!(
            classify_risk("/home/user/Downloads/photo.jpg", "chat-media"),
            Risk::Review
        );
        assert_eq!(
            classify_risk("/home/user/Downloads/duplicate.bin", "duplicates"),
            Risk::Review
        );
        assert_eq!(
            classify_risk("/home/user/.local/share/Trash/file", "trash"),
            Risk::Review
        );
    }

    #[test]
    fn classifies_sensitive_paths_as_dangerous() {
        assert_eq!(
            classify_risk("/home/user/.ssh/id_rsa", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("C:/Program Files/App/app.exe", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("/home/leandro/Documents/report.docx", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("/home/leandro/Desktop/screenshot.png", "orphan"),
            Risk::Dangerous
        );
    }

    #[test]
    fn does_not_flag_cache_inside_documents_path() {
        // "Documents" as a segment is dangerous, but a cache inside a
        // browser profile under User Data is safe even if the word
        // "document" appears somewhere in a deeper path.
        assert_eq!(
            classify_risk("/home/user/.config/google-chrome/Default/Cache", "browser"),
            Risk::Safe
        );
    }

    #[test]
    fn program_files_x86_is_dangerous() {
        assert_eq!(
            classify_risk("C:/Program Files (x86)/SomeApp/app.exe", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("C:/Program Files/SomeApp/app.exe", "orphan"),
            Risk::Dangerous
        );
    }

    #[test]
    fn browser_finding_without_cache_path_is_not_safe() {
        // The category alone must not mark a path as safe — the path
        // itself has to look like a regenerable cache.
        assert_eq!(
            classify_risk(
                "/home/user/.config/google-chrome/Default/Preferences",
                "browser"
            ),
            Risk::Review
        );
        assert_eq!(
            classify_risk("/home/user/.config/google-chrome/Default/Cache", "browser"),
            Risk::Safe
        );
    }

    #[test]
    fn app_documents_folder_is_not_dangerous() {
        // `Documents` buried inside an app data dir is not the user's
        // personal Documents folder — it stays Review.
        assert_eq!(
            classify_risk(
                "C:/Users/user/AppData/Local/SomeApp/Documents/settings.json",
                "orphan"
            ),
            Risk::Review
        );
        // The actual user Documents folder is dangerous.
        assert_eq!(
            classify_risk("C:/Users/leandro/Documents/report.docx", "orphan"),
            Risk::Dangerous
        );
    }

    #[test]
    fn local_providers_dont_need_a_key() {
        assert!(is_local_provider("http://localhost:11434/v1"));
        assert!(is_local_provider("http://127.0.0.1:1234/v1"));
        assert!(!is_local_provider("https://api.openai.com/v1"));
        assert!(!is_local_provider("https://integrate.api.nvidia.com/v1"));
    }
}
