import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { captureBackendError } from "./telemetry";
import { isEnabled as aiIsEnabled, scanBriefing, type ScanBriefing } from "./ai";
import { number, t } from "./i18n";
import { formatBytes, formatCount, getElement, platformName, showToast, state, type FeatureKey, type ScanResult } from "./state";
import { renderFindings } from "./results";

const scanOrder: FeatureKey[] = ["trash", "temp", "browser", "chat-media", "orphan", "large-old", "duplicates"];

const scanButton = getElement<HTMLButtonElement>("scan-button");
const scanButtonLabel = getElement<HTMLSpanElement>("scan-button-label");
const resultsButton = getElement<HTMLButtonElement>("results-button");
const statusLabel = getElement<HTMLSpanElement>("status-label");
const scanProgress = getElement<HTMLElement>("scan-progress");
const scanProgressPath = getElement<HTMLSpanElement>("scan-progress-path");
const scanProgressTime = getElement<HTMLSpanElement>("scan-progress-time");

let elapsedTimer = 0;
let scanStartedAt = 0;
let failedScans: { path: string; date: string; error: string }[] = [];

function setScanButtonState(running: boolean): void {
  scanButton.classList.toggle("is-running", running);
  scanButtonLabel.textContent = running ? t("scan.stop") : t("scan.start");
}

function startElapsedTimer(): void {
  scanStartedAt = performance.now();
  scanProgress.hidden = false;
  scanProgressTime.textContent = "0,0s";
  elapsedTimer = window.setInterval(() => {
    scanProgressTime.textContent = `${((performance.now() - scanStartedAt) / 1000).toFixed(1).replace(".", ",")}s`;
  }, 100);
}

function stopElapsedTimer(): void {
  window.clearInterval(elapsedTimer);
  scanProgress.hidden = true;
}

export async function runScan(): Promise<void> {
  if (!isTauri()) {
    showToast(t("scan.desktopOnly"));
    return;
  }

  state.scanning = true;
  state.scanCancelled = false;
  setScanButtonState(true);
  resultsButton.disabled = true;
  getElement("scan-visual").classList.add("is-scanning");
  getElement("system-status").classList.add("is-busy");
  state.findings = [];
  state.selectedPaths.clear();
  state.activeFilter = "all";
  getElement("scan-visual").classList.remove("is-done");
  resultsButton.classList.remove("is-ready");
  getElement("total-size").textContent = "0 B";
  getElement<HTMLSelectElement>("feature-filter").value = "all";
  failedScans = [];
  renderFailedScans();
  renderFindings();
  startElapsedTimer();

  const started = performance.now();
  let platform = "unknown";
  let failed = 0;
  let elapsed = 0;

  try {
    for (const [index, feature] of scanOrder.entries()) {
      if (state.scanCancelled) break;
      state.currentScanFeature = feature;
      statusLabel.textContent = `${t("scan.scanning")} ${index + 1}/${scanOrder.length}: ${t(`feature.${feature}`)}`;
      getElement("scan-caption").textContent = `${t("scan.scanning").toLowerCase()} ${t(`feature.${feature}`).toLowerCase()}...`;
      try {
        const result = await invoke<ScanResult>("scan_candidates", { feature });
        platform = result.platform;
        state.findings.push(...result.findings);
        renderSummary();
      } catch (error) {
        failed += 1;
        captureBackendError(`scan:${feature}`, error);
        failedScans.push({
          path: t(`feature.${feature}`),
          date: new Date().toLocaleString(),
          error: String(error),
        });
        renderFailedScans();
      }
    }
    state.currentScanFeature = null;

    elapsed = (performance.now() - started) / 1000;
    resultsButton.disabled = state.findings.length === 0;
    statusLabel.textContent = state.scanCancelled
      ? t("app.cancelled")
      : `${platformName(platform)} · ${t("app.done")}`;
    getElement("system-status").classList.remove("is-busy");
    getElement("scan-caption").textContent = state.scanCancelled
      ? t("scan.partial", { count: number(state.findings.length) })
      : t("scan.summary", { count: number(state.findings.length), time: elapsed.toFixed(1).replace(".", ",") });
    if (failed > 0) {
      showToast(failed === 1 ? t("scan.categoryFailed") : t("scan.categoriesFailed", { count: failed }));
    }
  } finally {
    state.scanning = false;
    stopElapsedTimer();
    setScanButtonState(false);
    scanButtonLabel.textContent = t("scan.again");
    getElement("scan-visual").classList.remove("is-scanning");
  }

  if (state.scanCancelled) return;
  revealScanResult();
  void notifyScanComplete();
  void generateBriefing(elapsed);
}

/// Briefings cached by the exact summary sent to the model — a repeated
/// scan with identical results skips the LLM call entirely (no tokens).
const briefingCache = new Map<string, ScanBriefing>();
const BRIEFING_CACHE_MAX = 8;
/// Bumped on every new scan and on cancel — stale replies are discarded.
let briefingReqId = 0;

function renderBriefing(briefing: ScanBriefing): void {
  const card = getElement("ai-briefing");
  card.classList.remove("is-loading");
  getElement("ai-briefing-title").textContent = briefing.headline;
  getElement("ai-briefing-body").textContent = briefing.bullets.join(" ");
  getElement("ai-briefing-error").hidden = true;
}

/// Shows an AI-generated scan briefing below the ring when the assistant
/// is enabled. Silently hidden otherwise.
async function generateBriefing(elapsed: number): Promise<void> {
  const reqId = ++briefingReqId;
  const card = getElement("ai-briefing");
  if (!(await aiIsEnabled())) {
    card.hidden = true;
    return;
  }
  const briefingBody = getElement("ai-briefing-body");
  const briefingTitle = getElement("ai-briefing-title");
  const counts = new Map<string, { count: number; size: number }>();
  for (const item of state.findings) {
    const current = counts.get(item.feature) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    counts.set(item.feature, current);
  }
  const summary = [...counts.entries()]
    .map(([feature, data]) => `${feature}: ${data.count} items, ${(data.size / 1_048_576).toFixed(0)} MiB`)
    .join("; ");

  const showLocalBriefing = (): void => {
    const total = state.findings.reduce((sum, item) => sum + item.size, 0);
    const totalMiB = (total / 1_048_576).toFixed(0);
    card.classList.remove("is-loading");
    getElement("ai-briefing-error").hidden = true;
    getElement("ai-briefing-title").textContent = t("scan.summary", {
      count: formatCount(state.findings.length),
      time: elapsed.toFixed(1).replace(".", ","),
    });
    briefingBody.textContent = `${totalMiB} MiB across ${counts.size} categories — ${summary}`;
  };

  // Surface the AI failure reason in the card — a fallback with no
  // explanation looks like the feature is just broken.
  const showError = (message: string): void => {
    const errorEl = getElement("ai-briefing-error");
    errorEl.hidden = false;
    errorEl.textContent = message.slice(0, 180);
  };

  // Cache hit — identical scan results, reuse the previous reply.
  const cached = briefingCache.get(summary);
  if (cached) {
    card.hidden = false;
    renderBriefing(cached);
    return;
  }

  // Loading state — the LLM takes a few seconds; the spinner plus the
  // cancel button let the user bail out instead of waiting.
  briefingTitle.textContent = t("scan.briefingPreparing");
  briefingBody.textContent = "";
  getElement("ai-briefing-error").hidden = true;
  card.classList.add("is-loading");
  card.hidden = false;

  try {
    const briefing = await scanBriefing(summary);
    // Discard stale replies — a newer scan started or the user cancelled.
    if (reqId !== briefingReqId) return;
    // Reject meaningless responses — the model may return empty strings
    // or trivial bullets; the local fallback is better than a blank card.
    const hasContent = briefing.headline.trim().length >= 8
      && briefing.bullets.some((bullet) => bullet.trim().length >= 10);
    if (!hasContent) {
      console.warn("[briefing] model reply lacked content, using local summary:", briefing);
      if (reqId !== briefingReqId) return;
      showLocalBriefing();
      showError(t("scan.briefingEmpty"));
      return;
    }
    briefingCache.set(summary, briefing);
    if (briefingCache.size > BRIEFING_CACHE_MAX) {
      const oldest = briefingCache.keys().next().value;
      if (oldest !== undefined) briefingCache.delete(oldest);
    }
    renderBriefing(briefing);
  } catch (error) {
    // Visible in devtools AND in the card — silent failures made briefing
    // issues undiagnosable.
    console.warn("[briefing] AI call failed, using local summary:", error);
    if (reqId !== briefingReqId) return;
    showLocalBriefing();
    const message = error instanceof Error ? error.message : String(error);
    showError(message);
  }
}

/// Shows a desktop notification when the scan completes while the window
/// is minimized — the user knows results are ready without switching back.
async function notifyScanComplete(): Promise<void> {
  if (!isTauri() || state.scanCancelled) return;
  try {
    const window = getCurrentWindow();
    if (!(await window.isMinimized())) return;

    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (!granted) return;

    const totalSize = state.findings.reduce((total, item) => total + item.size, 0);
    sendNotification({
      title: t("notification.scanTitle"),
      body: t("notification.scanBody", {
        count: formatCount(state.findings.length),
        size: formatBytes(totalSize),
      }),
    });
  } catch {
    // Notifications unavailable (e.g. WSL without a notification daemon).
  }
}

/// The completion moment: the ring pulses, the hero number counts up from
/// zero and the results button glows once — before this the scan ended with
/// everything snapping into place at once.
function revealScanResult(): void {
  const totalSize = state.findings.reduce((total, item) => total + item.size, 0);
  if (state.findings.length > 0) {
    getElement("scan-visual").classList.add("is-done");
    if (!resultsButton.disabled) resultsButton.classList.add("is-ready");
  }
  const element = getElement("total-size");
  if (
    totalSize <= 0 ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  ) {
    element.textContent = formatBytes(totalSize);
    return;
  }
  const duration = 900;
  const started = performance.now();
  const tick = (now: number) => {
    const progress = Math.min(1, (now - started) / duration);
    const eased = 1 - (1 - progress) ** 3;
    element.textContent = formatBytes(Math.round(totalSize * eased));
    if (progress < 1) window.requestAnimationFrame(tick);
  };
  window.requestAnimationFrame(tick);
}

function renderSummary(): void {
  const counts = new Map<FeatureKey, { count: number; size: number }>();
  const messengerCounts = { telegram: 0, discord: 0, whatsapp: 0, signal: 0, slack: 0, element: 0 };
  for (const item of state.findings) {
    const current = counts.get(item.feature) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    counts.set(item.feature, current);
    if (item.feature === "chat-media") {
      const path = item.path.toLowerCase();
      if (path.includes("telegram")) messengerCounts.telegram += 1;
      if (path.includes("discord")) messengerCounts.discord += 1;
      if (path.includes("whatsapp") || path.includes("whatsdesk") || path.includes("zapzap")) messengerCounts.whatsapp += 1;
      if (path.includes("signal")) messengerCounts.signal += 1;
      if (path.includes("slack")) messengerCounts.slack += 1;
      if (path.includes("element")) messengerCounts.element += 1;
    }
  }

  const browser = counts.get("browser") ?? { count: 0, size: 0 };
  getElement("browser-size").textContent = formatBytes(browser.size);
  getElement("browser-detail").textContent = t("card.candidates", { count: formatCount(browser.count) });
  getElement<HTMLButtonElement>("clean-browser-button").disabled = browser.count === 0;
  renderBrowserChips();
  const trash = counts.get("trash") ?? { count: 0, size: 0 };
  renderTrashCard(trash.count, trash.size);
  const chat = counts.get("chat-media") ?? { count: 0, size: 0 };
  getElement("messenger-size").textContent = formatBytes(chat.size);
  getElement<HTMLButtonElement>("clean-messenger-button").disabled = chat.count === 0;
  getElement("telegram-count").textContent = t("card.items", { count: formatCount(messengerCounts.telegram) });
  getElement("discord-count").textContent = t("card.items", { count: formatCount(messengerCounts.discord) });
  getElement("whatsapp-count").textContent = t("card.items", { count: formatCount(messengerCounts.whatsapp) });
  getElement("signal-count").textContent = t("card.items", { count: formatCount(messengerCounts.signal) });
  getElement("slack-count").textContent = t("card.items", { count: formatCount(messengerCounts.slack) });
  getElement("element-count").textContent = t("card.items", { count: formatCount(messengerCounts.element) });
  renderMediaTypes();
}

function renderBrowserChips(): void {
  const chips = getElement("browser-chips");
  chips.replaceChildren();
  const byBrowser = new Map<string, { count: number; size: number }>();
  for (const item of state.findings.filter((entry) => entry.feature === "browser")) {
    const name = browserName(item.path);
    const current = byBrowser.get(name) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    byBrowser.set(name, current);
  }
  for (const [name, data] of [...byBrowser.entries()].sort((a, b) => b[1].size - a[1].size)) {
    const chip = document.createElement("span");
    chip.className = "browser-chip";
    chip.innerHTML = `${browserIcon(name)}<span></span>`;
    chip.querySelector("span")!.textContent = `${name} · ${formatBytes(data.size)}`;
    chips.append(chip);
  }
}

function renderTrashCard(count: number, size: number): void {
  getElement("trash-detail").textContent = t("card.trashItems", { count: formatCount(count), size: formatBytes(size) });
  getElement<HTMLButtonElement>("empty-trash-button").disabled = count === 0;
}

type MediaKind = "video" | "image" | "audio" | "other";

function mediaKind(path: string): MediaKind {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (["mp4", "mov", "mkv", "webm", "avi"].includes(ext)) return "video";
  if (["jpg", "jpeg", "png", "webp", "gif", "bmp"].includes(ext)) return "image";
  if (["opus", "ogg", "mp3", "m4a", "wav", "aac"].includes(ext)) return "audio";
  return "other";
}

function renderMediaTypes(): void {
  const container = getElement("media-types");
  container.replaceChildren();
  const totals = new Map<MediaKind, { count: number; size: number }>();
  for (const item of state.findings.filter((entry) => entry.feature === "chat-media")) {
    const kind = mediaKind(item.path);
    const current = totals.get(kind) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    totals.set(kind, current);
  }
  if (totals.size === 0) return;
  const order: MediaKind[] = ["video", "image", "audio", "other"];
  for (const kind of order) {
    const data = totals.get(kind);
    if (!data) continue;
    const chip = document.createElement("span");
    chip.className = `media-chip media-${kind}`;
    chip.textContent = `${t(`media.${kind}`)} · ${formatBytes(data.size)}`;
    container.append(chip);
  }
}

function browserName(path: string): string {
  const lower = path.toLowerCase();
  if (lower.includes("brave")) return "Brave";
  if (lower.includes("edge")) return "Edge";
  if (lower.includes("chromium")) return "Chromium";
  if (lower.includes("chrome")) return "Chrome";
  if (lower.includes("firefox") || lower.includes("mozilla")) return "Firefox";
  if (lower.includes("safari")) return "Safari";
  return "Other";
}

function browserIcon(name: string): string {
  const icons: Record<string, string> = {
    Chrome: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="#ffc233" stroke-width="2"/><circle cx="12" cy="12" r="3.4" fill="#33e0ff"/><path d="M12 3a9 9 0 0 1 7.8 4.5H12" fill="none" stroke="#ff4d6a" stroke-width="2"/><path d="M4.2 16.5 9 12" stroke="#33ffb0" stroke-width="2"/></svg>`,
    Chromium: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="#6b9fff" stroke-width="2"/><circle cx="12" cy="12" r="3.4" fill="#6b9fff"/></svg>`,
    Firefox: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="#ff9f43" stroke-width="2"/><path d="M12 3c3 2 5 5 5 9a5 5 0 0 1-10 0c0-2 1-4 2-5" fill="none" stroke="#ff9f43" stroke-width="2" stroke-linecap="round"/></svg>`,
    Edge: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="#33e0ff" stroke-width="2"/><path d="M5 14c2 1 5 1 7-1s3-5 1-7" fill="none" stroke="#33e0ff" stroke-width="2" stroke-linecap="round"/></svg>`,
    Brave: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l7 4v5c0 4-3 7-7 9-4-2-7-5-7-9V7l7-4Z" fill="none" stroke="#ff3df0" stroke-width="2" stroke-linejoin="round"/></svg>`,
    Safari: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="#9d4dff" stroke-width="2"/><path d="m15 9-2 5-4 1 2-5 4-1Z" fill="#9d4dff"/></svg>`,
  };
  return icons[name] ?? `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2"/></svg>`;
}

function renderFailedScans(): void {
  const section = getElement("failed-scans");
  const summary = getElement("failed-summary");
  const body = getElement("failed-body");
  if (failedScans.length === 0) {
    section.hidden = true;
    return;
  }
  section.hidden = false;
  summary.textContent = t("failed.title", { count: formatCount(failedScans.length) });
  body.replaceChildren();
  for (const item of failedScans) {
    const row = document.createElement("tr");
    const pathCell = document.createElement("td");
    pathCell.textContent = item.path;
    const dateCell = document.createElement("td");
    dateCell.className = "numeric-cell muted-cell";
    dateCell.textContent = item.date;
    const errorCell = document.createElement("td");
    errorCell.className = "muted-cell";
    errorCell.textContent = item.error;
    row.append(pathCell, dateCell, errorCell);
    body.append(row);
  }
}

export function initScan(): void {
  getElement("ai-briefing-cancel").addEventListener("click", () => {
    briefingReqId++; // invalidate the in-flight request
    const card = getElement("ai-briefing");
    card.hidden = true;
    card.classList.remove("is-loading");
  });
  scanButton.addEventListener("click", () => {
    if (state.scanning) {
      state.scanCancelled = true;
    } else {
      void runScan();
    }
  });
  getElement("failed-toggle").addEventListener("click", () => {
    const wrap = getElement("failed-table-wrap");
    const toggle = getElement("failed-toggle");
    wrap.hidden = !wrap.hidden;
    toggle.setAttribute("aria-expanded", String(!wrap.hidden));
  });

  if (isTauri()) {
    void listen<string>("scan-progress", (event) => {
      scanProgressPath.textContent = event.payload;
      scanProgressPath.title = event.payload;
      if (state.scanning && state.currentScanFeature === "browser") {
        const detail = getElement("browser-detail");
        detail.textContent = t("card.scanningPath", { path: event.payload });
        detail.title = event.payload;
      }
    });
  }
}
