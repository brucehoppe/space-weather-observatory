# Custom solar-wind alert

A **user-configured measurement condition detector**. It says one thing: upstream
solar-wind speed stayed at or above a threshold *you chose* for a persistence window *you
chose*, with enough valid data to be sure. It is not a NOAA product, produces no G/R/S
level, no outage prediction, no personal risk score and no confidence percentage.

Implementation: `crates/swo-core/src/alert.rs` (pure function, no I/O, no clock of its own).
Acceptance tests: `crates/swo-core/src/alert_tests.rs` (29 cases) and
`src-tauri/src/demo.rs` (scenario walk-throughs). Rule version: `solar-wind-speed-threshold/1`.

## Settings

Stored locally in `settings.json` (per-user config directory). No account.

| Setting | Default | Valid range | Meaning |
|---|---|---|---|
| `enabled` | true | — | Configuration only; evaluation runs only while the app is open. |
| `entry_threshold_km_s` | **500** | 200–3000 | Entry when `speed ≥ threshold`. |
| `persistence_minutes` | **10** | 1–720 | Continuous at-or-above run required to qualify. |
| `hysteresis_km_s` | **25** | 0–200, `< threshold` | Clearing bound is `threshold − hysteresis`. |
| `clearance_minutes` | **5** | 1–720 | Continuous below-bound run required to clear. |
| `cadence_seconds` | 60 | 1–3600 | Product cadence used to derive coverage expectations. |
| `stale_after_minutes` | 15 | 1–1440 | Evaluation pauses when the newest accepted sample is older. |

Defaults are product defaults for reducing nuisance alerts — **not** NOAA criteria and not
physical impact boundaries. Invalid input is rejected with a plain-language message, never
coerced. Saving a change increments `settings_version`, which resets the pending window and
detaches any open episode into history unmodified; past episodes are never re-judged.
"Restore defaults" is one click.

## Numerical comparisons (exact)

- **Entry:** `speed >= entry_threshold`. 500.00 enters; 499.99 does not.
- **Clear:** every accepted sample in the last `clearance_minutes` has `speed < threshold − hysteresis`.
  With defaults, 474.99 clears; 475.00 does not.
- **Hysteresis band** `[threshold − hysteresis, threshold)`: retains the prior qualified state.
- **Onset:** first sample of the continuous ≥-threshold run ending at the newest sample.
- **Episode id:** `ep-<onset unix seconds>-v<settings_version>` — deterministic, so restart
  reconciliation recomputes the same id from the same data.

All timing uses provider measurement timestamps. Duplicate copies of one sample are one
minute of evidence, not thirty. Out-of-order input is re-sorted.

## Data-gap policy

A window (persistence or clearance) is **adequately covered** when:

1. accepted samples ≥ 75 % of `window / cadence`, and
2. the largest gap between consecutive accepted samples (and from window edges) ≤ **3 × cadence** (180 s at 1-min cadence).

Not adequate → *insufficient recent data*; nothing is asserted. Newest sample older than
`stale_after_minutes` → *stale feed*; evaluation pauses. In both cases an open episode is
**retained unchanged** — stopping observations never implies a condition ended. After a
long gap or a spacecraft change, fresh coverage must accumulate before persistence is asserted.

## State model

| State | Meaning |
|---|---|
| Disabled | User turned evaluation off; history kept. |
| Monitoring | Fresh valid coverage; no qualified episode. |
| Pending | Newest sample ≥ threshold, run shorter than persistence. Shows elapsed/required. |
| Active | Qualified sustained episode. One banner per episode; readings update in place. |
| Data unavailable / stale | Paused (`stale_feed`, `no_data`, `insufficient_coverage`); episode retained. |
| Cleared | Clearance rule met with adequate coverage; episode moved to history with evidence. |

**Dismissed** is an acknowledgement attribute on an episode, not a state. It survives
refresh and restart, cannot clear a condition, cannot disable the detector, and dismissed
active conditions remain discoverable ("A dismissed alert condition is still active → Show details").

## Episode record

`id, qualified_onset, onset_speed, last_observation_time, last_speed, peak_speed,
source_product, source_spacecraft, settings_version, rule_version, acknowledged,
cleared_at, clearance_evidence (coverage)`. Persisted in SQLite (`alert_memory`), reconciled
against fresh data on launch — an old banner is never resurrected just because the app
started. Provider corrections update `peak_speed` in place without a new banner.

## Example episodes (from `src-tauri/src/demo.rs`, synthetic and labelled)

| Scenario | Outcome |
|---|---|
| Quiet, 390 km/s | Monitoring throughout. |
| 4-minute spike to 640 | Pending, then Monitoring. **No alert.** |
| Sustained 560+ from t=45 | Active after 10 min; `newly_active` exactly once. |
| Fluctuating 480–495 after qualifying | Stays Active (hysteresis band). |
| Feed stops mid-episode | Data unavailable, episode retained. |
| Clear at 430–440, then 610 | Cleared with evidence; second crossing → distinct episode id. |

## Live / replay / demo separation

Replay and the demonstration dataset evaluate inside a separate `ReplaySession` memory that
is discarded on exit. Live episode history, acknowledgements and the current forecast are
untouched. The banner is labelled "Demo: solar-wind alert" and carries a historical notice.
The chart crosshair changes only the inspection selection; it never calls the evaluator.

## Explanatory text (template; numbers only from validated data)

> Solar-wind speed has remained above your configured threshold. Faster solar wind can
> contribute to geomagnetic activity, but magnetic orientation and persistence also matter.
> Check the official outlook for expected conditions.

Bz (GSM) is shown with its own observation time and frame, or marked stale/unavailable. It
is context, not a causal link. The official NOAA 3-day outlook is shown beside the banner
with issuance time; if unavailable the banner says "Official outlook unavailable" and remains usable.

## Interpretation limits

- A speed reading, high or low, establishes nothing about R- or S-domain hazards.
- No aurora-viewing advice is given for a location: darkness, cloud and latitude relative to
  the oval are not inputs this application has.
- Accessibility: a newly active episode is announced once via a polite live region; refreshes
  are not announced; nothing flashes, pulses or steals focus; all controls are keyboard-operable.
