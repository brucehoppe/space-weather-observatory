import { test } from "node:test";
import assert from "node:assert/strict";
import { snapshotsAt } from "./replay.ts";

test("replay uses the latest available product snapshot without borrowing future data", () => {
  const row = (id: number, product: string, hour: number) => ({ id, product, retrieved_at: `2026-09-06T${String(hour).padStart(2, "0")}:00:00Z` });
  const snapshots = [row(4, "wind", 12), row(1, "wind", 10), row(2, "mag", 10), row(3, "wind", 11), row(5, "aurora", 13)];
  assert.deepEqual(snapshotsAt(snapshots, Date.parse("2026-09-06T11:30:00Z")).sort(), [2, 3]);
  assert.deepEqual(snapshotsAt(snapshots, Date.parse("2026-09-06T09:00:00Z")), []);
});
