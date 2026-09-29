import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { getElement } from "./state";

const navItems = Array.from(document.querySelectorAll<HTMLButtonElement>(".nav-item[data-page]"));
const pages = Array.from(document.querySelectorAll<HTMLElement>(".page[data-page]"));

export function showPage(pageId: string): void {
  for (const item of navItems) {
    const selected = item.dataset.page === pageId;
    item.classList.toggle("is-active", selected);
    item.setAttribute("aria-current", selected ? "page" : "false");
  }
  for (const page of pages) {
    page.hidden = page.dataset.page !== pageId;
  }
}

export function initPages(): void {
  navItems.forEach((item) => {
    const page = item.dataset.page ?? "overview";
    if (page === "settings") return;
    item.addEventListener("click", () => showPage(page));
  });
}

export function initAppVersion(): void {
  if (!isTauri()) return;
  void getVersion()
    .then((version) => {
      getElement("app-version").textContent = `v${version}`;
    })
    .catch(() => {
      // Keep the static version when the runtime is unavailable.
    });
}
