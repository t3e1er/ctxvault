<#
.SYNOPSIS
    Starts the GroundControl multi-corpus MCP daemon and GraphView 3D dashboard for remote network access.

.DESCRIPTION
    1. Launches the background daemon bound to 0.0.0.0 (all network interfaces).
    2. Detects local LAN/Wi-Fi IPv4 addresses.
    3. Prints ready-to-use remote connection endpoints (GraphView UI, SSE MCP, HTTP MCP).
    4. Launches GraphView bound to 0.0.0.0.

.EXAMPLE
    .\scripts\start-remote.ps1
    .\scripts\start-remote.ps1 -Sync -Watch
    .\scripts\start-remote.ps1 -BackgroundGraphView
#>

param(
    [int]$DaemonPort = 9090,
    [int]$GraphViewPort = 9091,
    [switch]$Sync,
    [switch]$Watch,
    [switch]$BackgroundGraphView,
    [string]$IP
)

$ErrorActionPreference = 'Stop'

# 1. Resolve GroundControl executable
$gcExe = Get-Command "groundcontrol" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $gcExe) {
    $fallbackPath = "$env:LOCALAPPDATA\Programs\groundcontrol\bin\groundcontrol.exe"
    if (Test-Path $fallbackPath) {
        $gcExe = $fallbackPath
    } else {
        $repoTarget = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\groundcontrol.exe"
        if (Test-Path $repoTarget) {
            $gcExe = $repoTarget
        } else {
            Write-Error "groundcontrol executable not found in PATH or local build directories. Run 'scripts/install-local.ps1' first."
            exit 1
        }
    }
}

# 2. Start MCP Background Daemon on 0.0.0.0
Write-Host "`n[*] Starting GroundControl MCP daemon on 0.0.0.0:$DaemonPort..." -ForegroundColor Cyan

$daemonArgs = @("daemon", "start", "--bind", "0.0.0.0:$DaemonPort")
if ($Sync) { $daemonArgs += "--sync" }
if ($Watch) { $daemonArgs += "--watch" }

& $gcExe @daemonArgs

# 3. Detect Active Network IP Addresses
if ($IP) {
    $primaryIp = $IP
    $detectedIps = @($IP)
} else {
    $candidates = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object {
        $_.InterfaceAlias -notmatch 'Loopback|vEthernet|WSL|Docker|Bluetooth' -and
        $_.IPAddress -notmatch '^169\.254\.' -and
        $_.IPAddress -ne '127.0.0.1'
    }

    $wifi = $candidates | Where-Object { $_.InterfaceAlias -match 'Wi-?Fi|Wireless' } | Select-Object -First 1
    $eth = $candidates | Where-Object { $_.InterfaceAlias -match 'Ethernet|LAN' } | Select-Object -First 1

    $primaryIp = if ($wifi) {
        $wifi.IPAddress
    } elseif ($eth) {
        $eth.IPAddress
    } elseif ($candidates) {
        ($candidates | Select-Object -First 1).IPAddress
    } else {
        "127.0.0.1"
    }

    $detectedIps = $candidates | Select-Object -ExpandProperty IPAddress -Unique
}

# 4. Display Connection Card
Write-Host ""
Write-Host "==========================================================================" -ForegroundColor DarkCyan
Write-Host "  GroundControl Remote Services Ready" -ForegroundColor Green
Write-Host "==========================================================================" -ForegroundColor DarkCyan
Write-Host "  * Host IP Address : " -NoNewline -ForegroundColor Gray
Write-Host "$primaryIp" -ForegroundColor Yellow

if ($detectedIps.Count -gt 1) {
    $otherIps = ($detectedIps | Where-Object { $_ -ne $primaryIp }) -join ", "
    Write-Host "  * Other Interfaces: $otherIps" -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "  --- Remote Access Endpoints ---" -ForegroundColor White
Write-Host "  * GraphView 3D Web UI : " -NoNewline -ForegroundColor Gray
Write-Host "http://${primaryIp}:${GraphViewPort}" -ForegroundColor Cyan
Write-Host "  * MCP SSE Stream      : " -NoNewline -ForegroundColor Gray
Write-Host "http://${primaryIp}:${DaemonPort}/sse" -ForegroundColor Cyan
Write-Host "  * MCP JSON-RPC Stream : " -NoNewline -ForegroundColor Gray
Write-Host "http://${primaryIp}:${DaemonPort}/mcp" -ForegroundColor Cyan
Write-Host "  * Health Check Probe  : " -NoNewline -ForegroundColor Gray
Write-Host "http://${primaryIp}:${DaemonPort}/health" -ForegroundColor Cyan

Write-Host ""
Write-Host "  --- Remote Agent MCP Configuration (.agents/mcp_config.json) ---" -ForegroundColor White
Write-Host @"
  {
    "mcpServers": {
      "groundcontrol": {
        "command": "groundcontrol",
        "args": [
          "--mode", "proxy",
          "--server", "http://${primaryIp}:${DaemonPort}"
        ]
      }
    }
  }
"@ -ForegroundColor DarkGray
Write-Host "==========================================================================" -ForegroundColor DarkCyan
Write-Host ""

# 5. Launch GraphView
$graphviewArgs = @("graphview", "--bind", "0.0.0.0:$GraphViewPort", "--daemon", "http://127.0.0.1:$DaemonPort")

if ($BackgroundGraphView) {
    Write-Host "[*] Launching GraphView in background on port $GraphViewPort..." -ForegroundColor Cyan
    Start-Process $gcExe -ArgumentList $graphviewArgs -WindowStyle Hidden
    Write-Host "[+] GraphView started in background." -ForegroundColor Green
} else {
    Write-Host "[*] Launching GraphView (Press Ctrl+C to close dashboard; daemon remains running)..." -ForegroundColor Cyan
    & $gcExe @graphviewArgs
}
