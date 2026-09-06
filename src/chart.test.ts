/** Pure-function tests for the chart engine, plus a seven-day timing run.
 *  Run with `npm test` (Node's built-in runner, no browser). */
import { test } from "node:test";
import assert from "node:assert/strict";
import { ChartStack } from "./chart.ts";
import type { Observation, Series } from "./types.ts";

function series(samples: Observation[], cadence = 60): Series {
  return {
    id: { provider: "p", product: "x", measurement: "m" },
    label: "m", unit: "u", nominal_cadence_seconds: cadence,
    aggregation: { kind: "raw" },
    provenance: { source_url: "https://example.invalid", retrieved_at: "2026-09-06T00:00:00Z" },
    samples,
  };
}

const t0 = Date.UTC(2026, 8, 6, 0, 0, 0);
const obs = (min: number, value: number | null): Observation => ({
  time: new Date(t0 + min * 60_000).toISOString(),
  time_precision: "instant",
  value,
  quality: value === null ? "missing" : "good",
});

test("segments break across gaps and missing samples, matching the backend rule", () => {
  const s = series([obs(0, 1), obs(1, 2), obs(11, 3), obs(12, null), obs(13, 4)]);
  const segs = ChartStack.segments(s);
  assert.equal(segs.length, 3);
  assert.deepEqual(segs.map((g) => g.length), [2, 1, 1]);
});

test("nearest never interpolates and never exceeds the tolerance", () => {
  const s = series([obs(0, 10), obs(5, 20)]);
  assert.equal(ChartStack.nearest(s, t0 + 4 * 60_000, 120_000)?.value, 20);
  assert.equal(ChartStack.nearest(s, t0 + 60 * 60_000, 120_000), null);
});

test("nearest skips missing samples rather than returning null values", () => {
  const s = series([obs(0, 10), obs(1, null)]);
  assert.equal(ChartStack.nearest(s, t0 + 60_000, 120_000)?.value, 10);
});

test("seven-day minute-cadence series: selection lookup p95 under 100 ms", () => {
  const samples: Observation[] = [];
  for (let m = 0; m < 7 * 24 * 60; m++) samples.push(obs(m, m % 97 === 0 ? null : 380 + (m % 23)));
  const s = series(samples);
  const times: number[] = [];
  for (let i = 0; i < 300; i++) {
    const at = t0 + ((i * 20) % samples.length) * 60_000 + 17_000;
    const start = performance.now();
    ChartStack.nearest(s, at, 60_000);
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  const p95 = times[Math.floor(times.length * 0.95)]!;
  console.log(`TS nearest over ${samples.length} samples: p95 ${p95.toFixed(3)} ms`);
  assert.ok(p95 < 100, `p95 ${p95} ms`);

  const segStart = performance.now();
  const segs = ChartStack.segments(s);
  const segMs = performance.now() - segStart;
  console.log(`TS segments: ${segs.length} segments in ${segMs.toFixed(3)} ms`);
  assert.ok(segMs < 100);
});
