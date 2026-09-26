import { invoke, isTauri } from "@tauri-apps/api/core";
import { number, t } from "./i18n";
import { getElement, showToast } from "./state";

type TrashItem = {
  name: string;
  originalPath: string;
};

const overlay = getElement<HTMLElement>("restore-overlay");
const closeButton = getElement<HTMLButtonElement>("restore-close");
const openButton = getElement<HTMLButtonElement>("restore-trash-button");

let items: TrashItem[] = [];
const selected = new Set<string>();

async function openRestore(): Promise<void> {
  if (!isTauri()) return;
  overlay.hidden = false;
  closeButton.focus();
  try {
    items = await invoke<TrashItem[]>("list_trash_items");
    selected.clear();
    renderItems();
  } catch (error) {
    showToast(String(error));
  }
}

function renderItems(): void {
  const dialog = overlay.querySelector(".settings-dialog")!;
  dialog.querySelector(".restore-list")?.remove();
  dialog.querySelector(".restore-actions")?.remove();

  const list = document.createElement("ul");
  list.className = "restore-list";
  if (items.length === 0) {
    const empty = document.createElement("li");
    empty.className = "exclude-empty";
    empty.textContent = t("restore.empty");
    list.append(empty);
  } else {
    for (const item of items) {
      const row = document.createElement("li");
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = selected.has(item.name);
      checkbox.setAttribute("aria-label", item.name);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) selected.add(item.name);
        else selected.delete(item.name);
        updateAction();
      });
      const label = document.createElement("span");
      label.textContent = item.name;
      label.title = item.originalPath;
      row.append(checkbox, label);
      list.append(row);
    }
  }
  dialog.append(list);

  const actions = document.createElement("div");
  actions.className = "restore-actions";
  const button = document.createElement("button");
  button.className = "button button-primary";
  button.id = "restore-accept";
  button.textContent = t("restore.action");
  button.disabled = selected.size === 0;
  button.addEventListener("click", () => void restoreSelected());
  actions.append(button);
  dialog.append(actions);
}

function updateAction(): void {
  const button = overlay.querySelector<HTMLButtonElement>("#restore-accept");
  if (button) button.disabled = selected.size === 0;
}

async function restoreSelected(): Promise<void> {
  try {
    const count = await invoke<number>("restore_trash_items", { names: [...selected] });
    showToast(t("restore.done", { count: number(count) }));
    overlay.hidden = true;
  } catch (error) {
    showToast(String(error));
  }
}

export function initRestore(): void {
  openButton.addEventListener("click", () => void openRestore());
  closeButton.addEventListener("click", () => { overlay.hidden = true; });
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) overlay.hidden = true;
  });
}

export function isRestoreOpen(): boolean {
  return !overlay.hidden;
}

export function closeRestore(): void {
  overlay.hidden = true;
}
