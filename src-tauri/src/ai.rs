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
    std::fs::write(path, serde_json::to_string(settings).unwrap())
        .map_err(|error| error.to_string())
}

fn is_ready(settings: &AiSettings) -> bool {
    settings.enabled && !settings.api_key.trim().is_empty() && !settings.base_url.trim().is_empty()
}

// ─── Risk rules (local, no API needed) ───────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Review,
    Dangerous,
}

/// Local path-based risk classification — runs without any API call.
/// The AI enriches with `why`/`if_deleted`; the badge comes from here.
pub fn classify_risk(path: &str, category: &str) -> Risk {
    let lower = path.to_lowercase();

    // Dangerous: user documents, system dirs, credentials.
    if lower.contains(".ssh")
        || lower.contains("wallet")
        || lower.contains("program files")
        || lower.contains("/etc/")
        || lower.contains("save")
        || lower.contains("document")
        || lower.contains("desktop")
    {
        return Risk::Dangerous;
    }

    // Safe: regenerable caches the app rebuilds.
    if lower.contains("cache")
        || lower.contains("code cache")
        || lower.contains("gpucache")
        || lower.contains("shadercache")
        || lower.contains("thumbnail")
        || lower.contains("wasm")
        || category == "browser"
        || category == "temp"
    {
        return Risk::Safe;
    }

    // Review: media and data the user chose to keep.
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
    pub bullets: Vec<String>,
    pub safe_gb: f64,
    pub review_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NlFilter {
    pub categories: Vec<String>,
    pub min_age_days: Option<u64>,
    pub min_size_bytes: Option<u64>,
    pub apps: Option<Vec<String>>,
    pub query_echo: String,
}

// ─── OpenAI-compatible chat completion ────────────────────────────────────

fn chat_completion(settings: &AiSettings, system: &str, user: &str) -> Result<String, String> {
    let url = format!(
        "{}/chat/completions",
        settings.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "response_format": {"type": "json_object"},
        "temperature": 0.2,
        "max_tokens": 800,
    });

    let json_body = serde_json::to_string(&body).map_err(|error| format!("payload: {error}"))?;
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", settings.api_key))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(300))
        .send_string(&json_body)
        .map_err(|error| format!("AI request failed: {error}"))?;

    let text = response
        .into_string()
        .map_err(|error| format!("AI response read failed: {error}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("AI response invalid: {error}"))?;

    json.get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(|content| content.to_owned())
        .ok_or_else(|| "AI response missing choices[0].message.content".to_owned())
}

// ─── Public commands ──────────────────────────────────────────────────────

pub fn test_connection(settings: &AiSettings) -> Result<String, String> {
    if !is_ready(settings) {
        return Err("Fill in the base URL, model and API key first".to_owned());
    }
    let reply = chat_completion(
        settings,
        "You are a connection test. Reply with exactly: {\"ok\":true}",
        "Test",
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

    // If AI is not ready, return the local fallback.
    if !is_ready(settings) {
        return Ok(results);
    }

    // Build the payload — strip paths if the user opted out.
    let payload: Vec<GroupPayload> = groups
        .iter()
        .map(|g| {
            let mut g = g.clone();
            if !settings.send_paths {
                g.path = format!("[{}]", g.category);
            }
            g
        })
        .collect();

    let user = serde_json::to_string(&payload).map_err(|error| format!("payload: {error}"))?;

    let system = "You are a disk-cleaning assistant. You receive a JSON array of file groups (path, size, category). For each, respond with a JSON object: {\"id\": <original path>, \"risk\": \"safe\"|\"review\"|\"dangerous\", \"app\": <app name>, \"regenerable\": boolean, \"title\": ≤60 chars, \"why\": ≤280 chars, \"ifDeleted\": ≤160 chars}. Respond as a JSON array of these objects. Rules: caches the app rebuilds = safe. User media and downloads = review. Documents, credentials, system files = dangerous.";

    let reply = chat_completion(settings, system, &user)?;

    // Try to parse the model's response; on failure, keep the local fallback.
    if let Ok(enriched) = serde_json::from_str::<Vec<ExplainGroup>>(&reply) {
        // Merge: keep local risk if AI is wrong, keep AI's why/title.
        for (local, ai) in results.iter_mut().zip(enriched.iter()) {
            if !ai.why.is_empty() {
                local.why = ai.why.clone();
            }
            if !ai.title.is_empty() {
                local.title = ai.title.clone();
            }
            if !ai.if_deleted.is_empty() {
                local.if_deleted = ai.if_deleted.clone();
            }
        }
    }

    Ok(results)
}

pub fn scan_briefing(settings: &AiSettings, summary: &str) -> Result<ScanBriefing, String> {
    if !is_ready(settings) {
        return Err("AI assistant is disabled or missing configuration".to_owned());
    }

    let system = "You are a disk-cleaning assistant. You receive a summary of a scan (categories, item counts, sizes). Respond with JSON: {\"headline\": ≤80 chars, \"bullets\": 3-6 short strings, \"safeGb\": number, \"reviewGb\": number}. The headline says what the junk is. Bullets explain per category. safeGb = how many GB are safely removable (caches). reviewGb = how many GB need review (media, duplicates).";

    let reply = chat_completion(settings, system, summary)?;
    serde_json::from_str(&reply).map_err(|error| format!("briefing parse: {error}"))
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
        "You translate a natural-language query about disk cleaning into a JSON filter. Valid categories: [{categories_list}]. Respond with JSON: {{\"categories\": [subset of the valid categories], \"minAgeDays\": number or null, \"minSizeBytes\": number or null, \"apps\": [strings] or null, \"queryEcho\": <the original query>}}. Only use categories from the valid list.",
    );

    let reply = chat_completion(settings, &system, query)?;
    let mut filter: NlFilter =
        serde_json::from_str(&reply).map_err(|error| format!("filter parse: {error}"))?;

    // Clamp to valid categories.
    filter
        .categories
        .retain(|cat| valid_categories.contains(cat));
    filter.query_echo = query.to_owned();
    Ok(filter)
}

pub fn is_enabled() -> bool {
    is_ready(&load_settings())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{classify_risk, fallback_if_deleted, fallback_why, Risk};

    #[test]
    fn classifies_cache_paths_as_safe() {
        assert_eq!(
            classify_risk("/home/user/.cache/google-chrome/Default/Cache", "browser"),
            Risk::Safe
        );
        assert_eq!(
            classify_risk(
                "/home/user/.cache/mozilla/firefox/profile/cache2",
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
    }

    #[test]
    fn classifies_sensitive_paths_as_dangerous() {
        assert_eq!(
            classify_risk("/home/user/.ssh/id_rsa", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("C:/Program Files/App", "orphan"),
            Risk::Dangerous
        );
        assert_eq!(
            classify_risk("/home/user/Documents/report.docx", "orphan"),
            Risk::Dangerous
        );
    }

    #[test]
    fn fallback_texts_match_risk_level() {
        assert!(fallback_why(Risk::Safe, "browser").contains("regenerable"));
        assert!(fallback_why(Risk::Review, "chat-media").contains("review"));
        assert!(fallback_why(Risk::Dangerous, "orphan").contains("data loss"));
        assert!(fallback_if_deleted(Risk::Safe).contains("rebuilds"));
    }
}
