import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import { getElement, showToast } from "./state";
import bundledRegistry from "../themes.json";

/// A theme entry from the rusty-cleaner registry (web/themes.json).
export type ThemeRegistryEntry = {
  id: string;
  name: string;
  author: string;
  description: string;
  repo: string;
};

/// A downloaded theme with resolved asset paths (from the backend).
export type AppliedTheme = {
  id: string;
  name: string;
  version: string;
  author: string | null;
  description: string | null;
  colors: {
    background?: string | null;
    panel?: string | null;
    panel2?: string | null;
    border?: string | null;
    text?: string | null;
    textDim?: string | null;
    textFaint?: string | null;
    accent?: string | null;
    accentSecondary?: string | null;
    purple?: string | null;
    green?: string | null;
    amber?: string | null;
    red?: string | null;
    orange?: string | null;
    blue?: string | null;
  };
  background: { image: string | null; tint: string | null };
  sidebar: { image: string | null; tint: string | null };
  fonts: { text: string | null; mono: string | null };
  icons: { brand: string | null };
};

/// Converts a hex color to rgba with the given alpha, so background
/// images can show through semi-transparent panels.
function withAlpha(color: string | null | undefined, alpha: number): string | null {
  if (!color) return null;
  if (color.startsWith("rgba")) return color;
  const match = color.match(/^#([0-9a-f]{6})$/i);
  if (!match) return color;
  const r = parseInt(match[1].slice(0, 2), 16);
  const g = parseInt(match[1].slice(2, 4), 16);
  const b = parseInt(match[1].slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

const STORAGE_KEY = "rusty-cleaner-theme";
const REMOTE_REGISTRY =
  "https://raw.githubusercontent.com/leandroruel/rusty-cleaner/main/web/themes.json";

type ThemeStatus = "applied" | "installed" | "available";

type ThemeCard = {
  entry: ThemeRegistryEntry | null; // null = the built-in default
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

/// Applies a resolved theme: colors map onto the app's CSS variables,
/// images become tinted backgrounds, fonts are injected as @font-face.
export function applyTheme(theme: AppliedTheme): void {
  const root = document.documentElement;
  const colors: Array<[string, string | null | undefined]> = [
    ["--bg", theme.colors.background],
    ["--panel", theme.colors.panel],
    ["--panel-2", theme.colors.panel2],
    ["--border", theme.colors.border],
    ["--text", theme.colors.text],
    ["--text-dim", theme.colors.textDim],
    ["--text-faint", theme.colors.textFaint],
    ["--cyan", theme.colors.accent],
    ["--pink", theme.colors.accentSecondary],
    ["--purple", theme.colors.purple],
    ["--green", theme.colors.green],
    ["--amber", theme.colors.amber],
    ["--red", theme.colors.red],
    ["--orange", theme.colors.orange],
    ["--blue", theme.colors.blue],
  ];
  // With a background image, panels must be semi-transparent for it to
  // show through — an opaque panel hides the image completely.
  const hasBackground = Boolean(theme.background.image);
  const panelAlpha = hasBackground ? 0.87 : 1;
  for (const [variable, value] of colors) {
    if (!value) continue;
    if (hasBackground && (variable === "--panel" || variable === "--panel-2")) {
      root.style.setProperty(variable, withAlpha(value, panelAlpha) ?? value);
    } else {
      root.style.setProperty(variable, value);
    }
  }

  const surface = (image: string | null, tint: string | null): string => {
    if (!image) return "";
    const overlay = tint ?? "rgba(0,0,0,0.6)";
    return `linear-gradient(${overlay}, ${overlay}), url("${convertFileSrc(image)}")`;
  };
  const body = document.body;
  body.style.backgroundImage = surface(theme.background.image, theme.background.tint);
  body.style.backgroundSize = "cover";
  body.style.backgroundPosition = "center";
  body.style.backgroundAttachment = "fixed";

  const sidebar = getElement<HTMLElement>("sidebar");
  const sidebarImage = surface(theme.sidebar.image, theme.sidebar.tint);
  if (sidebarImage) {
    sidebar.style.backgroundImage = sidebarImage;
    sidebar.style.backgroundSize = "cover";
    sidebar.style.backgroundPosition = "center";
  } else {
    const panel = theme.colors.panel ?? "rgba(0,0,0,0.4)";
    sidebar.style.backgroundImage = `linear-gradient(${panel}, ${panel})`;
  }

  const fontFaces: string[] = [];
  let textStack = "";
  if (theme.fonts.text) {
    fontFaces.push(
      `@font-face { font-family: "RustyThemeText"; src: url("${convertFileSrc(theme.fonts.text)}"); }`,
    );
    textStack = `"RustyThemeText", `;
  }
  let monoStack = "";
  if (theme.fonts.mono) {
    fontFaces.push(
      `@font-face { font-family: "RustyThemeMono"; src: url("${convertFileSrc(theme.fonts.mono)}"); }`,
    );
    monoStack = `"RustyThemeMono", `;
  }
  const fontStyle = getElement<HTMLStyleElement>("theme-fonts");
  fontStyle.textContent = fontFaces.join("\n");
  if (textStack) root.style.setProperty("--font", `${textStack}'Manrope', 'Segoe UI', sans-serif`);
  if (monoStack) root.style.setProperty("--font-mono", `${monoStack}'DM Mono', 'Consolas', monospace`);

  const brand = document.getElementById("brand-icon") as HTMLImageElement | null;
  if (brand) {
    // SVG files are unreliable through the asset protocol in some WebViews;
    // only raster icons are safe to swap in.
    const isRaster = theme.icons.brand?.match(/\.(png|jpe?g|webp|ico)$/i);
    if (theme.icons.brand && isRaster) {
      brand.dataset.defaultSrc = brand.dataset.defaultSrc ?? brand.src;
      brand.src = convertFileSrc(theme.icons.brand);
    } else if (brand.dataset.defaultSrc) {
      brand.src = brand.dataset.defaultSrc;
    }
  }
}

/// Restores the built-in appearance.
export function applyDefault(): void {
  document.documentElement.removeAttribute("style");
  document.body.removeAttribute("style");
  getElement<HTMLElement>("sidebar").removeAttribute("style");
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

/// Builds the card list: default, then registry entries merged with the
/// installed ones.
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
  // Installed themes missing from the registry still show up.
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

function swatches(theme: AppliedTheme | null): string[] {
  if (!theme) return ["var(--cyan)", "var(--pink)", "var(--panel-2)", "var(--text)"];
  const colors = [
    theme.colors.accent,
    theme.colors.accentSecondary,
    theme.colors.background,
    theme.colors.text,
  ];
  return colors.filter((color): color is string => Boolean(color));
}

async function refreshPanel(): Promise<void> {
  const list = getElement("themes-list");
  list.replaceChildren();
  const cards = await themeCards();
  for (const card of cards) {
    const row = document.createElement("div");
    row.className = "theme-card";

    const info = document.createElement("div");
    info.className = "theme-card-info";
    const name = document.createElement("span");
    name.className = "theme-card-name";
    name.textContent = card.entry ? card.entry.name : t("themes.defaultName");
    const author = document.createElement("span");
    author.className = "theme-card-author";
    author.textContent = card.entry
      ? `${card.entry.author}${card.theme ? ` · v${card.theme.version}` : ""}`
      : t("themes.defaultAuthor");
    const description = document.createElement("span");
    description.className = "theme-card-description";
    description.textContent = card.entry
      ? (card.theme?.description ?? card.entry.description)
      : t("themes.defaultDescription");
    info.append(name, author, description);

    const dots = document.createElement("div");
    dots.className = "theme-card-swatches";
    for (const color of swatches(card.theme)) {
      const dot = document.createElement("i");
      dot.style.background = color;
      dots.append(dot);
    }

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

    row.append(info, dots, actions);
    list.append(row);
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
  getElement("themes-refresh").addEventListener("click", () => void refreshPanel());
  void refreshPanel();
}

export function refreshThemesPanel(): void {
  void refreshPanel();
}
