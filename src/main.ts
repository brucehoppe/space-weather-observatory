/** Application shell: navigation, layout, the shared time selection, and the
 *  refresh loop. Uses the entire desktop content area — there is no fixed-width
 *  centred wrapper anywhere in this application (spec §6). */
import "./styles.css";
import { openAlertExplanation, openAlertSettings, renderAlertBanner, setShowDismissed } from "./alertBanner";
import { renderAuroraView } from "./aurora";
import { ChartStack, renderChartExport } from "./chart";
import { announce, button, clear, el } from "./dom";
import { fmtUtc } from "./format";
import * as ipc from "./ipc";
import { renderLearnView } from "./learn";
import { buildPanels, renderMeaning, renderReadings, renderSidePanel, renderTimeline } from "./observatory";
import { renderSourcesView } from "./sources";
import { datasetNow, Store, type ViewName } from "./state";
import type { Lesson } from "./types";
import { SERIES } from "./types";

const store = new Store();
let chart: ChartStack | null = null;
let chartCanvas: HTMLCanvasElement | null = null;

const root = document.getElementById("app");
if (!root) throw new Error("application root missing");

// --- Rendering ----------------------------------------------------------------

function render(): void {
  const state = store.get();
  document.body.classList.toggle("reduced-motion", state.reducedMotion);
  clear(root!);
  root!.setAttribute("aria-busy", state.dashboard ? "false" : "true");

  root!.append(renderHeader());
  root!.append(renderAlertBanner(state.alert, state.dashboard, {
    onDismiss: (id) => {
      void ipc.acknowledgeEpisode(id).then(refreshAlert);
    },
    onConfigure: () => {
      if (state.settings) openAlertSettings(state.settings, (saved) => {
        store.set({ settings: saved });
        void refreshAlert();
      });
    },
    onExplain: () => {
      if (state.settings) openAlertExplanation(state.settings.alert);
    },
    onShowDismissed: () => render(),
  }, new Date(datasetNow(state))));

  if (state.view === "observatory") {
    root!.append(renderReadings(state));
    root!.append(renderWorkspace());
    root!.append(renderTimeline(state, {
      onRange: (range) => { store.set({ range }); chart?.setRange(range); },
      onSelect: (time) => setSelection(time, true),
      onTogglePause: () => store.set({ paused: !state.paused }),
      onReturnToNow: returnToNow,
    }));
  } else {
    const container = el("div", { style: "min-height:0;overflow:auto;grid-row:span 3" });
    if (state.view === "aurora") container.append(renderAuroraView(state));
    if (state.view === "learn") container.append(renderLearnView(state, {
      onStartLesson: startLesson,
      onLessonStep: applyLessonStep,
      onResetLesson: resetLesson,
      onNavigate: navigate,
    }));
    if (state.view === "sources") container.append(renderSourcesView(state));
    root!.append(container);
  }

  root!.append(renderFooter());
  if (state.error) announce(state.error);
}

function renderHeader(): HTMLElement {
  const state = store.get();
  const nav = el("nav", { class: "nav", "aria-label": "Main views" });
  const views: [ViewName, string][] = [
    ["observatory", "Observatory"],
    ["aurora", "Aurora"],
    ["learn", "Learn"],
    ["sources", "Sources"],
  ];
  for (const [id, label] of views) {
    const b = button(label, () => navigate(id), "");
    if (state.view === id) b.setAttribute("aria-current", "page");
    nav.append(b);
  }

  const mode = state.dashboard?.mode ?? "live";
  const modeBadge = el("span", { class: `mode-badge ${mode}` }, mode === "live" ? "Live" : mode === "demo" ? "Demonstration" : "Replay");

  const right = el("div", { class: "header-right" },
    modeBadge,
    el("span", { class: "displayed-time" }, fmtUtc(new Date(state.selection.time).toISOString(), true)),
    mode === "live"
      ? button("Demonstration dataset", () => void enterDemo(), "ghost")
      : button("Return to live", () => void exitReplay(), "primary"),
    button("Refresh", () => void refreshAll(), "ghost"),
  );

  return el("header", { class: "app-header" },
    el("div", { class: "brand" }, "Space Weather Observatory",
      el("span", { class: "tagline" }, "Follow the Sun. Understand its influence.")),
    nav,
    right,
  );
}

function renderWorkspace(): HTMLElement {
  const state = store.get();
  const workspace = el("div", {
    class: `workspace${state.sideCollapsed ? " side-collapsed" : ""}`,
    style: `--side-width:${state.sideWidth}px`,
  });

  const centre = el("div", { class: "centre-stack" });
  centre.append(renderMeaning(state, () => {
    store.set({ meaningCollapsed: !store.get().meaningCollapsed });
    render();
  }));

  const charts = el("div", { class: "charts" });
  chartCanvas = el("canvas", {
    tabindex: "0",
    role: "application",
    "aria-label": "Synchronized scientific charts. Use arrow keys to move the selected time, plus and minus to zoom, Escape to unpin.",
  }) as HTMLCanvasElement;
  charts.append(chartCanvas);
  centre.append(charts);

  const splitter = el("div", {
    class: "splitter",
    role: "separator",
    tabindex: "0",
    "aria-orientation": "vertical",
    "aria-label": "Resize the side panel",
    "aria-valuenow": String(state.sideWidth),
  });
  attachSplitter(splitter);

  workspace.append(centre, splitter, renderSidePanel(state, chart, exportSelection, exportChart));
  workspace.addEventListener("series-change", (e) => {
    store.set({ focusSeries: (e as CustomEvent<string>).detail });
    render();
  });

  queueMicrotask(mountChart);
  return workspace;
}

function renderFooter(): HTMLElement {
  const state = store.get();
  const version = state.dashboard?.app_version ?? "0.1.0";
  const footer = el("footer", { class: "app-footer" });
  footer.append(el("span", {}, `Space Weather Observatory ${version}`));
  for (const [label, url] of [
    ["NOAA SWPC", "https://www.swpc.noaa.gov/"],
    ["Real-time solar wind", "https://www.swpc.noaa.gov/products/real-time-solar-wind"],
    ["GOES X-ray flux", "https://www.swpc.noaa.gov/products/goes-x-ray-flux"],
    ["Planetary K-index", "https://www.swpc.noaa.gov/products/planetary-k-index"],
    ["Aurora forecast", "https://www.spaceweather.gov/products/aurora-30-minute-forecast"],
    ["NOAA scales", "https://www.spaceweather.gov/noaa-scales-explanation"],
  ] as [string, string][]) {
    const a = el("a", { href: "#" }, label);
    a.addEventListener("click", (e) => { e.preventDefault(); void ipc.openExternal(url); });
    footer.append(a);
  }
  footer.append(el("span", {}, "Sun imagery: NASA/SDO via Helioviewer · Coastlines: Natural Earth"));
  footer.append(button("All sources", () => navigate("sources"), "ghost"));
  return footer;
}

// --- Chart wiring ---------------------------------------------------------------

function mountChart(): void {
  const state = store.get();
  if (!chartCanvas || !state.dashboard) return;
  chart = new ChartStack(chartCanvas);
  chart.setReducedMotion(state.reducedMotion);
  chart.setRange(state.range);
  chart.setSelection(state.selection);
  chart.setPanels(buildPanels(state.dashboard));
  chart.onSelectionChange((sel) => {
    // Inspecting history never retargets the live detector: this updates the
    // shared selection only.
    store.set({ selection: sel, paused: sel.pinned ? true : store.get().paused });
    renderSideOnly();
  });
  chart.onRangeChange((range) => {
    store.set({ range });
    renderSideOnly();
  });
}

/** Re-render only the parts that depend on the selection, so moving the
 *  crosshair does not rebuild (or re-announce) the whole page. */
function renderSideOnly(): void {
  const state = store.get();
  const old = root!.querySelector(".side-panel");
  if (old) old.replaceWith(renderSidePanel(state, chart, exportSelection, exportChart));
  const timeline = root!.querySelector(".timeline");
  if (timeline) {
    timeline.replaceWith(renderTimeline(state, {
      onRange: (range) => { store.set({ range }); chart?.setRange(range); renderSideOnly(); },
      onSelect: (time) => setSelection(time, true),
      onTogglePause: () => { store.set({ paused: !store.get().paused }); renderSideOnly(); },
      onReturnToNow: returnToNow,
    }));
  }
  const displayed = root!.querySelector(".displayed-time");
  if (displayed) displayed.textContent = fmtUtc(new Date(state.selection.time).toISOString(), true);
}

function setSelection(time: number, pinned: boolean): void {
  store.set({ selection: { time, pinned }, paused: pinned ? true : store.get().paused });
  chart?.setSelection({ time, pinned });
  renderSideOnly();
}

function returnToNow(): void {
  const state = store.get();
  const now = datasetNow(state);
  const span = state.range.end - state.range.start;
  store.set({ selection: { time: now, pinned: false }, range: { start: now - span, end: now }, paused: false });
  chart?.setRange(store.get().range);
  chart?.setSelection(store.get().selection);
  render();
}

function attachSplitter(splitter: HTMLElement): void {
  let dragging = false;
  splitter.addEventListener("pointerdown", (e) => {
    dragging = true;
    splitter.setPointerCapture(e.pointerId);
  });
  splitter.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const width = Math.min(720, Math.max(240, window.innerWidth - e.clientX));
    store.set({ sideWidth: width });
    const workspace = root!.querySelector(".workspace") as HTMLElement | null;
    workspace?.style.setProperty("--side-width", `${width}px`);
    chart?.render();
  });
  splitter.addEventListener("pointerup", () => { dragging = false; });
  // Keyboard-operable splitter, plus collapse/expand.
  splitter.addEventListener("keydown", (e) => {
    const state = store.get();
    let width = state.sideWidth;
    if (e.key === "ArrowLeft") width = Math.min(720, width + 24);
    else if (e.key === "ArrowRight") width = Math.max(240, width - 24);
    else if (e.key === "Enter" || e.key === " ") {
      store.set({ sideCollapsed: !state.sideCollapsed });
      render();
      return;
    } else return;
    e.preventDefault();
    store.set({ sideWidth: width });
    render();
  });
}

// --- Data flow ------------------------------------------------------------------

async function refreshAll(): Promise<void> {
  try {
    const dashboard = await ipc.refresh();
    applyDashboard(dashboard);
  } catch (e) {
    store.set({ error: `Refresh failed: ${String(e)}` });
    render();
  }
}

function applyDashboard(dashboard: import("./types").Dashboard): void {
  const state = store.get();
  const now = datasetNow({ ...state, dashboard });
  const span = state.range.end - state.range.start || 6 * 3600_000;
  const patch: Partial<import("./state").AppState> = { dashboard, error: null };
  if (!state.paused && !state.selection.pinned) {
    patch.selection = { time: now, pinned: false };
    patch.range = { start: now - span, end: now };
  }
  store.set(patch);
  render();
  void refreshAlert();
}

async function refreshAlert(): Promise<void> {
  try {
    const alert = await ipc.evaluateAlert();
    store.set({ alert });
    const banner = root!.querySelector(".alert-banner");
    if (banner) {
      banner.replaceWith(renderAlertBanner(alert, store.get().dashboard, {
        onDismiss: (id) => { void ipc.acknowledgeEpisode(id).then(refreshAlert); },
        onConfigure: () => {
          const s = store.get().settings;
          if (s) openAlertSettings(s, (saved) => { store.set({ settings: saved }); void refreshAlert(); });
        },
        onExplain: () => {
          const s = store.get().settings;
          if (s) openAlertExplanation(s.alert);
        },
        onShowDismissed: () => render(),
      }, new Date(datasetNow(store.get()))));
    }
  } catch (e) {
    store.set({ error: `Alert evaluation failed: ${String(e)}` });
  }
}

async function enterDemo(): Promise<void> {
  setShowDismissed(false);
  const dashboard = await ipc.enterDemo();
  store.set({ paused: true });
  applyDashboardForced(dashboard);
}

async function exitReplay(): Promise<void> {
  const dashboard = await ipc.exitReplay();
  store.set({ paused: false, activeLesson: null });
  applyDashboardForced(dashboard);
}

function applyDashboardForced(dashboard: import("./types").Dashboard): void {
  const now = datasetNow({ ...store.get(), dashboard });
  store.set({
    dashboard,
    selection: { time: now, pinned: false },
    range: { start: now - 6 * 3600_000, end: now },
    error: null,
  });
  render();
  void refreshAlert();
}

function navigate(view: ViewName): void {
  store.set({ view });
  render();
}

// --- Lessons --------------------------------------------------------------------

async function startLesson(lesson: Lesson): Promise<void> {
  if (store.get().dashboard?.mode !== "demo") {
    await enterDemo();
  }
  store.set({ activeLesson: { id: lesson.id, step: 0 } });
  applyLessonStep(lesson, 0);
}

function applyLessonStep(lesson: Lesson, index: number): void {
  const step = lesson.steps[index];
  if (!step) return;
  store.set({ activeLesson: { id: lesson.id, step: index } });
  if (step.focus_series) store.set({ focusSeries: step.focus_series });
  if (step.select_time) {
    const t = new Date(step.select_time).getTime();
    store.set({ selection: { time: t, pinned: true }, range: { start: t - 3 * 3600_000, end: t + 3 * 3600_000 } });
  }
  const view = step.view === "aurora" ? "aurora" : step.view === "sources" ? "sources" : "observatory";
  store.set({ view: view as ViewName });
  render();
}

function resetLesson(): void {
  store.set({ activeLesson: null, view: "learn", focusSeries: null });
  void exitReplay();
}

// --- Export ---------------------------------------------------------------------

async function exportSelection(): Promise<void> {
  const state = store.get();
  if (!state.dashboard) return;
  const key = state.focusSeries ?? SERIES.speed;
  try {
    const payload = await ipc.exportSeries(
      [key],
      new Date(state.range.start).toISOString(),
      new Date(state.range.end).toISOString(),
    );
    const saved = await ipc.saveThroughDialog(payload.suggested_basename, "csv", payload.csv);
    if (saved) announce(`Exported to ${saved}`);
  } catch (e) {
    store.set({ error: `Export failed: ${String(e)}` });
    render();
  }
}

async function exportChart(): Promise<void> {
  const state = store.get();
  if (!chart || !state.dashboard) return;
  const d = state.dashboard;
  const sources = Array.from(new Set(Object.values(d.series).map((s) => s.provenance.source_url)));
  const units = Array.from(new Set(Object.values(d.series).map((s) => `${s.label}: ${s.unit}${s.frame ? ` (${s.frame})` : ""}`)));
  const status = `${d.mode === "live" ? "live snapshot" : d.mode === "demo" ? "frozen demonstration dataset" : "replay"} ` +
    `${d.snapshot_id.slice(0, 12)} assembled ${d.assembled_at}; raw provider values; app ${d.app_version}`;
  try {
    const canvas = renderChartExport(chart, { title: "Space Weather Observatory — synchronized charts", sources, status, units });
    const saved = await ipc.savePngThroughDialog(`space-weather-charts-${new Date(state.range.end).toISOString().slice(0, 16).replace(/[:T]/g, "")}Z`, canvas.toDataURL("image/png"));
    if (saved) announce(`Chart exported to ${saved}`);
  } catch (e) {
    store.set({ error: `Chart export failed: ${String(e)}` });
    render();
  }
}

// --- Boot -----------------------------------------------------------------------

async function boot(): Promise<void> {
  // `?view=aurora` etc. lets browser-preview screenshots open a view directly.
  const requested = new URLSearchParams(window.location.search).get("view");
  if (requested === "aurora" || requested === "learn" || requested === "sources") {
    store.set({ view: requested });
  }
  window.matchMedia("(prefers-reduced-motion: reduce)").addEventListener("change", (e) => {
    store.set({ reducedMotion: e.matches });
    chart?.setReducedMotion(e.matches);
    render();
  });

  try {
    const [dashboard, settings, sources, lessons] = await Promise.all([
      ipc.getDashboard(),
      ipc.getSettings(),
      ipc.getSources(),
      ipc.getLessons(),
    ]);
    store.set({ settings, sources, lessons, reducedMotion: settings.force_reduced_motion || store.get().reducedMotion });
    applyDashboard(dashboard);
  } catch (e) {
    store.set({ error: `Could not start: ${String(e)}` });
    render();
  }

  // Imagery is fetched separately so a slow image never delays the dashboard.
  ipc.getSunImages()
    .then((sunImages) => { store.set({ sunImages, imageryError: null }); render(); })
    .catch((e: unknown) => { store.set({ imageryError: `Solar imagery unavailable: ${String(e)}` }); });

  // Poll the backend for updated state. The backend does the actual network
  // work on each product's own cadence; this only reads what it has.
  window.setInterval(() => {
    if (store.get().dashboard?.mode !== "live") return;
    void ipc.getDashboard().then(applyDashboard).catch(() => {});
  }, 60_000);
}

store.subscribe(() => {});
void boot();
