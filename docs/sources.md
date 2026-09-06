# Source registry

**Verified:** 2026-09-06 (UTC), by direct request from the development machine.
Fixtures captured at 2026-09-06T18:04:33Z with `scripts/capture-fixtures.sh`.

Every product below was resolved by opening the official product page and following its
own links to the machine-readable service at `https://services.swpc.noaa.gov/`, then
fetching the endpoint and inspecting the actual payload. Nothing was guessed from a
filename. Where the spec's starting reference redirected (SWPC → spaceweather.gov), the
canonical page is recorded.

Re-run `scripts/capture-fixtures.sh` and update this date whenever a contract is re-verified.

## NOAA SWPC products

| Product | Product page | Endpoint (verified) | Cadence | Units / frame | Notes |
|---|---|---|---|---|---|
| Solar wind — plasma | https://www.swpc.noaa.gov/products/real-time-solar-wind | `https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json` | 1 min, ~24 h window | speed km/s, density cm⁻³, temperature K | Rows from **several spacecraft** are interleaved (`SOLAR1`, `ACE`, `IMAP` observed). `active: true` marks the stream SWPC is currently using. The app carries the spacecraft as provenance and never bakes it into a product name. `overall_quality` ≠ 0 → *suspect*. Nulls / `-9999` → *missing*. |
| Solar wind — magnetic field | same page | `https://services.swpc.noaa.gov/json/rtsw/rtsw_mag_1m.json` | 1 min, ~24 h window | nT; `bz_gsm` and `bz_gse` both supplied | GSM is the frame used for coupling; GSE retained separately. Frames differ and are never conflated. |
| GOES X-ray flux | https://www.swpc.noaa.gov/products/goes-x-ray-flux | `https://services.swpc.noaa.gov/json/goes/primary/xrays-1-day.json` | 1 min, 1 day | W/m²; `energy` = `0.1-0.8nm` or `0.05-0.4nm`; `satellite` integer | Class (A/B/C/M/X) is defined on 0.1–0.8 nm only. `electron_contaminaton` (sic, provider spelling) `true` → suspect. Zero/negative flux → excluded from the log axis and counted. |
| GOES instrument assignment | same page | `https://services.swpc.noaa.gov/json/goes/instrument-sources.json` | on change | — | Which satellite is primary/secondary per instrument, with effective time. |
| Planetary Kp (estimate) | https://www.swpc.noaa.gov/products/planetary-k-index | `https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json` | 3-hour intervals | Kp 0–9, `a_running`, `station_count` | NOAA estimate; interval start stamped. |
| Planetary Kp with forecast | same page | `https://services.swpc.noaa.gov/products/noaa-planetary-k-index-forecast.json` | 3-hour intervals | `observed` ∈ {`observed`, `estimated`, `predicted`}; `noaa_scale` e.g. `G1` | The three states are preserved separately. Forecast intervals never enter the observed series. |
| NOAA scales | https://www.spaceweather.gov/noaa-scales-explanation (SWPC page redirects here) | `https://services.swpc.noaa.gov/products/noaa-scales.json` | on change | Keys `-1`,`0`..`3` = day offset; `G/R/S.Scale` 0–5 or null; R `MinorProb`/`MajorProb`, S `Prob` in % | Null scale ≠ level 0. Probabilities kept with their provider labels. |
| Alerts, watches, warnings | https://www.spaceweather.gov/products/alerts-watches-and-warnings | `https://services.swpc.noaa.gov/products/alerts.json` | event-driven | fixed-format text body | Parsed fields: `Space Weather Message Code`, `Serial Number`, `Issue Time`, `Valid From`, `Now Valid Until`, `Cancel Serial Number`, `Extension to Serial Number`, `NOAA Scale:`, `THIS SUPERSEDES…`. Validity, cancellation, extension and supersession honoured. ALERT/SUMMARY bodies have no validity window → recorded as *issued*, never "in effect". |
| OVATION aurora | https://www.spaceweather.gov/products/aurora-30-minute-forecast | `https://services.swpc.noaa.gov/json/ovation_aurora_latest.json` | ~5 min | `Data Format: [Longitude, Latitude, Aurora]`; 360 × 181 cells, lon 0–359 E, lat −90..90; value = % probability of visible aurora in cell | Both `Observation Time` and `Forecast Time` preserved; lead time read from the product (varies; not a fixed 30 min). |
| 3-day forecast | https://www.spaceweather.gov/products/3-day-forecast | `https://services.swpc.noaa.gov/text/3-day-forecast.txt` | few times/day | text; `:Issued:` line; Kp breakdown table with `(G1)` annotations | Period read from the table; horizons beyond it are *unavailable*, never extrapolated. |
| 3-day geomagnetic forecast | https://www.spaceweather.gov/products/3-day-geomagnetic-forecast | `https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt` | daily | text; Ap, probabilities, Kp table | Own issue time, distinct from the 3-day forecast. |

Timestamp shapes observed and handled (all UTC): `2026-09-06T17:55:00Z`, `2026-09-06T17:57:00` (no marker),
`2026-09-06 12:12:15.890`, `2026 Sep 06 1212 UTC`.

## NASA imagery

| Product | Endpoint | Notes |
|---|---|---|
| SDO/AIA 193 Å | `https://api.helioviewer.org/v2/getClosestImage/?date=<ISO>&sourceId=11` then `…/v2/downloadImage/?id=<id>&scale=8&x0=0&y0=0&width=768&height=768&display=true&watermark=true` | `date` in the metadata response is the frame's own acquisition time (`YYYY-MM-DD HH:MM:SS`, UTC) and is what the app displays. |
| SDO/AIA 304 Å | same, `sourceId=13` | False colour stated on every image. |

Credit: NASA/SDO and the AIA, EVE, and HMI science teams, via the Helioviewer Project.
Refresh cadence used: 10 minutes. Requested instant is always explicit, so a clock change cannot relabel an image.

## Map data

Coastlines: `world-atlas` npm package v2.0.2 (`land-110m.json`), derived from Natural Earth (public domain).

## Terms and attribution

NOAA SWPC data are U.S. Government works not subject to domestic copyright. The application
identifies itself with a `SpaceWeatherObservatory/<version>` user-agent and polls at the
cadences above (never faster than a product updates). Outbound hosts are limited by an
allow-list in `src-tauri/src/providers.rs`: `services.swpc.noaa.gov`, `api.helioviewer.org`,
`sdo.gsfc.nasa.gov`.

## Not used

`https://services.swpc.noaa.gov/products/solar-wind/plasma-2-hour.json` and
`mag-2-hour.json` — historically referenced in third-party code — returned **404** on the
verification date. Do not reintroduce them without re-verifying.
