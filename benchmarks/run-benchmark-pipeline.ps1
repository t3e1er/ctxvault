<#
.SYNOPSIS
    ArXiv-Grade Multi-Repo & Multi-Dataset Evaluation Pipeline for ctxvault.

.DESCRIPTION
    Fully automated benchmark runner that:
    1. Stages benchmark workspaces with strict directory separation:
       - benchmarks/workspace/repos/{swe_bench,codesearchnet,repobench}/<repo_name>
       - benchmarks/workspace/data/{swe_bench,codesearchnet,repobench}/
       - benchmarks/workspace/converted/{swe_bench,codesearchnet,repobench}/
       - benchmarks/results/{swe_bench,codesearchnet,repobench}/
    2. Downloads & stages public reference datasets:
       - SWE-bench Lite (GitHub issue-to-patch localization tasks & target repos)
       - CodeSearchNet-AdvTest (Polyglot human evaluation annotations & repos)
       - RepoBench-R (Cross-file multi-module repository retrieval & repos)
    3. Converts external benchmarks into unified ctxvault format via `ctxv-bench import`.
    4. Indexes target repositories with hardware-accelerated AST, lexical, and vector pipelines.
    5. Executes retrieval ablation across modes:
       - Baseline: bm25, binary (AVX-512 Hamming), ppr (Petgraph PageRank), semantic (768-dim ONNX)
       - Hybrid: fast (HippoRAG-style 3-way RRF), full (4-modality hybrid)
    6. Fast deterministic seeded sampling (-FastSample -SamplePerRepo 10 -Seed 42).
    7. Streamlines publication outputs (.md & .csv only) and compiles a unified master
       root summary report: benchmarks/results/summary_report.md and summary_report.csv.

.PARAMETER WorkDir
    Workspace directory for staging benchmarks and cloned repos. Default: .\benchmarks\workspace

.PARAMETER OutputDir
    Output directory for publication reports. Default: .\benchmarks\results

.PARAMETER Datasets
    Comma-separated list of datasets to run: "all", or combination of "swe_bench,codesearchnet,repobench". Default: "swe_bench,codesearchnet,repobench"

.PARAMETER Modes
    Comma-separated retrieval modes: "bm25,binary,ppr,fast,semantic,full" or "all". Default: "bm25,binary,ppr,fast"

.PARAMETER K
    Cutoff rank K for evaluation. Default: 10

.PARAMETER FastSample
    Enable fast deterministic seeded sampling of queries per repo. Default: $false

.PARAMETER SamplePerRepo
    Number of queries to sample per repository when -FastSample is enabled. Default: 10

.PARAMETER Seed
    Deterministic random seed for sampling. Default: 42
#>
[CmdletBinding()]
param(
    [string]$WorkDir = "",
    [string]$OutputDir = "",
    [string[]]$Datasets = @("swe_bench", "codesearchnet", "repobench"),
    [string[]]$TargetRepos = @("pallets/flask", "psf/requests", "astropy/astropy"),
    [string[]]$Modes = @("bm25", "binary", "ppr", "fast"),
    [int]$K = 10,
    [int]$SampleLimit = 300,
    [switch]$FastSample,
    [int]$SamplePerRepo = 10,
    [uint32]$Seed = 42,
    [switch]$CleanIndex,
    [switch]$SkipIndex,
    [switch]$Force,
    [switch]$Release,
    [switch]$SkipDownload,
    [switch]$ForceDownload,
    [switch]$CloneAllRepos,
    [switch]$IncludeJson,
    [switch]$IncludeTex
)

$ErrorActionPreference = "Stop"

# Colors for terminal output
function Log-Title([string]$msg) { Write-Host "`n=== $msg ===" -ForegroundColor Cyan }
function Log-Step([string]$msg)  { Write-Host "  --> $msg" -ForegroundColor Green }
function Log-Info([string]$msg)  { Write-Host "      $msg" -ForegroundColor Gray }
function Log-Warn([string]$msg)  { Write-Host "  [!] $msg" -ForegroundColor Yellow }

# -----------------------------------------------------------------------------
# Determine Paths Relative to Benchmark Root
# -----------------------------------------------------------------------------
$BenchDir = $PSScriptRoot
$RootDir = Split-Path $BenchDir -Parent

if (-not $WorkDir) {
    $WorkDir = Join-Path $BenchDir "workspace"
} else {
    $WorkDir = [System.IO.Path]::GetFullPath($WorkDir)
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $BenchDir "results"
} else {
    $OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
}

# -----------------------------------------------------------------------------
# Phase 1: Build & Environment Verification
# -----------------------------------------------------------------------------
Log-Title "Phase 1: Building Benchmark Harness (ctxv-bench)"

$ReleaseBuild = $Release.IsPresent
$CargoProfile = if ($ReleaseBuild) { "--release" } else { "" }
$BinaryDir = if ($ReleaseBuild) { Join-Path $RootDir "target\release" } else { Join-Path $RootDir "target\debug" }
$BenchExe = Join-Path $BinaryDir "ctxv-bench.exe"

Log-Step "Compiling ctxvault-bench binary..."
Push-Location $RootDir
try {
    if ($ReleaseBuild) {
        cargo build --release -p ctxvault-bench
    } else {
        cargo build -p ctxvault-bench
    }
} finally {
    Pop-Location
}

if (-not (Test-Path $BenchExe)) {
    throw "Benchmark binary not found at $BenchExe"
}
Log-Step "Using binary: $BenchExe"

# -----------------------------------------------------------------------------
# Strict Relative Directory Separation
# -----------------------------------------------------------------------------
$DataDir = Join-Path $WorkDir "data"
$ConvertedDir = Join-Path $WorkDir "converted"
$ReposDir = Join-Path $WorkDir "repos"

# Raw Data Subdirectories
$DataDirSwe = Join-Path $DataDir "swe_bench"
$DataDirCsn = Join-Path $DataDir "codesearchnet"
$DataDirRb  = Join-Path $DataDir "repobench"

# Converted Query JSON Subdirectories
$ConvDirSwe = Join-Path $ConvertedDir "swe_bench"
$ConvDirCsn = Join-Path $ConvertedDir "codesearchnet"
$ConvDirRb  = Join-Path $ConvertedDir "repobench"

# Cloned Repositories Subdirectories
$RepoDirSwe = Join-Path $ReposDir "swe_bench"
$RepoDirCsn = Join-Path $ReposDir "codesearchnet"
$RepoDirRb  = Join-Path $ReposDir "repobench"

# Results Subdirectories
$ResDirSwe = Join-Path $OutputDir "swe_bench"
$ResDirCsn = Join-Path $OutputDir "codesearchnet"
$ResDirRb  = Join-Path $OutputDir "repobench"

foreach ($dir in @($DataDirSwe, $DataDirCsn, $DataDirRb, $ConvDirSwe, $ConvDirCsn, $ConvDirRb, $RepoDirSwe, $RepoDirCsn, $RepoDirRb, $ResDirSwe, $ResDirCsn, $ResDirRb)) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
}

# Auto-migrate any unnamespaced repositories previously cloned directly under repos/
Get-ChildItem -Path $ReposDir -Directory | Where-Object {
    $_.Name -ne "swe_bench" -and $_.Name -ne "codesearchnet" -and $_.Name -ne "repobench"
} | ForEach-Object {
    $dest = Join-Path $RepoDirSwe $_.Name
    if (-not (Test-Path $dest)) {
        Log-Info "Migrating existing repo $($_.Name) -> $dest"
        Move-Item -Path $_.FullName -Destination $dest -Force
    }
}

# -----------------------------------------------------------------------------
# Phase 2: Benchmark Dataset Acquisition
# -----------------------------------------------------------------------------
Log-Title "Phase 2: Benchmark Dataset Acquisition"

$TargetDatasets = @()
foreach ($d in $Datasets) {
    if ($d -eq "all") {
        $TargetDatasets += @("swe_bench", "codesearchnet", "repobench")
    } else {
        foreach ($sub in $d.Split(",")) {
            $trimmed = $sub.Trim().ToLower().Replace("-", "_")
            if ($trimmed -eq "swebench") { $trimmed = "swe_bench" }
            if ($trimmed) { $TargetDatasets += $trimmed }
        }
    }
}
$TargetDatasets = $TargetDatasets | Select-Object -Unique

$ModesString = ($Modes | ForEach-Object { $_.Split(",") } | ForEach-Object { $_.Trim().ToLower() } | Where-Object { $_ }) -join ","
$NeedsReembed = ($ModesString.Contains("semantic") -or $ModesString.Contains("full"))

# Reference committed datasets in benchmarks/data/
$CommittedSwe = Join-Path $BenchDir "data\swe_bench.json"
$CommittedCsn = Join-Path $BenchDir "data\codesearchnet.json"
$CommittedRb  = Join-Path $BenchDir "data\repobench.json"

# 1. SWE-bench Lite (GitHub issue-to-patch localization tasks)
$SweBenchRawPath = Join-Path $DataDirSwe "swebench_sample.json"
$OutSwe = Join-Path $ConvDirSwe "swebench_converted.json"

if ($TargetDatasets -contains "swe_bench") {
    if ((Test-Path $CommittedSwe) -and (-not $ForceDownload)) {
        Log-Info "Using committed SWE-bench reference queries from $CommittedSwe"
        Copy-Item $CommittedSwe $OutSwe -Force
    } elseif (-not $SkipDownload) {
        if ($ForceDownload -or (-not (Test-Path $SweBenchRawPath)) -or (Get-Item $SweBenchRawPath).Length -lt 1000) {
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
                Log-Warn "SWE-bench online fetch failed: $_"
            }
        }
    }
}

# 2. CodeSearchNet-AdvTest (Official GitHub 4,010 human annotations across Go, Python, Java, JS, etc.)
$CsnRawPath = Join-Path $DataDirCsn "csn_sample.jsonl"
$OutCsn = Join-Path $ConvDirCsn "csn_converted.json"

if ($TargetDatasets -contains "codesearchnet") {
    if ((Test-Path $CommittedCsn) -and (-not $ForceDownload)) {
        Log-Info "Using committed CodeSearchNet reference queries from $CommittedCsn"
        Copy-Item $CommittedCsn $OutCsn -Force
    } elseif (-not $SkipDownload) {
        if ($ForceDownload -or (-not (Test-Path $CsnRawPath)) -or (Get-Item $CsnRawPath).Length -lt 1000) {
            Log-Step "Fetching full CodeSearchNet human evaluation annotations from GitHub..."
            try {
                $annUrl = "https://raw.githubusercontent.com/github/CodeSearchNet/master/resources/annotationStore.csv"
                $csvData = Invoke-RestMethod -Uri $annUrl -TimeoutSec 45
                $csvRows = $csvData -split "`r?`n" | Select-Object -Skip 1 | Where-Object { $_.Trim() }
                if ($SampleLimit -gt 0) { $csvRows = $csvRows | Select-Object -First $SampleLimit }

                $csnLines = foreach ($line in $csvRows) {
                    $parts = $line -split ","
                    if ($parts.Count -ge 3) {
                        $lang = $parts[0].Trim()
                        $q = $parts[1].Trim()
                        $url = $parts[2].Trim()
                        $relPath = if ($url -match "blob/[^/]+/(.+)(#L\d+)?") { $Matches[1] } else { $url }
                        $repoName = if ($url -match "github\.com/([^/]+/[^/]+)") { $Matches[1] } else { "" }
                        @{ query = $q; path = $relPath; language = $lang; repo_name = $repoName } | ConvertTo-Json -Compress
                    }
                }
                $csnContent = $csnLines -join "`n"
                [System.IO.File]::WriteAllText($CsnRawPath, $csnContent, [System.Text.Encoding]::UTF8)
                Log-Info "Downloaded $($csnLines.Count) real CodeSearchNet queries to $CsnRawPath"
            } catch {
                Log-Warn "CodeSearchNet online fetch failed: $_"
            }
        }
    }
}

# 3. RepoBench-R (Real Cross-File Repository Context from Hugging Face)
$RepoBenchRawPath = Join-Path $DataDirRb "repobench_sample.jsonl"
$OutRb = Join-Path $ConvDirRb "repobench_converted.json"

if ($TargetDatasets -contains "repobench") {
    if ((Test-Path $CommittedRb) -and (-not $ForceDownload)) {
        Log-Info "Using committed RepoBench reference queries from $CommittedRb"
        Copy-Item $CommittedRb $OutRb -Force
    } elseif (-not $SkipDownload) {
        if ($ForceDownload -or (-not (Test-Path $RepoBenchRawPath)) -or (Get-Item $RepoBenchRawPath).Length -lt 1000) {
            Log-Step "Fetching real RepoBench-R cross-file dataset from Hugging Face..."
            try {
                $targetCount = if ($SampleLimit -gt 0) { $SampleLimit } else { 300 }
                $numPages = [math]::Max(1, [math]::Ceiling($targetCount / 100))
                $offsets = 0..($numPages - 1) | ForEach-Object { $_ * 100 }
                $rbLines = @()
                foreach ($off in $offsets) {
                    $pageLimit = [math]::Min(100, $targetCount - $rbLines.Count)
                    if ($pageLimit -le 0) { break }
                    $RbUrl = "https://datasets-server.huggingface.co/rows?dataset=tianyang%2Frepobench_python_v1.1&config=default&split=cross_file_first&offset=$off&limit=$pageLimit"
                    $resp = Invoke-RestMethod -Uri $RbUrl -TimeoutSec 45
                    foreach ($row in $resp.rows) {
                        $r = $row.row
                        $q = ($r.import_statement + " " + $r.next_line).Trim()
                        if (-not $q) { $q = $r.cropped_code }
                        $rbLines += (@{
                            id = "rb_" + $row.row_idx
                            query = $q
                            context = $r.context
                            gold_snippet_path = $r.file_path
                            repo_name = $r.repo_name
                        } | ConvertTo-Json -Compress)
                        if ($SampleLimit -gt 0 -and $rbLines.Count -ge $SampleLimit) { break }
                    }
                    if ($SampleLimit -gt 0 -and $rbLines.Count -ge $SampleLimit) { break }
                }
                $rbContent = $rbLines -join "`n"
                [System.IO.File]::WriteAllText($RepoBenchRawPath, $rbContent, [System.Text.Encoding]::UTF8)
                Log-Info "Downloaded $($rbLines.Count) real RepoBench-R instances to $RepoBenchRawPath"
            } catch {
                Log-Warn "RepoBench online fetch failed: $_"
            }
        }
    }
}

# -----------------------------------------------------------------------------
# Phase 3: Benchmark Conversion (ctxv-bench import)
# -----------------------------------------------------------------------------
Log-Title "Phase 3: Dataset Ingestion & Format Normalization"

$ConvertedDatasets = @{}

# 1. SWE-bench Lite
if ($TargetDatasets -contains "swe_bench") {
    if (-not (Test-Path $OutSwe) -and (Test-Path $SweBenchRawPath)) {
        Log-Step "Importing SWE-bench Lite dataset -> $OutSwe..."
        & $BenchExe import --input $SweBenchRawPath --format swebench --output $OutSwe
    }
    if (Test-Path $OutSwe) {
        $ConvertedDatasets["swe_bench"] = @{
            Path = (Resolve-Path $OutSwe).Path
            Name = "SWE-bench Lite"
        }
    }
}

# 2. CodeSearchNet
if ($TargetDatasets -contains "codesearchnet") {
    if (-not (Test-Path $OutCsn) -and (Test-Path $CsnRawPath)) {
        Log-Step "Importing CodeSearchNet dataset -> $OutCsn..."
        & $BenchExe import --input $CsnRawPath --format codesearchnet --output $OutCsn
    }
    if (Test-Path $OutCsn) {
        $ConvertedDatasets["codesearchnet"] = @{
            Path = (Resolve-Path $OutCsn).Path
            Name = "CodeSearchNet-AdvTest"
        }
    }
}

# 3. RepoBench-R
if ($TargetDatasets -contains "repobench") {
    if (-not (Test-Path $OutRb) -and (Test-Path $RepoBenchRawPath)) {
        Log-Step "Importing RepoBench-R dataset -> $OutRb..."
        & $BenchExe import --input $RepoBenchRawPath --format repobench --output $OutRb
    }
    if (Test-Path $OutRb) {
        $ConvertedDatasets["repobench"] = @{
            Path = (Resolve-Path $OutRb).Path
            Name = "RepoBench-R Cross-File"
        }
    }
}

# -----------------------------------------------------------------------------
# Phase 3b: Sub-Repository Acquisition (git clone)
# -----------------------------------------------------------------------------
Log-Title "Phase 3b: Sub-Repository Acquisition & Partitioning"

$ClonedReposSwe = @{}
$ClonedReposCsn = @{}
$ClonedReposRb  = @{}

# 1. SWE-bench Lite Repositories
if ($TargetDatasets -contains "swe_bench") {
    $sweReposToClone = if ($CloneAllRepos) {
        @(
            "pallets/flask", "psf/requests", "astropy/astropy",
            "django/django", "sympy/sympy", "pytest-dev/pytest",
            "scikit-learn/scikit-learn", "sphinx-doc/sphinx",
            "matplotlib/matplotlib", "pydata/xarray", "pylint-dev/pylint",
            "mwaskom/seaborn"
        )
    } else {
        $expanded = @()
        foreach ($r in $TargetRepos) {
            foreach ($sub in $r.Split(",")) {
                $trimmed = $sub.Trim()
                if ($trimmed) { $expanded += $trimmed }
            }
        }
        $expanded
    }

    foreach ($repo in $sweReposToClone) {
        $repoFolder = $repo.Replace("/", "__")
        $localPath = Join-Path $RepoDirSwe $repoFolder
        if (-not (Test-Path $localPath)) {
            Log-Step "Cloning SWE-bench repo: $repo -> $localPath..."
            git clone --depth 1 "https://github.com/$repo.git" $localPath
        } else {
            Log-Info "SWE-bench repo $repo already exists at $localPath"
        }
        if (Test-Path $localPath) {
            $ClonedReposSwe[$repo] = (Resolve-Path $localPath).Path
        }
    }
}

# 2. RepoBench-R Repositories
if ($TargetDatasets -contains "repobench") {
    $rbReposToClone = @(
        "DLYuanGod/TinyGPT-V",
        "jianchang512/vocal-separate",
        "ali-vilab/dreamtalk",
        "Meituan-AutoML/MobileVLM"
    )
    foreach ($repo in $rbReposToClone) {
        $repoFolder = $repo.Replace("/", "__")
        $localPath = Join-Path $RepoDirRb $repoFolder
        if (-not (Test-Path $localPath)) {
            Log-Step "Cloning RepoBench repo: $repo -> $localPath..."
            try {
                git clone --depth 1 "https://github.com/$repo.git" $localPath
            } catch {
                Log-Warn "Failed to clone ${repo}: $_"
            }
        } else {
            Log-Info "RepoBench repo $repo already exists at $localPath"
        }
        if (Test-Path $localPath) {
            $ClonedReposRb[$repo] = (Resolve-Path $localPath).Path
        }
    }
}

# 3. CodeSearchNet Repositories
if ($TargetDatasets -contains "codesearchnet") {
    $csnReposToClone = @(
        "coreos/go-omaha",
        "wcharczuk/go-chart",
        "tylertreat/BoomFilters",
        "grokify/gotilla"
    )
    foreach ($repo in $csnReposToClone) {
        $repoFolder = $repo.Replace("/", "__")
        $localPath = Join-Path $RepoDirCsn $repoFolder
        if (-not (Test-Path $localPath)) {
            Log-Step "Cloning CodeSearchNet repo: $repo -> $localPath..."
            try {
                git clone --depth 1 "https://github.com/$repo.git" $localPath
            } catch {
                Log-Warn "Failed to clone ${repo}: $_"
            }
        } else {
            Log-Info "CodeSearchNet repo $repo already exists at $localPath"
        }
        if (Test-Path $localPath) {
            $ClonedReposCsn[$repo] = (Resolve-Path $localPath).Path
        }
    }
}

# -----------------------------------------------------------------------------
# Phase 4: Indexing & Resource Profiling
# -----------------------------------------------------------------------------
Log-Title "Phase 4: Hardware-Accelerated Indexing & Resource Profiling"

function Build-CorpusIndex([string]$corpusPath, [string]$profOutput) {
    $idxDir = Join-Path $corpusPath ".index"
    $metaDb = Join-Path $idxDir "meta.db"

    # Smart index reuse: skip re-indexing if valid, healthy .index already exists
    if (-not $CleanIndex -and -not $Force) {
        if ($SkipIndex -and (Test-Path $idxDir)) {
            Log-Info "Skipping indexing for $corpusPath (-SkipIndex specified)"
            return
        }
        if (Test-Path $metaDb) {
            $statusOut = & $BenchExe status --corpus $corpusPath 2>&1
            if ($LASTEXITCODE -eq 0) {
                Log-Info "Skipping indexing for $corpusPath (healthy .index with documents found; use -CleanIndex or -Force to rebuild)"
                return
            }
        }
    }

    $doClean = $CleanIndex -or $Force
    Log-Step "Indexing corpus: $corpusPath (Clean: $doClean, Reembed: $NeedsReembed)..."
    $idxArgs = @("index", "--corpus", $corpusPath, "--output", $profOutput)
    if ($doClean) { $idxArgs += "--clean" }
    if ($NeedsReembed) { $idxArgs += "--reembed" }
    & $BenchExe $idxArgs
}

# Index SWE-bench repositories
foreach ($repo in $ClonedReposSwe.Keys) {
    $rPath = $ClonedReposSwe[$repo]
    $pJson = Join-Path $ResDirSwe "index_profile_$($repo.Replace('/', '__')).json"
    Build-CorpusIndex $rPath $pJson
}

# Index RepoBench repositories
foreach ($repo in $ClonedReposRb.Keys) {
    $rPath = $ClonedReposRb[$repo]
    $pJson = Join-Path $ResDirRb "index_profile_$($repo.Replace('/', '__')).json"
    Build-CorpusIndex $rPath $pJson
}

# Index CodeSearchNet repositories
foreach ($repo in $ClonedReposCsn.Keys) {
    $rPath = $ClonedReposCsn[$repo]
    $pJson = Join-Path $ResDirCsn "index_profile_$($repo.Replace('/', '__')).json"
    Build-CorpusIndex $rPath $pJson
}

# -----------------------------------------------------------------------------
# Phase 5: Multi-Mode Retrieval Evaluation
# -----------------------------------------------------------------------------
Log-Title "Phase 5: Retrieval Evaluation & Statistical Ablation Sweep"

# Helper to construct eval arguments
function Run-Eval(
    [string]$corpusPath,
    [string]$queriesPath,
    [string]$benchName,
    [string]$repoName,
    [string]$categoryFilter,
    [string]$outDir,
    [string]$outPrefix
) {
    $evalArgs = @(
        "eval",
        "--corpus", $corpusPath,
        "--queries", $queriesPath,
        "--modes", $ModesString,
        "--k", $K,
        "--output-dir", $outDir,
        "--output-prefix", $outPrefix,
        "--benchmark-name", $benchName,
        "--repository", $repoName,
        "--modality", "code"
    )
    if ($categoryFilter) {
        $evalArgs += @("--category", $categoryFilter)
    }
    if ($FastSample) {
        $evalArgs += @("--sample", $SamplePerRepo, "--seed", $Seed)
    }
    if ($IncludeJson) {
        $evalArgs += "--include-json"
    }
    if ($IncludeTex) {
        $evalArgs += "--include-tex"
    }

    & $BenchExe $evalArgs
}

# 1. SWE-bench Lite: Per-repository evaluation
if ($TargetDatasets -contains "swe_bench" -and (Test-Path $OutSwe)) {
    foreach ($repo in $ClonedReposSwe.Keys) {
        $repoPath = $ClonedReposSwe[$repo]
        $repoPrefix = $repo.Replace("/", "__")
        Log-Step "Evaluating SWE-bench on [$repo] at K=$K (Modes: $ModesString)..."
        Run-Eval `
            -corpusPath $repoPath `
            -queriesPath $OutSwe `
            -benchName "swe_bench" `
            -repoName $repo `
            -categoryFilter $repo `
            -outDir $ResDirSwe `
            -outPrefix $repoPrefix
    }
}

# 2. RepoBench-R: Per-repository evaluation
if ($TargetDatasets -contains "repobench" -and (Test-Path $OutRb)) {
    foreach ($repo in $ClonedReposRb.Keys) {
        $repoPath = $ClonedReposRb[$repo]
        $repoPrefix = $repo.Replace("/", "__")
        Log-Step "Evaluating RepoBench-R on [$repo] at K=$K (Modes: $ModesString)..."
        Run-Eval `
            -corpusPath $repoPath `
            -queriesPath $OutRb `
            -benchName "repobench" `
            -repoName $repo `
            -categoryFilter $repo `
            -outDir $ResDirRb `
            -outPrefix $repoPrefix
    }
}

# 3. CodeSearchNet: Per-repository evaluation
if ($TargetDatasets -contains "codesearchnet" -and (Test-Path $OutCsn)) {
    foreach ($repo in $ClonedReposCsn.Keys) {
        $repoPath = $ClonedReposCsn[$repo]
        $repoPrefix = $repo.Replace("/", "__")
        Log-Step "Evaluating CodeSearchNet on [$repo] at K=$K (Modes: $ModesString)..."
        Run-Eval `
            -corpusPath $repoPath `
            -queriesPath $OutCsn `
            -benchName "codesearchnet" `
            -repoName $repo `
            -categoryFilter $repo `
            -outDir $ResDirCsn `
            -outPrefix $repoPrefix
    }
}

# -----------------------------------------------------------------------------
# Phase 6: Unified Master Aggregate Report
# -----------------------------------------------------------------------------
Log-Title "Phase 6: Compiling Master Leaderboard & Publication Summary"

Log-Step "Aggregating sub-reports across all evaluated suites -> $OutputDir..."
& $BenchExe aggregate --results-dir $OutputDir --output-dir $OutputDir

Write-Host "`nAll benchmark suites completed successfully!" -ForegroundColor Green
Write-Host "Unified Master Artifacts:" -ForegroundColor Cyan
Write-Host "  - Markdown Master Summary: $(Join-Path $OutputDir 'summary_report.md')" -ForegroundColor Gray
Write-Host "  - CSV Master Data Table:   $(Join-Path $OutputDir 'summary_report.csv')" -ForegroundColor Gray

if (Test-Path (Join-Path $OutputDir "summary_report.md")) {
    Write-Host "`n--- Master Summary Preview ---`n" -ForegroundColor Yellow
    Get-Content (Join-Path $OutputDir "summary_report.md") | Select-Object -First 35 | ForEach-Object { Write-Host $_ }
}
