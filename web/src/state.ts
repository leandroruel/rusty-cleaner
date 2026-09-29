import { getLocale, number, t } from "./i18n";

export type FeatureKey =
  | "orphan"
  | "temp"
  | "chat-media"
  | "trash"
  | "browser"
  | "duplicates"
  | "large-old"
  | "registry";

export type Finding = {
  feature: FeatureKey;
  name: string;
  path: string;
  size: number;
  ageDays: number | null;
  meta: string | null;
};

export type ScanResult = {
  findings: Finding[];
  elapsedMs: number;
  platform: string;
};

export type TrashResult = {
  trashed: string[];
  failed: { path: string; error: string }[];
};

export type EmptyTrashResult = {
  removed: number;
  failed: number;
};

export type RegistryFixResult = {
  fixed: string[];
  failed: { path: string; error: string }[];
};

export type SystemMetrics = {
  cpuPercent: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
};

export const featureKeys: FeatureKey[] = [
  "orphan",
  "temp",
  "chat-media",
  "trash",
  "browser",
  "duplicates",
  "large-old",
  "registry",
];

export function featureLabels(): Record<FeatureKey, string> {
  return Object.fromEntries(featureKeys.map((key) => [key, t(`feature.${key}`)])) as Record<FeatureKey, string>;
}

export const featureColors: Record<FeatureKey, string> = {
  orphan: "var(--purple)",
  temp: "var(--cyan)",
  "chat-media": "var(--pink)",
  trash: "var(--red)",
  browser: "var(--amber)",
  duplicates: "var(--green)",
  "large-old": "var(--blue)",
  registry: "var(--orange)",
};

export const state = {
  findings: [] as Finding[],
  activeFilter: "all",
  searchQuery: "",
  sortMode: "size-desc",
  selectedPaths: new Set<string>(),
  scanning: false,
  scanCancelled: false,
  currentScanFeature: null as FeatureKey | null,
};

export function getElement<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing required UI element: #${id}`);
  return element as T;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** unitIndex;
  return `${new Intl.NumberFormat(getLocale(), { maximumFractionDigits: unitIndex === 0 ? 0 : 1 }).format(value)} ${units[unitIndex]}`;
}

let toastTimer = 0;

export function showToast(message: string): void {
  const toast = getElement<HTMLDivElement>("toast");
  toast.textContent = message;
  toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { toast.hidden = true; }, 4500);
}

export function platformName(platform: string): string {
  if (platform === "linux") return "Linux";
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  return "System";
}

export function formatCount(value: number): string {
  return number(value);
}
