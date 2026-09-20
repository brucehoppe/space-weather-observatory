# Quickstart: plain-language reports with a local language model

This adds an optional helper, `swo-mcp`, that lets a language model **running on your own
computer** read the Observatory's data and:

- write a short space weather report anyone can understand,
- keep that report up to date as the data changes,
- explain any reading on the dashboard ("What is Bz?", "Is Kp 4 a lot?").

Nothing is sent to the internet. No account, no API key, no cost. About 15 minutes, most of it
one download.

## How the pieces fit

```
  NOAA ──▶ Space Weather Observatory app ──▶ local cache (a file on your disk)
                                                   │  read-only
                                                   ▼
                                               swo-mcp ◀──▶ Ollama ◀── a model, e.g. qwen3:8b
                                                   │
                                                   ▼
                                     reports/latest.md  (plain language)
```

- **The desktop app** is the only thing that talks to NOAA. It saves what it downloads in a
  local cache. It never runs a language model itself.
- **Ollama** is a free program that runs language models on your own machine, the way a
  media player plays video files. A **model** is the file it runs (a few GB). Once
  downloaded, it works offline.
- **swo-mcp** sits between them. It reads the cache, works out the numbers itself, hands
  them to the model to put into words, and saves the result. "MCP" (Model Context Protocol)
  is the standard plug that lets a language model use tools like this one.

## Step 1. Get data into the cache

Install and open the Space Weather Observatory app once (see [`QUICKSTART.md`](../QUICKSTART.md)),
and let it load. That fills the cache. `swo-mcp` never downloads anything, so **the report is
only as fresh as the last time the app refreshed**. If the data is old, the report says so at
the top.

## Step 2. Install Ollama

| | |
|---|---|
| macOS | Download from <https://ollama.com/download>, or `brew install ollama`. Open the Ollama app; a llama icon appears in the menu bar. |
| Windows | Download from <https://ollama.com/download>, or `winget install Ollama.Ollama`. It starts automatically and sits in the system tray. |

Check that it is running:

```bash
ollama list
```

A table (even an empty one) means it works. "could not connect" means Ollama is not
running: open the app, or run `ollama serve` in another terminal.

## Step 3. Choose a model

The installer in step 4 downloads the default for you, so you can skip this. To choose yourself:

```bash
ollama pull qwen3:8b
```

| Model | Download | Memory needed | Notes |
|---|---|---|---|
| `qwen3:8b` | 5 GB | 8 GB+ | **Default.** Reliable at using tools; a report takes about 20-30 s on an Apple-silicon laptop. |
| `qwen3:14b` | 9 GB | 16 GB+ | Better wording, about twice as slow. |
| `llama3.1:8b` | 5 GB | 8 GB+ | A good alternative. |

Any Ollama model with **tool calling** works. Very small models (under about 4 billion
parameters) tend to ignore the writing rules. The numbers in the report's table are computed
by `swo-mcp`, not by the model, so they are right whichever model you pick.

## Step 4. Install swo-mcp

You need [Rust](https://rustup.rs) 1.88 or newer (`rustup update stable`). From the repository folder:

**macOS**

```bash
scripts/install-mcp-macos.sh
```

**Windows** (also needs the Microsoft C++ Build Tools, see [`windows-build.md`](windows-build.md))

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\install-mcp-windows.ps1
```

No Rust on Windows? Download `swo-mcp.exe` from the Windows release artifacts and run the
script with `-Exe C:\path\to\swo-mcp.exe`; it installs that file instead of building.

The script builds `swo-mcp`, installs it for your user only (no administrator rights), checks
Ollama, and downloads the model if it is missing. Useful options:

| macOS | Windows | What it does |
|---|---|---|
| `--autostart` | `-Autostart` | Keep the report up to date in the background, starting at every login. |
| `--register` | `-Register` | Also add it to Claude Desktop and Claude Code, if installed (same as `swo-mcp register`). |
| `--model NAME` | `-Model NAME` | Use a different model. |
| `--no-pull` | `-NoPull` | Do not download the model. |
| `--uninstall` | `-Uninstall` | Remove what the script installed. Your reports are kept. |

On Windows, open a new terminal afterwards so `swo-mcp` is found.

## Step 5. Use it

```bash
swo-mcp report
```

The first run takes a little longer while Ollama loads the model. You get something like:

```markdown
# Space weather report

**Conditions are calm, with no storms in progress.**

## Right now
The solar wind is flowing at about 425 km/s, which is ordinary...

## What it means for you
- **Aurora chances**: low; only at high latitudes under a dark, clear sky.
- **Radio/GPS/satellites**: no effects are expected.

## Next few days
NOAA forecasts quiet conditions through 20 September...

## Readings at a glance
| Reading | Latest | Context |
| Solar-wind speed | 425 km/s | last 120 min: 399 to 434 km/s; ordinary |
...
```

Run it again and it answers `report is current` without using the model: it only rewrites
the report when the app has fetched new data, or the report is more than 6 hours old.

**Keep it up to date** (checks every 5 minutes, rewrites only when needed; Ctrl-C to stop):

```bash
swo-mcp report --watch
```

or install with `--autostart` / `-Autostart` and forget about it.

**Ask about any reading:**

```bash
swo-mcp ask "What does Bz mean, and is the current value anything to worry about?"
swo-mcp ask "Explain every reading on the dashboard in one line each"
swo-mcp ask "Have there been any solar flares in the last day?"
```

The model looks up a reviewed explanation and the current value before answering, rather
than explaining from memory.

**Where the reports are**

| | |
|---|---|
| macOS | `~/Library/Application Support/SpaceWeatherObservatory/reports/` |
| Windows | `%APPDATA%\SpaceWeatherObservatory\reports\` |

`latest.md` is the current report, and every earlier one is kept in `history/`.

## Optional: use it from another AI app

`swo-mcp` with no arguments is a standard MCP server, so MCP-capable apps can use the same
tools. For Claude Desktop and Claude Code there is a command that sets it up:

```bash
swo-mcp register            # every client found; or: register claude-desktop | claude-code
swo-mcp unregister          # take it out again
```

It **merges** one entry into the client's existing configuration: other servers and settings
are kept, the previous file is copied to `claude_desktop_config.json.backup-<time>` first, and
a config that is not valid JSON is left untouched rather than "repaired". Quit and reopen
Claude Desktop afterwards. Then ask it, for example, *"Use the space-weather tools to update
the report"* or pick the `update_report` prompt.

For any other client (for example `mcphost` with Ollama), add this to its MCP configuration,
with the path the installer printed:

```json
{ "mcpServers": { "space-weather": { "command": "/Users/you/.local/bin/swo-mcp" } } }
```

Note that an app using a hosted model sends the readings to that provider. The `report` and
`ask` commands above stay entirely on your machine. Tool reference:
[`crates/swo-mcp/README.md`](../crates/swo-mcp/README.md).

## Troubleshooting

| You see | What to do |
|---|---|
| `cannot reach Ollama at http://127.0.0.1:11434` | Ollama is not running. Open the Ollama app, or run `ollama serve`. |
| `Ollama has no models installed` | `ollama pull qwen3:8b` |
| `cache was not found` | Open the desktop app once so the cache exists. An MCP client that started the server earlier picks it up on the next request; no restart needed. |
| The report starts with **Old data** | Open the desktop app and let it refresh, then run `swo-mcp report` again. |
| It is slow | The first request loads the model into memory. If every request is slow, try a smaller model with `--model`. |
| `swo-mcp: command not found` | macOS: add `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`. Windows: open a new terminal. |
| The background updater seems stuck | Read its log: `~/Library/Logs/swo-mcp.log` (macOS) or `%LOCALAPPDATA%\Programs\swo-mcp\swo-mcp.log` (Windows). |
| A different model | `swo-mcp report --model llama3.1:8b`, or set the `SWO_MODEL` environment variable. |
