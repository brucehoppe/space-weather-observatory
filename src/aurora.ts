/** Aurora view: paired polar projections of the official OVATION grid.
 *
 *  Geometry rules (spec §8):
 *  - the product's own 1° grid, coordinate order and units are used as
 *    documented, not re-derived from Kp;
 *  - longitude wraps at the date line without a seam; the poles are drawn as
 *    the actual pole rows;
 *  - the model's own observation and forecast times are shown, with the lead
 *    time that this issue actually states;
 *  - the colour scale is labelled with what the number means: a model
 *    probability for a grid cell, not a personal chance of seeing aurora.
 */
import { feature } from "topojson-client";
import land110m from "world-atlas/land-110m.json";
import { button, el } from "./dom";
import { fmtAge, fmtDuration, fmtUtc } from "./format";
import * as ipc from "./ipc";
import type { AppState } from "./state";
import type { AuroraGrid } from "./types";

/** Natural Earth land polygons, public domain, bundled with the application. */
export const MAP_CREDIT = "Coastlines: Natural Earth (public domain), via the world-atlas package";

interface LandGeometry {
  type: string;
  coordinates: number[][][] | number[][][][];
}

let landRings: number[][][] | null = null;

function getLandRings(): number[][][] {
  if (landRings) return landRings;
  // topojson-client returns GeoJSON; flatten to rings of [lon, lat].
  const topology = land110m as unknown as { objects: { land: never } };
  const collection = feature(topology as never, topology.objects.land) as unknown as {
    type: string;
    geometry?: LandGeometry;
    features?: { geometry: LandGeometry }[];
  };
  const rings: number[][][] = [];
  const push = (geom: LandGeometry) => {
    if (geom.type === "Polygon") {
      for (const ring of geom.coordinates as number[][][]) rings.push(ring);
    } else if (geom.type === "MultiPolygon") {
      for (const poly of geom.coordinates as number[][][][]) {
        for (const ring of poly) rings.push(ring);
      }
    }
  };
  if (collection.geometry) push(collection.geometry);
  for (const f of collection.features ?? []) push(f.geometry);
  landRings = rings;
  return rings;
}

type Hemisphere = "north" | "south";

/** Azimuthal equidistant projection centred on a pole. Returns null for points
 *  in the other hemisphere, so nothing is folded across the equator. */
function project(
  lon: number,
  lat: number,
  hemisphere: Hemisphere,
  cx: number,
  cy: number,
  radius: number,
): [number, number] | null {
  const colatitude = hemisphere === "north" ? 90 - lat : 90 + lat;
  if (colatitude < 0 || colatitude > 90) return null;
  const r = (colatitude / 90) * radius;
  // North: longitude increases anticlockwise from the top. South: mirrored, so
  // the map is not a rotated copy of the north.
  const angle = ((hemisphere === "north" ? lon : -lon) - 90) * (Math.PI / 180);
  return [cx + r * Math.cos(angle), cy + r * Math.sin(angle)];
}

/** Colour ramp for the model probability, in percent. Transparent below 1 %
 *  so an empty grid reads as empty rather than as a faint prediction. */
export function auroraColour(percent: number): string {
  if (!Number.isFinite(percent) || percent < 1) return "rgba(0,0,0,0)";
  const p = Math.min(100, percent) / 100;
  if (p < 0.35) {
    const f = p / 0.35;
    return `rgba(${Math.round(20 + 40 * f)}, ${Math.round(110 + 90 * f)}, ${Math.round(90 + 40 * f)}, ${0.25 + 0.45 * f})`;
  }
  if (p < 0.7) {
    const f = (p - 0.35) / 0.35;
    return `rgba(${Math.round(60 + 170 * f)}, ${Math.round(200 + 20 * f)}, ${Math.round(130 - 40 * f)}, ${0.7 + 0.2 * f})`;
  }
  const f = (p - 0.7) / 0.3;
  return `rgba(${Math.round(230 + 25 * f)}, ${Math.round(220 + 30 * f)}, ${Math.round(90 + 160 * f)}, 0.95)`;
}

function drawHemisphere(canvas: HTMLCanvasElement, grid: AuroraGrid, hemisphere: Hemisphere): void {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth || 320;
  const h = canvas.clientHeight || 320;
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const cx = w / 2;
  const cy = h / 2;
  const radius = Math.min(w, h) / 2 - 14;

  ctx.fillStyle = "#0d1119";
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
  ctx.fill();

  const valueAt = (lon: number, lat: number): number => {
    const l = ((lon % grid.lon_count) + grid.lon_count) % grid.lon_count;
    const row = lat - grid.lat_min;
    if (row < 0 || row >= grid.lat_count) return 0;
    return grid.values[row * grid.lon_count + l] ?? 0;
  };

  // Grid cells. Latitudes are walked from the pole to the equator so the poles
  // themselves are drawn, and longitude runs the full 0..359 so the date line
  // closes without a seam.
  const latFrom = hemisphere === "north" ? 40 : -90;
  const latTo = hemisphere === "north" ? 90 : -40;
  for (let lat = latFrom; lat <= latTo; lat++) {
    for (let lon = 0; lon < grid.lon_count; lon++) {
      const v = valueAt(lon, lat);
      if (v < 1) continue;
      const corners: ([number, number] | null)[] = [
        project(lon, lat, hemisphere, cx, cy, radius),
        project(lon + 1, lat, hemisphere, cx, cy, radius),
        project(lon + 1, lat + 1, hemisphere, cx, cy, radius),
        project(lon, lat + 1, hemisphere, cx, cy, radius),
      ];
      if (corners.some((c) => c === null)) continue;
      ctx.fillStyle = auroraColour(v);
      ctx.beginPath();
      ctx.moveTo(corners[0]![0], corners[0]![1]);
      for (let i = 1; i < corners.length; i++) ctx.lineTo(corners[i]![0], corners[i]![1]);
      ctx.closePath();
      ctx.fill();
    }
  }

  // Graticule: parallels every 10°, meridians every 30°.
  ctx.strokeStyle = "rgba(150,165,190,0.18)";
  ctx.lineWidth = 1;
  for (let colat = 10; colat <= 90; colat += 10) {
    ctx.beginPath();
    ctx.arc(cx, cy, (colat / 90) * radius, 0, 2 * Math.PI);
    ctx.stroke();
  }
  for (let lon = 0; lon < 360; lon += 30) {
    const p = project(lon, hemisphere === "north" ? 0 : 0, hemisphere, cx, cy, radius);
    if (!p) continue;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(p[0], p[1]);
    ctx.stroke();
  }

  // Coastlines on top.
  ctx.strokeStyle = "rgba(190,205,230,0.55)";
  ctx.lineWidth = 0.8;
  for (const ring of getLandRings()) {
    let started = false;
    ctx.beginPath();
    for (const [lon, lat] of ring) {
      const p = project(lon!, lat!, hemisphere, cx, cy, radius);
      if (!p) {
        if (started) ctx.stroke();
        ctx.beginPath();
        started = false;
        continue;
      }
      if (!started) {
        ctx.moveTo(p[0], p[1]);
        started = true;
      } else {
        ctx.lineTo(p[0], p[1]);
      }
    }
    if (started) ctx.stroke();
  }

  // Outline and pole marker.
  ctx.strokeStyle = "rgba(160,175,200,0.5)";
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
  ctx.stroke();
  ctx.fillStyle = "rgba(220,230,245,0.8)";
  ctx.beginPath();
  ctx.arc(cx, cy, 2, 0, 2 * Math.PI);
  ctx.fill();
  ctx.fillStyle = "rgba(170,185,210,0.9)";
  ctx.font = "11px system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(hemisphere === "north" ? "90° N" : "90° S", cx, cy - 8);
  ctx.fillText("0°", cx, cy - radius - 3);
  ctx.fillText("180°", cx, cy + radius + 11);
}

/** Text fallback used when canvas rendering is unavailable. */
export function auroraTextSummary(grid: AuroraGrid): string {
  let north = 0;
  let south = 0;
  for (let lat = grid.lat_min; lat < grid.lat_min + grid.lat_count; lat++) {
    for (let lon = 0; lon < grid.lon_count; lon++) {
      const v = grid.values[(lat - grid.lat_min) * grid.lon_count + lon] ?? 0;
      if (lat >= 0) north = Math.max(north, v);
      else south = Math.max(south, v);
    }
  }
  return `Highest modelled grid-cell value: ${north.toFixed(0)}% in the northern hemisphere, ` +
    `${south.toFixed(0)}% in the southern hemisphere.`;
}

export function renderAuroraView(state: AppState): HTMLElement {
  const view = el("div", { class: "aurora-view" });
  const header = el("div", { class: "aurora-header" });
  const panes = el("div", { class: "aurora-canvases" });
  view.append(header, panes);

  const meta = state.dashboard?.aurora ?? null;
  if (!meta) {
    header.append(el("p", { class: "note" },
      "The aurora product is unavailable. That is a gap in information, not evidence of quiet conditions."));
    return view;
  }

  header.append(
    el("div", {},
      el("div", {}, el("strong", {}, meta.model)),
      el("div", { class: "note" },
        `Observation time ${fmtUtc(meta.observation_time)} · forecast time ${fmtUtc(meta.forecast_time)} · ` +
        `stated lead ${fmtDuration(meta.lead_time_minutes * 60)} · product age ${fmtAge(meta.forecast_time)}`),
    ),
    el("div", { class: "aurora-legend" },
      el("span", {}, "0%"),
      legendBar(),
      el("span", {}, "100%"),
      el("span", { class: "note" }, meta.units),
    ),
  );

  header.append(el("p", { class: "note", style: "flex-basis:100%;margin:0" },
    "This is a model quantity for each grid cell, not your chance of seeing aurora. Daylight, cloud and local " +
    "viewing conditions matter and are not included. The product's lead time varies between issues; the value " +
    "shown above is the one this issue states."));

  for (const hemisphere of ["north", "south"] as Hemisphere[]) {
    const canvas = el("canvas", {
      role: "img",
      "aria-label": `${hemisphere === "north" ? "Northern" : "Southern"} hemisphere aurora model grid`,
    }) as HTMLCanvasElement;
    const fallback = el("p", { class: "note", style: "padding:.5rem" });
    const pane = el("div", { class: "aurora-pane" },
      el("h3", {}, hemisphere === "north" ? "Northern hemisphere" : "Southern hemisphere"),
      canvas,
    );
    panes.append(pane);

    void ipc.getAuroraGrid()
      .then((grid) => {
        try {
          drawHemisphere(canvas, grid, hemisphere);
          fallback.textContent = auroraTextSummary(grid);
          pane.append(fallback);
        } catch {
          // A meaningful textual view survives a graphics failure.
          canvas.remove();
          fallback.textContent = `Graphics unavailable. ${auroraTextSummary(grid)}`;
          pane.append(fallback);
        }
      })
      .catch((e: unknown) => {
        canvas.remove();
        fallback.textContent = `Aurora grid unavailable: ${String(e)}`;
        pane.append(fallback);
      });
  }

  const footer = el("div", { class: "aurora-header" },
    el("span", { class: "note" }, MAP_CREDIT),
    button("Open the official product", () => void ipc.openExternal(meta.product_page), "ghost"),
  );
  view.append(footer);
  return view;
}

function legendBar(): HTMLElement {
  const canvas = el("canvas", { class: "legend-bar", "aria-hidden": "true" }) as HTMLCanvasElement;
  queueMicrotask(() => {
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    canvas.width = 140;
    canvas.height = 10;
    for (let x = 0; x < 140; x++) {
      ctx.fillStyle = auroraColour((x / 139) * 100);
      ctx.fillRect(x, 0, 1, 10);
    }
  });
  return canvas;
}
