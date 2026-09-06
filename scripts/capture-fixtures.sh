#!/usr/bin/env bash
# Capture verbatim originals from NOAA SWPC / NASA for deterministic parser tests.
# Re-run to refresh; commit the result together with an updated docs/sources.md date.
set -euo pipefail
DEST="$(cd "$(dirname "$0")/.." && pwd)/fixtures/captured"
mkdir -p "$DEST"
get() { # get <url> <filename>
  echo "capturing $2"
  curl -sS --fail --retry 3 --retry-all-errors --max-time 120 -A "SpaceWeatherObservatory/0.1 (fixture capture)" "$1" -o "$DEST/$2"
}
get https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json            rtsw_wind_1m.json
get https://services.swpc.noaa.gov/json/rtsw/rtsw_mag_1m.json             rtsw_mag_1m.json
get https://services.swpc.noaa.gov/json/goes/primary/xrays-1-day.json     goes_primary_xrays_1day.json
get https://services.swpc.noaa.gov/json/goes/instrument-sources.json      goes_instrument_sources.json
get https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json   planetary_k_index.json
get https://services.swpc.noaa.gov/products/noaa-planetary-k-index-forecast.json planetary_k_index_forecast.json
get https://services.swpc.noaa.gov/products/noaa-scales.json             noaa_scales.json
get https://services.swpc.noaa.gov/products/alerts.json                  alerts.json
get https://services.swpc.noaa.gov/json/ovation_aurora_latest.json       ovation_aurora_latest.json
get https://services.swpc.noaa.gov/text/3-day-forecast.txt               3-day-forecast.txt
get https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt        3-day-geomag-forecast.txt
echo "capture complete: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
