# swo-mcp

Optional [MCP](https://modelcontextprotocol.io) server over the Space Weather Observatory's
local cache, with a built-in driver for a **local language model** (Ollama). It lets a model
running on your own machine:

1. **Write a plain-language space weather report** for a general reader.
2. **Keep that report up to date**: it knows when the saved report no longer matches the data.
3. **Explain every reading on the dashboard** from reviewed explanations, not from memory.

It runs as its own process. The desktop app still never loads a language model, makes no
extra network calls, and remains the only writer to the cache.

## Quick start (fully local)

Needs [Ollama](https://ollama.com) running with any tool-calling model (`ollama pull qwen3:8b`),
and the desktop app opened at least once so the cache has data.

```bash
cargo build --release -p swo-mcp

swo-mcp report                      # write the report if the saved one is out of date, print it
swo-mcp report --watch              # stay running; rewrite whenever the data changes (checks every 300 s)
swo-mcp ask "What does Bz mean, and is today's value unusual?"
swo-mcp status                      # one-screen text rollup of the latest readings and bulletins (no model)
swo-mcp status --watch 60           # stay running; reprint whenever the cache changes
swo-mcp dashboard                   # the readings the model is given, as JSON
swo-mcp register                    # add the server to Claude Desktop / Claude Code
```

Reports are saved beside the cache, in `SpaceWeatherObservatory/reports/`: `latest.md`,
`latest.json`, and every report under `history/`. Nothing is ever deleted by this program.

Options: `--model NAME` (or `SWO_MODEL`), `--db PATH` (`SWO_DB`), `--reports DIR`
(`SWO_REPORTS`), `--force`. The model is reached at `OLLAMA_HOST`, default
`http://127.0.0.1:11434`.

## How the report stays honest

A small local model is good at wording and unreliable with numbers, so the work is split:

| Part of the report | Written by |
| --- | --- |
| Prose: overall picture, what it means for you, next few days | the language model, under fixed writing rules (`src/prompts.rs`) |
| "Old data" warning, readings table, provenance footer | this program, computed from the data (`src/reports.rs`) |
| What a G/R/S level means | the app's reviewed rule layer (`swo_core::interpret`), handed to the model |
| What each reading *is* | the reviewed glossary (`src/glossary.rs`), handed to the model |

**Up to date** means: the report's `data_fingerprint` (a hash of the newest cached product
hashes) still matches the cache, *and* the report is under 6 hours old. `get_report` and
`swo-mcp report` both use that check, so the model is only run when something changed.

This server never downloads anything. If the desktop app has not refreshed the cache
recently, the dashboard carries a `stale_data_warning`, the report is framed with it, and the
model is told to write in the past tense.

## Tools

| Tool | What it returns |
| --- | --- |
| `get_dashboard` | Every dashboard reading in one call: solar wind (speed, density, Bt, Bz), X-ray flux and flare class, Kp, NOAA G/R/S scales, active bulletins, 3-day forecast, aurora model, solar cycle; plus the app's plain-language statements, data age and `data_fingerprint`. |
| `explain_reading` | Reviewed explanation of a reading (what it is, how to read it, why it matters, caveats) with its current value. Omit `reading` for all of them. Matching is forgiving: `bz`, `Bz (GSM)`, `northern lights`. |
| `get_interpretation` | The app's rule-layer statements in full: what each published G/R/S level means for aurora viewing, HF radio, GNSS, satellites and power, NOAA's forecast days, and the solar-wind note. Each carries basis, rule version and source ref. |
| `get_kp` | Kp per 3-hour interval with **observed**, **NOAA-estimated** and **forecast** values in separate lists, never merged. Intervals NOAA labels "estimated" that had not begun at retrieval are listed apart. `hours_back`, `hours_ahead`, `at`. |
| `get_xray_flares` | GOES long-band flare activity over 24 h: latest class, background, peak, and each period at or above `min_class` (default C), using the app's flux classifier. `at` replays. |
| `get_solar_wind` | 1-minute speed, density, Bt, Bz (GSM) over `window_minutes` (default 120) with a summary and provenance. `at` replays retained snapshots. |
| `list_products` | Every product key, whether it is cached, source URL, `retrieved_at`, age, replay snapshot count. |
| `get_report` | The latest saved report and whether it is still current, with the reasons if not. |
| `save_report` | Save a new report. Pass only the prose; the server adds the title, readings table and footer, and records the fingerprint. |

Prompts: `layman_report` (write and save a report), `update_report` (rewrite only if out of
date, and say what changed), `explain_dashboard` (one reading, or all of them).

All values are parsed by `swo-core`, so quality flags and provenance match the app.

## Connect another MCP client

Any MCP client can drive the same tools instead of the built-in Ollama commands.

Claude Desktop and Claude Code:

```bash
swo-mcp register            # every client found; or: register claude-desktop | claude-code
swo-mcp unregister
```

`register` merges a `space-weather` entry into the client's config. Other servers and
settings are kept, the previous file is backed up beside it, and invalid JSON is refused
rather than repaired. By hand, the entry is:

```json
{
  "mcpServers": {
    "space-weather": { "command": "/absolute/path/to/swo-mcp" }
  }
}
```

The server starts even if the cache does not exist yet; its tools then say to open the
desktop app, and work as soon as the cache appears.

For a local model, use an MCP client that drives Ollama with tool calling (for example
`mcphost`) pointed at the same command, then run the `update_report` prompt.

## Build and test

```bash
cargo build --release -p swo-mcp
cargo test -p swo-mcp          # fixture-only, fixed clock, no network, no language model
```

Requires Rust 1.88+ (rmcp 3.x); the rest of the workspace keeps its 1.82 floor.

## Privacy

`report` and `ask` send the dashboard readings to the Ollama endpoint, which by default is
this machine, so nothing leaves it. Pointing `OLLAMA_HOST` at another computer, or connecting
an MCP client that uses a hosted model, sends tool results there.
