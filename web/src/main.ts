import { invoke, isTauri } from "@tauri-apps/api/core";
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

const featureLabels: Record<FeatureKey, string> = {
  orphan: "Dados antigos de apps",
  temp: "Temporários",
  "chat-media": "Mídia de mensageiros",
  trash: "Lixeira",
  browser: "Navegadores",
  duplicates: "Duplicados",
  "large-old": "Grandes e antigos",
};

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

void applySystemTheme();
void refreshMetrics();
window.setInterval(() => void refreshMetrics(), 3000);

scanButton.addEventListener("click", () => void runScan());
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
  confirmText.textContent = `${selectedPaths.size.toLocaleString("pt-BR")} itens · ${formatBytes(size)} serão movidos para a lixeira.`;
  confirmOverlay.hidden = false;
  confirmCancel.focus();
});
confirmCancel.addEventListener("click", () => { confirmOverlay.hidden = true; });
confirmOverlay.addEventListener("click", (event) => {
  if (event.target === confirmOverlay) confirmOverlay.hidden = true;
});
confirmAccept.addEventListener("click", () => void trashSelected());
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
  return `${new Intl.NumberFormat("pt-BR", { maximumFractionDigits: unitIndex === 0 ? 0 : 1 }).format(value)} ${units[unitIndex]}`;
}

function showToast(message: string): void {
  toast.textContent = message;
  toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { toast.hidden = true; }, 4500);
}

async function runScan(): Promise<void> {
  if (!isTauri()) {
    showToast("Abra o aplicativo desktop com `npm run tauri dev` para executar a varredura.");
    return;
  }

  scanButton.disabled = true;
  scanButton.textContent = "Analisando...";
  resultsButton.disabled = true;
  getElement("scan-visual").classList.add("is-scanning");
  getElement("system-status").classList.add("is-busy");
  statusLabel.textContent = "Analisando pastas reconhecidas";
  getElement("scan-caption").textContent = "A varredura é somente leitura";

  try {
    const result = await invoke<ScanResult>("scan_candidates");
    findings = result.findings;
    renderSummary(result);
    activeFilter = "all";
    filterSelect.value = "all";
    renderFindings();
    resultsButton.disabled = findings.length === 0;
    statusLabel.textContent = `${platformName(result.platform)} · análise concluída`;
    getElement("system-status").classList.remove("is-busy");
    getElement("scan-caption").textContent = `${findings.length.toLocaleString("pt-BR")} itens encontrados em ${(result.elapsedMs / 1000).toFixed(1).replace(".", ",")}s`;
  } catch (error) {
    statusLabel.textContent = "Falha na análise";
    getElement("system-status").classList.remove("is-busy");
    getElement("scan-caption").textContent = "Não foi possível concluir a varredura";
    showToast(String(error));
  } finally {
    getElement("scan-visual").classList.remove("is-scanning");
    scanButton.disabled = false;
    scanButton.textContent = "Escanear novamente";
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
  const messengerCounts = { telegram: 0, discord: 0 };
  for (const item of findings) {
    const current = counts.get(item.feature) ?? { count: 0, size: 0 };
    current.count += 1;
    current.size += item.size;
    counts.set(item.feature, current);
    if (item.feature === "chat-media") {
      const path = item.path.toLowerCase();
      if (path.includes("telegram")) messengerCounts.telegram += 1;
      if (path.includes("discord")) messengerCounts.discord += 1;
    }
  }

  getElement("total-size").textContent = formatBytes(totalSize);
  getElement("item-count").textContent = findings.length.toLocaleString("pt-BR");
  getElement("space-count").textContent = formatBytes(totalSize);
  getElement("category-count").textContent = `${counts.size} / 7`;
  getElement("duration-count").textContent = `${(result.elapsedMs / 1000).toFixed(1).replace(".", ",")}s`;

  const breakdown = getElement("category-breakdown");
  breakdown.replaceChildren();
  const topFeatures = [...counts.entries()].sort((left, right) => right[1].size - left[1].size).slice(0, 3);
  if (topFeatures.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty-breakdown";
    empty.textContent = "Nenhum candidato encontrado nas pastas verificadas.";
    breakdown.append(empty);
  } else {
    for (const [feature, data] of topFeatures) {
      const row = document.createElement("button");
      row.className = "found-row";
      row.type = "button";
      row.dataset.filter = feature;
      row.innerHTML = `<span class="found-label"><i></i></span><span class="found-value"></span>`;
      row.querySelector<HTMLElement>(".found-label")!.style.setProperty("--swatch", featureColors[feature]);
      row.querySelector<HTMLElement>(".found-label")!.append(document.createTextNode(featureLabels[feature]));
      row.querySelector<HTMLElement>(".found-value")!.textContent = formatBytes(data.size);
      row.addEventListener("click", () => openResults(feature));
      breakdown.append(row);
    }
  }

  const browser = counts.get("browser") ?? { count: 0, size: 0 };
  getElement("browser-size").textContent = formatBytes(browser.size);
  getElement("browser-detail").textContent = `${browser.count.toLocaleString("pt-BR")} arquivos candidatos`;
  const trash = counts.get("trash") ?? { count: 0, size: 0 };
  getElement("trash-detail").textContent = `${trash.count.toLocaleString("pt-BR")} itens · ${formatBytes(trash.size)}`;
  const chat = counts.get("chat-media") ?? { count: 0, size: 0 };
  getElement("messenger-size").textContent = formatBytes(chat.size);
  getElement("telegram-count").textContent = `${messengerCounts.telegram.toLocaleString("pt-BR")} itens`;
  getElement("discord-count").textContent = `${messengerCounts.discord.toLocaleString("pt-BR")} itens`;
}

function visibleFindings(): Finding[] {
  return activeFilter === "all" ? findings : findings.filter((item) => item.feature === activeFilter);
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
      ? "Inicie uma varredura para revisar os candidatos. Nenhum arquivo será apagado."
      : "Nenhum item encontrado nesta categoria.";
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
      checkbox.setAttribute("aria-label", `Selecionar ${item.name}`);
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
      tag.textContent = featureLabels[item.feature];
      featureCell.append(tag);

      const sizeCell = document.createElement("td");
      sizeCell.className = "numeric-cell";
      sizeCell.textContent = formatBytes(item.size);

      const ageCell = document.createElement("td");
      ageCell.className = "numeric-cell muted-cell";
      ageCell.textContent = item.ageDays === null ? "—" : `${item.ageDays.toLocaleString("pt-BR")} dias`;
      row.append(selectCell, fileCell, featureCell, sizeCell, ageCell);
      body.append(row);
    }

    if (visible.length > 500) {
      const row = document.createElement("tr");
      const cell = document.createElement("td");
      cell.colSpan = 5;
      cell.className = "table-empty";
      cell.textContent = `Exibindo 500 de ${visible.length.toLocaleString("pt-BR")} itens.`;
      row.append(cell);
      body.append(row);
    }
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
  selectionSummary.textContent = `${count.toLocaleString("pt-BR")} ${count === 1 ? "item selecionado" : "itens selecionados"} · ${formatBytes(size)}`;
}

async function trashSelected(): Promise<void> {
  confirmOverlay.hidden = true;
  const paths = [...selectedPaths];
  trashButton.disabled = true;
  trashButton.textContent = "Movendo...";
  try {
    const result = await invoke<TrashResult>("trash_candidates", { paths });
    const trashedSet = new Set(result.trashed);
    findings = findings.filter((item) => !trashedSet.has(item.path));
    for (const path of result.trashed) selectedPaths.delete(path);
    renderFindings();
    if (result.failed.length === 0) {
      showToast(`${result.trashed.length.toLocaleString("pt-BR")} itens movidos para a lixeira.`);
    } else {
      showToast(`${result.trashed.length.toLocaleString("pt-BR")} movidos; ${result.failed.length.toLocaleString("pt-BR")} falharam (ex.: ${result.failed[0].error}).`);
    }
  } catch (error) {
    showToast(String(error));
  } finally {
    trashButton.disabled = false;
    trashButton.textContent = "Mover para a lixeira";
  }
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
