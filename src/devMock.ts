/** Browser-preview shim.
 *
 *  When the page is opened in an ordinary browser rather than the desktop
 *  shell, there is no Tauri IPC. This module serves the *frozen demonstration
 *  dataset* — the same JSON the backend would return — so layout, interaction,
 *  keyboard operation and narrow-width behaviour can be inspected in a browser.
 *
 *  This is a layout and interaction preview only. It is not a test of the
 *  desktop application, and never of the Windows build. It is inert inside the
 *  desktop shell, where the real backend answers every call.
 */

export const isDesktopShell = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const cache = new Map<string, unknown>();

async function fixture<T>(name: string): Promise<T> {
  if (!cache.has(name)) {
    const res = await fetch(`/dev-fixtures/${name}.json`);
    if (!res.ok) throw new Error(`preview fixture ${name} unavailable`);
    cache.set(name, await res.json());
  }
  return cache.get(name) as T;
}

/** Answers a command the way the backend would, from the frozen dataset. */
export async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "get_dashboard":
    case "refresh":
    case "enter_demo":
    case "exit_replay":
    case "enter_replay":
      return fixture<T>("dashboard");
    case "get_settings":
    case "save_settings":
    case "reset_alert_settings":
      return (args?.settings as T) ?? fixture<T>("settings");
    case "get_sources":
      return fixture<T>("sources");
    case "get_lessons":
      return fixture<T>("lessons");
    case "get_alert_scenarios":
      return fixture<T>("scenarios");
    case "get_aurora_grid":
      return fixture<T>("aurora");
    case "evaluate_alert": {
      const evaluation = await fixture<unknown>("alert");
      const settings = await fixture<Record<string, unknown>>("settings");
      return {
        evaluation,
        settings: settings.alert,
        context: "demo",
        source_product: "rtsw_wind_1m",
        source_spacecraft: "SOLAR1",
        bz_gsm_nt: null,
        bz_time: null,
        bz_stale: true,
      } as T;
    }
    case "acknowledge_episode":
      return undefined as T;
    case "list_snapshots":
      return [] as unknown as T;
    case "cache_status":
      return {
        size_bytes: 0, limit_bytes: 0, snapshot_count: 0,
        data_dir: "(browser preview: no local store)",
        config_dir: "(browser preview: no local store)",
      } as T;
    case "clear_cache":
      return mockInvoke<T>("cache_status");
    case "get_sun_images":
    case "get_sun_image_sequence":
      throw new Error("solar imagery is not available in browser preview");
    case "evaluate_scenario":
      throw new Error("scenario evaluation runs in the desktop backend");
    case "export_series":
    case "write_export":
      throw new Error("export runs in the desktop backend");
    default:
      throw new Error(`browser preview does not implement ${command}`);
  }
}
