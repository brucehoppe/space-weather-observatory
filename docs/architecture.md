# Architecture

```
┌──────────────────────── webview (TypeScript, bundled, no network) ────────────────────────┐
│  src/main.ts shell · observatory.ts · chart.ts · alertBanner.ts · aurora.ts · learn.ts    │
│  Reads a typed Dashboard snapshot; owns the shared selection {time, pinned} and interval.  │
└───────────────────────────────▲──────────────────── narrow typed commands (Tauri IPC) ────┘
                                │
┌───────────────────────────────┴────────── src-tauri (Rust) ───────────────────────────────┐
│ providers.rs  allow-listed HTTPS, timeouts, per-product cadence, coalescing, capped backoff│
│ store.rs      SQLite: verbatim snapshots + SHA-256, alert memory, bounded retention        │
│ snapshot.rs   assemble() → Dashboard; per-product degradation; provenance on every series  │
│ commands.rs   get_dashboard · refresh · settings · evaluate_alert · replay/demo · export   │
│ imagery.rs    Helioviewer metadata + image, acquisition time preserved                     │
│ demo.rs / lessons.rs   frozen real dataset · labelled synthetic scenarios · 4 lessons      │
└───────────────────────────────▲───────────────────────────────────────────────────────────┘
                                │ pure types and functions, no I/O
┌───────────────────────────────┴────────── crates/swo-core ────────────────────────────────┐
│ model.rs   Series/Observation/Quality/Provenance/ForecastRecord/NoaaScale                  │
│ parse/     rtsw · goes · kp · scales · bulletins · ovation · forecast_text                 │
│ timeline.rs nearest-within-tolerance · gap segments · normalize                            │
│ aggregate.rs min–max decimation · flare.rs long-band classification                        │
│ alert.rs   solar-wind alert state machine · interpret.rs versioned rule layer              │
│ export.rs  CSV (formula-guarded) + JSON (canonical) + metadata                             │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

Decisions:

- **Rust + Tauri v2 + TypeScript**, no Go, no Python at runtime. Frontend has no framework;
  charts are a small canvas engine so gap-breaking, log axes and interval rendering are
  under direct control rather than library defaults.
- **Time.** UTC internally; display zone is a setting. One selection shared by all views.
  `datasetNow` is derived from instantaneous series only and capped at assembly time.
- **Replay** reconstructs *what was published at a retrieval time* (that is what stored
  payloads contain), not revised physical event times.
- **No network listener, no auto-updater, no tray, no notifications** (spec §10).
