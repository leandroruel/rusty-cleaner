import { invoke, isTauri } from "@tauri-apps/api/core";
import { loadLocale, t } from "./i18n";
import { getElement, state } from "./state";
import { initScan } from "./scan";
import { initRegistry, refreshRegistryLabels } from "./registry";
import { initApplications, refreshApplicationLabels } from "./applications";
import { initPages, initAppVersion } from "./pages";
import { initResults, renderFindings, updateSelectionBar, closeResults } from "./results";
import { initCleaning, isConfirmOpen, closeConfirm, refreshCleaningLabels, updateCleanProgress } from "./cleaning";
import { initSettings, isSettingsOpen, closeSettings } from "./settings";
import { initRestore, isRestoreOpen, closeRestore } from "./restore";
import { initMessengerMedia, isMessengerMediaOpen, closeMessengerMedia } from "./messenger-media";
import { initMonitor } from "./monitor";
import { initUpdater } from "./updater";
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
  document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    el.setAttribute("title", t(el.dataset.i18nTitle!));
  });
  getElement<HTMLSpanElement>("scan-button-label").textContent = state.scanning
    ? t("scan.stop")
    : (state.findings.length > 0 ? t("scan.again") : t("scan.start"));
  refreshCleaningLabels();
}

loadLocale();
applyTranslations();
void applySystemTheme();
initPages();
initAppVersion();
initScan();
initResults();
initCleaning();
initRegistry();
initApplications();
initSettings(() => {
  applyTranslations();
  refreshRegistryLabels();
  refreshApplicationLabels();
  renderFindings();
  updateSelectionBar();
});
initRestore();
initMessengerMedia();
initMonitor();
if (isTauri()) initUpdater();

window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (isConfirmOpen()) {
    closeConfirm();
  } else if (isRestoreOpen()) {
    closeRestore();
  } else if (isMessengerMediaOpen()) {
    closeMessengerMedia();
  } else if (isSettingsOpen()) {
    closeSettings();
  } else {
    closeResults();
  }
});
