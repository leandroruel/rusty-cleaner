import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import { formatBytes, getElement, showToast } from "./state";
import bundledRegistry from "../themes.json";

/// A theme entry from the rusty-cleaner registry.
export type ThemeRegistryEntry = {
  id: string;
  name: string;
  author: string;
  description: string;
  repo: string;
};

/// A downloaded theme with resolved assets and the theme.css content.
export type AppliedTheme = {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string | null;
  css: string;
  root: string;
  size: number;
  brand: string | null;
  fonts: { family: string; path: string }[];
};

const STORAGE_KEY = "rusty-cleaner-theme";
const REMOTE_REGISTRY =
  "https://raw.githubusercontent.com/leandroruel/rusty-cleaner/main/web/themes.json";

type ThemeStatus = "applied" | "installed" | "available";
type ThemeCard = {
  entry: ThemeRegistryEntry | null;
  theme: AppliedTheme | null;
  status: ThemeStatus;
};

let installed: AppliedTheme[] = [];
let appliedId: string | null = null;

async function registry(): Promise<ThemeRegistryEntry[]> {
  try {
    const response = await fetch(REMOTE_REGISTRY);
    if (!response.ok) throw new Error(String(response.status));
    const remote = await response.json() as ThemeRegistryEntry[];
    return Array.isArray(remote) && remote.length > 0 ? remote : bundledRegistry;
  } catch {
    return bundledRegistry as ThemeRegistryEntry[];
  }
}

/// Resolves {{THEME_ROOT}}/<relative-path> placeholders in theme.css by
/// joining the theme's installation directory with the relative path at the
/// FILESYSTEM level, then converting the absolute FILE path through the
/// asset protocol. Calling convertFileSrc on the directory alone and
/// appending the rest breaks the URL encoding — the asset protocol
/// percent-encodes the whole path as one segment, so "/src/assets/…" after
/// an encoded directory is unreachable.
function resolveThemeUrls(css: string, themeRoot: string): string {
  const root = themeRoot.replace(/[\\/]+$/, "");
  return css.replace(
    /\{\{THEME_ROOT\}\}\/([^"')\s]+)/g,
    (_full, relative: string) => {
      const absolute = `${root}/${relative}`.replace(/\\/g, "/");
      return convertFileSrc(absolute);
    },
  );
}

/// Applies a resolved theme: the CSS is injected as a <style> element,
/// images go through real <img> layers, fonts are injected as @font-face,
/// and the brand icon swaps if it's a raster file.
export function applyTheme(theme: AppliedTheme): void {
  const root = document.documentElement;

  // Mark as community-themed so the built-in omarchy overrides stand down.
  root.dataset.communityTheme = "1";

  // Inject the theme.css with {{THEME_ROOT}} resolved to the asset
  // protocol URL — theme authors reference files by their path inside
  // the theme repo and the app resolves them.
  const resolvedCss = resolveThemeUrls(theme.css, theme.root);
  const styleEl = getElement<HTMLStyleElement>("theme-css");
  styleEl.textContent = resolvedCss;

  // Brand icon — only raster formats are reliable through the asset protocol.
  const brand = document.getElementById("brand-icon") as HTMLImageElement | null;
  if (brand) {
    const isRaster = theme.brand?.match(/\.(png|jpe?g|webp|ico)$/i);
    if (theme.brand && isRaster) {
      brand.dataset.defaultSrc = brand.dataset.defaultSrc ?? brand.src;
      brand.src = convertFileSrc(theme.brand);
    } else if (brand.dataset.defaultSrc) {
      brand.src = brand.dataset.defaultSrc;
    }
  }

  // Custom fonts from the theme.
  const fontFaces = theme.fonts.map(
    (font) =>
      `@font-face { font-family: "${font.family}"; src: url("${convertFileSrc(font.path)}"); }`,
  );
  const fontEl = getElement<HTMLStyleElement>("theme-fonts");
  fontEl.textContent = fontFaces.join("\n");
}

/// Restores the built-in appearance.
export function applyDefault(): void {
  const root = document.documentElement;
  delete root.dataset.communityTheme;
  root.removeAttribute("style");
  document.body.removeAttribute("style");
  getElement<HTMLElement>("sidebar").removeAttribute("style");
  getElement<HTMLStyleElement>("theme-css").textContent = "";
  getElement<HTMLStyleElement>("theme-fonts").textContent = "";
  const brand = document.getElementById("brand-icon") as HTMLImageElement | null;
  if (brand?.dataset.defaultSrc) brand.src = brand.dataset.defaultSrc;
}

async function persist(id: string | null): Promise<void> {
  appliedId = id;
  if (id === null) {
    localStorage.removeItem(STORAGE_KEY);
  } else {
    localStorage.setItem(STORAGE_KEY, id);
  }
}

async function ensureInstalled(entry: ThemeRegistryEntry): Promise<AppliedTheme> {
  const existing = installed.find((theme) => theme.id === entry.id);
  if (existing) return existing;
  const theme = await invoke<AppliedTheme>("download_theme", { repo: entry.repo });
  installed = await invoke<AppliedTheme[]>("installed_themes");
  return theme;
}

async function applyCard(card: ThemeCard): Promise<void> {
  // Spin the button that was clicked so the download has visible feedback.
  const button = document.activeElement as HTMLButtonElement | null;
  const spinner = button?.classList.contains("theme-apply") ? button : null;
  if (spinner) {
    spinner.disabled = true;
    spinner.dataset.originalLabel = spinner.textContent ?? "";
    spinner.classList.add("theme-downloading");
    spinner.textContent = t("themes.downloading");
  }
  try {
    if (card.entry === null) {
      applyDefault();
      await persist(null);
      showToast(t("themes.reverted"));
    } else {
      const theme = card.theme ?? await ensureInstalled(card.entry);
      applyTheme(theme);
      await persist(theme.id);
      showToast(t("themes.applied", { name: theme.name }));
    }
    await refreshPanel();
  } catch (error) {
    showToast(String(error));
  } finally {
    if (spinner) {
      spinner.classList.remove("theme-downloading");
      spinner.textContent = spinner.dataset.originalLabel ?? "";
      delete spinner.dataset.originalLabel;
    }
  }
}

async function removeCard(card: ThemeCard): Promise<void> {
  if (!card.theme) return;
  try {
    await invoke("delete_theme", { id: card.theme.id });
    installed = installed.filter((theme) => theme.id !== card.theme!.id);
    if (appliedId === card.theme.id) {
      await persist(null);
      applyDefault();
    }
    showToast(t("themes.removed", { name: card.theme.name }));
    await refreshPanel();
  } catch (error) {
    showToast(String(error));
  }
}

async function themeCards(): Promise<ThemeCard[]> {
  const entries = await registry();
  const cards: ThemeCard[] = [
    {
      entry: null,
      theme: null,
      status: appliedId === null ? "applied" : "available",
    },
  ];
  for (const entry of entries) {
    const theme = installed.find((item) => item.id === entry.id) ?? null;
    const status: ThemeStatus =
      appliedId === entry.id ? "applied" : theme ? "installed" : "available";
    cards.push({ entry, theme, status });
  }
  for (const theme of installed) {
    if (!entries.some((entry) => entry.id === theme.id)) {
      cards.push({
        entry: {
          id: theme.id,
          name: theme.name,
          author: theme.author ?? "",
          description: theme.description ?? "",
          repo: "",
        },
        theme,
        status: appliedId === theme.id ? "applied" : "installed",
      });
    }
  }
  return cards;
}

function renderCard(card: ThemeCard, list: HTMLElement): void {
  const row = document.createElement("div");
  row.className = "theme-card";

  const info = document.createElement("div");
  info.className = "theme-card-info";
  const name = document.createElement("span");
  name.className = "theme-card-name";
  name.textContent = card.entry ? card.entry.name : t("themes.defaultName");
  const author = document.createElement("span");
  author.className = "theme-card-author";
  const versionLabel = card.theme ? ` · v${card.theme.version}` : "";
  const sizeLabel = card.theme ? ` · ${formatBytes(card.theme.size)}` : "";
  author.textContent = card.entry
    ? `${card.entry.author}${versionLabel}${sizeLabel}`
    : t("themes.defaultAuthor");
  const description = document.createElement("span");
  description.className = "theme-card-description";
  description.textContent = card.entry
    ? (card.theme?.description ?? card.entry.description)
    : t("themes.defaultDescription");
  info.append(name, author, description);

  const actions = document.createElement("div");
  actions.className = "theme-card-actions";
  if (card.status === "applied") {
    const badge = document.createElement("span");
    badge.className = "theme-badge";
    badge.textContent = t("themes.statusApplied");
    actions.append(badge);
  } else {
    const apply = document.createElement("button");
    apply.type = "button";
    apply.className = "button button-quiet theme-apply";
    apply.textContent = card.theme ? t("themes.apply") : t("themes.downloadApply");
    apply.addEventListener("click", () => void applyCard(card));
    actions.append(apply);
  }
  if (card.theme) {
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "button button-outline-danger theme-remove";
    remove.textContent = t("themes.remove");
    remove.addEventListener("click", () => void removeCard(card));
    actions.append(remove);
  }

  row.append(info, actions);
  list.append(row);
}

async function refreshPanel(): Promise<void> {
  const list = getElement("themes-list");
  list.replaceChildren();
  const cards = await themeCards();
  for (const card of cards) {
    renderCard(card, list);
  }
}

/// Re-applies the persisted theme on boot.
export async function restoreAppliedTheme(): Promise<void> {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (!saved) return;
  try {
    installed = await invoke<AppliedTheme[]>("installed_themes");
    const theme = installed.find((item) => item.id === saved);
    if (theme) {
      appliedId = theme.id;
      applyTheme(theme);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    // Keep the built-in look when themes are unavailable.
  }
}

export function initThemes(): void {
  void restoreAppliedTheme();
  void refreshPanel();
}

export function refreshThemesPanel(): void {
  void refreshPanel();
}
