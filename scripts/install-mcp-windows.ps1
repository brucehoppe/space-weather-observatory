#!/usr/bin/env pwsh
# Install the swo-mcp server (MCP + local-LLM report writer) on Windows.
#
#   install-mcp-windows.ps1                 build and install swo-mcp.exe for this user
#   install-mcp-windows.ps1 -Autostart      also keep the report up to date in the background
#                                           (a per-user scheduled task running
#                                           `swo-mcp report --watch` at logon)
#   install-mcp-windows.ps1 -Register       also add it to the MCP clients found on this PC
#                                           (Claude Desktop, Claude Code); configs are
#                                           merged and backed up, never overwritten
#   install-mcp-windows.ps1 -Model NAME     Ollama model to use and pull (default qwen3:8b)
#   install-mcp-windows.ps1 -NoPull         do not download the model
#   install-mcp-windows.ps1 -Exe PATH       install a prebuilt swo-mcp.exe (from a release
#                                           download) instead of building; no Rust needed
#   install-mcp-windows.ps1 -Uninstall      remove exactly what this script installed
#
# Run from anywhere:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\install-mcp-windows.ps1
#
# -ExecutionPolicy Bypass is needed on a default Windows install, which blocks unsigned
# scripts even when run explicitly with -File; it affects only this process. Works in
# Windows PowerShell 5.1 and PowerShell 7+.
#
# Per-user only: no administrator rights, nothing outside your profile. It never touches
# the desktop app, its cache, or any saved reports (uninstall leaves reports in place).
# Prerequisites (unless -Exe is given): Rust (MSVC toolchain) and the Microsoft C++
# Build Tools, as in docs/windows-build.md.

param(
    [switch]$Autostart,
    [switch]$Register,
    [switch]$NoPull,
    [switch]$Uninstall,
    [string]$Exe = "",
    [string]$Model = "qwen3:8b"
)

$ErrorActionPreference = "Stop"
$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\swo-mcp"
$Installed = Join-Path $InstallDir "swo-mcp.exe"
$Launcher = Join-Path $InstallDir "swo-mcp-watch.vbs"
$Log = Join-Path $InstallDir "swo-mcp.log"
$TaskName = "SpaceWeatherObservatory swo-mcp report watcher"

function Stop-Watcher {
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    }
    # A running copy would keep the old exe locked. Only stop copies started from our install path.
    Get-Process -Name "swo-mcp" -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Installed } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

function Get-UserPathEntries {
    $raw = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($raw) { return @($raw.Split(";") | Where-Object { $_ }) }
    return @()
}

if ($Uninstall) {
    Write-Host "==> Uninstalling swo-mcp"
    Stop-Watcher
    # Take our entry out of MCP client configs while the exe that knows how still exists.
    if (Test-Path -LiteralPath $Installed) {
        & $Installed unregister
        if ($LASTEXITCODE -ne 0) { Write-Warning "Could not unregister from MCP clients; remove the 'space-weather' entry by hand." }
    }
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "removed scheduled task '$TaskName'"
    } else {
        Write-Host "not present: scheduled task '$TaskName'"
    }
    # Remove only the files this script creates, by name, and say what happened to each.
    foreach ($f in @($Installed, $Launcher)) {
        if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f; Write-Host "removed $f" }
        else { Write-Host "not present: $f" }
    }
    $entries = Get-UserPathEntries
    if ($entries -contains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", (($entries | Where-Object { $_ -ne $InstallDir }) -join ";"), "User")
        Write-Host "removed $InstallDir from your user PATH"
    }
    Write-Host "Left in place: your reports, the log ($Log), the desktop app and its cache, Ollama and its models."
    exit 0
}

if ($Exe) {
    if (-not (Test-Path -LiteralPath $Exe)) { Write-Error "No file at -Exe path: $Exe" }
    $Source = (Resolve-Path -LiteralPath $Exe).Path
    Write-Host "==> Using prebuilt $Source (skipping the build)"
} else {
    Write-Host "==> Checking prerequisites"
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "Rust is not installed. Install it from https://rustup.rs (MSVC toolchain; see docs/windows-build.md) and re-run, or pass -Exe with a prebuilt swo-mcp.exe."
    }
    $rustVersion = (rustc --version)
    if ($rustVersion -match "^rustc 1\.(\d+)") {
        if ([int]$Matches[1] -lt 88) {
            Write-Error "Rust 1.88 or newer is needed (found $rustVersion). Run: rustup update stable"
        }
    }
    Write-Host "ok: $rustVersion"

    Write-Host "==> Building swo-mcp (release)"
    cargo build --release -p swo-mcp --manifest-path (Join-Path $Repo "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $Source = Join-Path $Repo "target\release\swo-mcp.exe"
}

Write-Host "==> Installing to $Installed"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Stop-Watcher
Copy-Item -LiteralPath $Source -Destination $Installed -Force

$entries = Get-UserPathEntries
if ($entries -notcontains $InstallDir) {
    [Environment]::SetEnvironmentVariable("Path", (($entries + $InstallDir) -join ";"), "User")
    Write-Host "added $InstallDir to your user PATH (open a new terminal to pick it up)"
}

Write-Host "==> Checking Ollama (the local language model runtime)"
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "Ollama is not installed. Install it from https://ollama.com/download (or: winget install Ollama.Ollama),"
    Write-Host "then run: ollama pull $Model"
} else {
    $listed = & ollama list 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Ollama is installed but not running. Start the Ollama app, then: ollama pull $Model"
    } elseif ($listed | Select-Object -Skip 1 | Where-Object { ($_ -split "\s+")[0] -eq $Model }) {
        Write-Host "ok: model $Model is installed"
    } elseif (-not $NoPull) {
        Write-Host "Downloading model $Model (several GB, one time)"
        & ollama pull $Model
        if ($LASTEXITCODE -ne 0) { Write-Warning "The download failed. Later: ollama pull $Model" }
    } else {
        Write-Host "Model $Model is not installed (skipped because of -NoPull). Later: ollama pull $Model"
    }
}

if ($Autostart) {
    Write-Host "==> Installing the background report updater ('$TaskName')"
    # swo-mcp is a console program; a tiny VBScript launcher runs it with no window
    # and appends its output to the log.
    $cmd = "cmd /c """"$Installed"" report --watch --model $Model >> ""$Log"" 2>&1"""
    $vbs = "CreateObject(""WScript.Shell"").Run """ + $cmd.Replace('"', '""') + """, 0, False"
    Set-Content -LiteralPath $Launcher -Value $vbs -Encoding ASCII

    $action = New-ScheduledTaskAction -Execute "wscript.exe" -Argument "`"$Launcher`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
        -Description "Keeps the Space Weather Observatory plain-language report up to date using a local language model." `
        -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName
    Write-Host "ok: running now and at every logon. Log: $Log"
}

if ($Register) {
    Write-Host "==> Registering with MCP clients"
    & $Installed register
    if ($LASTEXITCODE -ne 0) { Write-Warning "Registration failed; run 'swo-mcp register' later to retry." }
}

$reports = Join-Path $env:APPDATA "SpaceWeatherObservatory\reports"
$exeJson = $Installed.Replace("\", "\\")
Write-Host @"

Installed: $Installed
Reports:   $reports\latest.md

Try it (open the Space Weather Observatory app first so the data is fresh):
  swo-mcp report                     write and print the plain-language report
  swo-mcp ask "What does Kp mean?"   have the local model explain a reading
  swo-mcp report --watch             keep the report up to date until you press Ctrl-C

To use it from Claude Desktop or Claude Code (merges into their config, with a backup):
  swo-mcp register                   undo with: swo-mcp unregister
Any other MCP client (mcphost...): { "mcpServers": { "space-weather": { "command": "$exeJson" } } }
"@
