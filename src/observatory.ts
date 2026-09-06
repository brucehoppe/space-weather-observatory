/** The Observatory view: readings strip, "What this means for you", the
 *  synchronized chart stack and the detail/status side panel. */
import { ChartStack, type Panel } from "./chart";
import { append, button, el } from "./dom";
import {
  bzOrientation, fmtAge, fmtDuration, fmtFlux, fmtInZone, fmtUtc, fmtWithUnit, fluxClass, NO_VALUE,
} from "./format";
import type { AppState } from "./state";
import { datasetNow, INTERVAL_PRESETS } from "./state";
import type { Dashboard, Observation, ProductStatus, Series, Statement } from "./types";
import { SERIES } from "./types";

// --- Readings strip -----------------------------------------------------------

function lastAccepted(series: Series | undefined): Observation | null {
  if (!series) return null;
  for (let i = series.samples.length - 1; i >= 0; i--) {
    const s = series.samples[i]!;
    if (s.value !== null && s.quality !== "missing") return s;
  }
  return null;
}

/** Newest accepted interval whose start is at or before `now`. An interval
 *  the provider has already stamped but which has not begun cannot be the
 *  "current" reading. */
function latestIntervalStartedBy(series: Series | undefined, now: number): Observation | null {
  if (!series) return null;
  let best: Observation | null = null;
  for (const s of series.samples) {
    if (s.value === null || s.quality === "missing") continue;
    if (new Date(s.time).getTime() <= now) best = s;
  }
  return best;
}

function isStale(obs: Observation | null, cadenceSeconds: number, now: number): boolean {
  if (!obs) return true;
  return now - new Date(obs.time).getTime() > cadenceSeconds * 5000;
}

/** Speed gauge: a compact arc that moves only between real new readings. */
function speedGauge(value: number | null, stale: boolean): HTMLCanvasElement {
  const canvas = el("canvas", { class: "gauge", "aria-hidden": "true" }) as HTMLCanvasElement;
  queueMicrotask(() => {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth || 160;
    const h = 42;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const cx = w / 2;
    const cy = h - 4;
    const r = Math.min(w / 2 - 6, h - 8);
    // Neutral track: speed alone is not a danger scale, so no red zone.
    ctx.lineWidth = 6;
    ctx.strokeStyle = "#232a38";
    ctx.beginPath();
    ctx.arc(cx, cy, r, Math.PI, 2 * Math.PI);
    ctx.stroke();
    if (value !== null) {
      const lo = 250;
      const hi = 900;
      const f = Math.min(1, Math.max(0, (value - lo) / (hi - lo)));
      ctx.strokeStyle = stale ? "#6e7889" : "#7cc0ff";
      ctx.beginPath();
      ctx.arc(cx, cy, r, Math.PI, Math.PI + Math.PI * f);
      ctx.stroke();
    }
  });
  return canvas;
}

/** Recent-history sparkline drawn from real samples only. */
function sparkline(series: Series | undefined, colour: string, windowMs = 3 * 3600_000): HTMLCanvasElement {
  const canvas = el("canvas", { class: "sparkline", "aria-hidden": "true" }) as HTMLCanvasElement;
  queueMicrotask(() => {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth || 160;
    const h = 22;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx || !series) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const end = series.samples.length ? new Date(series.samples[series.samples.length - 1]!.time).getTime() : Date.now();
    const start = end - windowMs;
    const points = series.samples
      .filter((s) => s.value !== null && s.quality !== "missing")
      .map((s) => ({ t: new Date(s.time).getTime(), v: s.value as number }))
      .filter((p) => p.t >= start);
    if (points.length < 2) return;
    const min = Math.min(...points.map((p) => p.v));
    const max = Math.max(...points.map((p) => p.v));
    ctx.strokeStyle = colour;
    ctx.lineWidth = 1.25;
    ctx.beginPath();
    points.forEach((p, i) => {
      const x = ((p.t - start) / (end - start || 1)) * w;
      const y = h - 2 - ((p.v - min) / (max - min || 1)) * (h - 4);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  });
  return canvas;
}

function readingCard(opts: {
  label: string;
  value: string;
  unit?: string;
  meta: string[];
  stale: boolean;
  extra?: HTMLElement;
}): HTMLElement {
  return el("div", { class: `reading${opts.stale ? " is-stale" : ""}` },
    el("div", { class: "label" }, opts.label),
    el("div", { class: "value" }, opts.value, opts.unit ? el("span", { class: "unit" }, opts.unit) : null),
    opts.extra ?? null,
    el("div", { class: "meta" }, ...opts.meta.map((m) => el("span", {}, m)),
      opts.stale ? el("span", { class: "stale-tag" }, "stale") : null),
  );
}

export function renderReadings(state: AppState): HTMLElement {
  const strip = el("section", { class: "readings", "aria-label": "Current readings" });
  const d = state.dashboard;
  if (!d) return strip;
  const now = datasetNow(state);

  const speed = d.series[SERIES.speed];
  const speedObs = lastAccepted(speed);
  const speedStale = isStale(speedObs, speed?.nominal_cadence_seconds ?? 60, now);
  strip.append(readingCard({
    label: "Solar-wind speed",
    value: speedObs?.value != null ? speedObs.value.toFixed(0) : NO_VALUE,
    unit: "km/s",
    meta: [fmtAge(speedObs?.time, new Date(now)), d.wind_spacecraft ?? "no active stream"],
    stale: speedStale,
    extra: el("div", {}, speedGauge(speedObs?.value ?? null, speedStale), sparkline(speed, "#7cc0ff")),
  }));

  const bz = d.series[SERIES.bzGsm];
  const bzObs = lastAccepted(bz);
  const bzStale = isStale(bzObs, bz?.nominal_cadence_seconds ?? 60, now);
  strip.append(readingCard({
    label: "Bz (GSM)",
    value: bzObs?.value != null ? bzObs.value.toFixed(1) : NO_VALUE,
    unit: "nT",
    meta: [bzOrientation(bzObs?.value), `zero reference · ${fmtAge(bzObs?.time, new Date(now))}`],
    stale: bzStale,
    extra: sparkline(bz, "#e8a33d"),
  }));

  const bt = d.series[SERIES.bt];
  const btObs = lastAccepted(bt);
  strip.append(readingCard({
    label: "Total field |B|",
    value: btObs?.value != null ? btObs.value.toFixed(1) : NO_VALUE,
    unit: "nT",
    meta: [fmtAge(btObs?.time, new Date(now))],
    stale: isStale(btObs, bt?.nominal_cadence_seconds ?? 60, now),
  }));

  const kp = d.series[SERIES.kp];
  const kpObs = latestIntervalStartedBy(kp, now);
  strip.append(readingCard({
    label: "Planetary Kp (estimated)",
    value: kpObs?.value != null ? kpObs.value.toFixed(2) : NO_VALUE,
    meta: [
      kpObs ? `3-hour interval from ${fmtUtc(kpObs.time)}` : "no interval",
      fmtAge(kpObs?.time, new Date(now)),
    ],
    stale: isStale(kpObs, kp?.nominal_cadence_seconds ?? 10800, now),
  }));

  const xray = d.series[SERIES.xrayLong];
  const xrayObs = lastAccepted(xray);
  const cls = fluxClass(xrayObs?.value);
  strip.append(readingCard({
    label: "GOES X-ray flux (0.1–0.8 nm)",
    value: cls ?? NO_VALUE,
    meta: [
      xrayObs?.value != null ? fmtFlux(xrayObs.value) : "no valid sample",
      `${d.xray_satellite ?? "unknown satellite"} · ${fmtAge(xrayObs?.time, new Date(now))}`,
    ],
    stale: isStale(xrayObs, xray?.nominal_cadence_seconds ?? 60, now),
  }));

  return strip;
}

// --- "What this means for you" -------------------------------------------------

const BASIS_LABEL: Record<Statement["basis"], string> = {
  provider_forecast: "NOAA SWPC forecast",
  provider_observation: "NOAA SWPC observation",
  interpretation: "Interpretation",
};

export function renderMeaning(state: AppState, onToggle: () => void): HTMLElement {
  const panel = el("section", {
    class: `meaning-panel${state.meaningCollapsed ? " collapsed" : ""}`,
    "aria-label": "What this means for you",
  });
  const toggle = button(state.meaningCollapsed ? "Show" : "Hide", onToggle, "ghost");
  toggle.setAttribute("aria-expanded", String(!state.meaningCollapsed));
  panel.append(el("h2", {}, "What this means for you", toggle));
  const d = state.dashboard;
  if (!d) return panel;

  const list = el("ul", { class: "meaning-list" });
  for (const st of d.statements.slice(0, 6)) {
    const item = el("li", { class: "meaning-item" },
      el("div", { class: `basis ${st.basis === "interpretation" ? "interpretation" : ""}` }, BASIS_LABEL[st.basis]),
      el("h3", {}, st.headline),
      el("details", {},
        el("summary", {}, "Why?"),
        el("p", {}, st.detail),
      ),
      st.region ? el("div", { class: "region" }, st.region) : null,
    );
    list.append(item);
  }
  if (!d.statements.length) {
    list.append(el("li", { class: "meaning-item" },
      el("h3", {}, "No sourced status available"),
      el("p", {}, "No official status has been retrieved yet. That is a gap in information, not evidence of quiet conditions."),
    ));
  }
  panel.append(list);
  return panel;
}

// --- Chart panels ---------------------------------------------------------------

export function buildPanels(d: Dashboard): Panel[] {
  const panels: Panel[] = [];
  const s = (key: string) => d.series[key];

  if (s(SERIES.speed)) {
    panels.push({
      id: "speed",
      title: "Solar-wind speed",
      unit: "km/s",
      scale: "linear",
      note: d.wind_spacecraft ? `spacecraft ${d.wind_spacecraft}` : "no active stream",
      series: [{ series: s(SERIES.speed)!, colour: "#7cc0ff" }],
      minHeight: 90,
    });
  }
  if (s(SERIES.density)) {
    panels.push({
      id: "density",
      title: "Solar-wind proton density",
      unit: "particles/cm³",
      scale: "linear",
      series: [{ series: s(SERIES.density)!, colour: "#9d8cf0" }],
      minHeight: 70,
    });
  }
  if (s(SERIES.bzGsm) || s(SERIES.bt)) {
    const entries = [];
    if (s(SERIES.bt)) entries.push({ series: s(SERIES.bt)!, colour: "#63c98b", label: "|B|" });
    if (s(SERIES.bzGsm)) entries.push({ series: s(SERIES.bzGsm)!, colour: "#e8a33d", label: "Bz GSM" });
    if (s(SERIES.bzGse)) entries.push({ series: s(SERIES.bzGse)!, colour: "#8a7a52", dashed: true, label: "Bz GSE" });
    panels.push({
      id: "field",
      title: "Magnetic field: |B| and Bz",
      unit: "nT",
      scale: "linear",
      zeroReference: true,
      note: "Bz in GSM (solid) and GSE (dashed); zero reference shown",
      series: entries,
      minHeight: 90,
    });
  }
  if (s(SERIES.xrayLong)) {
    const entries = [{ series: s(SERIES.xrayLong)!, colour: "#e8756b", label: "0.1–0.8 nm" }];
    if (s(SERIES.xrayShort)) entries.push({ series: s(SERIES.xrayShort)!, colour: "#a4564f", label: "0.05–0.4 nm" });
    const excluded = s(SERIES.xrayLong)!.samples.filter((x) => x.quality === "missing").length;
    panels.push({
      id: "xray",
      title: "GOES X-ray flux",
      unit: "W/m² (log)",
      scale: "log",
      note: `${d.xray_satellite ?? "unknown satellite"} · ${excluded} non-positive or invalid samples excluded from the log axis`,
      series: entries,
      minHeight: 90,
    });
  }
  if (s(SERIES.kp)) {
    panels.push({
      id: "kp",
      title: "Planetary Kp",
      unit: "Kp (3-hour intervals)",
      scale: "kp",
      note: "solid = NOAA estimate/observation · hatched = NOAA forecast",
      series: [{ series: s(SERIES.kp)!, colour: "#7f9bd0" }],
      forecast: d.kp
        .filter((i) => i.kind === "predicted" && i.observation.value !== null)
        .map((i) => ({ time: i.observation.time, value: i.observation.value as number, scale: i.noaa_scale })),
      minHeight: 80,
    });
  }
  return panels;
}

// --- Side panel ----------------------------------------------------------------

export function renderSidePanel(
  state: AppState,
  chart: ChartStack | null,
  onExport: () => void,
  onExportChart: () => void = () => {},
  onExportChartSvg: () => void = () => {},
): HTMLElement {
  const panel = el("aside", { class: "side-panel", "aria-label": "Selection details and source status" });
  const d = state.dashboard;
  if (!d) return panel;

  panel.append(renderSelectionDetail(state, chart, onExport, onExportChart, onExportChartSvg));
  panel.append(renderStatuses(d, state));
  panel.append(renderOutlook(d));
  panel.append(renderBulletins(d));
  return panel;
}

function renderSelectionDetail(
  state: AppState,
  _chart: ChartStack | null,
  onExport: () => void,
  onExportChart: () => void,
  onExportChartSvg: () => void,
): HTMLElement {
  const section = el("section", { "aria-label": "Selected measurement" });
  section.append(el("h2", {}, state.selection.pinned ? "Pinned measurement" : "Latest measurement"));
  const d = state.dashboard!;
  const key = state.focusSeries ?? SERIES.speed;
  const series = d.series[key];
  if (!series) {
    section.append(el("p", { class: "note" }, "This series is not available in the current dataset."));
    return section;
  }

  // Following "now" shows the series' own newest accepted sample. A pinned
  // time is looked up with the documented tolerance and never substituted.
  const tolerance = series.nominal_cadence_seconds * 1000;
  const obs = state.selection.pinned
    ? ChartStack.nearest(series, state.selection.time, tolerance)
    : lastAccepted(series);

  section.append(
    el("div", {}, el("strong", {}, series.label), series.frame ? ` (${series.frame})` : ""),
  );

  if (!obs) {
    section.append(
      el("p", { class: "warn-text" }, "No nearby sample"),
      el("p", { class: "note" },
        `The nearest ${series.label.toLowerCase()} sample is further than the lookup tolerance of ` +
        `${fmtDuration(series.nominal_cadence_seconds)} from the selected time. No substitute value is shown.`),
    );
  } else {
    const isFlux = key === SERIES.xrayLong || key === SERIES.xrayShort;
    append(section,
      el("div", { class: "detail-value" },
        isFlux ? fmtFlux(obs.value) : fmtWithUnit(obs.value, series.unit, obs.value != null && Math.abs(obs.value) < 10 ? 2 : 1)),
      key === SERIES.xrayLong && fluxClass(obs.value)
        ? el("div", { class: "note" }, `Instantaneous flux class ${fluxClass(obs.value)} on the 0.1–0.8 nm passband. This is not an officially identified flare event.`)
        : null,
      el("dl", { class: "detail-rows" },
        el("dt", {}, "Time"), el("dd", {}, fmtUtc(obs.time, true)),
        ...(state.settings && state.settings.display_time_zone !== "UTC"
          ? [el("dt", {}, "Local"), el("dd", {}, fmtInZone(obs.time, state.settings.display_time_zone, true))]
          : []),
        el("dt", {}, "Age"), el("dd", {}, fmtAge(obs.time, new Date(datasetNow(state)))),
        el("dt", {}, "Quality"), el("dd", {}, obs.quality),
        el("dt", {}, "Instrument"), el("dd", {}, obs.instrument ?? NO_VALUE),
        el("dt", {}, "Interval"), el("dd", {}, obs.interval_seconds ? fmtDuration(obs.interval_seconds) : "instantaneous"),
        el("dt", {}, "Data"), el("dd", {}, series.aggregation.kind === "raw" ? "raw" : "aggregated"),
        el("dt", {}, "Retrieved"), el("dd", {}, fmtUtc(series.provenance.retrieved_at)),
      ),
    );
  }

  const picker = el("select", { "aria-label": "Series to inspect" }) as HTMLSelectElement;
  for (const [k, s] of Object.entries(d.series)) {
    const option = el("option", { value: k }, `${s.label}${s.frame ? ` (${s.frame})` : ""}`) as HTMLOptionElement;
    if (k === key) option.selected = true;
    picker.append(option);
  }
  section.append(el("div", { style: "margin-top:.5rem" }, picker));
  section.append(el("div", { style: "margin-top:.4rem;display:flex;gap:.4rem;flex-wrap:wrap" },
    button("Export data (CSV)", onExport, "ghost"),
    button("Export chart (PNG)", onExportChart, "ghost"),
    button("Export chart (SVG)", onExportChartSvg, "ghost")));
  picker.addEventListener("change", () => {
    picker.dispatchEvent(new CustomEvent("series-change", { detail: picker.value, bubbles: true }));
  });
  return section;
}

function renderStatuses(d: Dashboard, state: AppState): HTMLElement {
  const section = el("section", { "aria-label": "Source status" });
  section.append(el("h2", {}, "Source status"));
  const list = el("ul", { class: "status-list" });
  const now = new Date(datasetNow(state));
  for (const s of d.statuses) {
    list.append(statusRow(s, now));
  }
  section.append(list);
  return section;
}

function statusRow(s: ProductStatus, now: Date): HTMLElement {
  const row = el("li", { class: "status-row" },
    el("span", { class: `dot ${s.state}`, "aria-hidden": "true" }),
    el("span", {}, s.product, s.message ? el("div", { class: "note" }, s.message) : null),
    el("span", { class: "age" }, s.state === "unavailable" ? "unavailable" : fmtAge(s.last_sample_time ?? s.last_success, now)),
  );
  row.setAttribute("title", `${s.product}: ${s.state}`);
  return row;
}

function renderOutlook(d: Dashboard): HTMLElement {
  const section = el("section", { "aria-label": "Official outlook" });
  section.append(el("h2", {}, "Official outlook"));
  const f = d.three_day;
  if (!f) {
    section.append(el("p", { class: "note" },
      "Official outlook unavailable. That is a gap in information, not evidence of quiet conditions."));
    return section;
  }
  section.append(
    el("div", { class: "note" }, `NOAA 3-day forecast issued ${fmtUtc(f.issued_at)}`),
  );
  const days = new Map<string, { max: number; scale: string | null }>();
  for (const cell of f.kp) {
    const day = cell.interval_start.slice(0, 10);
    const prev = days.get(day);
    if (!prev || cell.kp > prev.max) days.set(day, { max: cell.kp, scale: cell.noaa_scale });
  }
  const table = el("table", { class: "table" },
    el("tr", {}, el("th", {}, "UTC day"), el("th", {}, "Highest forecast Kp"), el("th", {}, "NOAA label")));
  for (const [day, v] of days) {
    table.append(el("tr", {}, el("td", {}, day), el("td", {}, v.max.toFixed(2)), el("td", {}, v.scale ?? "—")));
  }
  section.append(table);
  if (f.rationale) section.append(el("p", { class: "note" }, f.rationale));
  section.append(el("p", { class: "note" },
    "Forecast values describe possible conditions in the stated period. They are not observations and may not occur."));
  return section;
}

function renderBulletins(d: Dashboard): HTMLElement {
  const section = el("section", { "aria-label": "Alerts, watches and warnings" });
  section.append(el("h2", {}, "Alerts, watches and warnings"));
  const shown = d.bulletins.slice(0, 8);
  if (!shown.length) {
    section.append(el("p", { class: "note" },
      "No bulletins retrieved. A missing bulletin is not confirmation of quiet conditions."));
    return section;
  }
  for (const b of shown) {
    const card = el("div", { class: "bulletin" },
      el("h3", {}, b.headline),
      el("div", { class: "meta" },
        el("span", { class: `status-chip ${b.status}` }, b.status),
        ` ${b.product_id}${b.serial ? ` · serial ${b.serial}` : ""} · issued ${fmtUtc(b.issued_at)}`,
        b.valid_to ? ` · valid until ${fmtUtc(b.valid_to)}` : "",
      ),
      b.scale ? el("div", { class: "note" }, `NOAA scale ${b.scale.domain}${b.scale.level}`) : null,
      el("details", {}, el("summary", {}, "Full text"), el("pre", {}, b.text)),
    );
    section.append(card as HTMLElement);
  }
  return section;
}

// --- Timeline ------------------------------------------------------------------

export interface TimelineCallbacks {
  onRange: (range: { start: number; end: number }) => void;
  onSelect: (time: number) => void;
  onTogglePause: () => void;
  onReturnToNow: () => void;
}

export function renderTimeline(state: AppState, cb: TimelineCallbacks): HTMLElement {
  const bar = el("section", { class: "timeline", "aria-label": "Timeline" });
  const now = datasetNow(state);

  bar.append(button(state.paused ? "Resume" : "Pause", cb.onTogglePause, "ghost"));

  const presets = el("div", { class: "presets" });
  for (const p of INTERVAL_PRESETS) {
    const span = state.range.end - state.range.start;
    const active = Math.abs(span - p.ms) < p.ms * 0.05;
    const b = button(p.label, () => cb.onRange({ start: state.range.end - p.ms, end: state.range.end }),
      active ? "primary" : "ghost");
    presets.append(b);
  }
  bar.append(presets);

  const scrub = el("input", {
    type: "range",
    class: "scrub",
    min: String(state.range.start),
    max: String(state.range.end),
    step: "60000",
    value: String(Math.min(Math.max(state.selection.time, state.range.start), state.range.end)),
    "aria-label": "Selected time",
  }) as HTMLInputElement;
  scrub.addEventListener("input", () => cb.onSelect(Number(scrub.value)));
  bar.append(scrub);

  bar.append(el("span", { class: "note" },
    `${fmtUtc(new Date(state.selection.time).toISOString(), true)}${state.selection.pinned ? " (pinned)" : ""}`));

  bar.append(button("Return to now", cb.onReturnToNow, "ghost"));
  bar.append(el("span", { class: "note" }, `latest data ${fmtUtc(new Date(now).toISOString())}`));
  return bar;
}
