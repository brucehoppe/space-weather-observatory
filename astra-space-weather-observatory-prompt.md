# Astra build prompt — Space Weather Observatory

**Repository:** `space-weather-observatory`  
**Product:** Space Weather Observatory  
**Tagline:** Follow the Sun. Understand its influence.  
**Primary deliverable:** A polished Windows desktop application with a tested installer `.exe`.

## Start instruction

Open the intended repository in your coding environment, select Astra, and attach this file. Say: “Read this specification and implement the application through its release gates, including Windows packaging. Start with repository inspection.”

This document is a specification, not proof that software has been implemented or certified. The accompanying dashboard concept uses illustrative data and is a layout reference, not a scientific fixture.

---

## 1. Assignment

Act as lead engineer, scientific visualization designer, and release reviewer. Build Space Weather Observatory: an educational, source-grounded desktop application for inspecting solar observations, upstream solar wind, geomagnetic conditions, and official short-term forecasts.

Inspect the actual repository and instructions first. Reuse suitable existing work; do not overwrite unrelated changes. This is separate from the user's Earthquake Observatory and Hazard Atlas repositories. Do not modify their files, databases, deployment resources, or remotes. If reusing a shared component is helpful, inspect its license and adapt an independent copy deliberately.

Implement the product, not only a scaffold or static dashboard. Continue through tests, running UI inspection, scientific review, and release packaging. Record genuine blockers and complete unaffected work. Ask only when missing information materially prevents safe progress.

## 2. Language and desktop architecture

For a new repository, prefer **Rust + Tauri + TypeScript**:

- Rust manages provider access, validation, caching, SQLite storage, settings, imports/exports, and narrowly scoped desktop commands.
- Tauri supplies the native application window and platform packaging.
- TypeScript supplies the dashboard, time-series interaction, education, and an aurora display using a maintained graphics library.
- Python is optional for independent validation and dataset preparation. Users must not install Python to run the application.
- Go is not required alongside Rust. If the actual repository has a mature Go backend, document whether preserving it is better than introducing a second backend.

Verify current stable dependency versions and official documentation. Pin dependencies, retain lockfiles, and keep the architecture a local modular application. Do not add cloud services, user accounts, runtime LLM calls, telemetry, or a mandatory API subscription.

Use a typed internal interface between acquisition, normalized scientific datasets, and visual components. Keep data processing out of UI rendering code. Handle long operations asynchronously with cancellation. Bound cache storage and memory. Bundle frontend assets; do not load arbitrary remote websites inside a privileged desktop view.

## 3. Windows executable requirements

Deliver a native application executable, for example `SpaceWeatherObservatory.exe`, and a user-friendly Windows installer such as `SpaceWeatherObservatory-Setup-x64.exe`. The exact packaging output names may follow the toolchain, but document them clearly. Windows x64 is the primary release target; ARM64 is optional and must be separately tested.

A portable executable is optional. Do not claim that one executable is dependency-free: verify WebView2/runtime requirements and required companion assets. The installer must handle the selected runtime strategy and explain whether installation needs internet. Ordinary operation must not require Rust, Node, Python, or developer tools on the user's machine.

Use a Windows build environment for the primary release. When development happens on macOS, provide and configure a Windows CI build or documented Windows build procedure; a macOS development run is not proof that the `.exe` works. Include platform prerequisites and build commands based on official sources:

- [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri distribution](https://v2.tauri.app/distribute/)

Test install, launch, data refresh, offline operation, export, restart, upgrade, and uninstall on Windows. Keep local data in appropriate per-user application directories, not the installation folder. Define whether uninstall retains user data and offer an explicit supported removal path. Avoid requiring administrator privileges unless the chosen installer genuinely needs them.

Distinguish an unsigned local release candidate from a signed public release. Document signing and timestamping requirements without inventing credentials or buying certificates. Do not promise that signing eliminates reputation warnings. Produce SHA-256 checksums and release notes. Do not claim a configured CI job passed unless it ran. If Windows execution is unavailable, deliver the implementation and build setup but mark Windows verification incomplete.

## 4. Data sources and contracts

Use NOAA SWPC as the primary source. Resolve current product pages, then follow their official links to machine-readable data. Do not guess filenames, scrape chart pixels, or treat a failed request as an empty scientific dataset.

| Product | Required information | Starting reference |
|---|---|---|
| Solar wind | Speed, density, magnetic-field magnitude and Bz, with spacecraft/product provenance | https://www.swpc.noaa.gov/products/real-time-solar-wind |
| GOES X-ray flux | Flux, channel/passband, satellite identity and quality information when provided | https://www.swpc.noaa.gov/products/goes-x-ray-flux |
| Planetary K-index | Provider-estimated/observed Kp and separately identified forecast values | https://www.swpc.noaa.gov/products/planetary-k-index |
| NOAA scales | Separate G, R, and S descriptions and sourced status where available | https://www.swpc.noaa.gov/noaa-scales-explanation |
| Aurora | Official OVATION product and its actual issue/valid times | https://www.spaceweather.gov/products/aurora-30-minute-forecast |
| Machine-readable services | Resolve actual available products and schemas | https://services.swpc.noaa.gov/ |

Some SWPC product pages redirect to spaceweather.gov; follow verified canonical links. Record verification dates and resolved endpoints in `docs/sources.md`. Do not bake a current spacecraft assignment into the product name. SWPC may change the spacecraft providing an active solar-wind stream. Preserve that provenance. See [SWPC solar-wind documentation](https://www.swpc.noaa.gov/products/real-time-solar-wind).

Check data cadence, retention window, units, coordinate frame, quality flags, missing-value conventions, and terms for each product before implementation. Never assume all feeds use the same schema or sampling frequency. If a needed product is unavailable, use an attributed fixture for development and record the live integration blocker.

## 5. Scientific data model and handling

Store normalized observations alongside reproducible source snapshots or source hashes. Each series needs a provider/product ID, instrument/spacecraft if known, measurement name, unit, timestamp, time precision, quality state, retrieval time, and revision metadata where available.

Forecasts additionally require issuance and valid times, source model, and forecast status. Preserve interval start/end for interval-valued indices. Do not replace provider metadata with the application's download time.

Use UTC internally with an explicit display time zone. Support nullable measurements, sentinels, out-of-order rows, duplicated timestamps, corrections, and missing intervals. Use idempotent transactions and prevent older responses overwriting newer versions without a documented rule.

Do not connect chart lines across significant data gaps. Mark interpolated or downsampled data and retain raw values for inspection/export. Aggregate with documented methods that preserve important peaks; do not average away flare maxima while presenting the result as raw data.

Use one shared selected time and interval across views. Align different sample cadences with an explicit lookup tolerance. Show “no nearby sample” rather than substituting an unrelated measurement. No silent interpolation of categorical status or interval indices.

## 6. Required desktop dashboard

Use the entire desktop content area, both width and height. No fixed-width centred page wrapper. Start in a large resizable native window and remember safe window bounds across monitors. Offer maximize/fullscreen through normal platform conventions.

Recommended composition:

- Compact top navigation: Observatory, Aurora, Learn, Sources; persistent Live/Replay state and displayed time.
- Compact current-reading strip with units and per-product age. Use only meaningful readings: solar-wind speed, Bz, Kp and X-ray flux/class as appropriate.
- A large central stack of synchronized scientific charts, with a common time axis and crosshair.
- A resizable right panel for selected measurement, source status, official bulletin details if implemented, or educational explanation.
- A bottom timeline with pause, seek, interval presets and Return to now.
- A compact footer with real source references and application version.

Use resizable and collapsible panels with sensible minimum sizes, keyboard-operable splitters, and Reset layout. On narrow windows collapse secondary panels before sacrificing chart legibility. Verify at 1440×900, 1920×1080, 2560×1440 and 390px browser-preview width; distinguish browser-preview testing from native Windows tests.

Use restrained, high-contrast scientific styling. Show quantities and units beside readings. Keep G/R/S statuses separate; never invent a combined storm score. Missing information must not become “quiet” or “normal.” Maintain a clear visual boundary between measurements, forecasts, and educational illustrations.

## 7. Required scientific charts and interactions

Implement at least these coordinated views:

1. Solar-wind speed in km/s and density in particles/cm³ in separate aligned panels or explicitly labelled axes.
2. Bz and total magnetic field in nT, identifying the coordinate frame, with a zero reference for signed Bz.
3. GOES X-ray flux in W/m² on a logarithmic axis, with channel and satellite stated. Explain exclusions for zero/invalid values.
4. Kp on its appropriate interval scale, separating estimates/observations from forecast intervals.

Support pointer hover, click/tap to pin a time, keyboard time navigation, zooming an interval, reset, and export. Clicking a measurement opens a persistent detail view with its precise value, timestamp, data age, instrument, source link and plain-language meaning. Keep crosshair updates responsive without announcing every pointer movement to assistive technology.

Flux-derived flare labels must use the correct passband and sourced boundaries. Distinguish an instantaneous flux class from an officially identified flare event and its peak class. Do not infer an Earth-directed CME from an X-ray spike. See [NOAA GOES product documentation](https://www.swpc.noaa.gov/products/goes-x-ray-flux).

Do not render Kp as a continuously sampled local magnetic field or assume it is a personal aurora forecast. Distinguish NOAA estimates from definitive indices when the product makes that distinction. Translate scales only using verified official definitions; make app-derived classifications distinguishable from published NOAA status.

## 8. Aurora exploration

Include a rotatable Earth or paired polar views showing an official aurora product. Use published geographic geometry and the product's documented coordinate system, units and spatial grid. Do not invent an auroral oval from a scalar Kp value. Validate longitude wrapping, poles, grid orientation, transparency, and colour scale against the official rendering at the same product time.

Show model name, issue/observation/valid times as supplied, legend, and product age. The named NOAA “30-minute” product describes a variable short-term lead time; preserve its actual timestamps rather than promising a fixed arrival time. See [NOAA aurora product](https://www.spaceweather.gov/products/aurora-30-minute-forecast).

Describe the model quantity accurately. It is not automatically the user's probability of seeing aurora; daylight, clouds and viewing conditions matter. Do not estimate local visibility without additional validated inputs. No geolocation permission is needed for version 1. A user-selected geographic point may show coordinates and model-grid information with clear limitations.

Provide slow optional auto-rotation, off initially, with pause on selection/drag and explicit resume. Distinguish camera movement from simulation time. Preserve a meaningful fallback map or textual view if graphics support fails. Date any Sun imagery separately if included; imagery is optional for version 1.

## 9. Replay, education and scientific interpretation

Ship an attributed frozen demonstration dataset and three complete lessons:

- Read solar-wind speed and magnetic orientation together without treating either as a guaranteed storm outcome.
- Distinguish a solar flare, upstream solar-wind measurements, and a later geomagnetic response; align times but do not claim causality from coincidence alone.
- Distinguish observations from an aurora forecast and explore what the forecast actually says.

Each lesson needs a question, a small sequence of functioning interactions, sourced explanation, reset and return to the previous view. Keep content structured and reviewable. Do not invent a historical storm or modify measurements to fit a lesson.

Historical replay must use saved or legitimately retrieved data within supported coverage. Do not promise arbitrary dates from rolling feeds. State whether a snapshot reconstructs physical event times or what was published at that moment. The latter requires issuance/version history.

For an optional Sun–Earth schematic, label distances and sizes as schematic and animation as illustrative. Do not present particle travel time computed from a single speed as a validated forecast. Upstream observation time, estimated Earth-arrival time and geomagnetic response time are separate quantities.

## 10. Reliability and desktop security

Centralize acquisition inside the native backend. Poll at documented sensible cadences, coalesce requests, respect service limits, use bounded timeouts and provider-appropriate backoff, and preserve last-known-good data. Handle sleep/resume, clock changes, offline transitions, corrupted cache entries and server schema changes. One failed product must not disable the whole dashboard.

Show per-product freshness, coverage and errors with accessible retry. Avoid aggressive retries while offline. Do not refresh all providers every animation frame. Include a configurable cache limit and cleanup, plus explicit snapshot retention.

Use Tauri's least-privilege capabilities and narrowly typed commands. Validate export paths through native dialogs and prevent arbitrary file/shell access from UI input. Escape external text, constrain outbound hosts, set a suitable content security policy, and open verified external links in the system browser. Avoid a network-listening service unless actually required; if used, bind loopback and document its protections.

Do not add an auto-updater until its signing, verification and rollback requirements are implemented. Background notifications, system-tray persistence and launch-at-login are outside version 1 unless requested. Closing the application should stop its work cleanly.

## 11. References, privacy and attribution

Provide an accessible footer linking NOAA SWPC, the relevant product references, scientific methods, map/globe credits, and a complete Sources & References page. Credit only assets and sources actually used. Respect provider-required visible map credits and license notices.

The user's earlier request for public personal-email coding credit was withdrawn. Do not add that credit, a guessed mailto link, or institutional endorsement. If an existing repository establishes the agreed replacement attribution method, preserve it. Otherwise leave new personal attribution unspecified and ask for exact wording only when needed; continue independent work.

Document actual external requests and local storage in a concise privacy statement. No analytics or tracking by default. Preserve authorship and license notices of reused code. Propose an appropriate code license with its ownership assumptions explicit before publication.

## 12. Exports and reproducibility

Export selected scientific series as UTF-8 CSV and JSON, preserving units, source timestamps, quality flags and series identity. Export charts as PNG or SVG with readable title, interval, units, source and data-status labels. Match exported values to the selected snapshot, not a partially refreshed live view.

Include companion metadata with product IDs, source URLs, filters, displayed time zone, raw/aggregated status, retrieval/forecast times, app/schema versions and dataset hash. Protect CSV text fields from spreadsheet formula execution while preserving canonical source values in JSON. Verify exports independently against fixtures.

## 13. Release gates

Required evidence includes:

| Gate | Verification |
|---|---|
| Parsing | Headers/schema changes, null/sentinel values, duplicates, instrument changes and quality flags |
| Time | UTC/local display, interval boundaries, lookup tolerances, forecast issue/valid times and replay |
| Science | Correct units, Bz frame/sign, log flux, class boundaries, Kp semantics and forecast separation |
| Geography | Aurora grid orientation, date line, poles and official-product comparison |
| UI | Real chart selection, synchronized crosshair, resizing, focus, keyboard operation and reduced motion |
| Failure handling | Offline launch, partial source outage, stale data, resume after sleep and cache recovery |
| Performance | Measured reference-machine results with realistic seven-day minute-cadence series; responsive inspection and bounded memory |
| Education | Three reproducible lessons with source-supported explanations |
| Packaging | Windows build, clean-machine install/run, runtime provisioning, export, restart, upgrade and uninstall |
| Reproducibility | Fixture-only tests, matching exports, versioned snapshots and documented build commands |

Use deterministic tests in CI and separate opt-in live-provider smoke tests. Cross-check scientific transformations against known independent cases. Benchmark p95 selection latency and render performance; target selection under 100ms on the stated reference machine and investigate visible freezing. Do not invent performance results.

Run and inspect the actual application. Perform two review passes: data/scientific correctness, then visual/interaction quality. Verify a light or dark theme thoroughly before adding more themes. Automated accessibility checks do not replace keyboard and focus review. Record untested platforms, blocked live integrations, and signing status explicitly.

## 13A. Human-centred dashboard and forecast — required refinement

Make the product meaningful to a person who does not know what the instruments imply. Place a concise **What this means for you** panel near the top of the full-desktop dashboard. The observation notebook is deferred; do not implement notebook navigation or note-taking in this release. Keep educational detail available on demand.

Add a solar-wind speed gauge in km/s with smooth transitions between actual new values and a recent-history sparkline. Add a signed Bz indicator with a zero reference and frame label. Use neutral speed colours: speed alone is not an impact or danger score. Hold last-known readings on outage and mark them stale; do not keep simulating movement. In demo mode label all simulated values clearly.

Make real solar imagery required, replacing schematic imagery in normal operation. Support at least NASA SDO/AIA 193 Å and 304 Å, retaining acquisition times, false-colour labels and source credits. Verify current NASA SDO/Helioviewer endpoints and image metadata. Refresh at a provider-appropriate cadence; changing the clock must never silently present a current image as historical. Offer a dated image sequence with play/pause if an actual sequence is available; never animate unrelated wavelengths as successive moments. Keep imagery and measurements temporally distinct when acquisition times do not match.

### Now, next, and later

Provide these distinct views:

- **Now:** current sourced observations and official active status, with age and geographic scope. Describe potential implications; claim actual outages or impacts only with independent evidence.
- **Near-term:** the official short-lead aurora forecast and relevant active warnings with their stated validity. Do not arbitrarily extend a product to “the next few hours.”
- **Tonight and next three days:** published NOAA outlooks and appropriate regional products, showing issuance time and forecast period. Render unavailable horizons as unavailable.

Use official sources:

- NOAA 3-Day Forecast: https://www.spaceweather.gov/products/3-day-forecast
- NOAA 3-Day Geomagnetic Forecast: https://www.spaceweather.gov/products/3-day-geomagnetic-forecast
- NOAA Alerts, Watches and Warnings: https://www.spaceweather.gov/products/alerts-watches-and-warnings
- NOAA scale explanations: https://www.spaceweather.gov/noaa-scales-explanation
- NOAA geomagnetic-storm science: https://www.spaceweather.gov/phenomena/geomagnetic-storms

Resolve current machine-readable products from these pages and test parsing against captured originals. Preserve cancellations, superseding bulletins, validity intervals, probabilities and geographic qualifiers. A previously issued warning must not remain active after expiration. Never interpret missing bulletins or a failed fetch as confirmation of quiet conditions.

### Practical interpretation

Show a short headline, relevant timeframe, affected activity/region, and expandable “Why?” explanation. Useful categories are aurora viewing, HF radio, precision GNSS/navigation, and satellite/power-system context when the official level warrants it. Match effects to the correct G/R/S domain and geographic qualifiers; do not attach every possible space-weather impact to every elevated reading.

Use a reviewed, versioned rule/template layer to translate sourced statuses and forecasts into plain language. Forecasts come from the named forecasting provider; observation-based commentary is labelled interpretation. Do not require an LLM at runtime. Do not predict a user's phone, power, flight, or GPS failure from general scale descriptions. Avoid presenting high-altitude/space radiation implications as a surface health alert.

Allow an optional manually selected region and interests. Preserve a useful global default and show when only global information is available. Do not claim a precise city-level forecast from planetary Kp. “Tonight” requires a known time zone; otherwise use explicit UTC intervals. Cloud cover, darkness and other local visibility inputs must be obtained and identified before offering local aurora-viewing advice. Never turn the aurora model's quantity into an invented personal viewing probability.

An explanatory pattern for elevated solar-wind speed is: “Faster solar wind is arriving upstream of Earth. Its geomagnetic effect also depends on the magnetic field's orientation and persistence. See the official outlook below.” This is a template, not a claim about current conditions. Generate its numeric context only from valid observations and avoid causal certainty.

If a provider forecasts a storm, express the provider's expected level and window, then summarize relevant possible effects. Say “forecast” or “possible,” not “will happen.” Show numeric probabilities only when supplied by a documented source, retaining their event definition and period. Do not invent confidence percentages or arrival countdowns. Where sources disagree or fresh observations diverge from an issued forecast, display both with timestamps rather than secretly modifying the official forecast.

### Additional release checks

Test quiet, elevated, severe, stale, missing, cancelled, expired and contradictory-source cases. Verify geographic qualification, local-date conversion, daylight saving, forecast periods, correct G/R/S effects, and the distinction between current observations and future predictions. An expired forecast must never read as current. Save evidence snapshots so each generated headline can be traced to its inputs, rule version, source and issuance time. Confirm that the top panel is useful without understanding the gauges and remains readable at narrow widths.


## 13B. Solar-wind dashboard alert — required implementation

Implement an in-application solar-wind alert that makes an important reading understandable at a glance. This request is for a dashboard feature, not a scheduled ChatGPT notification, email, OS notification, system-tray monitor, or background service. Do not add those delivery mechanisms.

### Placement and visual design

Place the alert below the compact application header and above the main instruments. It spans the available workspace width and reflows gracefully when narrow. Use a restrained amber accent for an active custom threshold condition, neutral styling for ordinary status, and explicit text rather than colour alone. Reserve official warning styling and G/R/S labels for actual official products.

Present:

1. **Identity:** “Custom solar-wind alert” or “Demo: solar-wind alert,” clearly separate from NOAA warnings.
2. **Headline:** “Solar wind above your threshold,” using user settings and qualified data.
3. **Evidence:** latest accepted speed in km/s, configured threshold, persistence duration, observation time, and feed age. Show concurrent Bz with its own time and frame, or mark it unavailable/stale.
4. **Meaning:** a short scientifically reviewed explanation and a link to the official outlook; do not invent a forecast from the threshold.
5. **Actions:** What does this mean?, Configure, and Dismiss. Provide a quiet route to review dismissed active conditions.

Suggested illustrative wording when conditions qualify:

“Solar-wind speed has remained above your configured threshold. Faster solar wind can contribute to geomagnetic activity, but magnetic orientation and persistence also matter. Check the official outlook for expected conditions.”

This text is a template. Populate numbers only from validated data; do not ship the preview's synthetic values as real observations.

### Settings and evaluation contract

Provide an explicit enable/disable control and local settings. Suggested initial product defaults are a **500 km/s entry threshold**, **10-minute persistence requirement**, and **25 km/s hysteresis margin**, but expose and document them as customizable application settings, not NOAA alert criteria or physically guaranteed impact boundaries. Let the user restore defaults. Validate values and units; do not silently coerce invalid input.

An enabled setting is configuration, not consent to run while the app is closed. Evaluate only while the application is running. Settings must persist locally; no account is required.

Implement a pure, testable evaluation function separated from rendering. Input should include validated speed observations, source/product identity, quality, timestamps, settings, and prior episode state. Use measurement timestamps, not download count, render frames or repeated copies of the same sample. Require adequate continuous coverage over the configured persistence window using a documented gap tolerance derived from the actual product cadence. If coverage or quality is insufficient, show “insufficient recent data” rather than manufacturing a sustained condition.

Define crossing comparisons exactly: entry at or above the threshold; clearing below threshold minus hysteresis margin for a documented clearance duration, initially five minutes with adequate coverage. Include boundary tests. A reading in the hysteresis band retains the prior qualified state. These are product rules for reducing nuisance alerts, not scientific forecasts.

State model:

- **Disabled:** user turned off custom evaluation.
- **Monitoring:** fresh valid coverage; no qualified threshold episode.
- **Pending:** threshold exceeded but persistence window not yet satisfied.
- **Active:** qualified sustained threshold episode.
- **Data unavailable/stale:** evaluation paused; retain previous episode history without implying it ended.
- **Cleared:** sufficient valid evidence meets the clearance rule; return to monitoring while retaining episode history.

Treat Dismissed as a presentation/acknowledgement attribute on an episode, not as a scientific state. Dismissing cannot clear a condition or disable the detector.

### Episode lifecycle and nuisance prevention

Emit at most one initial visible alert per episode; update its readings in place. Dismissal remains attached to that episode across normal refresh and restart. A genuinely cleared condition followed by a newly qualified crossing may create a new episode. Do not use a timeout alone to repeatedly resurrect a dismissed alert.

Store episode identity, qualified onset, last accepted observation, clearance evidence, settings/rule version, acknowledgement state, and referenced dataset/source IDs. On restart, reconcile against fresh data; do not restart an old banner solely because the application launched. When a user changes thresholds, reset the pending evaluation window, record the configuration change, and do not rewrite past episodes under the new settings.

Outages, sleep/resume, out-of-order updates and source changes need explicit treatment. Never infer that a storm ended because observations stopped. After a long gap or spacecraft transition, gather sufficient fresh coverage before asserting persistence unless product documentation supports continuity and that continuity is recorded. Revised historical samples may correct episode history but should not trigger repeated intrusive banners; preserve the correction provenance.

### Practical meaning and official forecasts

The speed alert says a configured measurement condition occurred. It is not an official geomagnetic-storm warning and must never generate G/R/S levels, outage predictions, personal risk scores or invented confidence percentages.

Show a relevant official forecast summary when available, with provider, issuance time, valid period, region, and a link to the full source. Handle cancelled, superseded and expired products. If unavailable, display “Official outlook unavailable” and keep the custom measured alert usable. A low speed reading does not establish that all space-weather hazards are absent.

Explain that aurora prospects and technological effects depend on additional factors. Use the practical interpretation rules in section 13A. Do not tell the user to expect aurora at their location without suitable regional information, darkness and weather context. Bz adds observation context; it is not an automatic causal link or personal forecast.

### Instrument and replay integration

Synchronize the speed gauge, history chart and alert with the same accepted live dataset. A chart crosshair used to inspect an old reading must not retarget the live detector. Separate live monitoring time from the inspection/replay cursor.

In replay or offline demonstration mode, evaluate only inside a separate replay state and label the banner as historical/demo. It must not overwrite live episode history, user acknowledgements or current forecast status. Do not display today's forecast as if it applied to a historical event. In the visual mockup the gauges may animate synthetic samples, but the production application must animate only new readings or deliberate replay.

Respect reduced motion. Do not flash, pulse endlessly, play sounds or steal keyboard focus. Announce a newly active episode once to assistive technology, not every numerical refresh. Provide keyboard-operable configuration and details controls. Dismissed alerts must remain discoverable without obscuring the main charts.

### Acceptance tests and release evidence

Add deterministic tests for:

- Below-threshold data; exact entry and clearance boundaries; sustained crossing; brief spike; hysteresis-band fluctuations.
- Duplicate samples, cadence gaps, null/invalid quality, missing Bz, stale speed, out-of-order observations and retrospective corrections.
- Feed outage during an active episode, sleep/resume, restart reconciliation and source changes.
- Dismissal persistence, one alert per episode, clearance followed by a new episode, and settings changes during pending/active conditions.
- Replay/live separation, crosshair inspection without alert mutation, and demo isolation.
- Official forecast expiration, cancellation, missing products and disagreements between observations and a still-valid forecast.
- Keyboard access, one-time screen-reader announcement, narrow layouts and reduced motion.

Write `docs/solar-wind-alert.md` describing the settings, state machine, numerical comparisons, data-gap policy, example episodes, source-backed explanatory text and interpretation limits. Include the implemented feature in the release-readiness matrix. Demonstrate a spike that does not trigger, a sustained episode that does, dismissal, stale-data handling and subsequent recovery.


## 14. Implementation sequence and handoff

1. Inspect repository, pick compatible versions, document architecture and verified source contracts.
2. Build a working vertical slice: native window, one NOAA dataset, persistent cache, chart and selection panel.
3. Complete required series, coordinated time interaction, forecasts, aurora view, and the solar-wind dashboard alert.
4. Complete lessons, honest offline mode, exports and references.
5. Harden failure handling, performance, security and accessibility.
6. Build and test the Windows release candidate and installer; document signing/publication prerequisites.

Deliver source, dependency locks, migrations, attributed fixtures, repeatable development/build/test commands, CI configuration, README screenshots, configuration reference, scientific methods, source registry, operations notes, release notes/checksums, and `docs/release-readiness.md` with actual evidence for every gate.

Maintain `docs/implementation-status.md` with decisions, completed work, exact failures and next actions so another session can resume. Do not silently remove difficult requirements or stop after the first screen.

Prepare local artifacts before requesting public publication, certificate purchases, or account setup. Never claim a `.exe` is tested because source compilation passed elsewhere. Finish with exact run/install instructions, artifact locations, verified features, and any remaining publication blockers.

Begin now with repository inspection and proceed through implementation.
