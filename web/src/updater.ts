import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

export function initUpdater(): void {
  const badge = document.getElementById("update-badge") as HTMLButtonElement | null;
  const label = document.getElementById("update-badge-label");
  if (!badge || !label) return;

  let installing = false;

  void (async () => {
    try {
      const version = await invoke<string | null>("check_for_update");
      if (version) {
        label.textContent = t("update.available", { version });
        badge.classList.remove("hidden");
      }
    } catch {
      // Sem rede ou endpoint indisponível — ignora silenciosamente.
    }
  })();

  badge.addEventListener("click", async () => {
    if (installing) return;
    installing = true;
    badge.disabled = true;
    label.textContent = t("update.installing");
    try {
      await invoke("install_update");
      // O app reinicia automaticamente após instalar.
    } catch {
      label.textContent = t("update.failed");
      badge.disabled = false;
      installing = false;
    }
  });
}
