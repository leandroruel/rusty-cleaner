import { invoke } from "@tauri-apps/api/core";
import { number, t } from "./i18n";
import { formatBytes, getElement, showToast, state, type EmptyTrashResult, type TrashResult } from "./state";
import { renderFindings, updateSelectionBar } from "./results";

const trashButton = getElement<HTMLButtonElement>("trash-button");
const emptyTrashButton = getElement<HTMLButtonElement>("empty-trash-button");
const confirmOverlay = getElement<HTMLElement>("confirm-overlay");
const confirmText = getElement<HTMLParagraphElement>("confirm-text");
const confirmCancel = getElement<HTMLButtonElement>("confirm-cancel");
const confirmAccept = getElement<HTMLButtonElement>("confirm-accept");

let confirmAction: (() => void) | null = null;

function openConfirm(message: string, action: () => void): void {
  confirmText.textContent = message;
  confirmAction = action;
  confirmOverlay.hidden = false;
  confirmCancel.focus();
}

async function trashSelected(): Promise<void> {
  const paths = [...state.selectedPaths];
  trashButton.disabled = true;
  trashButton.textContent = t("selection.moving");
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

export function initCleaning(): void {
  trashButton.addEventListener("click", () => {
    const size = state.findings.filter((item) => state.selectedPaths.has(item.path)).reduce((total, item) => total + item.size, 0);
    openConfirm(
      t("confirm.trash", { count: number(state.selectedPaths.size), size: formatBytes(size) }),
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
