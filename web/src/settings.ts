import { invoke, isTauri } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getLocale, setLocale, t, type Locale } from "./i18n";
import { getElement, showToast } from "./state";

const settingsButton = getElement<HTMLButtonElement>("settings-button");
const settingsOverlay = getElement<HTMLElement>("settings-overlay");
const settingsClose = getElement<HTMLButtonElement>("settings-close");
const logPathEl = getElement<HTMLElement>("log-path");
const openLogButton = getElement<HTMLButtonElement>("open-log-button");
const excludeInput = getElement<HTMLInputElement>("exclude-input");
const excludeAdd = getElement<HTMLButtonElement>("exclude-add");
const excludeBrowse = getElement<HTMLButtonElement>("exclude-browse");
const excludeList = getElement<HTMLUListElement>("exclude-list");
const languageSelect = getElement<HTMLSelectElement>("language-select");

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

export function initSettings(onLocaleChange: () => void): void {
  settingsButton.addEventListener("click", () => void openSettings());
  settingsClose.addEventListener("click", () => { settingsOverlay.hidden = true; });
  settingsOverlay.addEventListener("click", (event) => {
    if (event.target === settingsOverlay) settingsOverlay.hidden = true;
  });
  openLogButton.addEventListener("click", () => {
    invoke("open_log_folder").catch((error) => showToast(String(error)));
  });
  excludeAdd.addEventListener("click", () => void addExclusion());
  excludeBrowse.addEventListener("click", () => void browseExclusion());
  excludeInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void addExclusion();
  });
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
