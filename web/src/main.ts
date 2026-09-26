import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getLocale, loadLocale, number, setLocale, t, type Locale } from "./i18n";
import "@fontsource/manrope/400.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import "@fontsource/manrope/800.css";
import "@fontsource/dm-mono/400.css";
import "@fontsource/dm-mono/500.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/700.css";
import "./styles.css";

type FeatureKey = "orphan" | "temp" | "chat-media" | "trash" | "browser" | "duplicates" | "large-old";

type Finding = {
  feature: FeatureKey;
  name: string;
  path: string;
  size: number;
  ageDays: number | null;
};

type ScanResult = {
  findings: Finding[];
  elapsedMs: number;
  platform: string;
};

type TrashResult = {
  trashed: string[];
  failed: { path: string; error: string }[];
};

type SystemMetrics = {
  cpuPercent: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
};

type EmptyTrashResult = {
  removed: number;
  failed: number;
};

const featureKeys: FeatureKey[] = ["orphan", "temp", "chat-media", "trash", "browser", "duplicates", "large-old"];

function featureLabels(): Record<FeatureKey, string> {
  return Object.fromEntries(featureKeys.map((key) => [key, t(`feature.${key}`)])) as Record<FeatureKey, string>;
}

const featureColors: Record<FeatureKey, string> = {
  orphan: "var(--purple)",
  temp: "var(--cyan)",
  "chat-media": "var(--pink)",
  trash: "var(--red)",
  browser: "var(--amber)",
  duplicates: "var(--green)",
  "large-old": "var(--blue)",
};

let findings: Finding[] = [];
let activeFilter = "all";
let searchQuery = "";
let sortMode = "size-desc";
let toastTimer = 0;
const selectedPaths = new Set<string>();

const scanButton = getElement<HTMLButtonElement>("scan-button");
const resultsButton = getElement<HTMLButtonElement>("results-button");
const filterSelect = getElement<HTMLSelectElement>("feature-filter");
const resultsPanel = getElement<HTMLElement>("results-panel");
const closeResultsButton = getElement<HTMLButtonElement>("close-results");
const statusLabel = getElement<HTMLSpanElement>("status-label");
const toast = getElement<HTMLDivElement>("toast");
const selectAll = getElement<HTMLInputElement>("select-all");
const selectionBar = getElement<HTMLElement>("selection-bar");
const selectionSummary = getElement<HTMLSpanElement>("selection-summary");
const trashButton = getElement<HTMLButtonElement>("trash-button");
const confirmOverlay = getElement<HTMLElement>("confirm-overlay");
const confirmText = getElement<HTMLParagraphElement>("confirm-text");
const confirmCancel = getElement<HTMLButtonElement>("confirm-cancel");
const confirmAccept = getElement<HTMLButtonElement>("confirm-accept");
const emptyTrashButton = getElement<HTMLButtonElement>("empty-trash-button");
const searchInput = getElement<HTMLInputElement>("search-input");
const sortSelect = getElement<HTMLSelectElement>("sort-select");
const selectCategoryButton = getElement<HTMLButtonElement>("select-category-button");
const settingsButton = getElement<HTMLButtonElement>("settings-button");
const settingsOverlay = getElement<HTMLElement>("settings-overlay");
const settingsClose = getElement<HTMLButtonElement>("settings-close");
const logPathEl = getElement<HTMLElement>("log-path");
const openLogButton = getElement<HTMLButtonElement>("open-log-button");
const excludeInput = getElement<HTMLInputElement>("exclude-input");
const excludeAdd = getElement<HTMLButtonElement>("exclude-add");
const excludeList = getElement<HTMLUListElement>("exclude-list");
const languageSelect = getElement<HTMLSelectElement>("language-select");
let confirmAction: (() => void) | null = null;

function setTheme(theme: "rusty" | "omarchy"): void {
  document.documentElement.dataset.theme = theme;
}

async function applySystemTheme(): Promise<void> {
  if (!isTauri()) {
    setTheme("rusty");
    return;
  }

  try {
    setTheme(await invoke<"rusty" | "omarchy">("detect_theme"));
  } catch {
    setTheme("rusty");
  }
}

loadLocale();
applyTranslations();
void applySystemTheme();
void refreshMetrics();
window.setInterval(() => void refreshMetrics(), 3000);

scanButton.addEventListener("click", () => {
  if (scanning) {
    scanCancelled = true;
  } else {
    void runScan();
  }
});

if (isTauri()) {
  void listen<string>("scan-progress", (event) => {
    scanProgressPath.textContent = event.payload;
    scanProgressPath.title = event.payload;
    if (scanning && currentScanFeature === "browser") {
      const detail = getElement("browser-detail");
      detail.textContent = `Varrendo: ${event.payload}`;
      detail.title = event.payload;
    }
  });
}

settingsButton.addEventListener("click", () => void openSettings());
settingsClose.addEventListener("click", () => { settingsOverlay.hidden = true; });
settingsOverlay.addEventListener("click", (event) => {
  if (event.target === settingsOverlay) settingsOverlay.hidden = true;
});
openLogButton.addEventListener("click", () => {
  invoke("open_log_folder").catch((error) => showToast(String(error)));
});
excludeAdd.addEventListener("click", () => void addExclusion());
excludeInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") void addExclusion();
});
languageSelect.addEventListener("change", () => {
  setLocale(languageSelect.value as Locale);
  applyTranslations();
  renderFindings();
  updateSelectionBar();
});

function applyTranslations(): void {
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n!);
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-placeholder]").forEach((el) => {
    (el as HTMLInputElement).placeholder = t(el.dataset.i18nPlaceholder!);
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-aria]").forEach((el) => {
    el.setAttribute("aria-label", t(el.dataset.i18nAria!));
  });
  scanButtonLabel.textContent = scanning ? t("scan.stop") : (findings.length > 0 ? t("scan.again") : t("scan.start"));
  trashButton.textContent = t("selection.trash");
  emptyTrashButton.textContent = t("card.empty");
}

async function openSettings(): Promise<void> {
  settingsOverlay.hidden = false;
  settingsClose.focus();
  languageSelect.value = getLocale();
  if (!isTauri()) return;
  try {
    logPathEl.textContent = await invoke<string | null>("log_file_path") ?? t("settings.unavailable");
    renderExclusions(await invoke<string[]>("list_excluded_dirs"));
  } catch (error) {
    showToast(String(error));
  }
}

function renderExclusions(dirs: string[]): void {
  excludeList.replaceChildren();
  if (dirs.length === 0) {
    const empty = document.createElement("li");
    empty.className = "exclude-empty";
    empty.textContent = t("settings.noExclusions");
    excludeList.append(empty);
    return;
  }
  for (const dir of dirs) {
    const item = document.createElement("li");
    const label = document.createElement("span");
    label.textContent = dir;
    label.title = dir;
    const remove = document.createElement("button");
    remove.className = "icon-button";
    remove.type = "button";
    remove.setAttribute("aria-label", t("settings.remove", { path: dir }));
    remove.innerHTML = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>`;
    remove.addEventListener("click", () => {
      invoke<string[]>("remove_excluded_dir", { path: dir })
        .then(renderExclusions)
        .catch((error) => showToast(String(error)));
    });
    item.append(label, remove);
    excludeList.append(item);
  }
}

async function addExclusion(): Promise<void> {
  const path = excludeInput.value.trim();
  if (!path) return;
  try {
    renderExclusions(await invoke<string[]>("add_excluded_dir", { path }));
    excludeInput.value = "";
  } catch (error) {
    showToast(String(error));
  }
}
selectCategoryButton.addEventListener("click", () => {
  const visible = visibleFindings();
  const allSelected = visible.length > 0 && visible.every((item) => selectedPaths.has(item.path));
  if (allSelected) {
    visible.forEach((item) => selectedPaths.delete(item.path));
  } else {
    visible.forEach((item) => selectedPaths.add(item.path));
  }
  renderFindings();
});
selectAll.addEventListener("change", () => {
  const visible = visibleFindings();
  if (selectAll.checked) {
    visible.forEach((item) => selectedPaths.add(item.path));
  } else {
    visible.forEach((item) => selectedPaths.delete(item.path));
  }
  renderFindings();
});
trashButton.addEventListener("click", () => {
  const size = findings.filter((item) => selectedPaths.has(item.path)).reduce((total, item) => total + item.size, 0);
  openConfirm(
    t("confirm.trash", { count: number(selectedPaths.size), size: formatBytes(size) }),
    () => void trashSelected(),
  );
});
emptyTrashButton.addEventListener("click", () => {
  openConfirm(t("confirm.emptyTrash"), () => void emptyTrash());
});
confirmCancel.addEventListener("click", () => { confirmOverlay.hidden = true; });
confirmOverlay.addEventListener("click", (event) => {
  if (event.target === confirmOverlay) confirmOverlay.hidden = true;
});
confirmAccept.addEventListener("click", () => {
  confirmOverlay.hidden = true;
  confirmAction?.();
  confirmAction = null;
});

function openConfirm(message: string, action: () => void): void {
  confirmText.textContent = message;
  confirmAction = action;
  confirmOverlay.hidden = false;
  confirmCancel.focus();
}
resultsButton.addEventListener("click", () => openResults());
closeResultsButton.addEventListener("click", closeResults);
window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (!confirmOverlay.hidden) {
    confirmOverlay.hidden = true;
  } else if (!resultsPanel.hidden) {
    closeResults();
  }
});
filterSelect.addEventListener("change", () => {
  activeFilter = filterSelect.value;
  renderFindings();
});
searchInput.addEventListener("input", () => {
  searchQuery = searchInput.value.trim().toLowerCase();
  renderFindings();
});
sortSelect.addEventListener("change", () => {
  sortMode = sortSelect.value;
  renderFindings();
});

document.querySelectorAll<HTMLButtonElement>("[data-filter]").forEach((button) => {
  button.addEventListener("click", () => {
    const feature = button.dataset.filter ?? "all";
    openResults(feature);
  });
});

function openResults(feature = activeFilter): void {
  activeFilter = feature;
  filterSelect.value = feature;
  renderFindings();
  resultsPanel.hidden = false;
  closeResultsButton.focus();
}

function closeResults(): void {
  resultsPanel.hidden = true;
  resultsButton.focus();
}

function getElement<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing required UI element: #${id}`);
  return element as T;
}

function formatBytes(bytes: number): string {
  if (bytes < 1) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unitIndex;
  return `${new Intl.NumberFormat(getLocale(), { maximumFractionDigits: unitIndex === 0 ? 0 : 1 }).format(value)} ${units[unitIndex]}`;
}

function showToast(message: string): void {
  toast.textContent = message;
  toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { toast.hidden = true; }, 4500);
}

const scanOrder: FeatureKey[] = ["trash", "temp", "browser", "chat-media", "orphan", "large-old", "duplicates"];
let scanCancelled = false;
let scanning = false;
let elapsedTimer = 0;
let scanStartedAt = 0;
let currentScanFeature: FeatureKey | null = null;

const scanButtonLabel = getElement<HTMLSpanElement>("scan-button-label");
const scanProgress = getElement<HTMLElement>("scan-progress");
const scanProgressPath = getElement<HTMLSpanElement>("scan-progress-path");
const scanProgressTime = getElement<HTMLSpanElement>("scan-progress-time");

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

async function runScan(): Promise<void> {
  if (!isTauri()) {
    showToast(t("scan.desktopOnly"));
    return;
  }

  scanning = true;
  scanCancelled = false;
  setScanButtonState(true);
  resultsButton.disabled = true;
  getElement("scan-visual").classList.add("is-scanning");
  getElement("system-status").classList.add("is-busy");
  findings = [];
  selectedPaths.clear();
  activeFilter = "all";
  filterSelect.value = "all";
  renderFindings();
  startElapsedTimer();

  const started = performance.now();
  let platform = "unknown";
  let failed = 0;

  try {
    for (const [index, feature] of scanOrder.entries()) {
      if (scanCancelled) break;
      currentScanFeature = feature;
      statusLabel.textContent = `${t("scan.scanning")} ${index + 1}/${scanOrder.length}: ${featureLabels()[feature]}`;
      getElement("scan-caption").textContent = `${t("scan.scanning").toLowerCase()} ${featureLabels()[feature].toLowerCase()}...`;
      try {
        const result = await invoke<ScanResult>("scan_candidates", { feature });
        platform = result.platform;
        findings.push(...result.findings);
        renderSummary({ findings, elapsedMs: performance.now() - started, platform });
      } catch {
        failed += 1;
      }
    }
    currentScanFeature = null;

    const elapsed = (performance.now() - started) / 1000;
    resultsButton.disabled = findings.length === 0;
    statusLabel.textContent = scanCancelled
      ? t("app.cancelled")
      : `${platformName(platform)} · ${t("app.done")}`;
    getElement("system-status").classList.remove("is-busy");
    getElement("scan-caption").textContent = scanCancelled
      ? t("scan.partial", { count: number(findings.length) })
      : t("scan.summary", { count: number(findings.length), time: elapsed.toFixed(1).replace(".", ",") });
    if (failed > 0) {
      showToast(failed === 1 ? t("scan.categoryFailed") : t("scan.categoriesFailed", { count: failed }));
    }
  } finally {
    scanning = false;
    stopElapsedTimer();
    setScanButtonState(false);
    scanButtonLabel.textContent = t("scan.again");
    getElement("scan-visual").classList.remove("is-scanning");
  }
}

function platformName(platform: string): string {
  if (platform === "linux") return "Linux";
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  return "Sistema";
}

function renderSummary(result: ScanResult): void {
  const totalSize = findings.reduce((total, item) => total + item.size, 0);
  const counts = new Map<FeatureKey, { count: number; size: number }>();
  const messengerCounts = { telegram: 0, discord: 0, whatsapp: 0 };
  for (const item of findings) {
    const current = counts.get(item.feature) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    counts.set(item.feature, current);
    if (item.feature === "chat-media") {
      const path = item.path.toLowerCase();
      if (path.includes("telegram")) messengerCounts.telegram += 1;
      if (path.includes("discord")) messengerCounts.discord += 1;
      if (path.includes("whatsapp") || path.includes("whatsdesk") || path.includes("zapzap")) messengerCounts.whatsapp += 1;
    }
  }

  getElement("total-size").textContent = formatBytes(totalSize);
  getElement("item-count").textContent = number(findings.length);
  getElement("space-count").textContent = formatBytes(totalSize);
  getElement("category-count").textContent = `${counts.size} / 7`;
  getElement("duration-count").textContent = `${(result.elapsedMs / 1000).toFixed(1).replace(".", ",")}s`;

  const breakdown = getElement("category-breakdown");
  breakdown.replaceChildren();
  const topFeatures = [...counts.entries()].sort((left, right) => right[1].size - left[1].size).slice(0, 3);
  if (topFeatures.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-breakdown";
    empty.textContent = t("scan.noneFound");
    breakdown.append(empty);
  } else {
    for (const [feature, data] of topFeatures) {
      const row = document.createElement("button");
      row.className = "found-row";
      row.type = "button";
      row.dataset.filter = feature;
      row.innerHTML = `<span class="found-label"><i></i></span><span class="found-value"></span>`;
      row.querySelector<HTMLElement>(".found-label")!.style.setProperty("--swatch", featureColors[feature]);
      row.querySelector<HTMLElement>(".found-label")!.append(document.createTextNode(featureLabels()[feature]));
      row.querySelector<HTMLElement>(".found-value")!.textContent = formatBytes(data.size);
      row.addEventListener("click", () => openResults(feature));
      breakdown.append(row);
    }
  }

  const browser = counts.get("browser") ?? { count: 0, size: 0 };
  getElement("browser-size").textContent = formatBytes(browser.size);
  getElement("browser-detail").textContent = t("card.candidates", { count: number(browser.count) });
  const chips = getElement("browser-chips");
  chips.replaceChildren();
  const byBrowser = new Map<string, { count: number; size: number }>();
  for (const item of findings.filter((entry) => entry.feature === "browser")) {
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
  const trash = counts.get("trash") ?? { count: 0, size: 0 };
  renderTrashCard(trash.count, trash.size);
  const chat = counts.get("chat-media") ?? { count: 0, size: 0 };
  getElement("messenger-size").textContent = formatBytes(chat.size);
  getElement("telegram-count").textContent = t("card.items", { count: number(messengerCounts.telegram) });
  getElement("discord-count").textContent = t("card.items", { count: number(messengerCounts.discord) });
  getElement("whatsapp-count").textContent = t("card.items", { count: number(messengerCounts.whatsapp) });
  renderMediaTypes();
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
  for (const item of findings.filter((entry) => entry.feature === "chat-media")) {
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

function visibleFindings(): Finding[] {
  let visible = activeFilter === "all" ? [...findings] : findings.filter((item) => item.feature === activeFilter);
  if (searchQuery) {
    visible = visible.filter((item) =>
      item.name.toLowerCase().includes(searchQuery) || item.path.toLowerCase().includes(searchQuery)
    );
  }
  const byName = (a: Finding, b: Finding) => a.name.localeCompare(b.name);
  switch (sortMode) {
    case "size-asc": visible.sort((a, b) => a.size - b.size || byName(a, b)); break;
    case "age-desc": visible.sort((a, b) => (b.ageDays ?? -1) - (a.ageDays ?? -1) || byName(a, b)); break;
    case "age-asc": visible.sort((a, b) => (a.ageDays ?? Number.MAX_SAFE_INTEGER) - (b.ageDays ?? Number.MAX_SAFE_INTEGER) || byName(a, b)); break;
    case "name-asc": visible.sort(byName); break;
    default: visible.sort((a, b) => b.size - a.size || byName(a, b));
  }
  return visible;
}

function renderFindings(): void {
  const visible = visibleFindings();
  const body = getElement("results-body");
  body.replaceChildren();
  if (visible.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "table-empty";
    cell.textContent = findings.length === 0
      ? t("results.empty")
      : t("results.emptyFilter");
    row.append(cell);
    body.append(row);
  } else {
    for (const item of visible.slice(0, 500)) {
      const row = document.createElement("tr");

      const selectCell = document.createElement("td");
      selectCell.className = "select-col";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = selectedPaths.has(item.path);
      checkbox.setAttribute("aria-label", t("results.selectItem", { name: item.name }));
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) {
          selectedPaths.add(item.path);
        } else {
          selectedPaths.delete(item.path);
        }
        updateSelectionBar();
      });
      selectCell.append(checkbox);

      const fileCell = document.createElement("td");
      const fileName = document.createElement("span");
      fileName.className = "file-name";
      fileName.textContent = item.name;
      fileName.title = item.path;
      const filePath = document.createElement("span");
      filePath.className = "file-path";
      filePath.textContent = item.path;
      fileCell.append(fileName, filePath);

      const featureCell = document.createElement("td");
      const tag = document.createElement("span");
      tag.className = "feature-tag";
      tag.style.setProperty("--tag-color", featureColors[item.feature]);
      tag.textContent = featureLabels()[item.feature];
      featureCell.append(tag);

      const sizeCell = document.createElement("td");
      sizeCell.className = "numeric-cell";
      sizeCell.textContent = formatBytes(item.size);

      const ageCell = document.createElement("td");
      ageCell.className = "numeric-cell muted-cell";
      ageCell.textContent = item.ageDays === null ? "—" : t("table.days", { count: number(item.ageDays) });
      row.append(selectCell, fileCell, featureCell, sizeCell, ageCell);
      body.append(row);
    }

  }

  const footer = getElement("results-footer");
  const footerText = getElement("results-footer-text");
  if (visible.length > 500) {
    footer.hidden = false;
    const filterNote = activeFilter === "all"
      ? ""
      : t("results.filterNote", { total: number(findings.length) });
    footerText.textContent = t("results.showing", { shown: 500, total: number(visible.length), filter: filterNote });
  } else if (activeFilter !== "all" && visible.length > 0 && visible.length < findings.length) {
    footer.hidden = false;
    footerText.textContent = t("results.filterSummary", { count: number(visible.length), total: number(findings.length) });
  } else {
    footer.hidden = true;
  }

  const visiblePaths = new Set(visible.map((item) => item.path));
  const selectedVisible = [...selectedPaths].filter((path) => visiblePaths.has(path));
  selectAll.checked = visible.length > 0 && selectedVisible.length === visible.length;
  selectAll.indeterminate = selectedVisible.length > 0 && selectedVisible.length < visible.length;
  updateSelectionBar();
}

function updateSelectionBar(): void {
  const count = selectedPaths.size;
  selectionBar.hidden = count === 0;
  const size = findings.filter((item) => selectedPaths.has(item.path)).reduce((total, item) => total + item.size, 0);
  selectionSummary.textContent = count === 1
    ? t("selection.summary.one", { size: formatBytes(size) })
    : t("selection.summary.many", { count: number(count), size: formatBytes(size) });
}

async function trashSelected(): Promise<void> {
  const paths = [...selectedPaths];
  trashButton.disabled = true;
  trashButton.textContent = t("selection.moving");
  try {
    const result = await invoke<TrashResult>("trash_candidates", { paths });
    const trashedSet = new Set(result.trashed);
    findings = findings.filter((item) => !trashedSet.has(item.path));
    for (const path of result.trashed) selectedPaths.delete(path);
    renderFindings();
    if (result.failed.length === 0) {
      showToast(t("toast.trashed", { count: number(result.trashed.length) }));
    } else {
      showToast(t("toast.trashedPartial", {
        moved: number(result.trashed.length),
        failed: number(result.failed.length),
        error: result.failed[0].error,
      }));
    }
  } catch (error) {
    showToast(String(error));
  } finally {
    trashButton.disabled = false;
    trashButton.textContent = t("selection.trash");
  }
}

async function emptyTrash(): Promise<void> {
  emptyTrashButton.disabled = true;
  emptyTrashButton.textContent = t("card.emptying");
  try {
    const result = await invoke<EmptyTrashResult>("empty_trash");
    findings = findings.filter((item) => item.feature !== "trash");
    for (const item of [...selectedPaths]) selectedPaths.delete(item);
    renderFindings();
    renderTrashCard(0, 0);
    showToast(result.failed === 0
      ? t("toast.trashEmptied", { count: number(result.removed) })
      : t("toast.trashEmptiedPartial", { removed: number(result.removed), failed: number(result.failed) }));
  } catch (error) {
    showToast(String(error));
  } finally {
    emptyTrashButton.textContent = t("card.empty");
  }
}

function renderTrashCard(count: number, size: number): void {
  getElement("trash-detail").textContent = t("card.trashItems", { count: number(count), size: formatBytes(size) });
  emptyTrashButton.disabled = count === 0;
}

function browserName(path: string): string {
  const lower = path.toLowerCase();
  if (lower.includes("brave")) return "Brave";
  if (lower.includes("edge")) return "Edge";
  if (lower.includes("chromium")) return "Chromium";
  if (lower.includes("chrome")) return "Chrome";
  if (lower.includes("firefox") || lower.includes("mozilla")) return "Firefox";
  if (lower.includes("safari")) return "Safari";
  return "Outros";
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

async function refreshMetrics(): Promise<void> {
  if (!isTauri()) return;
  try {
    const metrics = await invoke<SystemMetrics>("system_metrics");
    getElement("cpu-count").textContent = `${Math.round(metrics.cpuPercent)}%`;
    getElement("cpu-bar").style.width = `${Math.min(metrics.cpuPercent, 100)}%`;
    getElement("memory-count").textContent = `${formatBytes(metrics.memoryUsed)} / ${formatBytes(metrics.memoryTotal)}`;
    getElement("memory-bar").style.width = metrics.memoryTotal > 0 ? `${(metrics.memoryUsed / metrics.memoryTotal) * 100}%` : "0%";
    getElement("disk-count").textContent = `${formatBytes(metrics.diskUsed)} / ${formatBytes(metrics.diskTotal)}`;
    getElement("disk-bar").style.width = metrics.diskTotal > 0 ? `${(metrics.diskUsed / metrics.diskTotal) * 100}%` : "0%";
  } catch {
    // Mantém os últimos valores se a leitura falhar.
  }
}
