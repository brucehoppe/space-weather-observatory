import test from "node:test";
import assert from "node:assert/strict";
import { scaleFor } from "./scale.ts";

test("windows at or below the reference size are not shrunk", () => {
  assert.equal(scaleFor(1600, 900), 1);
  assert.equal(scaleFor(1280, 720), 1);
  assert.equal(scaleFor(390, 844), 1);
});

test("a 27-inch 2560x1440 window scales the interface up by 1.6", () => {
  assert.equal(scaleFor(2560, 1440), 1.6);
});

test("the tighter dimension decides, so wide-but-short windows do not overflow", () => {
  assert.equal(scaleFor(3440, 900), 1);
  assert.equal(scaleFor(1920, 1080), 1.2);
});

test("the scale is capped and steps in twentieths", () => {
  assert.equal(scaleFor(7680, 4320), 2);
  assert.equal(scaleFor(1700, 1000), 1.05);
  assert.equal(scaleFor(Number.NaN, 900), 1);
});
