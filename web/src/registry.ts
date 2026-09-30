import { invoke, isTauri } from "@tauri-apps/api/core";
import { number, t } from "./i18n";
import { formatCount, getElement, showToast, state, type ScanResult } from "./state";
import { renderFindings } from "./results";
import { openConfirm, fixRegistryItems } from "./cleaning";

const ruleIds = [
  "missing-shared-dlls",
  "unused-file-extensions",
  "activex-class",
  "type-libraries",
  "applications",
  "fonts",
  "application-paths",
  "help-files",
  "installer",
  "obsolete-software",
  "run-at-startup",
  "start-menu-ordering",
  "mui-cache",
  "sound-events",
  "windows-services",
];

const STORAGE_KEY = "rusty-cleaner-registry-rules";

function checkedRules(): string[] {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === null) return [...ruleIds];
  try {
    const parsed = JSON.parse(saved) as unknown;
    if (!Array.isArray(parsed)) return [...ruleIds];
    return parsed.filter((rule): rule is string => typeof rule === "string" && ruleIds.includes(rule));
  } catch {
    return [...ruleIds];
  }
}

function saveCheckedRules(rules: string[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(rules));
}

function registryFindings() {
  return state.findings.filter((item) => item.feature === "registry");
}

function updateFixButton(): void {
  getElement<HTMLButtonElement>("registry-fix-button").disabled = registryFindings().length === 0;
}

function renderRules(): void {
  const container = getElement("registry-rules");
  container.replaceChildren();
  const checked = new Set(checkedRules());
  for (const rule of ruleIds) {
    const label = document.createElement("label");
    label.className = "registry-rule";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = checked.has(rule);
    checkbox.setAttribute("aria-label", t(`registry.rule.${rule}`));
    checkbox.addEventListener("change", () => {
      const current = new Set(checkedRules());
      if (checkbox.checked) {
        current.add(rule);
      } else {
        current.delete(rule);
      }
      saveCheckedRules([...current]);
      updateScanButton();
    });
    const text = document.createElement("span");
    text.textContent = t(`registry.rule.${rule}`);
    label.append(checkbox, text);
    container.append(label);
  }
}

function updateScanButton(): void {
  const scanButton = getElement<HTMLButtonElement>("registry-scan-button");
  scanButton.disabled = checkedRules().length === 0;
  scanButton.title = checkedRules().length === 0 ? t("registry.noRules") : "";
}

async function scanIssues(): Promise<void> {
  const rules = checkedRules();
  if (rules.length === 0) {
    showToast(t("registry.noRules"));
    return;
  }
  if (!isTauri()) {
    showToast(t("scan.desktopOnly"));
    return;
  }
  const scanButton = getElement<HTMLButtonElement>("registry-scan-button");
  const detail = getElement("registry-detail");
  scanButton.disabled = true;
  detail.textContent = t("registry.scanning");
  try {
    const result = await invoke<ScanResult>("scan_registry_issues", { rules });
    const stalePaths = new Set(registryFindings().map((item) => item.path));
    for (const path of stalePaths) state.selectedPaths.delete(path);
    state.findings = state.findings.filter((item) => item.feature !== "registry");
    state.findings.push(...result.findings);
    renderFindings();
    if (result.platform !== "windows") {
      getElement("registry-size").textContent = "—";
      detail.textContent = t("registry.windowsOnly");
      getElement<HTMLButtonElement>("registry-fix-button").disabled = true;
      return;
    }
    getElement("registry-size").textContent = formatCount(result.findings.length);
    detail.textContent = result.findings.length === 0
      ? t("registry.noneFound")
      : t("registry.found", { count: formatCount(result.findings.length), time: (result.elapsedMs / 1000).toFixed(1).replace(".", ",") });
    updateFixButton();
  } catch (error) {
    detail.textContent = t("app.failed");
    showToast(String(error));
  } finally {
    scanButton.disabled = false;
    updateScanButton();
  }
}

function fixIssues(): void {
  const items = registryFindings();
  if (items.length === 0) return;
  openConfirm(
    t("confirm.registryFix", { count: number(items.length) }),
    () => {
      fixRegistryItems(items);
      getElement("registry-size").textContent = formatCount(0);
      getElement("registry-detail").textContent = t("registry.noneFound");
      updateFixButton();
    },
    {
      label: t("card.openSystemProtection"),
      action: () => {
        void invoke("open_system_protection").catch((error) => showToast(String(error)));
      },
    },
    { label: t("confirm.restorePointGate") },
  );
}

export function refreshRegistryLabels(): void {
  renderRules();
  const detail = getElement("registry-detail");
  const items = registryFindings();
  if (items.length === 0) {
    detail.textContent = t("registry.noneFound");
  } else {
    detail.textContent = t("registry.found", { count: formatCount(items.length), time: "0,0" });
  }
}

export function initRegistry(): void {
  renderRules();
  updateScanButton();
  updateFixButton();
  getElement("registry-scan-button").addEventListener("click", () => void scanIssues());
  getElement("registry-fix-button").addEventListener("click", fixIssues);
}
