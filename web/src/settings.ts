import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getLocale, setLocale, t, type Locale } from "./i18n";
import { getElement, showToast } from "./state";
import { refreshThemesPanel } from "./themes";
import { isOptedIn, setOptedIn } from "./telemetry";
import { getSettings as getAiSettings, saveSettings as saveAiSettings, testConnection, type AiSettings } from "./ai";


const settingsButton = getElement<HTMLButtonElement>("settings-button");
const settingsOverlay = getElement<HTMLElement>("settings-overlay");
const settingsClose = getElement<HTMLButtonElement>("settings-close");
const sectionTitle = getElement<HTMLHeadingElement>("settings-section-title");
const navItems = Array.from(document.querySelectorAll<HTMLButtonElement>(".settings-nav-item"));
const panels = Array.from(document.querySelectorAll<HTMLElement>(".settings-panel"));
const logPathEl = getElement<HTMLElement>("log-path");
const openLogButton = getElement<HTMLButtonElement>("open-log-button");
const excludeInput = getElement<HTMLInputElement>("exclude-input");
const excludeAdd = getElement<HTMLButtonElement>("exclude-add");
const excludeBrowse = getElement<HTMLButtonElement>("exclude-browse");
const excludeList = getElement<HTMLUListElement>("exclude-list");
const languageSelect = getElement<HTMLSelectElement>("language-select");
const crashReportsToggle = getElement<HTMLInputElement>("crash-reports-toggle");
const aiEnabledToggle = getElement<HTMLInputElement>("ai-enabled-toggle");
const aiBaseUrl = getElement<HTMLInputElement>("ai-base-url");
const aiModel = getElement<HTMLInputElement>("ai-model");
const aiApiKey = getElement<HTMLInputElement>("ai-api-key");
const aiSendPaths = getElement<HTMLInputElement>("ai-send-paths");
const aiProviderPreset = getElement<HTMLSelectElement>("ai-provider-preset");
const aiTestButton = getElement<HTMLButtonElement>("ai-test-button");
const aiTestStatus = getElement<HTMLElement>("ai-test-status");
const aboutVersion = getElement<HTMLElement>("about-version");
const aboutStatus = getElement<HTMLElement>("about-update-status");
const checkUpdateButton = getElement<HTMLButtonElement>("check-update-button");

let activeSection = "general";
let pendingUpdate: string | null = null;

function selectSection(target: string): void {
  activeSection = target;
  for (const item of navItems) {
    const selected = item.dataset.settingsTarget === target;
    item.classList.toggle("is-active", selected);
    item.setAttribute("aria-current", selected ? "page" : "false");
  }
  for (const panel of panels) {
    panel.hidden = panel.dataset.settingsPanel !== target;
  }
  const key = `settings.menu.${target}`;
  sectionTitle.dataset.i18n = key;
  sectionTitle.textContent = t(key);
  if (target === "themes") refreshThemesPanel();
}

async function openSettings(): Promise<void> {
  settingsOverlay.hidden = false;
  settingsClose.focus();
  languageSelect.value = getLocale();
  crashReportsToggle.checked = isOptedIn();
  void loadAiSettings();
  resetUpdateControls();
  selectSection(activeSection);
  if (!isTauri()) return;
  try {
    logPathEl.textContent = await invoke<string | null>("log_file_path") ?? t("settings.unavailable");
    renderExclusions(await invoke<string[]>("list_excluded_dirs"));
  } catch (error) {
    showToast(String(error));
  }
  try {
    aboutVersion.textContent = `v${await getVersion()}`;
  } catch {
    aboutVersion.textContent = t("settings.unavailable");
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

async function browseExclusion(): Promise<void> {
  try {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === "string" && selected) {
      excludeInput.value = selected;
      excludeInput.focus();
    }
  } catch (error) {
    showToast(String(error));
  }
}

function resetUpdateControls(): void {
  pendingUpdate = null;
  aboutStatus.textContent = "";
  checkUpdateButton.disabled = false;
  checkUpdateButton.dataset.i18n = "settings.checkUpdates";
  checkUpdateButton.textContent = t("settings.checkUpdates");
}

async function handleUpdateAction(): Promise<void> {
  if (!isTauri()) return;
  if (pendingUpdate) {
    checkUpdateButton.disabled = true;
    aboutStatus.textContent = t("update.installing");
    try {
      await invoke("install_update");
    } catch {
      aboutStatus.textContent = t("update.failed");
      checkUpdateButton.disabled = false;
    }
    return;
  }
  checkUpdateButton.disabled = true;
  aboutStatus.textContent = t("settings.checking");
  try {
    const version = await invoke<string | null>("check_for_update");
    if (version) {
      pendingUpdate = version;
      aboutStatus.textContent = t("update.available", { version });
      checkUpdateButton.dataset.i18n = "settings.installUpdate";
      checkUpdateButton.textContent = t("settings.installUpdate");
    } else {
      aboutStatus.textContent = t("settings.upToDate");
    }
  } catch {
    aboutStatus.textContent = t("update.failed");
  } finally {
    checkUpdateButton.disabled = false;
  }
}

function openPrivacyPolicy(): void {
  const url = "https://github.com/leandroruel/rusty-cleaner#privacy-and-telemetry";
  if (isTauri()) {
    void invoke("open_url", { url });
  } else {
    window.open(url, "_blank");
  }
}

async function loadAiSettings(): Promise<void> {
  try {
    const settings = await getAiSettings();
    aiEnabledToggle.checked = settings.enabled;
    aiBaseUrl.value = settings.baseUrl;
    aiModel.value = settings.model;
    aiApiKey.value = settings.apiKey;
    aiSendPaths.checked = settings.sendPaths;
  } catch {
    // Settings unavailable — leave fields empty.
  }
}

async function saveAi(): Promise<void> {
  const settings: AiSettings = {
    enabled: aiEnabledToggle.checked,
    baseUrl: aiBaseUrl.value.trim(),
    model: aiModel.value.trim(),
    apiKey: aiApiKey.value.trim(),
    sendPaths: aiSendPaths.checked,
  };
  try {
    await saveAiSettings(settings);
  } catch (error) {
    showToast(String(error));
  }
}

async function testAi(): Promise<void> {
  aiTestButton.disabled = true;
  aiTestStatus.textContent = t("settings.checking");
  try {
    await saveAi();
    const message = await testConnection();
    aiTestStatus.textContent = message;
  } catch (error) {
    aiTestStatus.textContent = String(error);
  } finally {
    aiTestButton.disabled = false;
  }
}

export function initSettings(onLocaleChange: () => void): void {
  settingsButton.addEventListener("click", () => void openSettings());
  settingsClose.addEventListener("click", () => { settingsOverlay.hidden = true; });
  for (const item of navItems) {
    item.addEventListener("click", () => selectSection(item.dataset.settingsTarget ?? "general"));
  }
  openLogButton.addEventListener("click", () => {
    invoke("open_log_folder").catch((error) => showToast(String(error)));
  });
  excludeAdd.addEventListener("click", () => void addExclusion());
  excludeBrowse.addEventListener("click", () => void browseExclusion());
  excludeInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void addExclusion();
  });
  checkUpdateButton.addEventListener("click", () => void handleUpdateAction());
  getElement<HTMLButtonElement>("open-privacy-button").addEventListener("click", () => {
    void openPrivacyPolicy();
  });
  getElement<HTMLButtonElement>("copy-tip-address").addEventListener("click", () => {
    const address = getElement<HTMLSpanElement>("tip-address").textContent ?? "";
    void navigator.clipboard.writeText(address).then(() => {
      showToast(t("settings.copied"));
    });
  });
  crashReportsToggle.addEventListener("change", () => {
    setOptedIn(crashReportsToggle.checked);
  });
  aiEnabledToggle.addEventListener("change", () => void saveAi());
  aiBaseUrl.addEventListener("change", () => void saveAi());
  aiModel.addEventListener("change", () => void saveAi());
  aiApiKey.addEventListener("change", () => void saveAi());
  aiSendPaths.addEventListener("change", () => void saveAi());
  aiProviderPreset.addEventListener("change", () => {
    if (aiProviderPreset.value) {
      aiBaseUrl.value = aiProviderPreset.value;
      void saveAi();
    }
  });
  aiTestButton.addEventListener("click", () => void testAi());
  languageSelect.addEventListener("change", () => {
    setLocale(languageSelect.value as Locale);
    onLocaleChange();
  });
}

export function isSettingsOpen(): boolean {
  return !settingsOverlay.hidden;
}

export function closeSettings(): void {
  settingsOverlay.hidden = true;
}
