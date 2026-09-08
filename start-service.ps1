# ctxvault Central MCP Server Starter
# Launches ctxvault in streamable HTTP / SSE mode with all indexed corpora and full capability profile

$ErrorActionPreference = 'Stop'

$Bin = if (Test-Path "$PSScriptRoot\target\release\ctxvault.exe") {
    "$PSScriptRoot\target\release\ctxvault.exe"
} elseif (Get-Command ctxvault -ErrorAction SilentlyContinue) {
    (Get-Command ctxvault).Source
} elseif (Test-Path "$env:LOCALAPPDATA\Programs\ctxvault\bin\ctxvault.exe") {
    "$env:LOCALAPPDATA\Programs\ctxvault\bin\ctxvault.exe"
} else {
    Write-Error "ctxvault.exe not found. Build with 'cargo build --release' or run install.ps1"
    exit 1
}

$Port = if ($env:CTXV_PORT) { $env:CTXV_PORT } else { "9090" }
$BindAddr = "127.0.0.1:$Port"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  Starting ctxvault Central Multi-Corpus MCP Server        " -ForegroundColor Cyan
Write-Host "  Binary:  $Bin" -ForegroundColor Gray
Write-Host "  Address: http://$BindAddr" -ForegroundColor Gray
Write-Host "  Profile: all (full capability: 39 tools)" -ForegroundColor Gray
Write-Host "============================================================" -ForegroundColor Cyan

& $Bin --mode server `
  --bind $BindAddr `
  --profile all `
  --corpus "kubernetes=C:\dev\ctx\corpus\kubernetes" `
  --corpus "rust=C:\dev\ctx\corpus\rust" `
  --corpus "typescript=C:\dev\ctx\corpus\typescript" `
  --default-corpus "kubernetes" `
  $args
