import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { number, t } from "./i18n";
import { formatBytes, getElement, showToast, state, type CacheCleanResult, type EmptyTrashResult, type Finding, type RegistryFixResult, type TrashResult } from "./state";
import { renderFindings, updateSelectionBar } from "./results";

const trashButton = getElement<HTMLButtonElement>("trash-button");
const emptyTrashButton = getElement<HTMLButtonElement>("empty-trash-button");
const cleanBrowserButton = getElement<HTMLButtonElement>("clean-browser-button");
const cleanMessengerButton = getElement<HTMLButtonElement>("clean-messenger-button");
const confirmOverlay = getElement<HTMLElement>("confirm-overlay");
const confirmText = getElement<HTMLParagraphElement>("confirm-text");
const confirmCancel = getElement<HTMLButtonElement>("confirm-cancel");
const confirmAccept = getElement<HTMLButtonElement>("confirm-accept");
const confirmExtra = getElement<HTMLButtonElement>("confirm-extra");

let confirmAction: (() => void) | null = null;
let extraAction: (() => void) | null = null;

export type ConfirmExtra = { label: string; action: () => void };

const cleanProgressOverlay = getElement<HTMLElement>("clean-progress-overlay");
const cleanProgressFill = getElement<HTMLElement>("clean-progress-fill");
const cleanProgressDetail = getElement<HTMLElement>("clean-progress-detail");

function showCleanProgress(total: number): void {
  if (total === 0) return;
  cleanProgressFill.style.width = "0%";
  cleanProgressDetail.textContent = t("clean.progressLabel", { current: number(0), total: number(total) });
  cleanProgressOverlay.hidden = false;
}

export function updateCleanProgress(current: number, total: number): void {
  if (cleanProgressOverlay.hidden) return;
  const percent = total > 0 ? Math.min(100, (current / total) * 100) : 0;
  cleanProgressFill.style.width = `${percent}%`;
  cleanProgressDetail.textContent = t("clean.progressLabel", { current: number(current), total: number(total) });
}

function hideCleanProgress(): void {
  cleanProgressOverlay.hidden = true;
}

export function openConfirm(
  message: string,
  action: () => void,
  extra?: ConfirmExtra,
): void {
  confirmText.textContent = message;
  confirmAction = action;
  if (extra) {
    confirmExtra.textContent = extra.label;
    confirmExtra.hidden = false;
    extraAction = extra.action;
  } else {
    confirmExtra.hidden = true;
    extraAction = null;
  }
  confirmOverlay.hidden = false;
  confirmCancel.focus();
}

/// Gracefully quits every running browser so cache locks are released
/// before a clean.
async function closeBrowsers(): Promise<void> {
  showToast(t("toast.closingBrowsers"));
  try {
    const closed = await invoke<{ name: string; wasRunning: boolean }[]>("close_browsers");
    const running = closed.filter((browser) => browser.wasRunning).map((browser) => browser.name);
    showToast(running.length === 0
      ? t("toast.noBrowsersRunning")
      : t("toast.browsersClosed", { list: running.join(", ") }));
  } catch (error) {
    showToast(String(error));
  }
}

const closeBrowsersExtra = (): ConfirmExtra => ({
  label: t("card.closeBrowsers"),
  action: () => void closeBrowsers(),
});

async function trashSelected(): Promise<void> {
  const selected = [...state.selectedPaths];
  const cacheItems = state.findings.filter(
    (item) =>
      selected.includes(item.path) &&
      (item.feature === "browser" || item.meta === "cache-dir")
  );
  const registryItems = state.findings.filter(
    (item) => item.feature === "registry" && selected.includes(item.path)
  );
  const filePaths = selected.filter(
    (path) =>
      !cacheItems.some((item) => item.path === path) &&
      !registryItems.some((item) => item.path === path)
  );
  trashButton.disabled = true;
  trashButton.textContent = t("selection.moving");
  try {
    if (filePaths.length > 0) {
      await trashPaths(filePaths);
    }
    if (cacheItems.length > 0) {
      await deleteCachePaths(cacheItems.map((item) => item.path));
    }
    if (registryItems.length > 0) {
      fixRegistryItems(registryItems);
    }
  } finally {
    trashButton.disabled = false;
    trashButton.textContent = t("selection.trash");
  }
}

/// Browser caches are regenerable and can hold hundreds of thousands of
/// files; they are permanently deleted in place instead of going through the
/// OS trash, which is what previously exhausted memory on Windows.
async function deleteCachePaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  showCleanProgress(paths.length);
  try {
    const result = await invoke<CacheCleanResult>("purge_cache_dirs", { paths });
    const removedSet = new Set(result.removed);
    const freed = state.findings
      .filter((item) => (item.feature === "browser" || item.meta === "cache-dir") && removedSet.has(item.path))
      .reduce((total, item) => total + item.size, 0);
    state.findings = state.findings.filter(
      (item) => !((item.feature === "browser" || item.meta === "cache-dir") && removedSet.has(item.path))
    );
    for (const path of result.removed) state.selectedPaths.delete(path);
    hideCleanProgress();
    renderFindings();
    if (result.failed.length === 0) {
      showToast(t("toast.cacheCleaned", {
        count: number(result.removed.length),
        size: formatBytes(freed),
      }));
    } else {
      showToast(t("toast.cacheCleanedPartial", {
        count: number(result.removed.length),
        failed: number(result.failed.length),
        error: result.failed[0].error,
      }));
    }
  } catch (error) {
    hideCleanProgress();
    showToast(String(error));
  }
}

export function fixRegistryItems(items: Finding[]): void {
  const fixed = async () => {
    if (items.length === 0) return;
    showCleanProgress(items.length);
    try {
      const result = await invoke<RegistryFixResult>("fix_registry_issues", {
        items: items.map((item) => ({ key: item.path, value: item.meta ?? null })),
      });
      const fixedSet = new Set(result.fixed);
      state.findings = state.findings.filter((item) => !fixedSet.has(item.path));
      for (const path of result.fixed) state.selectedPaths.delete(path);
      hideCleanProgress();
      renderFindings();
      if (result.failed.length === 0) {
        showToast(result.backupPath
          ? t("toast.registryFixedBackup", { count: number(result.fixed.length), path: result.backupPath })
          : t("toast.registryFixed", { count: number(result.fixed.length) }));
      } else {
        showToast(t("toast.registryFixedPartial", {
          fixed: number(result.fixed.length),
          failed: number(result.failed.length),
          error: result.failed[0].error,
        }));
      }
    } catch (error) {
      hideCleanProgress();
      showToast(String(error));
    }
  };
  void fixed();
}

async function emptyTrash(): Promise<void> {
  emptyTrashButton.disabled = true;
  emptyTrashButton.textContent = t("card.emptying");
  try {
    const result = await invoke<EmptyTrashResult>("empty_trash");
    state.findings = state.findings.filter((item) => item.feature !== "trash");
    state.selectedPaths.clear();
    renderFindings();
    getElement("trash-detail").textContent = t("card.trashItems", { count: number(0), size: formatBytes(0) });
    emptyTrashButton.disabled = true;
    showToast(result.failed === 0
      ? t("toast.trashEmptied", { count: number(result.removed) })
      : t("toast.trashEmptiedPartial", { removed: number(result.removed), failed: number(result.failed) }));
  } catch (error) {
    showToast(String(error));
  } finally {
    emptyTrashButton.textContent = t("card.empty");
  }
}

async function cleanCategory(feature: "browser" | "chat-media"): Promise<void> {
  const all = state.findings.filter((item) => item.feature === feature);
  // Messenger caches are regenerable directories (purged in place); saved
  // media files are the user's (trashed). Age/size filters apply to media
  // files only — caches are rebuilt anyway.
  const cacheItems = all.filter((item) => item.meta === "cache-dir");
  let mediaItems = all.filter((item) => item.meta !== "cache-dir");
  if (feature === "chat-media") {
    const filter = getElement<HTMLSelectElement>("media-filter").value;
    if (filter === "old") {
      mediaItems = mediaItems.filter((item) => (item.ageDays ?? 0) >= 90);
    } else if (filter === "large") {
      mediaItems = mediaItems.filter((item) => item.size >= 50 * 1024 * 1024);
    }
  }
  const items = [...cacheItems, ...mediaItems];
  if (items.length === 0) return;
  const size = items.reduce((total, item) => total + item.size, 0);
  if (feature === "browser") {
    openConfirm(
      t("card.cleanCacheConfirm", { count: number(items.length), size: formatBytes(size) }),
      () => void deleteCachePaths(items.map((item) => item.path)),
      closeBrowsersExtra(),
    );
    return;
  }
  const mediaSize = mediaItems.reduce((total, item) => total + item.size, 0);
  const message = cacheItems.length > 0 && mediaItems.length > 0
    ? t("card.cleanMessengerMixedConfirm", {
        count: number(mediaItems.length),
        size: formatBytes(mediaSize),
        cacheCount: number(cacheItems.length),
        cacheSize: formatBytes(size - mediaSize),
      })
    : cacheItems.length > 0
      ? t("card.cleanMessengerCacheConfirm", { count: number(cacheItems.length), size: formatBytes(size) })
      : t("card.cleanConfirm", { count: number(mediaItems.length), feature: t(`feature.${feature}`), size: formatBytes(size) });
  openConfirm(message, () => {
    if (mediaItems.length > 0) void trashPaths(mediaItems.map((item) => item.path));
    if (cacheItems.length > 0) void deleteCachePaths(cacheItems.map((item) => item.path));
  });
}

async function trashPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  showCleanProgress(paths.length);
  try {
    const result = await invoke<TrashResult>("trash_candidates", { paths });
    const trashedSet = new Set(result.trashed);
    state.findings = state.findings.filter((item) => !trashedSet.has(item.path));
    for (const path of result.trashed) state.selectedPaths.delete(path);
    hideCleanProgress();
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
    hideCleanProgress();
    showToast(String(error));
  }
}

export function initCleaning(): void {
  trashButton.addEventListener("click", () => {
    const selected = state.findings.filter((item) => state.selectedPaths.has(item.path));
    const size = selected.reduce((total, item) => total + item.size, 0);
    const cacheItems = selected.filter((item) => item.feature === "browser" || item.meta === "cache-dir");
    const cacheOnly = selected.length > 0 && cacheItems.length === selected.length;
    const registryOnly = selected.length > 0 && selected.every((item) => item.feature === "registry");
    const hasCache = cacheItems.length > 0 && !cacheOnly;
    const message = registryOnly
      ? t("confirm.registryFix", { count: number(selected.length) })
      : cacheOnly
        ? t("card.cleanCacheConfirm", { count: number(cacheItems.length), size: formatBytes(size) })
        : hasCache
          ? t("confirm.trashAndCache", {
              count: number(selected.length - cacheItems.length),
              cacheCount: number(cacheItems.length),
              cacheSize: formatBytes(cacheItems.reduce((total, item) => total + item.size, 0)),
            })
          : t("confirm.trash", { count: number(state.selectedPaths.size), size: formatBytes(size) });
    const extra = cacheItems.length > 0 ? closeBrowsersExtra() : undefined;
    openConfirm(message, () => void trashSelected(), extra);
  });
  emptyTrashButton.addEventListener("click", () => {
    openConfirm(t("confirm.emptyTrash"), () => void emptyTrash());
  });
  cleanBrowserButton.addEventListener("click", () => void cleanCategory("browser"));
  cleanMessengerButton.addEventListener("click", () => void cleanCategory("chat-media"));
  const dismissConfirm = () => {
    confirmOverlay.hidden = true;
    confirmAction = null;
    confirmExtra.hidden = true;
    extraAction = null;
  };
  confirmCancel.addEventListener("click", dismissConfirm);
  confirmOverlay.addEventListener("click", (event) => {
    if (event.target === confirmOverlay) dismissConfirm();
  });
  confirmExtra.addEventListener("click", () => extraAction?.());
  confirmAccept.addEventListener("click", () => {
    confirmOverlay.hidden = true;
    confirmExtra.hidden = true;
    const action = confirmAction;
    confirmAction = null;
    extraAction = null;
    action?.();
  });

  if (isTauri()) {
    void listen<{ current: number; total: number }>("clean-progress", (event) => {
      updateCleanProgress(event.payload.current, event.payload.total);
    });
  }
}

export function isConfirmOpen(): boolean {
  return !confirmOverlay.hidden;
}

export function closeConfirm(): void {
  confirmOverlay.hidden = true;
  confirmAction = null;
  confirmExtra.hidden = true;
  extraAction = null;
}

export function refreshCleaningLabels(): void {
  trashButton.textContent = t("selection.trash");
  emptyTrashButton.textContent = t("card.empty");
  updateSelectionBar();
}
