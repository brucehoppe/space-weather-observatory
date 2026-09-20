#!/usr/bin/env bash
# Install the swo-mcp server (MCP + local-LLM report writer) on macOS.
#
#   scripts/install-mcp-macos.sh                 build and install ~/.local/bin/swo-mcp
#   scripts/install-mcp-macos.sh --autostart     also keep the report up to date in the
#                                                background (a per-user LaunchAgent running
#                                                `swo-mcp report --watch`)
#   scripts/install-mcp-macos.sh --register      also add it to the MCP clients found on this Mac
#                                                (Claude Desktop, Claude Code); configs are
#                                                merged and backed up, never overwritten
#   scripts/install-mcp-macos.sh --model NAME    Ollama model to use and pull (default qwen3:8b)
#   scripts/install-mcp-macos.sh --no-pull       do not download the model
#   scripts/install-mcp-macos.sh --uninstall     remove exactly what this script installed
#
# Per-user only: no sudo, nothing outside your home directory. It never touches the
# desktop app, its cache, or any saved reports (uninstall leaves reports in place).
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$HOME/.local/bin"
BIN="$BIN_DIR/swo-mcp"
LABEL="com.spaceweatherobservatory.swo-mcp"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
LOG="$HOME/Library/Logs/swo-mcp.log"
MODEL="qwen3:8b"
AUTOSTART=0
REGISTER=0
PULL=1
UNINSTALL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --autostart) AUTOSTART=1 ;;
    --register) REGISTER=1 ;;
    --no-pull) PULL=0 ;;
    --uninstall) UNINSTALL=1 ;;
    --model) MODEL="${2:?--model needs a name}"; shift ;;
    -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

[ "$(uname -s)" = "Darwin" ] || { echo "This script is for macOS. On Windows use scripts/install-mcp-windows.ps1." >&2; exit 1; }

stop_agent() {
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
}

if [ "$UNINSTALL" = 1 ]; then
  echo "==> Uninstalling swo-mcp"
  stop_agent
  # Take our entry out of MCP client configs while the binary that knows how still exists.
  if [ -x "$BIN" ]; then "$BIN" unregister || echo "could not unregister from MCP clients; remove the 'space-weather' entry by hand"; fi
  # Remove only the two files this script creates, by name, and say what happened to each.
  for f in "$PLIST" "$BIN"; do
    if [ -e "$f" ]; then rm "$f" && echo "removed $f"; else echo "not present: $f"; fi
  done
  echo "Left in place: your reports, the log ($LOG), the desktop app and its cache, Ollama and its models."
  exit 0
fi

echo "==> Checking prerequisites"
command -v cargo >/dev/null || { echo "Rust is not installed. Install it from https://rustup.rs and re-run." >&2; exit 1; }
RUST_MINOR="$(rustc --version | sed -E 's/^rustc 1\.([0-9]+).*/\1/')"
if [ "$RUST_MINOR" -lt 88 ]; then
  echo "Rust 1.88 or newer is needed (found $(rustc --version)). Run: rustup update stable" >&2; exit 1
fi
echo "ok: $(rustc --version)"

echo "==> Building swo-mcp (release)"
cargo build --release -p swo-mcp --manifest-path "$REPO/Cargo.toml"

echo "==> Installing to $BIN"
mkdir -p "$BIN_DIR"
stop_agent   # a running copy would keep the old binary open
install -m 755 "$REPO/target/release/swo-mcp" "$BIN"

echo "==> Checking Ollama (the local language model runtime)"
if ! command -v ollama >/dev/null; then
  echo "Ollama is not installed. Install it from https://ollama.com/download (or: brew install ollama),"
  echo "then run: ollama pull $MODEL"
elif ! ollama list >/dev/null 2>&1; then
  echo "Ollama is installed but not running. Start the Ollama app (or: ollama serve), then: ollama pull $MODEL"
elif ollama list | awk 'NR>1 {print $1}' | grep -qx "$MODEL"; then
  echo "ok: model $MODEL is installed"
elif [ "$PULL" = 1 ]; then
  echo "Downloading model $MODEL (several GB, one time)"
  ollama pull "$MODEL"
else
  echo "Model $MODEL is not installed (skipped because of --no-pull). Later: ollama pull $MODEL"
fi

if [ "$AUTOSTART" = 1 ]; then
  echo "==> Installing the background report updater ($LABEL)"
  mkdir -p "$(dirname "$PLIST")" "$(dirname "$LOG")"
  cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$BIN</string><string>report</string><string>--watch</string>
    <string>--model</string><string>$MODEL</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>60</integer>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>$LOG</string>
  <key>StandardErrorPath</key><string>$LOG</string>
</dict>
</plist>
EOF
  launchctl bootstrap "gui/$(id -u)" "$PLIST"
  echo "ok: running now and at every login. Log: $LOG"
fi

if [ "$REGISTER" = 1 ]; then
  echo "==> Registering with MCP clients"
  "$BIN" register
fi

REPORTS="$HOME/Library/Application Support/SpaceWeatherObservatory/reports"
cat <<EOF

Installed: $BIN
Reports:   $REPORTS/latest.md

Try it (open the Space Weather Observatory app first so the data is fresh):
  swo-mcp report                     write and print the plain-language report
  swo-mcp ask "What does Kp mean?"   have the local model explain a reading
  swo-mcp report --watch             keep the report up to date until you press Ctrl-C

To use it from Claude Desktop or Claude Code (merges into their config, with a backup):
  swo-mcp register                   undo with: swo-mcp unregister
Any other MCP client (mcphost...): { "mcpServers": { "space-weather": { "command": "$BIN" } } }
EOF
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf '\nNote: %s is not on your PATH. Add this line to ~/.zshrc:\n  export PATH="$HOME/.local/bin:$PATH"\n' "$BIN_DIR" ;;
esac
