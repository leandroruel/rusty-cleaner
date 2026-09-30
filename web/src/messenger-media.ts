import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { number, t } from "./i18n";
import { formatBytes, formatCount, getElement, showToast, state } from "./state";
import { openConfirm, trashPaths } from "./cleaning";
import { renderFindings } from "./results";

type MediaKind = "image" | "video" | "audio" | "document";

type MediaItem = {
  path: string;
  kind: MediaKind;
  size: number;
  modified: number | null;
};

const PAGE_SIZE = 60;

const overlay = getElement<HTMLElement>("media-overlay");
const title = getElement<HTMLElement>("media-title");
const grid = getElement<HTMLElement>("media-grid");
const summary = getElement<HTMLElement>("media-summary");
const kindFilter = getElement<HTMLSelectElement>("media-kind-filter");
const selectAll = getElement<HTMLInputElement>("media-select-all");
const loadMore = getElement<HTMLButtonElement>("media-load-more");
const selectionBar = getElement<HTMLElement>("media-selection-bar");
const selectionSummary = getElement<HTMLElement>("media-selection-summary");
const cleanButton = getElement<HTMLButtonElement>("media-clean-button");
const closeButton = getElement<HTMLButtonElement>("media-close");
const menu = getElement<HTMLElement>("media-menu");
const menuView = getElement<HTMLButtonElement>("media-menu-view");
const menuCopy = getElement<HTMLButtonElement>("media-menu-copy");
const menuMove = getElement<HTMLButtonElement>("media-menu-move");

let menuPath: string | null = null;

let items: MediaItem[] = [];
let selection = new Set<string>();
let shownCount = 0;
let loading = false;

function visibleItems(): MediaItem[] {
  const kind = kindFilter.value;
  return kind === "all" ? items : items.filter((item) => item.kind === kind);
}

function updateSummary(): void {
  const visible = visibleItems();
  const size = visible.reduce((total, item) => total + item.size, 0);
  summary.textContent = t("media.summary", { count: formatCount(visible.length), size: formatBytes(size) });

  const selectedSize = visible
    .filter((item) => selection.has(item.path))
    .reduce((total, item) => total + item.size, 0);
  selectionBar.hidden = selection.size === 0;
  selectionSummary.textContent = selection.size === 1
    ? t("selection.summary.one", { size: formatBytes(selectedSize) })
    : t("selection.summary.many", { count: formatCount(selection.size), size: formatBytes(selectedSize) });
  cleanButton.disabled = selection.size === 0 || loading;

  const allVisibleSelected = visible.length > 0 && visible.every((item) => selection.has(item.path));
  selectAll.checked = allVisibleSelected;
  selectAll.indeterminate = !allVisibleSelected && visible.some((item) => selection.has(item.path));
}

function tile(item: MediaItem): HTMLElement {
  const card = document.createElement("div");
  card.className = "media-tile";
  card.dataset.path = item.path;

  const preview = document.createElement("div");
  preview.className = "media-preview";
  const source = convertFileSrc(item.path);
  if (item.kind === "image") {
    const image = document.createElement("img");
    image.src = source;
    image.loading = "lazy";
    image.alt = item.path;
    preview.append(image);
  } else if (item.kind === "video") {
    const video = document.createElement("video");
    video.src = source;
    video.preload = "metadata";
    video.muted = true;
    preview.append(video);
  } else {
    preview.classList.add(`media-glyph-${item.kind}`);
    preview.append(kindIcon(item.kind));
  }

  const more = document.createElement("button");
  more.type = "button";
  more.className = "media-more";
  more.textContent = "⋯";
  more.setAttribute("aria-label", t("media.menuAria", { name: item.path }));
  more.addEventListener("click", (event) => {
    event.stopPropagation();
    openMenu(more, item.path);
  });

  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.className = "media-check";
  checkbox.checked = selection.has(item.path);
  checkbox.setAttribute("aria-label", t("results.selectItem", { name: item.path }));

  const info = document.createElement("div");
  info.className = "media-tile-info";
  const kindLabel = document.createElement("span");
  kindLabel.textContent = t(`media.${item.kind}`);
  const sizeLabel = document.createElement("span");
  sizeLabel.textContent = formatBytes(item.size);
  info.append(kindLabel, sizeLabel);

  const toggle = () => {
    if (selection.has(item.path)) {
      selection.delete(item.path);
      checkbox.checked = false;
    } else {
      selection.add(item.path);
      checkbox.checked = true;
    }
    updateSummary();
  };
  checkbox.addEventListener("click", (event) => event.stopPropagation());
  checkbox.addEventListener("change", toggle);
  card.addEventListener("click", toggle);

  card.append(preview, more, checkbox, info);
  return card;
}

function kindIcon(kind: MediaKind): HTMLElement {
  const icons: Record<MediaKind, string> = {
    image: `<svg viewBox="0 0 24 24" fill="none"><rect x="3.5" y="5" width="17" height="14" rx="1.5" stroke="currentColor" stroke-width="1.6"/><circle cx="9" cy="10" r="1.6" stroke="currentColor" stroke-width="1.4"/><path d="m6 17 4.2-4 3 2.8 3.3-3.4L21 17" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>`,
    video: `<svg viewBox="0 0 24 24" fill="none"><rect x="3.5" y="5.5" width="12" height="13" rx="1.5" stroke="currentColor" stroke-width="1.6"/><path d="m16.5 10.5 4-2.5v8l-4-2.5Z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>`,
    audio: `<svg viewBox="0 0 24 24" fill="none"><path d="M4 10v4h3l5 4V6L7 10H4Z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M15.5 9.5a4 4 0 0 1 0 5M17.8 7.2a7 7 0 0 1 0 9.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>`,
    document: `<svg viewBox="0 0 24 24" fill="none"><path d="M6 3.5h8.5L19 8v12.5H6z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M14 3.5V8h5M9 12.5h6M9 16h6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>`,
  };
  const span = document.createElement("span");
  span.innerHTML = icons[kind];
  return span;
}

function renderGrid(): void {
  const visible = visibleItems();
  grid.replaceChildren();
  for (const item of visible.slice(0, shownCount)) {
    grid.append(tile(item));
  }
  loadMore.hidden = shownCount >= visible.length;
  updateSummary();
}

async function open(messenger: string, name: string): Promise<void> {
  if (!isTauri()) {
    showToast(t("scan.desktopOnly"));
    return;
  }
  items = [];
  selection = new Set();
  shownCount = PAGE_SIZE;
  loading = true;
  title.textContent = name;
  overlay.hidden = false;
  grid.replaceChildren();
  summary.textContent = t("media.loading");
  selectionBar.hidden = true;
  cleanButton.disabled = true;
  loadMore.hidden = true;
  try {
    const result = await invoke<MediaItem[]>("list_messenger_media", { messenger });
    items = result.sort((left, right) => (right.modified ?? 0) - (left.modified ?? 0));
    loading = false;
    if (items.length === 0) {
      summary.textContent = t("media.noneFound", { messenger: name });
      return;
    }
    renderGrid();
  } catch (error) {
    loading = false;
    summary.textContent = t("app.failed");
    showToast(String(error));
  }
}

async function cleanSelected(): Promise<void> {
  const paths = [...selection];
  if (paths.length === 0) return;
  const size = items
    .filter((item) => selection.has(item.path))
    .reduce((total, item) => total + item.size, 0);
  openConfirm(
    t("media.cleanConfirm", { count: number(paths.length), size: formatBytes(size) }),
    () => void runClean(paths),
  );
}

async function runClean(paths: string[]): Promise<void> {
  const removed = new Set(paths);
  try {
    await trashPaths(paths);
    items = items.filter((item) => !removed.has(item.path));
    for (const path of paths) {
      selection.delete(path);
      state.selectedPaths.delete(path);
    }
    // The findings list may reference these paths; drop them too.
    state.findings = state.findings.filter((item) => !removed.has(item.path));
    renderFindings();
    shownCount = Math.min(shownCount, Math.max(visibleItems().length, 0));
    renderGrid();
  } catch {
    // trashPaths already surfaces errors via toast.
  }
}

function openMenu(anchor: HTMLElement, path: string): void {
  menuPath = path;
  const rect = anchor.getBoundingClientRect();
  menu.hidden = false;
  const menuHeight = 110;
  const top = rect.bottom + 4 + menuHeight > window.innerHeight
    ? Math.max(8, rect.top - menuHeight - 4)
    : rect.bottom + 4;
  menu.style.top = `${top}px`;
  menu.style.left = `${Math.min(rect.right - 170, window.innerWidth - 178)}px`;
}

function closeMenu(): void {
  menu.hidden = true;
  menuPath = null;
}

async function pickFolder(): Promise<string | null> {
  const selected = await openDialog({ directory: true, multiple: false });
  return typeof selected === "string" && selected ? selected : null;
}

function fileLabel(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

async function viewCurrent(): Promise<void> {
  if (!menuPath) return;
  try {
    await invoke("open_media_file", { path: menuPath });
  } catch (error) {
    showToast(String(error));
  }
}

async function copyCurrent(): Promise<void> {
  if (!menuPath) return;
  const destination = await pickFolder();
  if (!destination) return;
  try {
    const created = await invoke<string>("copy_media_file", { path: menuPath, destination });
    showToast(t("media.copied", { name: fileLabel(created) }));
  } catch (error) {
    showToast(String(error));
  }
}

async function moveCurrent(): Promise<void> {
  if (!menuPath) return;
  const source = menuPath;
  const destination = await pickFolder();
  if (!destination) return;
  try {
    const created = await invoke<string>("move_media_file", { path: source, destination });
    showToast(t("media.moved", { name: fileLabel(created) }));
    items = items.filter((item) => item.path !== source);
    selection.delete(source);
    state.selectedPaths.delete(source);
    state.findings = state.findings.filter((item) => item.path !== source);
    renderFindings();
    shownCount = Math.min(shownCount, Math.max(visibleItems().length, 0));
    renderGrid();
  } catch (error) {
    showToast(String(error));
  }
}

export function initMessengerMedia(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-messenger]").forEach((row) => {
    row.addEventListener("click", () => {
      const messenger = row.dataset.messenger;
      if (!messenger) return;
      const name = row.querySelector(".messenger-name")?.textContent ?? messenger;
      void open(messenger, name);
    });
  });
  kindFilter.addEventListener("change", () => {
    shownCount = PAGE_SIZE;
    renderGrid();
  });
  loadMore.addEventListener("click", () => {
    shownCount += PAGE_SIZE;
    renderGrid();
  });
  selectAll.addEventListener("change", () => {
    for (const item of visibleItems().slice(0, shownCount)) {
      if (selectAll.checked) {
        selection.add(item.path);
      } else {
        selection.delete(item.path);
      }
    }
    renderGrid();
  });
  cleanButton.addEventListener("click", () => void cleanSelected());
  menuView.addEventListener("click", () => { closeMenu(); void viewCurrent(); });
  menuCopy.addEventListener("click", () => { closeMenu(); void copyCurrent(); });
  menuMove.addEventListener("click", () => { closeMenu(); void moveCurrent(); });
  overlay.addEventListener("click", (event) => {
    if (!menu.hidden && !menu.contains(event.target as Node)) closeMenu();
    if (event.target === overlay) closeMessengerMedia();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !menu.hidden) {
      event.stopImmediatePropagation();
      closeMenu();
    }
  }, true);
  closeButton.addEventListener("click", closeMessengerMedia);
}

export function isMessengerMediaOpen(): boolean {
  return !overlay.hidden;
}

export function closeMessengerMedia(): void {
  overlay.hidden = true;
  closeButton.focus();
}
