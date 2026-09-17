# Release readiness

Status as of **2026-09-06 (UTC)**. Evidence paths are relative to the repository root.
"Verified" means it was executed in this session and the result is recorded; "Configured"
means the mechanism exists but has not been run; "Not done" is not done.

Reference machine for measurements: Apple M5, 24 GB, macOS 26 (Darwin 25.6). **This is
not a Windows reference machine**; Windows figures are pending.

| Gate | Status | Evidence |
|---|---|---|
| Parsing | **Verified** | `crates/swo-core/src/parse/*` tests against captured originals: schema rejection (`a_changed_schema_is_an_error_not_an_empty_dataset`), nulls/sentinels (`null_measurements_become_missing_not_zero`, `sentinel_values_become_missing`), duplicates (`normalize_sorts_and_resolves_duplicates_keeping_latest_real_value`), instrument changes (`instrument_source_assignment_is_read_from_the_newest_entry`, per-spacecraft streams), quality flags (`nonzero_quality_flag_marks_the_sample_suspect_not_good`, `electron_contaminated_samples_are_marked_suspect`). 123 core tests. |
| Time | **Verified** | All SWPC timestamp shapes → UTC (`all_observed_swpc_timestamp_shapes_parse_as_utc`, `a_naive_timestamp_is_read_as_utc_not_local`); Kp 3-h boundaries; lookup tolerance (`nearest_reports_no_nearby_sample_instead_of_substituting`); bulletin validity boundaries exact (`validity_boundaries_are_exact`); 3-day forecast period bounded (`coverage_is_bounded_and_horizons_beyond_it_are_not_covered`); replay/demo use the dataset clock (`a_feed_a_few_minutes_behind_the_clock_is_fresh_not_stale`). Local-zone display implemented (`fmtInZone`) and shown in the detail panel when a zone is configured; DST handled by `Intl`. |
| Science | **Verified** | Units on every series (`units_and_frames_are_attached_to_every_series`); Bz GSM vs GSE distinct and signed (`mag_fixture_yields_both_frames_separately`, `signed_bz_retains_negative_values`); log flux exclusion of ≤0 (`zero_negative_and_nan_are_excluded_not_clamped`); class boundaries incl. independent cases C5.2/M2.8/X4.5 (`known_boundaries_classify_exactly`, `independent_reference_cases`); short band never classified; Kp kinds separated (`forecast_product_separates_observed_estimated_and_predicted`, `forecast_kp_stays_out_of_the_observed_series`); lesson claims asserted against data (`the_flare_lesson_matches_the_real_peak_in_the_data` → C5.1 at 10:42 UTC). |
| Geography | **Partially verified** | Grid geometry 360×181, poles addressable, date-line wrap without seam, orientation (auroral > equatorial), tuple order enforced (`parse/ovation.rs` tests). Rendering inspected in browser preview (`docs/screenshots/aurora-1920.png`). **Side-by-side comparison with the official rendering at the same product time not performed.** |
| UI | **Partially verified** | Browser preview at 1440×900, 1920×1080, 2560×1440 and 390 px (`docs/screenshots/`). Synchronized crosshair, pinning, keyboard (arrows/Home/End/±/Escape), splitter keyboard operation, reduced-motion class and media query implemented. Pointer movement not announced; one-time announcement per episode. **Native window inspection was not possible in this session** (screen capture unavailable); focus order reviewed by code, not by assistive-technology run. |
| Failure handling | **Verified (backend), configured (native)** | Offline launch from cache (`hydrate_from_cache`, `last_known_good_survives_a_failed_refresh`); partial outage (`one_failing_product_does_not_disable_the_rest`); stale marking (`old_data_is_reported_stale_rather_than_current`); corrupt cache/settings recovery (`alert_memory_round_trips_and_survives_corruption`, `a_corrupt_file_falls_back_to_defaults_and_is_preserved`); backoff capped (`backoff_grows_and_is_capped`); older response never overwrites newer (`an_older_response_does_not_become_the_latest`). Sleep/resume: polling loops are wall-clock `sleep`s and evaluation uses measurement timestamps, so resume produces a stale mark then recovery (`feed_outage_then_recovery_resumes_the_same_episode`); **not exercised on a real sleep cycle.** |
| Performance | **Verified on dev machine** | `cargo test --release -p swo-core --test perf -- --nocapture` (7-day, 1-min, 10 080 samples): nearest-sample lookup p95 **0.078 ms**; segments 0.105 ms; min–max downsample 0.272 ms (peak preserved); alert evaluate p95 0.071 ms; serialize week 1.03 MB in 2.8 ms. `npm test` (Node 26): TS nearest p95 **1.07 ms**, segments 1.26 ms. Selection target <100 ms met with large margin. Memory bounded by cache limit (`a_size_limit_trims_oldest_snapshots`). **No Windows measurement.** |
| Education | **Verified** | Four lessons with question, ≥3 functioning steps (series focus, time selection, view switch), sourced explanation, reset/return (`src-tauri/src/lessons.rs`; tests assert every referenced series/time exists and every quoted number matches the data). Fourth lesson ("G, R and S are three scales, not one storm score") added 2026-09-17, tested against the same frozen fixture (S1 radiation storm 05 Sep, G1 geomagnetic watch for 08 Sep). |
| Solar Cycle 25 progression | **Verified (parsing/assembly), not GUI-verified** | Two new products (`observed_solar_cycle_indices.json`, `predicted_solar_cycle.json`) added 2026-09-17: endpoints resolved from the official product page, payloads inspected, `-1` sentinel handling verified against the fixture (negative in 2976/3332 and 3069/3332 rows for `observed_swpc_ssn`/`f10.7`). 9 new parser tests + 2 snapshot-assembly tests. Live fetch confirmed via SQLite cache on this machine. Rendered panel (`src/sources.ts`) not inspected in a GUI — see UI row. |
| Sun-image sequence playback | **Verified (backend), not GUI-verified** | `get_sun_image_sequence` fetches evenly-spaced past frames, deduped by provider image id; 2 new unit tests for spacing/clamping. Frontend play/pause/step wiring (`src/sources.ts`, `src/main.ts`) type-checks and builds; playback only starts on an explicit user click (never ambient motion). Not exercised through a GUI. |
| Solar-wind alert (13B) | **Verified** | 29 acceptance tests in `crates/swo-core/src/alert_tests.rs` + 6 scenario walk-throughs in `src-tauri/src/demo.rs` (spike-no-trigger, sustained-trigger, hysteresis, outage, clearance→new episode, quiet). Dismissal persistence, one alert per episode, settings-change isolation, replay/live separation (separate `ReplaySession` memory), crosshair never calls the evaluator. `docs/solar-wind-alert.md`. |
| Packaging | **Not done (Windows)** / **Verified (macOS RC, .app only)** | macOS: `npm run build:macos-zip` → see "Local artifacts" below. DMG bundling is currently broken on this machine (macOS 26.6.2) due to a `create-dmg`/`bundle_dmg.sh` argument error unrelated to this app; `.app` bundling and zipping are unaffected. Windows: `.github/workflows/windows-release.yml` configured; **has not run**; clean-machine install/run/runtime-provisioning/export/restart/upgrade/uninstall **untested**. Signing: unsigned RC; requirements documented in `docs/windows-build.md`, no certificate acquired. |
| Reproducibility | **Verified** | Fixture-only tests (no network in `cargo test`); exports round-trip (`round_trip_matches_the_source_series_exactly`); payload SHA-256 stored and exported; `cargo run --bin dump-demo` regenerates preview fixtures; build commands in README and `docs/windows-build.md`; lockfiles committed (`Cargo.lock`, `package-lock.json`). |

## Two review passes

1. **Data/scientific correctness** — done through the test suites above plus a reading of the
   assembled demo dashboard (`public/dev-fixtures/dashboard.json`): units, frames, kinds,
   statuses and statements checked by hand. Corrections made during the pass: Kp "future
   interval" selection, event-driven freshness, newest-sample on interleaved payloads.
2. **Visual/interaction quality** — done on browser preview only (see UI row). Corrections:
   panel squeeze on short windows, tick density, axis-label collision, wall-clock ages in
   demo, compacted top region, collapsible meaning panel.

## Live-provider smoke (opt-in, separate from CI)

Executed 2026-09-06 ~18:47 UTC on macOS: the running desktop app fetched all 11 products,
stored them with hashes (`~/Library/Application Support/SpaceWeatherObservatory/cache/`)
and evaluated the alert (Monitoring, no episode). Helioviewer imagery endpoints returned
metadata with acquisition times and PNG bytes.

Re-executed 2026-09-17 ~16:41 UTC on macOS after adding the two solar-cycle products: the
running desktop app fetched all **13** products, including `solar_cycle_observed` (512 287
bytes) and `solar_cycle_predicted` (18 173 bytes), confirmed via `sqlite3` query against the
cache (`SELECT product, count(*) FROM snapshots GROUP BY product` returned all 13 keys).

## Local artifacts

`npm run build:macos-zip` (2026-09-06, Apple M5/macOS 26.6.2), rebuilt after adding
vector SVG chart export: produced `target/release/bundle/macos/Space Weather Observatory.app.zip`,
8.7 MB, SHA-256 `38e0e0a07a81f8d390cee1484d8cb657e2ecb23f627c79788ecf4bdfd3c468cb`.
Unsigned. Launched from a prior build of the same `.app` bundle and confirmed it opens
(see "Live-provider smoke" above); the SVG export dialog was exercised in a `tauri dev`
session (native save dialog opens with the correct suggested filename and SVG filter)
but the file save itself was not completed in that session.

Note: `tauri build` with the default `["nsis", "app", "dmg"]` targets fails on this
machine — the vendored `bundle_dmg.sh` (`create-dmg` 1.2.1) exits with "Not enough
arguments" on macOS 26.6.2, unrelated to this app's code. `build:macos-zip` now passes
`--bundles app` to skip DMG bundling; the `.app` bundle and zip are unaffected.

## Explicitly untested / blocked

- Windows x64 build, install, WebView2 provisioning, upgrade, uninstall — **no Windows machine available**.
- Windows ARM64 — not attempted.
- Native macOS window screenshots — screen-recording permission unavailable to this session.
- Official-rendering comparison of the aurora map — not performed.
- Real sleep/resume cycle, clock-change while running — not exercised.
- Code signing and timestamping — no certificate; documented only.
- macOS DMG bundling — broken on this machine's `create-dmg`/`bundle_dmg.sh`; `.app` zip used instead.
