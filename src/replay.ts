import type { SnapshotRef } from "./types";

/** Latest available payload per product at or before the selected retrieval time. */
export function snapshotsAt(snapshots: SnapshotRef[], at: number): number[] {
  const selected = new Map<string, SnapshotRef>();
  for (const snapshot of snapshots) {
    const time = Date.parse(snapshot.retrieved_at);
    if (!Number.isFinite(time) || time > at) continue;
    const prior = selected.get(snapshot.product);
    if (!prior || Date.parse(prior.retrieved_at) < time ||
      (Date.parse(prior.retrieved_at) === time && prior.id < snapshot.id)) {
      selected.set(snapshot.product, snapshot);
    }
  }
  return [...selected.values()].map((s) => s.id);
}
