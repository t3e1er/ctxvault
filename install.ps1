# ctxvault Universal Installer for Windows
# Installs standalone native binary directly into %LOCALAPPDATA%\Programs\ctxvault\bin\ctxvault.exe

$ErrorActionPreference = 'Stop'

$Repo = if ($env:CTXV_GITHUB_REPO) { $env:CTXV_GITHUB_REPO } elseif ($env:CXTV_GITHUB_REPO) { $env:CXTV_GITHUB_REPO } else { "t3e1er/ctxvault" }
$InstallDir = if ($env:CTXV_INSTALL_DIR) { $env:CTXV_INSTALL_DIR } elseif ($env:CXTV_INSTALL_DIR) { $env:CXTV_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\ctxvault\bin" }

Write-Host "[*] Resolving latest release for $Repo..." -ForegroundColor Cyan
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "ctxvault-installer" }
    $Tag = $Release.tag_name
} catch {
    Write-Error "Failed to query latest release from GitHub API: $_"
    exit 1
}

if (-not $Tag) {
    Write-Error "Could not parse latest release tag name."
    exit 1
}

$Target = "x86_64-pc-windows-msvc"
$ArchiveName = "ctxvault-$Tag-$Target.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Tag/$ArchiveName"

Write-Host "[*] Downloading $DownloadUrl..." -ForegroundColor Cyan
$TempDir = Join-Path $env:TEMP ("ctxvault-install-" + [Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ZipFile = Join-Path $TempDir $ArchiveName

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipFile -UseBasicParsing

    # SHA-256 Digest Validation
    $ChecksumUrls = @(
        "https://github.com/$Repo/releases/download/$Tag/SHA256SUMS.txt",
        "https://github.com/$Repo/releases/download/$Tag/checksums.txt"
    )
    $ChecksumFile = Join-Path $TempDir "checksums.txt"
    $ChecksumFound = $false
    foreach ($Url in $ChecksumUrls) {
        try {
            Invoke-WebRequest -Uri $Url -OutFile $ChecksumFile -UseBasicParsing -ErrorAction SilentlyContinue
            if ((Test-Path $ChecksumFile) -and ((Get-Item $ChecksumFile).Length -gt 0)) {
                $ChecksumFound = $true
                break
            }
        } catch { }
    }

    if ($ChecksumFound) {
        $ExpectedHash = Get-Content $ChecksumFile | Select-String $ArchiveName | ForEach-Object { ($_ -split '\s+')[0] }
        if ($ExpectedHash) {
            Write-Host "[*] Verifying SHA-256 checksum..." -ForegroundColor Cyan
            $ActualHash = (Get-FileHash -Path $ZipFile -Algorithm SHA256).Hash.ToLower()
            if ($ActualHash -ne $ExpectedHash.ToLower()) {
                Write-Error "Checksum verification failed! Expected: $ExpectedHash, Actual: $ActualHash"
                exit 1
            }
            Write-Host "[+] Checksum verified ($ActualHash)." -ForegroundColor Green
        }
    } else {
        Write-Host "[*] Checksum file not available, skipping verification." -ForegroundColor DarkGray
    }

    Write-Host "[*] Extracting binary..." -ForegroundColor Cyan
    Expand-Archive -Path $ZipFile -DestinationPath $TempDir -Force

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    $SourceExe = Get-ChildItem -Path $TempDir -Filter "ctxvault.exe" -Recurse | Select-Object -First 1
    if (-not $SourceExe) {
        Write-Error "ctxvault.exe not found in extracted archive."
        exit 1
    }

    # In-place Windows executable retirement (retires locked running binary to allow hot upgrade)
    $DestExe = Join-Path $InstallDir "ctxvault.exe"
    if (Test-Path $DestExe) {
        $Timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
        $RetiredExe = "$DestExe.retired-$Timestamp"
        try {
            Move-Item -Path $DestExe -Destination $RetiredExe -Force -ErrorAction Stop
            Write-Host "[*] Retired existing binary to $RetiredExe" -ForegroundColor Cyan
        } catch {
            # Continue with copy if rename not needed or fails
        }
    }
    # Clean up stale retired binaries older than 24h
    Get-ChildItem -Path $InstallDir -Filter "ctxvault.exe.retired-*" -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -lt (Get-Date).AddDays(-1) } |
        Remove-Item -Force -ErrorAction SilentlyContinue

    Copy-Item -Path $SourceExe.FullName -Destination "$InstallDir\ctxvault.exe" -Force
    # Optional alias copy
    Copy-Item -Path $SourceExe.FullName -Destination "$InstallDir\ctxv.exe" -Force -ErrorAction SilentlyContinue

    # Place updater script beside binary so ctxvault update points to local immutable script
    if ($PSCommandPath -and (Test-Path $PSCommandPath)) {
        Copy-Item -Path $PSCommandPath -Destination (Join-Path $InstallDir "install.ps1") -Force -ErrorAction SilentlyContinue
    }

    # Install the bundled embedding model as a sidecar next to the binary so the
    # embedder resolves it at <exe_dir>\models\<model>\ (no separate download).
    $SourceModels = Join-Path $SourceExe.Directory.FullName "models"
    if (Test-Path $SourceModels) {
        Write-Host "[*] Installing bundled embedding model (sidecar)..." -ForegroundColor Cyan
        $DestModels = Join-Path $InstallDir "models"
        if (Test-Path $DestModels) { Remove-Item -Recurse -Force $DestModels }
        Copy-Item -Recurse -Path $SourceModels -Destination $DestModels -Force
    }

    Write-Host ""
    Write-Host "[+] Successfully installed 'ctxvault.exe' to $InstallDir\ctxvault.exe" -ForegroundColor Green
    Write-Host ""

    # Ensure $InstallDir is in User PATH
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -split ";" -notcontains $InstallDir) {
        $NewPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:PATH = "$env:PATH;$InstallDir"
        Write-Host "[+] Added $InstallDir to your User PATH environment variable." -ForegroundColor Yellow
        Write-Host "    (Restart your terminal/IDE for PATH changes to take full effect)." -ForegroundColor Yellow
    }

    # Auto-configure installed coding agents
    Write-Host "[*] Auto-configuring coding agents..." -ForegroundColor Cyan
    & "$InstallDir\ctxvault.exe" install -y --dir="$InstallDir"

    Write-Host "[>] Run 'ctxvault --version' to verify your installation." -ForegroundColor Cyan
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
