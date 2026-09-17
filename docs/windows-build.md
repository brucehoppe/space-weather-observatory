# Windows build and release procedure

Primary target: **Windows x64**. Development happened on macOS; a macOS run is *not* proof
the `.exe` works. Windows verification status is tracked in `docs/release-readiness.md`.

## Outputs (Tauri v2 toolchain naming)

- Executable: `target/release/space-weather-observatory.exe`
  (product name "Space Weather Observatory"; rename in `tauri.conf.json` → `productName` if a
  space-free name is preferred for the raw exe).
- Installer (NSIS, per-user, no admin): `target/release/bundle/nsis/Space Weather Observatory_0.1.0_x64-setup.exe`
- SHA-256 checksums: produced by the CI job as `SHA256SUMS.txt` next to the artifacts.

## Runtime strategy

The app renders in **WebView2**. `bundle.windows.webviewInstallMode` is `downloadBootstrapper`:
on a machine without WebView2 (rare on Windows 10 21H2+/11, where it ships with the OS) the
installer downloads the Evergreen bootstrapper, which **needs internet once**. Ordinary
operation after install needs internet only to refresh data; the app launches and shows
cached data offline. No Rust, Node or Python is required on the user's machine.

Alternative for offline installs: `offlineInstaller` (adds ~150 MB) — change in `tauri.conf.json`.

## Prerequisites (from https://v2.tauri.app/start/prerequisites/)

1. Microsoft C++ Build Tools (Desktop development with C++) or Visual Studio 2022.
2. WebView2 runtime (present on modern Windows).
3. Rust via rustup (MSVC toolchain): `winget install Rustlang.Rustup`, then `rustup default stable-msvc`.
4. Node.js 20+ LTS.

## Build

```powershell
git clone <repo> && cd space-weather-observatory
pwsh -File scripts/build-windows.ps1
```

`scripts/build-windows.ps1` runs `npm ci`, `cargo test --workspace`, `npm test`,
`npm run build` and `npx tauri build` in order, then writes SHA-256 checksums next to the
installer. It is the same script CI runs (see below), so a local run reproduces CI output
exactly. Outputs land under the workspace-root `target/release/bundle/nsis/` (the Cargo
workspace is rooted at the repo root, not `src-tauri/`).

CI: `.github/workflows/windows-release.yml` runs `scripts/build-windows.ps1` on
`windows-latest` and uploads the installer, exe and checksums as workflow artifacts. It is
configured but, at the time of writing, **has not been run** (see release-readiness).

## Data locations (never the install folder)

- Settings: `%APPDATA%\SpaceWeatherObservatory\settings.json`
- Cache/episodes: `%APPDATA%\SpaceWeatherObservatory\cache\observatory.sqlite3`
  (`dirs::data_dir()` on Windows resolves to Roaming AppData.)

**Uninstall retains user data.** Supported removal: delete the `%APPDATA%\SpaceWeatherObservatory`
folder, or use "Trim to newest snapshot per product" in Sources before uninstalling. A future
installer option to purge data is listed in implementation-status.

## Clean-machine test checklist (to be executed on Windows)

Install → launch → data refresh → disconnect network, relaunch (cached data + stale marks) →
export CSV via dialog → restart (window bounds, dismissed episode persist) → upgrade over
previous version → uninstall → confirm `%APPDATA%` data retained and no admin prompt at any step.

## Signing (public release only)

- Unsigned local release candidate: SmartScreen will warn ("unknown publisher"). Expected.
- Signed public release requires an OV/EV code-signing certificate (not purchased, no
  credentials invented here) plus RFC-3161 timestamping (`signtool sign /fd SHA256 /tr <url> /td SHA256`).
  Tauri: set `bundle.windows.certificateThumbprint` and `timestampUrl`.
- Signing **does not eliminate** SmartScreen reputation warnings for a new publisher; reputation
  accrues with downloads. EV certificates historically start with reputation but this is not guaranteed.
- No auto-updater is shipped: it would require signature verification and rollback, which are
  not implemented.
