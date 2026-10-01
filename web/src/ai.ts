import { invoke } from "@tauri-apps/api/core";

/// AI assistant types matching the Rust module (src-tauri/src/ai.rs).
export type Risk = "safe" | "review" | "dangerous";

export type AiSettings = {
  enabled: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
  sendPaths: boolean;
};

export type ExplainGroup = {
  id: string;
  risk: Risk;
  app: string;
  regenerable: boolean;
  title: string;
  why: string;
  ifDeleted: string;
};

export type ScanBriefing = {
  headline: string;
  bullets: string[];
  safeGb: number;
  reviewGb: number;
};

export type NlFilterResult = {
  categories: string[];
  minAgeDays: number | null;
  minSizeBytes: number | null;
  apps: string[] | null;
  queryEcho: string;
};

type GroupInput = {
  path: string;
  extHint: string;
  bytes: number;
  mtimeDays: number | null;
  appGuess: string;
  category: string;
};

export async function getSettings(): Promise<AiSettings> {
  return invoke<AiSettings>("ai_get_settings");
}

export async function saveSettings(settings: AiSettings): Promise<void> {
  await invoke("ai_save_settings", { settings });
}

export async function isEnabled(): Promise<boolean> {
  return invoke<boolean>("ai_is_enabled");
}

export async function explainGroups(groups: GroupInput[]): Promise<ExplainGroup[]> {
  return invoke<ExplainGroup[]>("ai_explain_groups", { groups });
}

export async function scanBriefing(summary: string): Promise<ScanBriefing> {
  return invoke<ScanBriefing>("ai_scan_briefing", { summary });
}

export async function nlFilter(query: string, categories: string[]): Promise<NlFilterResult> {
  return invoke<NlFilterResult>("ai_nl_filter", { query, categories });
}

export async function testConnection(): Promise<string> {
  return invoke<string>("ai_test_connection");
}

/// Feature keys that can be filtered by the NL query.
export const aiCategories = ["browser", "chat-media", "trash", "orphan", "duplicates", "large-old"];
