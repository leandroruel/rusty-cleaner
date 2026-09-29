import { invoke } from "@tauri-apps/api/core";
import { number, t } from "./i18n";
import { formatBytes, getElement, showToast, state, type EmptyTrashResult, type Finding, type RegistryFixResult, type TrashResult } from "./state";
import { renderFindings, updateSelectionBar } from "./results";

const trashButton = getElement<HTMLButtonElement>("trash-button");
const emptyTrashButton = getElement<HTMLButtonElement>("empty-trash-button");
const cleanBrowserButton = getElement<HTMLButtonElement>("clean-browser-button");
const cleanMessengerButton = getElement<HTMLButtonElement>("clean-messenger-button");
const confirmOverlay = getElement<HTMLElement>("confirm-overlay");
const confirmText = getElement<HTMLParagraphElement>("confirm-text");
const confirmCancel = getElement<HTMLButtonElement>("confirm-cancel");
const confirmAccept = getElement<HTMLButtonElement>("confirm-accept");

let confirmAction: (() => void) | null = null;

export function openConfirm(message: string, action: () => void): void {
  confirmText.textContent = message;
  confirmAction = action;
  confirmOverlay.hidden = false;
  confirmCancel.focus();
}

async function trashSelected(): Promise<void> {
  const selected = [...state.selectedPaths];
  const registryItems = state.findings.filter(
    (item) => item.feature === "registry" && selected.includes(item.path)
  );
  const filePaths = selected.filter((path) => !registryItems.some((item) => item.path === path));
  trashButton.disabled = true;
  trashButton.textContent = t("selection.moving");
  try {
    if (filePaths.length > 0) {
      await trashPaths(filePaths);
    }
    if (registryItems.length > 0) {
      fixRegistryItems(registryItems);
    }
  } finally {
    trashButton.disabled = false;
    trashButton.textContent = t("selection.trash");
  }
}

export function fixRegistryItems(items: Finding[]): void {
  const fixed = async () => {
    try {
      const result = await invoke<RegistryFixResult>("fix_registry_issues", {
        items: items.map((item) => ({ key: item.path, value: item.meta ?? null })),
      });
      const fixedSet = new Set(result.fixed);
      state.findings = state.findings.filter((item) => !fixedSet.has(item.path));
      for (const path of result.fixed) state.selectedPaths.delete(path);
      renderFindings();
      if (result.failed.length === 0) {
        showToast(t("toast.registryFixed", { count: number(result.fixed.length) }));
      } else {
        showToast(t("toast.registryFixedPartial", {
          fixed: number(result.fixed.length),
          failed: number(result.failed.length),
          error: result.failed[0].error,
        }));
      }
    } catch (error) {
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
  let items = state.findings.filter((item) => item.feature === feature);
  if (feature === "chat-media") {
    const filter = getElement<HTMLSelectElement>("media-filter").value;
    if (filter === "old") {
      items = items.filter((item) => (item.ageDays ?? 0) >= 90);
    } else if (filter === "large") {
      items = items.filter((item) => item.size >= 50 * 1024 * 1024);
    }
  }
  if (items.length === 0) return;
  const size = items.reduce((total, item) => total + item.size, 0);
  openConfirm(
    t("card.cleanConfirm", { count: number(items.length), feature: t(`feature.${feature}`), size: formatBytes(size) }),
    () => void trashPaths(items.map((item) => item.path)),
  );
}

async function trashPaths(paths: string[]): Promise<void> {
  try {
    const result = await invoke<TrashResult>("trash_candidates", { paths });
    const trashedSet = new Set(result.trashed);
    state.findings = state.findings.filter((item) => !trashedSet.has(item.path));
    for (const path of result.trashed) state.selectedPaths.delete(path);
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
  }
}

export function initCleaning(): void {
  trashButton.addEventListener("click", () => {
    const selected = state.findings.filter((item) => state.selectedPaths.has(item.path));
    const size = selected.reduce((total, item) => total + item.size, 0);
    const registryOnly = selected.length > 0 && selected.every((item) => item.feature === "registry");
    const message = registryOnly
      ? t("confirm.registryFix", { count: number(selected.length) })
      : t("confirm.trash", { count: number(state.selectedPaths.size), size: formatBytes(size) });
    openConfirm(message, () => void trashSelected());
  });
  emptyTrashButton.addEventListener("click", () => {
    openConfirm(t("confirm.emptyTrash"), () => void emptyTrash());
  });
  cleanBrowserButton.addEventListener("click", () => void cleanCategory("browser"));
  cleanMessengerButton.addEventListener("click", () => void cleanCategory("chat-media"));
  confirmCancel.addEventListener("click", () => { confirmOverlay.hidden = true; });
  confirmOverlay.addEventListener("click", (event) => {
    if (event.target === confirmOverlay) confirmOverlay.hidden = true;
  });
  confirmAccept.addEventListener("click", () => {
    confirmOverlay.hidden = true;
    confirmAction?.();
    confirmAction = null;
  });
}

export function isConfirmOpen(): boolean {
  return !confirmOverlay.hidden;
}

export function closeConfirm(): void {
  confirmOverlay.hidden = true;
}

export function refreshCleaningLabels(): void {
  trashButton.textContent = t("selection.trash");
  emptyTrashButton.textContent = t("card.empty");
  updateSelectionBar();
}
