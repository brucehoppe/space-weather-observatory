# Implementation status

Maintained so another session can resume. Dates are absolute (UTC).

## Decisions

| Date | Decision | Why |
|---|---|---|
| 2026-09-06 | Rust + Tauri v2.11 + TypeScript, no Go, no runtime Python | New repository; spec preference; single backend. |
| 2026-09-06 | Endpoints: `json/rtsw/rtsw_wind_1m.json`, `json/rtsw/rtsw_mag_1m.json`, `json/goes/primary/xrays-1-day.json`, `products/noaa-planetary-k-index-forecast.json`, `products/noaa-scales.json`, `products/alerts.json`, `json/ovation_aurora_latest.json`, `text/3-day-*.txt` | Resolved from product pages and inspected. The commonly cited `products/solar-wind/*-2-hour.json` paths return 404. |
| 2026-09-06 | Spacecraft is provenance data (`active` flag), never a product name | RTSW interleaves SOLAR1/ACE/IMAP; SWPC switches the active stream. |
| 2026-09-06 | Frozen demo dataset = real captured originals; alert demonstrations = separately labelled synthetic scenarios | Captured interval is quiet (max 381 km/s). Spec forbids inventing a storm; §13B permits clearly labelled demo values. |
| 2026-09-06 | Point-in-time bulletins (ALERT/SUMMARY) get status `issued`, never `active` | No validity window exists to be "in effect". |
| 2026-09-06 | Event-driven products' freshness judged by fetch time, not newest item | A quiet bulletin feed is not a stale feed. |
| 2026-09-06 | Staleness threshold = 10 cadences, floor 10 min | RTSW routinely lags 3–6 min; 5 min cried wolf. |
| 2026-09-06 | `datasetNow` from instantaneous series only, capped at assembly time | Kp interval starts (incl. a provider-stamped future interval) pushed "now" ahead of data. |
| 2026-09-06 | Hand-written canvas chart engine instead of a chart library | Direct control of gap-breaking, log exclusion, interval rendering, shared crosshair. |
| 2026-09-06 | Paired polar (azimuthal equidistant) aurora views, not a 3-D globe | Spec allows "paired polar views"; avoids a heavy graphics dependency; fallback text view included. |
| 2026-09-06 | Browser-preview shim (`src/devMock.ts`) serving frozen JSON | Screen capture and the Chrome extension were both unavailable to this session; needed for the 390 px / multi-resolution checks the spec asks for. Inert inside the desktop shell. |
| 2026-09-06 | Replay reconstructs "what was published at retrieval time" | That is what stored payloads contain; revised event times would need provider version history. |

## Completed

- Source contracts verified, captured, documented (`docs/sources.md`).
- `swo-core`: model, 7 parsers, time alignment, aggregation, flux classification, alert state machine (29 acceptance tests), interpretation rule layer, export — 123 unit tests + 5 perf tests.
- Backend: allow-listed fetcher with backoff/coalescing, SQLite store (idempotent, bounded, LKG-preserving), snapshot assembly with per-product degradation, 21 narrow commands, imagery, demo, lessons — 60 tests.
- Frontend: full-height shell, alert banner + settings dialog + explanation, readings with gauge/sparkline, "What this means for you", 5-panel synchronized chart stack with keyboard navigation, side panel (detail / status / outlook / bulletins), timeline, footer, aurora paired polar view with coastlines, learn view with 3 lessons + 6 scenarios, sources view with cache controls — 10 Node tests.
- Live run on macOS verified: all 11 products fetched, stored, alert evaluated (monitoring).
- Docs: sources, alert, architecture, windows-build, privacy, licence proposal, README.
- CI: `ci.yml` (deterministic), `windows-release.yml` (unsigned x64 RC + SHA-256).

## Exact failures / open issues

- **Windows verification not performed.** No Windows machine was available; the workflow is configured but has not run. Every Windows-specific claim (install, WebView2 bootstrap, upgrade, uninstall) is untested.
- **Screen capture of the native window unavailable** in this session (macOS screen-recording permission and Chrome extension both absent). Native inspection was done via the app's own store/logs; visual inspection via headless-Chrome browser preview.
- Chart PNG/SVG export is **not implemented** (CSV/JSON export is). Listed as next action.
- Window bounds are **not yet persisted** (`window_bounds` setting exists; no save/restore wiring).
- Local display time zone setting exists but the UI shows UTC everywhere; `fmtInZone` is implemented and unused in views.
- Sun-image sequence play/pause not implemented (single latest frame per passband).
- Historical replay UI (`list_snapshots` / `enter_replay`) exists as commands but has no UI beyond the demo dataset.
- `three_day_geomag_forecast` is parsed and shown in status but not rendered as its own panel.
- Uninstall-time data purge option not offered by the installer (manual path documented).

## Next actions (in order)

1. Run `windows-release.yml` on a Windows runner; execute the clean-machine checklist in `docs/windows-build.md`; record results.
2. Chart PNG/SVG export with title/interval/units/source/status labels (spec §12).
3. Persist and validate window bounds across monitors.
4. Wire `display_time_zone` into the timeline/detail panel with explicit zone labels.
5. Replay picker UI over stored snapshots.
6. Apply the licence after the owner confirms attribution wording.
