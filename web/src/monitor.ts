import { invoke, isTauri } from "@tauri-apps/api/core";
import { formatBytes, getElement, type SystemMetrics } from "./state";

async function refreshMetrics(): Promise<void> {
  if (!isTauri()) return;
  try {
    const metrics = await invoke<SystemMetrics>("system_metrics");
    getElement("cpu-count").textContent = `${Math.round(metrics.cpuPercent)}%`;
    getElement("cpu-bar").style.width = `${Math.min(metrics.cpuPercent, 100)}%`;
    getElement("memory-count").textContent = `${formatBytes(metrics.memoryUsed)} / ${formatBytes(metrics.memoryTotal)}`;
    getElement("memory-bar").style.width = metrics.memoryTotal > 0 ? `${(metrics.memoryUsed / metrics.memoryTotal) * 100}%` : "0%";
    getElement("disk-count").textContent = `${formatBytes(metrics.diskUsed)} / ${formatBytes(metrics.diskTotal)}`;
    getElement("disk-bar").style.width = metrics.diskTotal > 0 ? `${(metrics.diskUsed / metrics.diskTotal) * 100}%` : "0%";
  } catch {
    // Keep the last values if a read fails.
  }
}

export function initMonitor(): void {
  void refreshMetrics();
  window.setInterval(() => void refreshMetrics(), 3000);
}
