<#
.SYNOPSIS
    Download the ctxvault embedding model into a sidecar layout that mirrors the
    upstream Hugging Face repo 1:1 (Windows).

.DESCRIPTION
    Downloads the INT8-quantized ONNX weights + tokenizer for
    jinaai/jina-embeddings-v2-base-code into the exact upstream paths, so the
    ctxvault embedder (resolve_model_files in
    crates/ctxvault-core/src/embedding.rs) finds them as-is:

        <ModelsDir>\jina-embeddings-v2-base-code\onnx\model_quantized.onnx
        <ModelsDir>\jina-embeddings-v2-base-code\tokenizer.json

    The embedder resolves this via CTX_MODELS_DIR, or a `models\<model>\` dir next
    to the executable (production sidecar), or `..\models\<model>\` for cargo
    test/deps builds.

    Idempotent: skips download when files already exist with the expected size.

.PARAMETER ModelsDir
    Target models directory. Defaults to $env:CTX_MODELS_DIR, then .\models.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\fetch-model.ps1
    powershell -ExecutionPolicy Bypass -File scripts\fetch-model.ps1 -ModelsDir target\debug\models
#>
[CmdletBinding()]
param(
    [string]$ModelsDir = $(if ($env:CTX_MODELS_DIR) { $env:CTX_MODELS_DIR } else { "./models" })
)

$ErrorActionPreference = "Stop"

# Upstream model (Apache-2.0). Revision pinned for reproducibility.
$HfRepo       = "jinaai/jina-embeddings-v2-base-code"
$HfRevision   = "516f4baf13dec4ddddda8631e019b5737c8bc250"
$ModelDirName = "jina-embeddings-v2-base-code"
$BaseUrl      = "https://huggingface.co/$HfRepo/resolve/$HfRevision"
$DestDir      = Join-Path $ModelsDir $ModelDirName

# Files mirrored verbatim from the HF repo (relative path + expected size).
# model_quantized.onnx = INT8 dynamic quantization (preferred, smallest).
$Files = @(
    @{ Rel = "onnx/model_quantized.onnx"; Bytes = 161895621 },
    @{ Rel = "tokenizer.json";            Bytes = 2561316   }
)

function Get-FileSize([string]$Path) {
    if (Test-Path $Path) { return (Get-Item $Path).Length }
    return 0
}

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

foreach ($f in $Files) {
    $dest = Join-Path $DestDir ($f.Rel -replace '/', '\')
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dest) | Out-Null
    if ((Test-Path $dest) -and ((Get-FileSize $dest) -eq $f.Bytes)) {
        Write-Host "[=] $($f.Rel) already present ($($f.Bytes) bytes), skipping."
        continue
    }
    $url  = "$BaseUrl/$($f.Rel)"
    $part = "$dest.part"
    Write-Host "[*] Downloading $($f.Rel) ($($f.Bytes) bytes)..."
    Invoke-WebRequest -Uri $url -OutFile $part -MaximumRedirection 5
    $got = Get-FileSize $part
    if ($got -ne $f.Bytes) {
        Remove-Item -Force $part
        Write-Error "size mismatch for $($f.Rel): got $got, expected $($f.Bytes)"
        exit 1
    }
    Move-Item -Force $part $dest
    Write-Host "[+] $($f.Rel) ok."
}

$resolved = (Resolve-Path $ModelsDir).Path
Write-Host ""
Write-Host "[+] Model ready at: $DestDir (mirrors the Hugging Face repo layout)"
Write-Host "    Point ctxvault at it with:  `$env:CTX_MODELS_DIR = `"$resolved`""
