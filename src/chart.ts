/** Canvas chart stack with a shared time axis (spec §7).
 *
 *  Behaviours this implements deliberately:
 *  - lines break across data gaps and missing samples; they are never bridged;
 *  - the log axis excludes non-positive values and says how many it excluded;
 *  - interval-valued series (Kp) draw as intervals, not as a sampled curve;
 *  - forecast intervals are drawn distinctly from observed ones;
 *  - one crosshair and one selected time are shared by every panel;
 *  - keyboard navigation moves the selection; pointer movement does not
 *    announce anything to assistive technology.
 */
import type { Observation, Series } from "./types";
import { uiScale } from "./scale.ts";

export type Scale = "linear" | "log" | "kp";

export interface PanelSeries {
  series: Series;
  colour: string;
  /** Drawn as a dashed line, used for secondary traces such as Bz in GSE. */
  dashed?: boolean;
  label?: string;
}

export interface Panel {
  id: string;
  title: string;
  unit: string;
  scale: Scale;
  series: PanelSeries[];
  /** Draw a zero reference line (signed quantities such as Bz). */
  zeroReference?: boolean;
  /** Extra note rendered in the panel header, e.g. the coordinate frame. */
  note?: string;
  /** Forecast intervals for a Kp panel, kept visually separate. */
  forecast?: { time: string; value: number; scale: string | null }[];
  minHeight?: number;
}

export interface ChartSelection {
  /** Selected instant in epoch milliseconds. */
  time: number;
  pinned: boolean;
}

export interface ChartRange {
  start: number;
  end: number;
}

interface Point {
  t: number;
  v: number;
  obs: Observation;
}

const PADDING = { left: 76, right: 18, top: 26, bottom: 8 };
const AXIS_HEIGHT = 26;

export class ChartStack {
  readonly canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private panels: Panel[] = [];
  private range: ChartRange = { start: 0, end: 1 };
  private selection: ChartSelection = { time: 0, pinned: false };
  private hover: number | null = null;
  private panelBoxes: { panel: Panel; y: number; height: number }[] = [];
  private onSelect: (sel: ChartSelection) => void = () => {};
  private onRange: (range: ChartRange) => void = () => {};
  private dragStart: number | null = null;
  private dragCurrent: number | null = null;
  private reducedMotion = false;
  private resizeObserver: ResizeObserver | null = null;

  destroy(): void {
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    this.onSelect = () => {};
    this.onRange = () => {};
  }

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2d context unavailable");
    this.ctx = ctx;
    this.attachEvents();
  }

  setReducedMotion(value: boolean): void {
    this.reducedMotion = value;
  }

  setPanels(panels: Panel[]): void {
    this.panels = panels;
    this.render();
  }

  getPanels(): Panel[] {
    return this.panels;
  }

  setRange(range: ChartRange): void {
    if (range.end > range.start) {
      this.range = range;
      this.render();
    }
  }

  getRange(): ChartRange {
    return { ...this.range };
  }

  setSelection(selection: ChartSelection): void {
    this.selection = selection;
    this.render();
  }

  getSelection(): ChartSelection {
    return { ...this.selection };
  }

  onSelectionChange(handler: (sel: ChartSelection) => void): void {
    this.onSelect = handler;
  }

  onRangeChange(handler: (range: ChartRange) => void): void {
    this.onRange = handler;
  }

  /** Nearest real sample to `time` within `toleranceMs`, or null. Never
   *  interpolates and never returns a sample from outside the tolerance. */
  static nearest(series: Series, time: number, toleranceMs: number): Observation | null {
    let best: Observation | null = null;
    let bestDelta = Infinity;
    for (const obs of series.samples) {
      if (obs.value === null || obs.quality === "missing") continue;
      const delta = Math.abs(new Date(obs.time).getTime() - time);
      if (delta <= toleranceMs && delta < bestDelta) {
        best = obs;
        bestDelta = delta;
      }
    }
    return best;
  }

  /** Contiguous runs, broken by missing samples and by gaps larger than
   *  2.5 nominal cadences — the same rule the backend applies. */
  static segments(series: Series): Point[][] {
    const gapMs = series.nominal_cadence_seconds * 2500;
    const out: Point[][] = [];
    let current: Point[] = [];
    let prev: number | null = null;
    for (const obs of series.samples) {
      if (obs.value === null || obs.quality === "missing") {
        if (current.length) out.push(current);
        current = [];
        prev = null;
        continue;
      }
      const t = new Date(obs.time).getTime();
      if (prev !== null && t - prev > gapMs) {
        if (current.length) out.push(current);
        current = [];
      }
      current.push({ t, v: obs.value, obs });
      prev = t;
    }
    if (current.length) out.push(current);
    return out;
  }

  private x(t: number, width: number): number {
    const span = this.range.end - this.range.start || 1;
    return PADDING.left + ((t - this.range.start) / span) * (width - PADDING.left - PADDING.right);
  }

  /** `px` and `width` are CSS pixels on the canvas; the chart lays out in design units. */
  private timeAt(cssPx: number, cssWidth: number): number {
    const scale = uiScale();
    const px = cssPx / scale;
    const width = cssWidth / scale;
    const span = this.range.end - this.range.start || 1;
    const usable = width - PADDING.left - PADDING.right;
    return this.range.start + ((px - PADDING.left) / usable) * span;
  }

  render(): void {
    const dpr = window.devicePixelRatio || 1;
    // Everything below draws in unscaled design units (fonts, padding, line
    // widths). The transform maps them onto the real canvas, so on a large
    // display the whole chart grows with the rest of the interface.
    const scale = uiScale();
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(320, Math.floor(rect.width / scale));
    const height = Math.max(200, Math.floor(rect.height / scale));
    const bitmapW = Math.round(rect.width * dpr);
    const bitmapH = Math.round(rect.height * dpr);
    if (this.canvas.width !== bitmapW || this.canvas.height !== bitmapH) {
      this.canvas.width = bitmapW;
      this.canvas.height = bitmapH;
    }
    const ctx = this.ctx;
    ctx.setTransform(dpr * scale, 0, 0, dpr * scale, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const style = getComputedStyle(this.canvas);
    const fg = style.getPropertyValue("--chart-fg").trim() || "#e8ecf4";
    const muted = style.getPropertyValue("--chart-muted").trim() || "#8a94a8";
    const grid = style.getPropertyValue("--chart-grid").trim() || "#252c3a";

    const plotHeight = height - AXIS_HEIGHT;
    this.panelBoxes = layoutPanels(this.panels, plotHeight);
    for (const { panel, y, height: h } of this.panelBoxes) {
      this.drawPanel(ctx, panel, y, h, width, { fg, muted, grid });
    }
    this.drawTimeAxis(ctx, width, plotHeight, muted, grid);
    this.drawCrosshair(ctx, width, plotHeight, fg, muted);
    this.drawDragSelection(ctx, width, plotHeight);
  }

  private drawPanel(
    ctx: CanvasRenderingContext2D,
    panel: Panel,
    top: number,
    height: number,
    width: number,
    colours: { fg: string; muted: string; grid: string },
  ): void {
    const plotTop = top + PADDING.top;
    const plotBottom = top + height - PADDING.bottom;
    const plotHeight = Math.max(10, plotBottom - plotTop);

    // Header: title, unit, and any frame/instrument note.
    ctx.font = "600 12px system-ui, -apple-system, sans-serif";
    ctx.fillStyle = colours.fg;
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    ctx.fillText(`${panel.title} (${panel.unit})`, PADDING.left, top + 6);
    if (panel.note) {
      ctx.font = "11px system-ui, -apple-system, sans-serif";
      ctx.fillStyle = colours.muted;
      const titleWidth = ctx.measureText(`${panel.title} (${panel.unit})`).width;
      ctx.fillText(panel.note, PADDING.left + titleWidth + 60, top + 7);
    }

    const domain = ChartStack.domainFor(panel, this.range);
    const yOf = (v: number): number => {
      if (panel.scale === "log") {
        const lo = Math.log10(domain.min);
        const hi = Math.log10(domain.max);
        const f = (Math.log10(Math.max(v, domain.min)) - lo) / (hi - lo || 1);
        return plotBottom - f * plotHeight;
      }
      const f = (v - domain.min) / (domain.max - domain.min || 1);
      return plotBottom - f * plotHeight;
    };

    // Gridlines and value labels.
    ctx.strokeStyle = colours.grid;
    ctx.lineWidth = 1;
    ctx.font = "11px system-ui, -apple-system, sans-serif";
    ctx.fillStyle = colours.muted;
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    for (const tick of ChartStack.ticksFor(panel, domain, plotHeight)) {
      const ty = Math.round(yOf(tick.value)) + 0.5;
      if (ty < plotTop - 1 || ty > plotBottom + 1) continue;
      ctx.beginPath();
      ctx.moveTo(PADDING.left, ty);
      ctx.lineTo(width - PADDING.right, ty);
      ctx.stroke();
      ctx.fillText(tick.label, PADDING.left - 8, ty);
    }

    if (panel.zeroReference && domain.min < 0 && domain.max > 0) {
      const zy = Math.round(yOf(0)) + 0.5;
      ctx.save();
      ctx.strokeStyle = colours.fg;
      ctx.globalAlpha = 0.5;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(PADDING.left, zy);
      ctx.lineTo(width - PADDING.right, zy);
      ctx.stroke();
      ctx.restore();
    }

    // Forecast intervals (Kp), drawn behind observed values and hatched.
    if (panel.forecast?.length) {
      for (const cell of panel.forecast) {
        const t0 = new Date(cell.time).getTime();
        const t1 = t0 + 3 * 3600 * 1000;
        if (t1 < this.range.start || t0 > this.range.end) continue;
        const x0 = this.x(t0, width);
        const x1 = this.x(t1, width);
        const yTop = yOf(cell.value);
        ctx.save();
        ctx.fillStyle = "rgba(140, 170, 220, 0.22)";
        ctx.strokeStyle = "rgba(160, 190, 235, 0.75)";
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.rect(x0 + 1, yTop, Math.max(1, x1 - x0 - 2), plotBottom - yTop);
        ctx.fill();
        ctx.stroke();
        ctx.restore();
      }
    }

    for (const entry of panel.series) {
      if (panel.scale === "kp" || entry.series.samples.some((s) => s.time_precision === "interval")) {
        this.drawIntervals(ctx, entry, yOf, plotBottom, width);
      } else {
        this.drawLine(ctx, entry, yOf, width);
      }
    }
  }

  private drawLine(
    ctx: CanvasRenderingContext2D,
    entry: PanelSeries,
    yOf: (v: number) => number,
    width: number,
  ): void {
    ctx.save();
    ctx.strokeStyle = entry.colour;
    ctx.lineWidth = 1.5;
    ctx.lineJoin = "round";
    if (entry.dashed) ctx.setLineDash([4, 3]);
    for (const segment of ChartStack.segments(entry.series)) {
      let started = false;
      ctx.beginPath();
      for (const p of segment) {
        if (p.t < this.range.start - 60000 || p.t > this.range.end + 60000) {
          if (started) {
            ctx.stroke();
            ctx.beginPath();
            started = false;
          }
          continue;
        }
        const px = this.x(p.t, width);
        const py = yOf(p.v);
        if (!started) {
          ctx.moveTo(px, py);
          started = true;
        } else {
          ctx.lineTo(px, py);
        }
      }
      if (started) ctx.stroke();
    }
    ctx.restore();
  }

  private drawIntervals(
    ctx: CanvasRenderingContext2D,
    entry: PanelSeries,
    yOf: (v: number) => number,
    plotBottom: number,
    width: number,
  ): void {
    ctx.save();
    ctx.fillStyle = entry.colour;
    for (const obs of entry.series.samples) {
      if (obs.value === null || obs.quality === "missing") continue;
      const t0 = new Date(obs.time).getTime();
      const t1 = t0 + (obs.interval_seconds ?? entry.series.nominal_cadence_seconds) * 1000;
      if (t1 < this.range.start || t0 > this.range.end) continue;
      const x0 = this.x(t0, width);
      const x1 = this.x(t1, width);
      const yTop = yOf(obs.value);
      ctx.globalAlpha = obs.quality === "suspect" ? 0.5 : 0.85;
      ctx.fillRect(x0 + 1, yTop, Math.max(1, x1 - x0 - 2), plotBottom - yTop);
    }
    ctx.restore();
  }

  static domainFor(panel: Panel, range: ChartRange): { min: number; max: number } {
    let min = Infinity;
    let max = -Infinity;
    const consider = (v: number) => {
      if (!Number.isFinite(v)) return;
      if (panel.scale === "log" && v <= 0) return;
      min = Math.min(min, v);
      max = Math.max(max, v);
    };
    for (const entry of panel.series) {
      for (const obs of entry.series.samples) {
        if (obs.value === null || obs.quality === "missing") continue;
        const t = new Date(obs.time).getTime();
        if (t < range.start || t > range.end) continue;
        consider(obs.value);
      }
    }
    for (const cell of panel.forecast ?? []) consider(cell.value);

    if (!Number.isFinite(min) || !Number.isFinite(max)) {
      return panel.scale === "log" ? { min: 1e-9, max: 1e-3 } : { min: 0, max: 1 };
    }
    if (panel.scale === "log") {
      const lo = Math.floor(Math.log10(min));
      const hi = Math.max(lo + 1, Math.ceil(Math.log10(max)));
      return { min: Math.pow(10, lo), max: Math.pow(10, hi) };
    }
    if (panel.scale === "kp") return { min: 0, max: 9 };
    if (panel.zeroReference) {
      const bound = Math.max(Math.abs(min), Math.abs(max), 1) * 1.1;
      return { min: -bound, max: bound };
    }
    const pad = (max - min || Math.abs(max) || 1) * 0.1;
    return { min: min - pad, max: max + pad };
  }

  static ticksFor(
    panel: Panel,
    domain: { min: number; max: number },
    plotHeight: number,
  ): { value: number; label: string }[] {
    // Never place ticks closer than ~16 px: overlapping labels are unreadable.
    const maxTicks = Math.max(2, Math.floor(plotHeight / 16));
    if (panel.scale === "log") {
      const out: { value: number; label: string }[] = [];
      const lo = Math.round(Math.log10(domain.min));
      const hi = Math.round(Math.log10(domain.max));
      const every = Math.max(1, Math.ceil((hi - lo + 1) / maxTicks));
      for (let e = lo; e <= hi; e += every) {
        out.push({ value: Math.pow(10, e), label: `1e${e}` });
      }
      return out;
    }
    if (panel.scale === "kp") {
      const all = maxTicks >= 5 ? [0, 3, 5, 7, 9] : [0, 5, 9];
      return all.map((v) => ({ value: v, label: String(v) }));
    }
    const steps = Math.min(4, Math.max(1, maxTicks - 1));
    const out: { value: number; label: string }[] = [];
    for (let i = 0; i <= steps; i++) {
      const v = domain.min + ((domain.max - domain.min) * i) / steps;
      const digits = Math.abs(v) >= 100 ? 0 : 1;
      out.push({ value: v, label: v.toFixed(digits) });
    }
    return out;
  }

  private drawTimeAxis(
    ctx: CanvasRenderingContext2D,
    width: number,
    plotHeight: number,
    muted: string,
    grid: string,
  ): void {
    const span = this.range.end - this.range.start;
    const stepMs = niceTimeStep(span);
    ctx.strokeStyle = grid;
    ctx.fillStyle = muted;
    ctx.font = "11px system-ui, -apple-system, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const first = Math.ceil(this.range.start / stepMs) * stepMs;
    for (let t = first; t <= this.range.end; t += stepMs) {
      const px = Math.round(this.x(t, width)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(px, plotHeight);
      ctx.lineTo(px, plotHeight + 4);
      ctx.stroke();
      // Leave room for the zone label at the right edge.
      if (px < width - PADDING.right - 40) ctx.fillText(axisLabel(t, stepMs), px, plotHeight + 7);
    }
    ctx.textAlign = "right";
    ctx.fillText("UTC", width - PADDING.right, plotHeight + 7);
  }

  private drawCrosshair(
    ctx: CanvasRenderingContext2D,
    width: number,
    plotHeight: number,
    fg: string,
    muted: string,
  ): void {
    const t = this.hover ?? this.selection.time;
    if (!Number.isFinite(t) || t < this.range.start || t > this.range.end) return;
    const px = Math.round(this.x(t, width)) + 0.5;
    ctx.save();
    ctx.strokeStyle = this.selection.pinned && this.hover === null ? fg : muted;
    ctx.lineWidth = 1;
    if (!(this.selection.pinned && this.hover === null)) ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(px, 0);
    ctx.lineTo(px, plotHeight);
    ctx.stroke();
    ctx.restore();
  }

  private drawDragSelection(ctx: CanvasRenderingContext2D, width: number, plotHeight: number): void {
    if (this.dragStart === null || this.dragCurrent === null) return;
    const x0 = this.x(Math.min(this.dragStart, this.dragCurrent), width);
    const x1 = this.x(Math.max(this.dragStart, this.dragCurrent), width);
    ctx.save();
    ctx.fillStyle = "rgba(120, 170, 240, 0.18)";
    ctx.fillRect(x0, 0, x1 - x0, plotHeight);
    ctx.restore();
  }

  private attachEvents(): void {
    const rectWidth = () => this.canvas.getBoundingClientRect().width;

    this.canvas.addEventListener("pointermove", (e) => {
      const rect = this.canvas.getBoundingClientRect();
      const t = this.timeAt(e.clientX - rect.left, rect.width);
      this.hover = clamp(t, this.range.start, this.range.end);
      if (this.dragStart !== null) this.dragCurrent = this.hover;
      // Pointer movement updates the crosshair only. It does not change the
      // selection and is never announced to assistive technology.
      this.render();
    });

    this.canvas.addEventListener("pointerleave", () => {
      this.hover = null;
      this.render();
    });

    this.canvas.addEventListener("pointerdown", (e) => {
      const rect = this.canvas.getBoundingClientRect();
      this.dragStart = clamp(this.timeAt(e.clientX - rect.left, rect.width), this.range.start, this.range.end);
      this.dragCurrent = this.dragStart;
      this.canvas.setPointerCapture(e.pointerId);
    });

    this.canvas.addEventListener("pointerup", (e) => {
      const rect = this.canvas.getBoundingClientRect();
      const t = clamp(this.timeAt(e.clientX - rect.left, rect.width), this.range.start, this.range.end);
      const start = this.dragStart;
      this.dragStart = null;
      this.dragCurrent = null;
      if (start !== null && Math.abs(t - start) > (this.range.end - this.range.start) / 100) {
        // A drag zooms to the dragged interval.
        this.range = { start: Math.min(start, t), end: Math.max(start, t) };
        this.onRange(this.getRange());
      } else {
        // A click pins the time.
        this.selection = { time: t, pinned: true };
        this.onSelect(this.getSelection());
      }
      this.render();
    });

    this.canvas.addEventListener("keydown", (e) => {
      const span = this.range.end - this.range.start;
      const step = e.shiftKey ? span / 20 : span / 200;
      let handled = true;
      switch (e.key) {
        case "ArrowLeft":
          this.selection = { time: clamp(this.selection.time - step, this.range.start, this.range.end), pinned: true };
          break;
        case "ArrowRight":
          this.selection = { time: clamp(this.selection.time + step, this.range.start, this.range.end), pinned: true };
          break;
        case "Home":
          this.selection = { time: this.range.start, pinned: true };
          break;
        case "End":
          this.selection = { time: this.range.end, pinned: true };
          break;
        case "+":
        case "=": {
          const centre = this.selection.time;
          this.range = { start: centre - span / 4, end: centre + span / 4 };
          this.onRange(this.getRange());
          break;
        }
        case "-":
        case "_": {
          const centre = this.selection.time;
          this.range = { start: centre - span, end: centre + span };
          this.onRange(this.getRange());
          break;
        }
        case "Escape":
          this.selection = { ...this.selection, pinned: false };
          break;
        default:
          handled = false;
      }
      if (handled) {
        e.preventDefault();
        this.onSelect(this.getSelection());
        this.render();
      }
    });

    const observer = new ResizeObserver(() => {
      // Resizing re-renders immediately; there is no animation to respect here,
      // but honour reduced motion for any future transition.
      if (!this.reducedMotion) this.render();
      else this.render();
    });
    this.resizeObserver = observer;
    observer.observe(this.canvas);
    void rectWidth;
  }
}

/** Render the current chart into a fresh canvas with a labelled header:
 *  title, interval, units, sources and data status (spec §12). The values are
 *  the ones on screen, i.e. the current snapshot, not a partially refreshed view. */
export function renderChartExport(
  stack: ChartStack,
  meta: { title: string; sources: string[]; status: string; units: string[] },
): HTMLCanvasElement {
  const src = stack.canvas;
  const headerLines = [
    meta.title,
    `Interval: ${new Date(stack.getRange().start).toISOString()} → ${new Date(stack.getRange().end).toISOString()} (UTC)`,
    `Units: ${meta.units.join(" · ")}`,
    `Sources: ${meta.sources.join(" · ")}`,
    `Data status: ${meta.status}`,
  ];
  const lineH = 16;
  const headerH = 12 + headerLines.length * lineH;
  const dpr = window.devicePixelRatio || 1;
  const width = src.width / dpr;
  const height = src.height / dpr;
  const out = document.createElement("canvas");
  out.width = Math.round(width * dpr);
  out.height = Math.round((height + headerH) * dpr);
  const ctx = out.getContext("2d");
  if (!ctx) throw new Error("canvas unavailable");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#0b0e14";
  ctx.fillRect(0, 0, width, height + headerH);
  ctx.fillStyle = "#e9edf5";
  ctx.font = "600 13px system-ui, sans-serif";
  ctx.textBaseline = "top";
  headerLines.forEach((line, i) => {
    if (i === 1) ctx.font = "12px system-ui, sans-serif";
    ctx.fillText(line, 12, 8 + i * lineH, width - 24);
  });
  ctx.drawImage(src, 0, headerH * dpr / dpr, width, height);
  return out;
}

/** Vertical stacking shared by the canvas renderer and the SVG export: panels
 *  below their combined minimum height shrink proportionally rather than
 *  letting the last one fall off the bottom. */
function layoutPanels(panels: Panel[], plotHeight: number): { panel: Panel; y: number; height: number }[] {
  const totalMin = panels.reduce((sum, p) => sum + (p.minHeight ?? 0), 0);
  const squeeze = totalMin > plotHeight && totalMin > 0 ? plotHeight / totalMin : 1;
  const flexible = Math.max(0, plotHeight - totalMin * squeeze);
  const share = panels.length ? flexible / panels.length : 0;
  const out: { panel: Panel; y: number; height: number }[] = [];
  let y = 0;
  for (const panel of panels) {
    const h = (panel.minHeight ?? 0) * squeeze + share;
    out.push({ panel, y, height: h });
    y += h;
  }
  return out;
}

function xOf(t: number, range: ChartRange, width: number): number {
  const span = range.end - range.start || 1;
  return PADDING.left + ((t - range.start) / span) * (width - PADDING.left - PADDING.right);
}

function escapeXml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Render the current chart as a standalone vector SVG document, matching
 *  `renderChartExport`'s header and layout but drawn from the underlying
 *  series data rather than copied from the canvas bitmap. Fixed colours are
 *  used (not the live theme) so the export looks the same regardless of the
 *  viewer's display settings. */
export function renderChartExportSvg(
  stack: ChartStack,
  meta: { title: string; sources: string[]; status: string; units: string[] },
): string {
  const rect = stack.canvas.getBoundingClientRect();
  // In design units, so the export looks like the screen whatever the UI scale.
  const width = Math.max(320, Math.floor(rect.width / uiScale()));
  const height = Math.max(200, Math.floor(rect.height / uiScale()));
  const range = stack.getRange();
  const panels = stack.getPanels();

  const headerLines = [
    meta.title,
    `Interval: ${new Date(range.start).toISOString()} → ${new Date(range.end).toISOString()} (UTC)`,
    `Units: ${meta.units.join(" · ")}`,
    `Sources: ${meta.sources.join(" · ")}`,
    `Data status: ${meta.status}`,
  ];
  const lineH = 16;
  const headerH = 12 + headerLines.length * lineH;
  const plotHeight = height - AXIS_HEIGHT;
  const fg = "#e8ecf4";
  const muted = "#8a94a8";
  const grid = "#252c3a";

  const parts: string[] = [];
  parts.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height + headerH}" ` +
    `viewBox="0 0 ${width} ${height + headerH}" font-family="system-ui, -apple-system, sans-serif">`,
  );
  parts.push(`<rect x="0" y="0" width="${width}" height="${height + headerH}" fill="#0b0e14"/>`);
  headerLines.forEach((line, i) => {
    const size = i === 0 ? 13 : 12;
    const weight = i === 0 ? 600 : 400;
    parts.push(
      `<text x="12" y="${8 + i * lineH + 11}" font-size="${size}" font-weight="${weight}" ` +
      `fill="#e9edf5">${escapeXml(line)}</text>`,
    );
  });

  for (const { panel, y, height: h } of layoutPanels(panels, plotHeight)) {
    parts.push(renderPanelSvg(panel, y + headerH, h, width, range, { fg, muted, grid }));
  }
  parts.push(renderTimeAxisSvg(width, plotHeight + headerH, range, muted, grid));
  parts.push("</svg>");
  return parts.join("");
}

function renderPanelSvg(
  panel: Panel,
  top: number,
  height: number,
  width: number,
  range: ChartRange,
  colours: { fg: string; muted: string; grid: string },
): string {
  const plotTop = top + PADDING.top;
  const plotBottom = top + height - PADDING.bottom;
  const plotHeight = Math.max(10, plotBottom - plotTop);
  const parts: string[] = [`<g>`];

  const titleText = `${panel.title} (${panel.unit})`;
  parts.push(`<text x="${PADDING.left}" y="${top + 6 + 11}" font-size="12" font-weight="600" fill="${colours.fg}">${escapeXml(titleText)}</text>`);
  if (panel.note) {
    // Canvas measures the title's actual rendered width; SVG text width isn't
    // known without layout, so a fixed monospace-ish estimate is used instead.
    const approxWidth = titleText.length * 7;
    parts.push(`<text x="${PADDING.left + approxWidth + 60}" y="${top + 7 + 10}" font-size="11" fill="${colours.muted}">${escapeXml(panel.note)}</text>`);
  }

  const domain = ChartStack.domainFor(panel, range);
  const yOf = (v: number): number => {
    if (panel.scale === "log") {
      const lo = Math.log10(domain.min);
      const hi = Math.log10(domain.max);
      const f = (Math.log10(Math.max(v, domain.min)) - lo) / (hi - lo || 1);
      return plotBottom - f * plotHeight;
    }
    const f = (v - domain.min) / (domain.max - domain.min || 1);
    return plotBottom - f * plotHeight;
  };

  for (const tick of ChartStack.ticksFor(panel, domain, plotHeight)) {
    const ty = Math.round(yOf(tick.value)) + 0.5;
    if (ty < plotTop - 1 || ty > plotBottom + 1) continue;
    parts.push(`<line x1="${PADDING.left}" y1="${ty}" x2="${width - PADDING.right}" y2="${ty}" stroke="${colours.grid}" stroke-width="1"/>`);
    parts.push(`<text x="${PADDING.left - 8}" y="${ty + 4}" font-size="11" text-anchor="end" fill="${colours.muted}">${escapeXml(tick.label)}</text>`);
  }

  if (panel.zeroReference && domain.min < 0 && domain.max > 0) {
    const zy = Math.round(yOf(0)) + 0.5;
    parts.push(`<line x1="${PADDING.left}" y1="${zy}" x2="${width - PADDING.right}" y2="${zy}" stroke="${colours.fg}" stroke-opacity="0.5" stroke-dasharray="4,3"/>`);
  }

  if (panel.forecast?.length) {
    for (const cell of panel.forecast) {
      const t0 = new Date(cell.time).getTime();
      const t1 = t0 + 3 * 3600 * 1000;
      if (t1 < range.start || t0 > range.end) continue;
      const x0 = xOf(t0, range, width);
      const x1 = xOf(t1, range, width);
      const yTop = yOf(cell.value);
      parts.push(
        `<rect x="${x0 + 1}" y="${yTop}" width="${Math.max(1, x1 - x0 - 2)}" height="${plotBottom - yTop}" ` +
        `fill="rgba(140, 170, 220, 0.22)" stroke="rgba(160, 190, 235, 0.75)" stroke-dasharray="3,3"/>`,
      );
    }
  }

  for (const entry of panel.series) {
    if (panel.scale === "kp" || entry.series.samples.some((s) => s.time_precision === "interval")) {
      parts.push(renderIntervalsSvg(entry, yOf, plotBottom, width, range));
    } else {
      parts.push(renderLineSvg(entry, yOf, width, range));
    }
  }

  parts.push(`</g>`);
  return parts.join("");
}

function renderLineSvg(
  entry: PanelSeries,
  yOf: (v: number) => number,
  width: number,
  range: ChartRange,
): string {
  const dash = entry.dashed ? ` stroke-dasharray="4,3"` : "";
  const parts: string[] = [];
  for (const segment of ChartStack.segments(entry.series)) {
    let d = "";
    let started = false;
    for (const p of segment) {
      if (p.t < range.start - 60000 || p.t > range.end + 60000) {
        started = false;
        continue;
      }
      const px = xOf(p.t, range, width);
      const py = yOf(p.v);
      d += started ? ` L ${px} ${py}` : `${d ? " " : ""}M ${px} ${py}`;
      started = true;
    }
    if (d) {
      parts.push(`<path d="${d}" fill="none" stroke="${entry.colour}" stroke-width="1.5" stroke-linejoin="round"${dash}/>`);
    }
  }
  return parts.join("");
}

function renderIntervalsSvg(
  entry: PanelSeries,
  yOf: (v: number) => number,
  plotBottom: number,
  width: number,
  range: ChartRange,
): string {
  const parts: string[] = [];
  for (const obs of entry.series.samples) {
    if (obs.value === null || obs.quality === "missing") continue;
    const t0 = new Date(obs.time).getTime();
    const t1 = t0 + (obs.interval_seconds ?? entry.series.nominal_cadence_seconds) * 1000;
    if (t1 < range.start || t0 > range.end) continue;
    const x0 = xOf(t0, range, width);
    const x1 = xOf(t1, range, width);
    const yTop = yOf(obs.value);
    const opacity = obs.quality === "suspect" ? 0.5 : 0.85;
    parts.push(
      `<rect x="${x0 + 1}" y="${yTop}" width="${Math.max(1, x1 - x0 - 2)}" height="${plotBottom - yTop}" ` +
      `fill="${entry.colour}" fill-opacity="${opacity}"/>`,
    );
  }
  return parts.join("");
}

function renderTimeAxisSvg(
  width: number,
  axisTop: number,
  range: ChartRange,
  muted: string,
  grid: string,
): string {
  const span = range.end - range.start;
  const stepMs = niceTimeStep(span);
  const parts: string[] = [`<g>`];
  const first = Math.ceil(range.start / stepMs) * stepMs;
  for (let t = first; t <= range.end; t += stepMs) {
    const px = Math.round(xOf(t, range, width)) + 0.5;
    parts.push(`<line x1="${px}" y1="${axisTop}" x2="${px}" y2="${axisTop + 4}" stroke="${grid}"/>`);
    if (px < width - PADDING.right - 40) {
      parts.push(`<text x="${px}" y="${axisTop + 17}" font-size="11" text-anchor="middle" fill="${muted}">${escapeXml(axisLabel(t, stepMs))}</text>`);
    }
  }
  parts.push(`<text x="${width - PADDING.right}" y="${axisTop + 17}" font-size="11" text-anchor="end" fill="${muted}">UTC</text>`);
  parts.push(`</g>`);
  return parts.join("");
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

function niceTimeStep(spanMs: number): number {
  const candidates = [
    60_000, 5 * 60_000, 15 * 60_000, 30 * 60_000,
    3600_000, 3 * 3600_000, 6 * 3600_000, 12 * 3600_000,
    86_400_000, 2 * 86_400_000, 7 * 86_400_000,
  ];
  for (const c of candidates) {
    if (spanMs / c <= 10) return c;
  }
  return candidates[candidates.length - 1]!;
}

function axisLabel(t: number, stepMs: number): string {
  const d = new Date(t);
  const pad = (n: number) => String(n).padStart(2, "0");
  if (stepMs >= 86_400_000) return `${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}`;
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}
