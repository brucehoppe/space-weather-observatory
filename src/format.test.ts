import { test } from "node:test";
import assert from "node:assert/strict";
import { bzOrientation, csvSafe, fluxClass, fmtAge, fmtFlux, fmtUtc, fmtWithUnit, NO_VALUE } from "./format.ts";

test("flux class mirrors the Rust classification on the long band", () => {
  assert.equal(fluxClass(5.2e-6), "C5.2");
  assert.equal(fluxClass(1e-4), "X1.0");
  assert.equal(fluxClass(9.99e-7), "B10.0");
  assert.equal(fluxClass(0), null);
  assert.equal(fluxClass(-1), null);
  assert.equal(fluxClass(null), null);
});

test("missing values never print as zero or as quiet", () => {
  assert.equal(fmtWithUnit(null, "km/s"), NO_VALUE);
  assert.equal(fmtFlux(0), NO_VALUE);
  assert.equal(bzOrientation(undefined), NO_VALUE);
  assert.equal(fmtAge(null), "no data");
});

test("Bz sign carries meaning", () => {
  assert.equal(bzOrientation(-4.2), "southward");
  assert.equal(bzOrientation(3), "northward");
});

test("UTC display is explicit about the zone", () => {
  assert.equal(fmtUtc("2026-09-06T17:59:00Z"), "2026-09-06 17:59 UTC");
});

test("ages are measured against the supplied clock, not the wall clock", () => {
  const now = new Date("2026-09-06T18:04:33Z");
  assert.equal(fmtAge("2026-09-06T17:59:00Z", now), "6 min ago");
});

test("csv text guard exists for the frontend too", () => {
  assert.equal(csvSafe("=1+1"), "'=1+1");
  assert.equal(csvSafe("plain"), "plain");
});
