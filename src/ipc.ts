/** Typed wrappers over the backend command surface. The UI never constructs a
 *  URL or a file path of its own; both come from the backend or a native dialog. */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isDesktopShell, mockInvoke } from "./devMock";
import type {
  AlertScenario, AlertView, AuroraGrid, CacheStatus, Dashboard, Evaluation,
  ExportPayload, Lesson, Settings, SnapshotRef, SourceEntry, SunImage,
} from "./types";

/** Routes to the real backend in the desktop shell, and to the frozen
 *  demonstration dataset when the page is opened in a browser for layout
 *  inspection. */
function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return isDesktopShell() ? tauriInvoke<T>(command, args) : mockInvoke<T>(command, args);
}

export const getDashboard = () => invoke<Dashboard>("get_dashboard");
export const refresh = (product?: string) => invoke<Dashboard>("refresh", { product: product ?? null });
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });
export const resetAlertSettings = () => invoke<Settings>("reset_alert_settings");
export const evaluateAlert = () => invoke<AlertView>("evaluate_alert");
export const acknowledgeEpisode = (episodeId: string) =>
  invoke<void>("acknowledge_episode", { episodeId });
export const getAuroraGrid = () => invoke<AuroraGrid>("get_aurora_grid");
export const getSunImages = (at?: string) => invoke<SunImage[]>("get_sun_images", { at: at ?? null });
export const getSunImageSequence = (
  passband: "aia193" | "aia304",
  frames: number,
  stepMinutes: number,
  at?: string,
) => invoke<SunImage[]>("get_sun_image_sequence", { passband, frames, stepMinutes, at: at ?? null });
export const listSnapshots = (product: string, limit?: number) =>
  invoke<SnapshotRef[]>("list_snapshots", { product, limit: limit ?? null });
export const enterReplay = (snapshotIds: number[]) =>
  invoke<Dashboard>("enter_replay", { snapshotIds });
export const enterDemo = () => invoke<Dashboard>("enter_demo");
export const exitReplay = () => invoke<Dashboard>("exit_replay");
export const getAlertScenarios = () => invoke<AlertScenario[]>("get_alert_scenarios");
export const evaluateScenario = (scenarioId: string) =>
  invoke<Evaluation[]>("evaluate_scenario", { scenarioId });
export const exportSeries = (seriesKeys: string[], start?: string, end?: string) =>
  invoke<ExportPayload>("export_series", { seriesKeys, start: start ?? null, end: end ?? null });
export const writeExport = (path: string, contents: string) =>
  invoke<string>("write_export", { path, contents });
export const writeExportBinary = (path: string, base64Contents: string) =>
  invoke<string>("write_export_binary", { path, base64Contents });
export const getSources = () => invoke<SourceEntry[]>("get_sources");
export const getLessons = () => invoke<Lesson[]>("get_lessons");
export const cacheStatus = () => invoke<CacheStatus>("cache_status");
export const clearCache = () => invoke<CacheStatus>("clear_cache");

/** Save through the platform's own dialog; the UI never types a path. */
export async function saveThroughDialog(
  suggestedName: string,
  extension: "csv" | "json" | "png" | "svg",
  contents: string,
): Promise<string | null> {
  if (!isDesktopShell()) throw new Error("export runs in the desktop application");
  const path = await save({
    defaultPath: `${suggestedName}.${extension}`,
    filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
  });
  if (!path) return null;
  return writeExport(path, contents);
}

/** Save a PNG rendered by the frontend through the native dialog. */
export async function savePngThroughDialog(suggestedName: string, dataUrl: string): Promise<string | null> {
  if (!isDesktopShell()) throw new Error("export runs in the desktop application");
  const path = await save({
    defaultPath: `${suggestedName}.png`,
    filters: [{ name: "PNG", extensions: ["png"] }],
  });
  if (!path) return null;
  const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");
  return writeExportBinary(path, base64);
}

/** Only verified https links from the backend's own source registry are
 *  opened, and always in the system browser rather than in the app window. */
export async function openExternal(url: string): Promise<void> {
  if (!url.startsWith("https://")) throw new Error("refusing to open a non-https link");
  if (!isDesktopShell()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(url);
}
