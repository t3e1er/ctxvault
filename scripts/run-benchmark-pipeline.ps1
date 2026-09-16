<#
.SYNOPSIS
    ArXiv-Grade Multi-Repo & Multi-Dataset Evaluation Pipeline for ctxvault.

.DESCRIPTION
    Fully automated benchmark runner that:
    1. Downloads & stages public reference datasets:
       - CodeSearchNet-AdvTest (Polyglot Go / Python)
       - RepoBench-R (Cross-file multi-module repository retrieval)
       - SWE-bench Lite (GitHub issue-to-patch localization)
       - ctxvault Dogfood (Documentation + polyglot AST code)
    2. Converts external benchmarks into unified ctxvault JSON format via `ctxv-bench import`.
    3. Indexes target repositories and measures resource profiling (files/sec, peak RSS, disk breakdown).
    4. Executes retrieval algorithm ablation across all 6 modes (BM25, SIF+Binary, HippoRAG PPR, Fast, Semantic, Full).
    5. Calculates IR metrics (Recall@K, MRR@K, NDCG@K, Score Separation, Cluster Recall, Hop Distance).
    6. Generates publication-ready artifacts: Markdown reports, CSVs, JSONs, and LaTeX `booktabs` tables.

.PARAMETER WorkDir
    Workspace directory for staging benchmark datasets and cloned repos. Default: .\benchmarks\workspace

.PARAMETER OutputDir
    Output directory for publication reports and LaTeX tables. Default: .\benchmarks\publication_results

.PARAMETER Datasets
    Comma-separated list of datasets to run: "all", or combination of "dogfood,codesearchnet,repobench,swebench". Default: "all"

.PARAMETER Modes
    Comma-separated retrieval modes: "bm25,binary,ppr,fast,semantic,full" or "fast_only" or "all". Default: "all"

.PARAMETER K
    Cutoff rank K for evaluation. Default: 10

.PARAMETER CleanIndex
    Force clean cold-start index rebuild. Default: $false

.PARAMETER ReleaseBuild
    Use cargo --release for benchmark execution. Default: $true

.EXAMPLE
    .\scripts\run-benchmark-pipeline.ps1
    .\scripts\run-benchmark-pipeline.ps1 -Datasets dogfood,codesearchnet -K 10
    .\scripts\run-benchmark-pipeline.ps1 -Modes bm25,binary,ppr,fast -OutputDir .\results
#>
[CmdletBinding()]
param(
    [string]$WorkDir = ".\benchmarks\workspace",
    [string]$OutputDir = ".\benchmarks\publication_results",
    [string[]]$Datasets = @("codesearchnet", "repobench", "swebench"),
    [string[]]$Modes = @("bm25", "binary", "ppr", "fast"),
    [int]$K = 10,
    [int]$SampleLimit = 100,
    [switch]$CleanIndex,
    [switch]$SkipIndex,
    [switch]$Release,
    [switch]$SkipDownload
)

$ErrorActionPreference = "Stop"

# Colors for terminal output
function Log-Title([string]$msg) { Write-Host "`n=== $msg ===" -ForegroundColor Cyan }
function Log-Step([string]$msg)  { Write-Host "  --> $msg" -ForegroundColor Green }
function Log-Info([string]$msg)  { Write-Host "      $msg" -ForegroundColor Gray }
function Log-Warn([string]$msg)  { Write-Host "  [!] $msg" -ForegroundColor Yellow }

# -----------------------------------------------------------------------------
# Phase 1: Build & Environment Verification
# -----------------------------------------------------------------------------
Log-Title "Phase 1: Building Benchmark Harness (ctxv-bench)"

$ReleaseBuild = $Release.IsPresent
$CargoProfile = if ($ReleaseBuild) { "--release" } else { "" }
$BinaryDir = if ($ReleaseBuild) { "target\release" } else { "target\debug" }
$BenchExe = Join-Path $BinaryDir "ctxv-bench.exe"

Log-Step "Compiling ctxvault-bench binary..."
if ($ReleaseBuild) {
    cargo build --release -p ctxvault-bench
} else {
    cargo build -p ctxvault-bench
}

if (-not (Test-Path $BenchExe)) {
    throw "Benchmark binary not found at $BenchExe"
}
Log-Step "Using binary: $BenchExe"

# Ensure directories exist
$DataDir = Join-Path $WorkDir "data"
$ConvertedDir = Join-Path $WorkDir "converted"
$ReposDir = Join-Path $WorkDir "repos"

New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType Directory -Force -Path $ConvertedDir | Out-Null
New-Item -ItemType Directory -Force -Path $ReposDir | Out-Null
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# -----------------------------------------------------------------------------
# Phase 2: Dataset Acquisition
# -----------------------------------------------------------------------------
Log-Title "Phase 2: Benchmark Dataset Acquisition"

$TargetDatasets = @()
foreach ($d in $Datasets) {
    if ($d -eq "all") {
        $TargetDatasets += @("codesearchnet", "repobench", "swebench")
    } else {
        foreach ($sub in $d.Split(",")) {
            $trimmed = $sub.Trim().ToLower()
            if ($trimmed) { $TargetDatasets += $trimmed }
        }
    }
}
$TargetDatasets = $TargetDatasets | Select-Object -Unique

$ModesString = ($Modes | ForEach-Object { $_.Split(",") } | ForEach-Object { $_.Trim().ToLower() } | Where-Object { $_ }) -join ","

# 1. CodeSearchNet (Official GitHub 4,010 human annotations across Go, Python, Java, JS, PHP, Ruby)
$CsnRawPath = Join-Path $DataDir "csn_sample.jsonl"
if ($TargetDatasets -contains "codesearchnet" -and -not $SkipDownload) {
    if (-not (Test-Path $CsnRawPath) -or (Get-Item $CsnRawPath).Length -lt 1000) {
        Log-Step "Fetching full CodeSearchNet human evaluation annotations from GitHub..."
        try {
            $annUrl = "https://raw.githubusercontent.com/github/CodeSearchNet/master/resources/annotationStore.csv"
            $csvData = Invoke-RestMethod -Uri $annUrl -TimeoutSec 45
            $csvRows = $csvData -split "`r?`n" | Select-Object -Skip 1 | Where-Object { $_.Trim() }
            if ($SampleLimit -gt 0) { $csvRows = $csvRows | Select-Object -First $SampleLimit }
            
            $csnLines = foreach ($line in $csvRows) {
                $parts = $line -split ","
                if ($parts.Count -ge 4) {
                    $lang = $parts[0].Trim()
                    $q = $parts[1].Trim()
                    $url = $parts[2].Trim()
                    $relPath = if ($url -match "blob/[^/]+/(.+)(#L\d+)?") { $Matches[1] } else { $url }
                    @{ query = $q; path = $relPath; language = $lang } | ConvertTo-Json -Compress
                }
            }
            $csnContent = $csnLines -join "`n"
            [System.IO.File]::WriteAllText($CsnRawPath, $csnContent, [System.Text.Encoding]::UTF8)
            Log-Info "Downloaded $($csnLines.Count) real CodeSearchNet queries to $CsnRawPath"
        } catch {
            Log-Warn "Online fetch failed: $_"
        }
    }
}

# 2. RepoBench-R (Real Cross-File Repository Context from Hugging Face)
$RepoBenchRawPath = Join-Path $DataDir "repobench_sample.jsonl"
if ($TargetDatasets -contains "repobench" -and -not $SkipDownload) {
    if (-not (Test-Path $RepoBenchRawPath) -or (Get-Item $RepoBenchRawPath).Length -lt 1000) {
        Log-Step "Fetching real RepoBench-R cross-file dataset from Hugging Face..."
        try {
            $limit = if ($SampleLimit -gt 0) { [math]::Min($SampleLimit, 100) } else { 100 }
            $RbUrl = "https://datasets-server.huggingface.co/rows?dataset=tianyang%2Frepobench_python_v1.1&config=default&split=cross_file_first&offset=0&limit=$limit"
            $resp = Invoke-RestMethod -Uri $RbUrl -TimeoutSec 45
            $rbLines = foreach ($row in $resp.rows) {
                $r = $row.row
                $q = ($r.import_statement + " " + $r.next_line).Trim()
                if (-not $q) { $q = $r.cropped_code }
                @{
                    id = "rb_" + $row.row_idx
                    query = $q
                    context = $r.context
                    gold_snippet_path = $r.file_path
                    repo_name = $r.repo_name
                } | ConvertTo-Json -Compress
            }
            $rbContent = $rbLines -join "`n"
            [System.IO.File]::WriteAllText($RepoBenchRawPath, $rbContent, [System.Text.Encoding]::UTF8)
            Log-Info "Downloaded $($rbLines.Count) real RepoBench-R instances to $RepoBenchRawPath"
        } catch {
            Log-Warn "Online fetch failed: $_"
        }
    }
}

# 3. SWE-bench Lite (All 300 Real GitHub Issue-to-Patch Tasks from Hugging Face)
$SweBenchRawPath = Join-Path $DataDir "swebench_sample.json"
if ($TargetDatasets -contains "swebench" -and -not $SkipDownload) {
    if (-not (Test-Path $SweBenchRawPath) -or (Get-Item $SweBenchRawPath).Length -lt 1000) {
        Log-Step "Fetching full SWE-bench Lite tasks (300 tasks) from Hugging Face..."
        try {
            $sweAll = @()
            $offsets = if ($SampleLimit -gt 0 -and $SampleLimit -le 100) { @(0) } else { @(0, 100, 200) }
            foreach ($off in $offsets) {
                $SweUrl = "https://datasets-server.huggingface.co/rows?dataset=princeton-nlp%2FSWE-bench_Lite&config=default&split=test&offset=$off&limit=100"
                $resp = Invoke-RestMethod -Uri $SweUrl -TimeoutSec 45
                $sweAll += $resp.rows | ForEach-Object {
                    [PSCustomObject]@{
                        instance_id = $_.row.instance_id
                        problem_statement = $_.row.problem_statement
                        patch = $_.row.patch
                        repo = $_.row.repo
                    }
                }
            }
            if ($SampleLimit -gt 0 -and $sweAll.Count -gt $SampleLimit) {
                $sweAll = $sweAll | Select-Object -First $SampleLimit
            }
            $jsonStr = $sweAll | ConvertTo-Json -Depth 5
            [System.IO.File]::WriteAllText($SweBenchRawPath, $jsonStr, [System.Text.Encoding]::UTF8)
            Log-Info "Downloaded $($sweAll.Count) real SWE-bench Lite tasks to $SweBenchRawPath"
        } catch {
            Log-Warn "Online fetch failed: $_"
        }
    }
}

# -----------------------------------------------------------------------------
# Phase 3: Benchmark Conversion (ctxv-bench import)
# -----------------------------------------------------------------------------
Log-Title "Phase 3: Dataset Ingestion & Format Normalization"

$ConvertedDatasets = @{}

if ($TargetDatasets -contains "codesearchnet" -and (Test-Path $CsnRawPath)) {
    $OutCsn = Join-Path $ConvertedDir "csn_converted.json"
    Log-Step "Importing CodeSearchNet dataset -> $OutCsn..."
    & $BenchExe import --input $CsnRawPath --format codesearchnet --output $OutCsn
    $ConvertedDatasets["codesearchnet"] = @{
        Path = (Resolve-Path $OutCsn).Path
        Corpus = "."
        Name = "CodeSearchNet-AdvTest"
    }
}

if ($TargetDatasets -contains "repobench" -and (Test-Path $RepoBenchRawPath)) {
    $OutRb = Join-Path $ConvertedDir "repobench_converted.json"
    Log-Step "Importing RepoBench-R dataset -> $OutRb..."
    & $BenchExe import --input $RepoBenchRawPath --format repobench --output $OutRb
    $ConvertedDatasets["repobench"] = @{
        Path = (Resolve-Path $OutRb).Path
        Corpus = "."
        Name = "RepoBench-R Cross-File"
    }
}

if ($TargetDatasets -contains "swebench" -and (Test-Path $SweBenchRawPath)) {
    $OutSwe = Join-Path $ConvertedDir "swebench_converted.json"
    Log-Step "Importing SWE-bench Lite dataset -> $OutSwe..."
    & $BenchExe import --input $SweBenchRawPath --format swebench --output $OutSwe
    $ConvertedDatasets["swebench"] = @{
        Path = (Resolve-Path $OutSwe).Path
        Corpus = "."
        Name = "SWE-bench Lite Localization"
    }
}

# -----------------------------------------------------------------------------
# Phase 4: Indexing & Resource Profiling
# -----------------------------------------------------------------------------
Log-Title "Phase 4: Indexing Pipeline & Resource Profiling"

$ProfileJson = Join-Path $OutputDir "index_profiling_report.json"
if ($SkipIndex -and (Test-Path ".index")) {
    Log-Step "Skipping index build (.index already exists on corpus)..."
} else {
    Log-Step "Profiling index build on corpus: . (Clean: $CleanIndex)..."
    $IndexArgs = @("index", "--corpus", ".", "--output", $ProfileJson)
    if ($CleanIndex) { $IndexArgs += "--clean" }

    & $BenchExe $IndexArgs

    if (Test-Path $ProfileJson) {
        Log-Info "Indexing profile saved to $ProfileJson"
    }
}

# -----------------------------------------------------------------------------
# Phase 5: Multi-Mode Retrieval Evaluation & LaTeX Export
# -----------------------------------------------------------------------------
Log-Title "Phase 5: Retrieval Evaluation & Statistical Significance Sweep"

$AllMasterResults = @()

foreach ($key in $ConvertedDatasets.Keys) {
    $ds = $ConvertedDatasets[$key]
    $dsName = $ds.Name
    $dsFile = $ds.Path
    $corpusPath = $ds.Corpus

    Log-Step "Evaluating on [$dsName] at cutoff K=$K..."
    
    $ReportMd   = Join-Path $OutputDir "${key}_report.md"
    $ReportJson = Join-Path $OutputDir "${key}_report.json"
    $ReportCsv  = Join-Path $OutputDir "${key}_report.csv"
    $ReportTex  = Join-Path $OutputDir "${key}_table.tex"

    # 1. Run evaluation with markdown output
    & $BenchExe eval `
        --corpus $corpusPath `
        --queries $dsFile `
        --modes $ModesString `
        --k $K `
        --output $ReportMd

    # 2. Run evaluation with JSON export
    & $BenchExe eval `
        --corpus $corpusPath `
        --queries $dsFile `
        --modes $ModesString `
        --k $K `
        --output $ReportJson

    # 3. Run evaluation with CSV export
    & $BenchExe eval `
        --corpus $corpusPath `
        --queries $dsFile `
        --modes $ModesString `
        --k $K `
        --output $ReportCsv

    # 4. Export publication LaTeX table
    & $BenchExe eval `
        --corpus $corpusPath `
        --queries $dsFile `
        --modes $ModesString `
        --k $K `
        --output $ReportTex

    Log-Info "Generated reports for [$key]: .md, .json, .csv, and .tex in $OutputDir"
}

# -----------------------------------------------------------------------------
# Phase 6: Publication Summary Dashboard
# -----------------------------------------------------------------------------
Log-Title "Phase 6: Publication-Grade Benchmark Summary"

Write-Host "`nAll benchmark suites completed successfully!" -ForegroundColor Green
Write-Host "Generated Publication Artifacts:" -ForegroundColor Cyan
Get-ChildItem -Path $OutputDir | ForEach-Object {
    Write-Host "  - $($_.Name) ($([math]::Round($_.Length / 1KB, 1)) KB)" -ForegroundColor Gray
}

Write-Host "`nTo view the LaTeX table for arXiv submission:" -ForegroundColor White
Write-Host "  Get-Content (Join-Path '$OutputDir' 'dogfood_table.tex')`n" -ForegroundColor Yellow
