# demo-activations.ps1
# Simulates AI agent activations across multiple clients (Antigravity, Claude, Gemini, Roo)
# Runs with clear pauses between each event to watch the 3D WebGL animations unfold.
#
# Usage:
#   .\scripts\demo-activations.ps1
#   .\scripts\demo-activations.ps1 -PauseSeconds 5

param (
    [string]$TargetUri = "http://127.0.0.1:9091/api/mcp/activity",
    [int]$PauseSeconds = 4
)

function Show-Pause {
    param ([int]$Seconds, [string]$VisualNote)
    Write-Host ("    --> Visualizer: " + $VisualNote) -ForegroundColor DarkGray
    for ($i = $Seconds; $i -gt 0; $i--) {
        Write-Host -NoNewline ("    [Pausing " + $i + "s to watch animation...] ")
        Start-Sleep -Seconds 1
        Write-Host ""
    }
}

function Send-Activation {
    param (
        [string]$Tool,
        [string]$ClientId,
        [string]$ClientName,
        [string]$Color,
        [string]$Query,
        [string[]]$Paths,
        [double]$DurationMs = 18.2
    )

    $payload = @{
        timestamp    = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        tool         = $Tool
        client_id    = $ClientId
        client_name  = $ClientName
        client_color = $Color
        query        = $Query
        paths        = $Paths
        duration_ms  = $DurationMs
        success      = $true
    } | ConvertTo-Json -Compress

    try {
        Invoke-RestMethod -Uri $TargetUri -Method Post -ContentType "application/json" -Body $payload | Out-Null
        Write-Host ("  [" + $ClientName + "] ") -NoNewline -ForegroundColor White
        Write-Host ($Tool + " ") -NoNewline -ForegroundColor Yellow
        Write-Host ("-> " + $Query + " ") -NoNewline -ForegroundColor Gray
        Write-Host ("(" + $Paths.Count + " hits)") -ForegroundColor Green
    } catch {
        Write-Host ("  [Error sending activation] " + $_) -ForegroundColor Red
    }
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "       ctxvault 3D Multi-Agent Visualizer Showcase          " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host ("Target Endpoint: " + $TargetUri) -ForegroundColor DarkGray
Write-Host ("Pause Duration:  " + $PauseSeconds + " seconds between events") -ForegroundColor DarkGray
Write-Host ""

# --- Event 1: Antigravity - Hybrid Search ---
Write-Host "[1/6] Inbound Orbital Strike Meteors (Search Hits)" -ForegroundColor Cyan
$paths1 = @(
    "crates/ctxvault-core/src/engine.rs",
    "crates/ctxvault-graphview/src/loader.rs",
    "crates/ctxvault-core/src/graph/code.rs"
)
Send-Activation -Tool "search" -ClientId "antigravity" -ClientName "Antigravity Agent" -Color "#38bdf8" -Query "Search: AST chunking and graph topology" -Paths $paths1 -DurationMs 24.6
Show-Pause -Seconds $PauseSeconds -VisualNote "Watch cyan orbital meteors plunge from space into hit nodes, detonating shockwaves and fog auras"

# --- Event 2: Claude - Read File ---
Write-Host "[2/6] Targeted Read File (Book Badge and PointLight)" -ForegroundColor Cyan
$paths2 = @("crates/ctxvault-core/src/search/mod.rs")
Send-Activation -Tool "read_file" -ClientId "claude" -ClientName "Claude Desktop" -Color "#f97316" -Query "read_file: search/mod.rs [1..120]" -Paths $paths2 -DurationMs 8.4
Show-Pause -Seconds $PauseSeconds -VisualNote "Watch orange beacon with book badge float upward and illuminate surrounding nodes"

# --- Event 3: Gemini - Write Note ---
Write-Host "[3/6] High-energy Note Mutation (Hot Pink / Magenta Pulse)" -ForegroundColor Cyan
$paths3 = @("docs/concepts/progressive-disclosure/tier-3.md")
Send-Activation -Tool "write_note" -ClientId "gemini" -ClientName "Gemini CLI" -Color "#ec4899" -Query "write_note: docs/architecture/visualizer.md" -Paths $paths3 -DurationMs 42.1
Show-Pause -Seconds $PauseSeconds -VisualNote "Watch magenta orbital strike and note badge with expanding neon shockwave"

# --- Event 4: Roo Code - Get Snippet ---
Write-Host "[4/6] Symbol Snippet Lookup (Emerald Sparkle)" -ForegroundColor Cyan
$paths4 = @("crates/ctxvault-core/src/layout/tiered.rs")
Send-Activation -Tool "get_snippet" -ClientId "roo" -ClientName "Roo Code" -Color "#10b981" -Query "get_snippet: CodeSymbol::GraphStore" -Paths $paths4 -DurationMs 11.2
Show-Pause -Seconds $PauseSeconds -VisualNote "Watch emerald green inbound strike and lightning badge hit snippet node"

# --- Event 5: Antigravity - Cascading N-Hop Trace Traversal ---
Write-Host "[5/6] Sequential N-Hop Trace Traversal (Cascading Multi-Hop)" -ForegroundColor Cyan
$paths5 = @(
    "crates/ctxvault-core/src/engine.rs",
    "crates/ctxvault-core/src/search/mod.rs",
    "crates/ctxvault-core/src/graph/code.rs",
    "crates/ctxvault-graphview/src/loader.rs"
)
Send-Activation -Tool "graph_match" -ClientId "antigravity" -ClientName "Antigravity Agent" -Color "#38bdf8" -Query "(:CodeSymbol)-[:calls*1..3]->(target)" -Paths $paths5 -DurationMs 38.9
Show-Pause -Seconds ($PauseSeconds + 2) -VisualNote "Watch cyan meteor fly hop-by-hop (engine -> search -> code -> loader) detonating each node upon arrival"

# --- Event 6: Claude - 1-to-Many Concurrent Fanout Trace ---
Write-Host "[6/6] 1-to-Many Concurrent Fanout Trace (Simultaneous Branching)" -ForegroundColor Cyan
$paths6 = @(
    "crates/ctxvault-core/src/engine.rs",
    "crates/ctxvault-common/src/client.rs",
    "crates/ctxvault-graphview/src/server/routes.rs",
    "crates/ctxvault-mcp/src/transport/http.rs"
)
Send-Activation -Tool "graph_match" -ClientId "claude" -ClientName "Claude Desktop" -Color "#f97316" -Query "(:Engine)-[:defines]->(all)" -Paths $paths6 -DurationMs 29.3
Show-Pause -Seconds ($PauseSeconds + 1) -VisualNote "Watch root node pulse, then simultaneous orange meteors fan out across the graph"

Write-Host ""
Write-Host "=== Showcase Complete! All 6 activations processed successfully. ===" -ForegroundColor Green
Write-Host "Events are captured in the top-right Multi-Agent Activations HUD." -ForegroundColor DarkGray
Write-Host ""
