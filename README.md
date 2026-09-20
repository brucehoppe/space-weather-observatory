# Space Weather Observatory

**Follow the Sun. Understand its influence.**

A local, source-grounded desktop application for inspecting solar observations, upstream
solar wind, geomagnetic conditions and official NOAA SWPC short-term forecasts — with a
plain-language "What this means for you" layer, a configurable solar-wind alert, an
official-product aurora view with sun-image sequence playback, a NOAA G/R/S space weather
scales panel, Solar Cycle 25 progression against the official consensus prediction, and
four lessons over a frozen real dataset.

No account, no cloud, no telemetry, no runtime language model. Every number on screen
carries its provider, instrument, timestamp, quality and source URL.

**New here? See `QUICKSTART.md`** — the fastest path from clone to a running app.

**Optional: plain-language reports from a local language model.** `swo-mcp` is a separate
MCP server that lets a model running on your own machine (via Ollama) write a space weather
report for a general reader, keep it up to date as the data changes, and explain every
dashboard reading. The desktop app itself still never loads a model. See
[`docs/local-llm-quickstart.md`](docs/local-llm-quickstart.md).

| | |
|---|---|
| Stack | Rust + Tauri v2 + TypeScript (no framework; small canvas chart engine) |
| Primary target | Windows x64 (NSIS per-user installer). macOS builds for development. |
| Data | NOAA SWPC (`services.swpc.noaa.gov`), NASA SDO via Helioviewer, Natural Earth coastlines |
| Status | **Release candidate on macOS; Windows build configured in CI but not yet run.** See `docs/release-readiness.md`. |

## Screenshots (browser preview of the frozen demonstration dataset)

Browser preview renders the same frozen NOAA dataset the desktop app ships for offline use
and lessons. It is a layout/interaction check, not a desktop or Windows test.

| 1920×1080 | 1440×900 |
|---|---|
| ![Observatory at 1920×1080](docs/screenshots/observatory-1920.png) | ![Observatory at 1440×900](docs/screenshots/observatory-1440.png) |

| Aurora | Learn | 390 px |
|---|---|---|
| ![Aurora view](docs/screenshots/aurora-1920.png) | ![Learn view](docs/screenshots/learn-1920.png) | ![Narrow](docs/screenshots/observatory-390.png) |

## Example: a space weather report written by a local AI model

Written by `qwen3:8b` running on a laptop through Ollama, from live NOAA data in the app's
cache, with `swo-mcp report`. Nothing left the machine. The model wrote only the prose: the
**Readings at a glance** table and the footer are computed by the program from the data, so the
numbers are right whichever model is used. Verbatim file: [`docs/example-report.md`](docs/example-report.md).
Set it up with [`docs/local-llm-quickstart.md`](docs/local-llm-quickstart.md).

> _Sunday 20 September 2026, 15:53 UTC_
>
> **Calm conditions are expected right now.**
>
> **Right now**
>
> The solar wind is moving at about 407 km/s, which is within its ordinary range. The magnetic field's Bz component is northward, which usually means less geomagnetic activity. The latest geomagnetic activity, measured by the Kp index, is quiet with a value of 1.0. The highest Kp in the last 24 hours was 3.0, which is still considered quiet.
>
> **What it means for you**
>
> - **Aurora chances**: Low. Auroras may be seen only at high latitudes, and only if the sky is dark and clear.
> - **Radio/GPS/satellites**: No effects are expected.
> - **Other**: No significant space weather impacts are expected.
>
> **Next few days**
>
> NOAA forecasts geomagnetic activity to remain quiet, with a maximum Kp of 2.0 expected on September 21 and gradually decreasing to 1.7 by September 22.
>
> **Readings at a glance**
>
> | Reading | Latest | Context |
> |---|---|---|
> | Solar-wind speed | 407 km/s | last 120 min: 393 to 414 km/s; ordinary |
> | Solar-wind density | 1.7 /cm3 | last 120 min: 1.5 to 2.1 |
> | Magnetic field Bt | 5.1 nT | last 120 min: 4.7 to 5.3 |
> | Magnetic field Bz (GSM) | 2.3 nT | last 120 min: 1.9 to 3.1; southward for 0 min |
> | Solar X-ray flux | B3.0 | 24 h peak C3.3 |
> | Kp index | 1.00 (NOAA estimate) | quiet; 24 h max 3.00; NOAA forecast max next 24 h 2.00 |
> | NOAA scales now | G0 / R0 / S0 | G geomagnetic, R radio blackout, S radiation; 0 = none |
> | NOAA bulletins | 0 active | 0 issued in the last 24 h |
> | NOAA 3-day forecast | highest Kp per day | 2026-09-20 Kp 3.00; 2026-09-21 Kp 2.00; 2026-09-22 Kp 1.67 |
> | Aurora model (north) | max 10 % | 10 % zone reaches 70 deg N |
> | Aurora model (south) | max 11 % | 10 % zone reaches 52 deg S |
> | Sunspot number | 76 (2026-08) | official prediction 91 (range 79 to 100) |
>
> ---
> Data: NOAA SWPC products cached by Space Weather Observatory; real-time readings retrieved 4 minutes before this report (slower-changing products up to 2 days before). Wording by the local language model `qwen3:8b`; the table above is computed directly from the data. This is not an official forecast. Official products: <https://www.spaceweather.gov>. Data fingerprint `e52363f1e279fdf2`.

## Run from source

Prerequisites: Rust (stable), Node 24+, and the Tauri platform prerequisites for your OS
(<https://v2.tauri.app/start/prerequisites/>). No Python.

```bash
npm ci
cargo test --workspace         # 225 deterministic tests, fixture-only, no network
npm test                       # 12 frontend unit tests (Node)
npm run tauri dev              # desktop app with hot reload
npm run tauri build            # installer for the current platform
```

Browser preview only (no backend, frozen dataset): `npm run dev` → <http://localhost:5173/>
(`?view=aurora|learn|sources`). Regenerate its fixtures with `cargo run --bin dump-demo`.

Use **Saved snapshots** to replay retained provider data at a chosen retrieval time. Products
without a retained snapshot at or before that time remain unavailable. **Return to live** exits replay.

Refresh the captured provider originals (network, opt-in): `scripts/capture-fixtures.sh`,
then update the date in `docs/sources.md`.

## Where things live

| Path | Purpose |
|---|---|
| `crates/swo-core/` | Pure scientific core: parsers, model, time alignment, aggregation, flux class, alert state machine, interpretation rules, export. No I/O. |
| `crates/swo-mcp/` | Optional MCP server over the cache, read-only: dashboard, Kp, flare, interpretation and explanation tools, saved plain-language reports, and a built-in Ollama driver (`report`, `ask`). Separate process. |
| `src-tauri/` | Desktop backend: allow-listed acquisition, SQLite cache, snapshot assembly, commands, imagery, demo dataset, lessons. |
| `src/` | Frontend: shell, chart engine, alert banner, aurora, learn, sources. |
| `fixtures/captured/` | Verbatim NOAA originals (2026-09-06T18:04Z; solar cycle products 2026-09-17) used by tests and as the frozen demo dataset. |
| `docs/` | `sources.md`, `solar-wind-alert.md`, `architecture.md`, `windows-build.md`, `privacy.md`, `license-proposal.md`, `release-readiness.md`, `implementation-status.md`. |
| `.github/workflows/` | `ci.yml` (deterministic tests, lint), `windows-release.yml` (x64 installer + SHA-256). |
| `scripts/` | `capture-fixtures.sh` (refresh captured originals), `build-windows.ps1` (local Windows build, same steps as CI), `install-mcp-macos.sh` / `install-mcp-windows.ps1` (install `swo-mcp` per user, optional background report updater). |
| `LICENSE`, `THIRD-PARTY.md`, `CONTRIBUTING.md` | Licence, third-party notices, contribution guide. |

## Configuration reference

`settings.json` in the per-user config directory (macOS `~/Library/Application Support/SpaceWeatherObservatory/`,
Windows `%APPDATA%\SpaceWeatherObservatory\`):

| Key | Default | Notes |
|---|---|---|
| `display_time_zone` | `"UTC"` | IANA name; "tonight" wording needs a known zone. |
| `region_label` | `null` | Free text used only to qualify wording. |
| `alert.*` | see `docs/solar-wind-alert.md` | Threshold 500 km/s, persistence 10 min, hysteresis 25 km/s, clearance 5 min. |
| `cache_limit_mb` | 256 | Enforced on start and after each refresh; last-known-good per product always survives. |
| `snapshot_retention` | 48 | Snapshots kept per product for replay. |
| `force_reduced_motion` | false | OS preference is honoured regardless. |

## Operations notes

- Data locations are per-user; the install folder holds no data. Uninstall retains data
  (`docs/windows-build.md` describes removal).
- One failed product degrades its own panel only; last-known-good is kept and marked stale.
- Offline launch shows cached data with per-product freshness. Backoff is exponential and
  capped at 30 minutes so an offline machine is not hammered.
- Closing the window stops all polling; there is no tray, notification or background service.

## Licence

MIT — see `LICENSE`. Third-party notices: `THIRD-PARTY.md`.
