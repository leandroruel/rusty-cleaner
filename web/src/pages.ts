import { getVersion } from "@tauri-apps/api/app";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getElement } from "./state";

const navItems = Array.from(document.querySelectorAll<HTMLButtonElement>(".nav-item[data-page]"));
const pages = Array.from(document.querySelectorAll<HTMLElement>(".page[data-page]"));

let activePageId = "overview";
let resultsOrigin = "overview";

export function showPage(pageId: string): void {
  // The results page is reachable only through buttons, never the sidebar;
  // remember where it was opened from so the back arrow returns there.
  if (pageId === "results" && activePageId !== "results") {
    resultsOrigin = activePageId;
  }
  for (const item of navItems) {
    const selected = item.dataset.page === pageId;
    item.classList.toggle("is-active", selected);
    item.setAttribute("aria-current", selected ? "page" : "false");
  }
  for (const page of pages) {
    page.hidden = page.dataset.page !== pageId;
  }
  activePageId = pageId;
}

/// Leaves the results page, returning to wherever it was opened from.
export function returnFromResults(): string {
  showPage(resultsOrigin);
  return resultsOrigin;
}

export function isPageActive(pageId: string): boolean {
  return activePageId === pageId;
}

export function initPages(): void {
  navItems.forEach((item) => {
    const page = item.dataset.page ?? "overview";
    if (page === "settings") return;
    item.addEventListener("click", () => showPage(page));
  });
  hideRegistryOutsideWindows();
}

/// The registry is a Windows-only feature; its page and results filter stay
/// hidden everywhere else.
function hideRegistryOutsideWindows(): void {
  const hide = () => {
    document
      .querySelectorAll<HTMLElement>('.nav-item[data-page="registry"], #feature-filter option[value="registry"]')
      .forEach((element) => element.setAttribute("hidden", ""));
  };
  if (!isTauri()) {
    hide();
    return;
  }
  invoke<string>("detect_platform")
    .then((platform) => {
      if (platform !== "windows") hide();
    })
    .catch(() => {
      // Without a platform answer, keep the entry visible.
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
