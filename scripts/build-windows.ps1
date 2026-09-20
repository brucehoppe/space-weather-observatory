#!/usr/bin/env pwsh
# Build the Windows x64 release candidate: deterministic tests, then the NSIS
# installer, then SHA-256 checksums next to the artifacts.
#
# Mirrors .github/workflows/windows-release.yml step for step, so a local run
# on a Windows machine reproduces exactly what CI produces. Run from the
# repository root:
#
#   pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows.ps1
#
# -ExecutionPolicy Bypass is needed on a default Windows install, which
# blocks unsigned scripts by policy even when run explicitly with -File; it
# affects only this process, not the machine's persistent policy.
#
# Prerequisites: see docs/windows-build.md (Rust MSVC toolchain, Node 20+,
# Microsoft C++ Build Tools). Requires PowerShell 7+ (pwsh).

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "==> npm ci"
npm ci
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> cargo test --workspace (deterministic, fixture-only)"
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> npm test"
npm test
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> npm run build (typecheck + bundle frontend)"
npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> npx tauri build (exe + NSIS installer)"
npx tauri build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Workspace-root target directory (Cargo.toml declares the workspace at the
# repo root), matching CI — not src-tauri/target.
$bundleDir = "target/release/bundle/nsis"
if (-not (Test-Path $bundleDir)) {
    Write-Error "Expected bundle directory not found: $bundleDir"
    exit 1
}

Write-Host "==> cargo build --release -p swo-mcp (optional MCP server / local-LLM report writer)"
cargo build --release -p swo-mcp
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Beside the installer, so it is checksummed and uploaded with it.
Copy-Item "target/release/swo-mcp.exe" (Join-Path $bundleDir "swo-mcp.exe") -Force

Write-Host "==> SHA-256 checksums"
$sumsFile = Join-Path $bundleDir "SHA256SUMS.txt"
Get-ChildItem "$bundleDir/*.exe" | ForEach-Object {
    "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash)  $($_.Name)"
} | Set-Content $sumsFile
Get-Content $sumsFile

Write-Host ""
Write-Host "Done. Artifacts:"
Write-Host "  Installer: $bundleDir\*.exe"
Write-Host "  Checksums: $sumsFile"
Write-Host "  Raw exe:   target\release\space-weather-observatory.exe"
Write-Host "  MCP server: $bundleDir\swo-mcp.exe (install with scripts\install-mcp-windows.ps1 -Exe <path>)"
Write-Host ""
Write-Host "Unsigned build: Windows SmartScreen will warn on first run (expected)." -ForegroundColor Yellow
Write-Host "See docs/windows-build.md for signing and the clean-machine test checklist."
