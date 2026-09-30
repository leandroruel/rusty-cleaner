import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { number, t } from "./i18n";
import { formatBytes, formatCount, getElement, showToast } from "./state";
import { openConfirm } from "./cleaning";

type AppEntry = {
  name: string;
  path: string;
  icon: string | null;
  size: number | null;
  lastUsed: number | null;
  uninstallKind: string | null;
  uninstallArg: string | null;
};

let apps: AppEntry[] = [];
let uninstalling = false;

function daysUnused(app: AppEntry): number {
  if (app.lastUsed === null) return Number.POSITIVE_INFINITY;
  return (Date.now() / 1000 - app.lastUsed) / 86400;
}

function formatLastUsed(app: AppEntry): string {
  if (app.lastUsed === null) return t("applications.neverUsed");
  return new Date(app.lastUsed * 1000).toLocaleDateString();
}

function visibleApps(): AppEntry[] {
  const filter = getElement<HTMLSelectElement>("applications-filter").value;
  const filtered = apps.filter((app) => {
    if (filter === "all") return true;
    if (filter === "never") return app.lastUsed === null;
    return daysUnused(app) >= Number(filter);
  });
  return filtered.sort((left, right) => daysUnused(right) - daysUnused(left));
}

function renderApps(): void {
  const list = getElement("applications-list");
  list.replaceChildren();
  const visible = visibleApps();
  for (const app of visible) {
    const row = document.createElement("div");
    row.className = "app-row";

    const glyph = document.createElement("span");
    glyph.className = "app-glyph";
    if (app.icon) {
      const image = document.createElement("img");
      image.src = convertFileSrc(app.icon);
      image.alt = "";
      image.loading = "lazy";
      glyph.append(image);
    } else {
      glyph.textContent = app.name.slice(0, 1).toUpperCase();
    }

    const info = document.createElement("div");
    info.className = "app-info";
    const name = document.createElement("span");
    name.className = "app-name";
    name.textContent = app.name;
    const path = document.createElement("span");
    path.className = "app-path";
    path.textContent = app.path;
    path.title = app.path;
    info.append(name, path);

    const lastUsed = document.createElement("span");
    lastUsed.className = "app-last-used";
    lastUsed.textContent = formatLastUsed(app);

    const size = document.createElement("span");
    size.className = "app-size";
    size.textContent = app.size === null ? "—" : formatBytes(app.size);

    const uninstall = document.createElement("button");
    uninstall.type = "button";
    uninstall.className = "button button-outline-danger app-uninstall";
    uninstall.textContent = t("applications.uninstall");
    if (!app.uninstallKind || uninstalling) {
      uninstall.disabled = true;
      if (!app.uninstallKind) uninstall.title = t("applications.noUninstaller");
    }
    uninstall.addEventListener("click", () => {
      openConfirm(t("applications.uninstallConfirm", { name: app.name }), () => void uninstallApp(app));
    });

    row.append(glyph, info, lastUsed, size, uninstall);
    list.append(row);
  }

  const knownSize = visible.reduce((total, app) => total + (app.size ?? 0), 0);
  getElement("applications-size").textContent = visible.length === 0
    ? "—"
    : formatBytes(knownSize);
  const detail = getElement("applications-detail");
  detail.textContent = apps.length === 0
    ? t("applications.noneFound")
    : t("applications.found", { count: formatCount(visible.length), total: formatCount(apps.length) });
}

async function scanApps(): Promise<void> {
  if (!isTauri()) {
    showToast(t("scan.desktopOnly"));
    return;
  }
  const button = getElement<HTMLButtonElement>("applications-scan-button");
  button.disabled = true;
  getElement("applications-detail").textContent = t("applications.scanning");
  try {
    apps = await invoke<AppEntry[]>("list_applications");
    renderApps();
  } catch (error) {
    getElement("applications-detail").textContent = t("app.failed");
    showToast(String(error));
  } finally {
    button.disabled = false;
  }
}

async function uninstallApp(app: AppEntry): Promise<void> {
  uninstalling = true;
  renderApps();
  try {
    await invoke("uninstall_application", {
      request: { kind: app.uninstallKind, arg: app.uninstallArg, name: app.name },
    });
    showToast(t("applications.uninstalled", { name: app.name }));
    apps = apps.filter((item) => item !== app);
  } catch (error) {
    showToast(String(error));
  } finally {
    uninstalling = false;
    renderApps();
  }
}

export function refreshApplicationLabels(): void {
  if (apps.length > 0) renderApps();
}

export function initApplications(): void {
  getElement("applications-scan-button").addEventListener("click", () => void scanApps());
  getElement("applications-filter").addEventListener("change", renderApps);
}
