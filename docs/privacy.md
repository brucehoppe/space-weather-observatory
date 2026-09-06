# Privacy statement

**External requests.** The application contacts exactly three hosts, all over HTTPS, and
refuses any other host before a request is made (`src-tauri/src/providers.rs`, `ALLOWED_HOSTS`):

- `services.swpc.noaa.gov` — NOAA SWPC measurements, indices, scales, bulletins, forecasts and the aurora model grid.
- `api.helioviewer.org` — NASA SDO/AIA imagery and its acquisition metadata.
- `sdo.gsfc.nasa.gov` — reserved for SDO imagery; not currently requested.

Each request carries a `User-Agent: SpaceWeatherObservatory/<version> (local desktop
application; contact via repository)` header and nothing else that identifies you. No
cookies are stored. Requests happen at fixed per-product cadences (see `docs/sources.md`),
only while the application is open. The webview itself makes **no** network requests: the
content-security policy in `src-tauri/tauri.conf.json` restricts it to bundled assets and IPC.

**Local storage** (per-user directories, never the installation folder):

- Settings (`settings.json`): display time zone, optional region label, alert thresholds,
  cache limit, retention, window bounds.
- Cache (`cache/observatory.sqlite3`): verbatim copies of retrieved provider payloads with
  SHA-256 hashes and retrieval times; the custom alert's episode history and acknowledgements.
  Bounded by the cache limit (default 256 MB) and retention (default 48 snapshots per product).

**Not collected, not sent:** analytics, telemetry, crash reports, location, account
identity, usage statistics. There is no account and no sign-in. No language model is called
at runtime; all plain-language text comes from a reviewed rule layer in the source code.

**Exports** are written only to a path you choose in the native save dialog.
