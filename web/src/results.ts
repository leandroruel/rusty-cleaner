import { featureColors, featureLabels, formatBytes, formatCount, getElement, state, type Finding } from "./state";
import { t } from "./i18n";
import { isPageActive, returnFromResults, showPage } from "./pages";

const filterSelect = getElement<HTMLSelectElement>("feature-filter");
const closeResultsButton = getElement<HTMLButtonElement>("close-results");
const selectAll = getElement<HTMLInputElement>("select-all");
const selectionBar = getElement<HTMLElement>("selection-bar");
const selectionSummary = getElement<HTMLSpanElement>("selection-summary");
const searchInput = getElement<HTMLInputElement>("search-input");
const sortSelect = getElement<HTMLSelectElement>("sort-select");
const selectCategoryButton = getElement<HTMLButtonElement>("select-category-button");

function groupDuplicates(items: Finding[]): Finding[][] {
  const byContent = new Map<string, Finding[]>();
  for (const item of items) {
    const key = `${item.name}:${item.size}`;
    const group = byContent.get(key) ?? [];
    group.push(item);
    byContent.set(key, group);
  }
  return [...byContent.values()].sort((a, b) => b.length - a.length || b[0].size - a[0].size);
}

export function visibleFindings(): Finding[] {
  let visible = state.activeFilter === "all" ? [...state.findings] : state.findings.filter((item) => item.feature === state.activeFilter);
  if (state.searchQuery) {
    visible = visible.filter((item) =>
      item.name.toLowerCase().includes(state.searchQuery) || item.path.toLowerCase().includes(state.searchQuery)
    );
  }
  const byName = (a: Finding, b: Finding) => a.name.localeCompare(b.name);
  switch (state.sortMode) {
    case "size-asc": visible.sort((a, b) => a.size - b.size || byName(a, b)); break;
    case "age-desc": visible.sort((a, b) => (b.ageDays ?? -1) - (a.ageDays ?? -1) || byName(a, b)); break;
    case "age-asc": visible.sort((a, b) => (a.ageDays ?? Number.MAX_SAFE_INTEGER) - (b.ageDays ?? Number.MAX_SAFE_INTEGER) || byName(a, b)); break;
    case "name-asc": visible.sort(byName); break;
    default: visible.sort((a, b) => b.size - a.size || byName(a, b));
  }
  return visible;
}

export function renderFindings(): void {
  const visible = visibleFindings();
  const body = getElement("results-body");
  body.replaceChildren();
  if (visible.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "table-empty";
    cell.textContent = state.findings.length === 0 ? t("results.empty") : t("results.emptyFilter");
    row.append(cell);
    body.append(row);
  } else {
    const rows = state.activeFilter === "duplicates" ? groupDuplicates(visible) : visible.map((item) => [item]);
    let rendered = 0;
    for (const group of rows) {
      if (rendered >= 500) break;
      if (group.length > 1) {
        const header = document.createElement("tr");
        header.className = "duplicate-group";
        const cell = document.createElement("td");
        cell.colSpan = 5;
        const totalSize = group.reduce((total, item) => total + item.size, 0);
        cell.textContent = t("results.duplicateGroup", { count: formatCount(group.length), size: formatBytes(totalSize) });
        header.append(cell);
        body.append(header);
      }
      for (const item of group) {
        if (rendered >= 500) break;
        rendered += 1;
        const row = document.createElement("tr");
        row.dataset.path = item.path;
        if (group.length > 1) row.className = "duplicate-member";

        const selectCell = document.createElement("td");
        selectCell.className = "select-col";
        const checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.checked = state.selectedPaths.has(item.path);
        checkbox.setAttribute("aria-label", t("results.selectItem", { name: item.name }));
        checkbox.addEventListener("change", () => {
          if (checkbox.checked) {
            state.selectedPaths.add(item.path);
          } else {
            state.selectedPaths.delete(item.path);
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
        ageCell.textContent = item.ageDays === null ? "—" : t("table.days", { count: formatCount(item.ageDays) });
        row.append(selectCell, fileCell, featureCell, sizeCell, ageCell);
        body.append(row);
      }
    }
  }

  const footer = getElement("results-footer");
  const footerText = getElement("results-footer-text");
  if (visible.length > 500) {
    footer.hidden = false;
    const filterNote = state.activeFilter === "all"
      ? ""
      : t("results.filterNote", { total: formatCount(state.findings.length) });
    footerText.textContent = t("results.showing", { shown: 500, total: formatCount(visible.length), filter: filterNote });
  } else if (state.activeFilter !== "all" && visible.length > 0 && visible.length < state.findings.length) {
    footer.hidden = false;
    footerText.textContent = t("results.filterSummary", { count: formatCount(visible.length), total: formatCount(state.findings.length) });
  } else {
    footer.hidden = true;
  }

  const visiblePaths = new Set(visible.map((item) => item.path));
  const selectedVisible = [...state.selectedPaths].filter((path) => visiblePaths.has(path));
  selectAll.checked = visible.length > 0 && selectedVisible.length === visible.length;
  selectAll.indeterminate = selectedVisible.length > 0 && selectedVisible.length < visible.length;
  updateSelectionBar();
}

export function updateSelectionBar(): void {
  const count = state.selectedPaths.size;
  selectionBar.hidden = count === 0;
  const size = state.findings.filter((item) => state.selectedPaths.has(item.path)).reduce((total, item) => total + item.size, 0);
  selectionSummary.textContent = count === 1
    ? t("selection.summary.one", { size: formatBytes(size) })
    : t("selection.summary.many", { count: formatCount(count), size: formatBytes(size) });
}

export function openResults(feature = state.activeFilter): void {
  state.activeFilter = feature;
  filterSelect.value = feature;
  renderFindings();
  showPage("results");
  closeResultsButton.focus();
}

export function closeResults(): void {
  // Escape reaches this handler from any page; only act on the results page.
  if (!isPageActive("results")) return;
  const origin = returnFromResults();
  if (origin === "overview") {
    getElement<HTMLButtonElement>("results-button").focus();
  }
}

export function initResults(): void {
  filterSelect.addEventListener("change", () => {
    state.activeFilter = filterSelect.value;
    renderFindings();
  });
  searchInput.addEventListener("input", () => {
    state.searchQuery = searchInput.value.trim().toLowerCase();
    renderFindings();
  });
  sortSelect.addEventListener("change", () => {
    state.sortMode = sortSelect.value;
    renderFindings();
  });
  selectAll.addEventListener("change", () => {
    const visible = visibleFindings();
    if (selectAll.checked) {
      visible.forEach((item) => state.selectedPaths.add(item.path));
    } else {
      visible.forEach((item) => state.selectedPaths.delete(item.path));
    }
    renderFindings();
  });
  selectCategoryButton.addEventListener("click", () => {
    const visible = visibleFindings();
    const allSelected = visible.length > 0 && visible.every((item) => state.selectedPaths.has(item.path));
    if (allSelected) {
      visible.forEach((item) => state.selectedPaths.delete(item.path));
    } else {
      visible.forEach((item) => state.selectedPaths.add(item.path));
    }
    renderFindings();
  });
  getElement<HTMLButtonElement>("results-button").addEventListener("click", () => openResults());
  closeResultsButton.addEventListener("click", closeResults);
  document.querySelectorAll<HTMLButtonElement>("[data-filter]").forEach((button) => {
    button.addEventListener("click", () => openResults(button.dataset.filter ?? "all"));
  });
}
