# Quickstart

Three ways to see the app, from fastest to most complete. Pick one.

## 0. Just look at it (no install, 30 seconds)

If you only want to see the UI — no live data, no backend — open the frozen demonstration
dataset in a browser:

```bash
npm ci
npm run dev
```

Then open <http://localhost:5173/>. Add `?view=aurora`, `?view=learn` or `?view=sources` to
jump to a specific view. This is a layout/interaction preview of the same real NOAA data the
desktop app ships offline — it is **not** the desktop app and proves nothing about the
Windows build.

## 1. Run the real desktop app (a few minutes)

This builds and launches the actual application, with live data and hot reload.

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 24+
- Platform build tools for Tauri — see <https://v2.tauri.app/start/prerequisites/>
  (macOS: Xcode Command Line Tools, `xcode-select --install`. Windows: Microsoft C++ Build
  Tools. Linux: a few system packages the Tauri docs list for your distro.)

```bash
git clone <this-repo-url>
cd space-weather-observatory
npm ci
npm run tauri dev
```

A native window opens. It fetches live data from NOAA SWPC on first launch; offline, it
falls back to cached data automatically. No account, no API key, no configuration needed.

## 2. Build an installable package

**macOS** (produces a `.app`, zipped):

```bash
npm run build:macos-zip
```

Output: `target/release/bundle/macos/Space Weather Observatory.app.zip`. Unsigned — macOS
Gatekeeper will warn on first open (right-click → Open to bypass once).

**Windows** (produces an NSIS installer `.exe`), from a Windows machine with
[PowerShell 7+](https://learn.microsoft.com/powershell/scripting/install/installing-powershell)
(`pwsh`):

```powershell
git clone <this-repo-url>
cd space-weather-observatory
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows.ps1
```

(`-ExecutionPolicy Bypass` is needed on a default Windows install, which otherwise blocks
running an unsigned script even with `-File`; it only applies to this one process.)

This runs the full test suite before packaging, then writes the installer plus SHA-256
checksums to `target/release/bundle/nsis/`. It is exactly what CI runs
(`.github/workflows/windows-release.yml`), so a local run reproduces CI output. Unsigned —
Windows SmartScreen will warn on first run. See `docs/windows-build.md` for signing and the
full clean-machine test checklist.

## Sanity check: does it actually work?

```bash
cargo test --workspace   # 225 deterministic tests, no network
npm test                 # 12 frontend tests
```

Both should pass with zero failures before you trust a build. Neither touches the network —
they run against fixtures captured from real NOAA responses in `fixtures/captured/`.

## What next

- `README.md` — full feature list, configuration reference, data locations.
- `docs/architecture.md` — how the pieces fit together.
- `docs/sources.md` — every data product, its endpoint, and how it's verified.
- `CONTRIBUTING.md` — if you want to change something.

## 3. Optional: plain-language reports from a local language model

Have a model running on your own machine (via [Ollama](https://ollama.com)) write a space
weather report anyone can read, keep it up to date, and explain any dashboard reading.
Nothing leaves your computer. Step-by-step guide, including what Ollama is and which model to
pick: [`docs/local-llm-quickstart.md`](docs/local-llm-quickstart.md). The short version:

```bash
scripts/install-mcp-macos.sh          # Windows: scripts\install-mcp-windows.ps1
swo-mcp status                        # latest readings and bulletins in your terminal, no model needed
swo-mcp report
swo-mcp ask "What does Kp mean?"
```

## If something goes wrong

- **Rust build fails on macOS**: run `xcode-select --install` and retry.
- **`npm run tauri dev` never opens a window**: check the terminal output for a Rust
  compile error above the "Running ..." line — the frontend (Vite) starts first and can look
  successful even if the backend fails to build.
- **No data appears / everything shows "unavailable"**: check your network. The app polls
  `services.swpc.noaa.gov`, `api.helioviewer.org` and `sdo.gsfc.nasa.gov` only; nothing else.
  Corporate proxies/firewalls sometimes block one of these.
- **Windows build script fails immediately**: confirm you're running `pwsh` (PowerShell 7+),
  not `powershell.exe` (Windows PowerShell 5.1) — some syntax in the script needs 7+.
- **"running scripts is disabled on this system" / execution policy error**: use the full
  command above with `-ExecutionPolicy Bypass`. A default Windows install blocks unsigned
  scripts by policy, even when run explicitly with `-File`.
