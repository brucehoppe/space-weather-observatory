import { test } from "node:test";
import assert from "node:assert/strict";
import { ChartStack, renderChartExportSvg } from "./chart.ts";

(globalThis as any).ResizeObserver ??= class {
  observe() {}
  disconnect() {}
};
(globalThis as any).window ??= { devicePixelRatio: 1 };
(globalThis as any).getComputedStyle ??= () => ({ getPropertyValue: () => "" });

function fakeCanvas(): any {
  const ctx2d = {
    setTransform() {}, clearRect() {}, fillRect() {}, strokeRect() {},
    beginPath() {}, moveTo() {}, lineTo() {}, stroke() {}, fill() {},
    save() {}, restore() {}, setLineDash() {}, rect() {}, fillText() {},
    measureText: () => ({ width: 40 }),
  };
  return {
    getContext: () => ctx2d,
    getBoundingClientRect: () => ({ width: 900, height: 420, top: 0, left: 0 }),
    addEventListener() {},
    style: {},
    width: 900, height: 420,
  };
}

function series(key: string, samples: any[]): any {
  return {
    key, label: key, unit: "u", frame: null,
    nominal_cadence_seconds: 60,
    aggregation: { kind: "raw" },
    provenance: { source_url: "https://example.test", retrieved_at: new Date().toISOString() },
    samples,
  };
}

test("renderChartExportSvg produces well-formed SVG with axis, gridlines and a path per series", () => {
  const now = Date.parse("2026-09-06T00:00:00Z");
  const pts = [];
  for (let i = 0; i < 30; i++) {
    pts.push({
      time: new Date(now + i * 60000).toISOString(),
      value: 400 + 50 * Math.sin(i / 5),
      quality: i === 10 ? "missing" : "good",
      instrument: "TEST", time_precision: "instant", interval_seconds: null,
    });
  }
  const canvas = fakeCanvas();
  const chart = new ChartStack(canvas);
  chart.setPanels([
    { id: "speed", title: "Solar wind speed", unit: "km/s", scale: "linear",
      series: [{ series: series("speed", pts), colour: "#5ac8fa" }], minHeight: 100 },
  ]);
  chart.setRange({ start: now, end: now + 29 * 60000 });

  const svg = renderChartExportSvg(chart, {
    title: "Test export", sources: ["https://example.test"], status: "demo status", units: ["speed: km/s"],
  });

  assert.match(svg, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
  assert.match(svg, /<\/svg>$/);
  assert.match(svg, /Test export/);
  assert.match(svg, /<path d="M /); // the speed series line
  assert.match(svg, /UTC<\/text>/);
  // Broken at the missing sample: two separate path segments, not one bridging line.
  assert.equal((svg.match(/<path /g) ?? []).length, 2);
});
