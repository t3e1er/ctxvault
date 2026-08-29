# cxtvault Universal Installer for Windows
# Installs standalone native binary directly into %LOCALAPPDATA%\Programs\cxtvault\bin\cxtvault.exe

$ErrorActionPreference = 'Stop'

$Repo = if ($env:CXTV_GITHUB_REPO) { $env:CXTV_GITHUB_REPO } else { "t3e1er/ctxvault" }
$InstallDir = if ($env:CXTV_INSTALL_DIR) { $env:CXTV_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\cxtvault\bin" }

Write-Host "🔍 Resolving latest release for $Repo..." -ForegroundColor Cyan
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "cxtvault-installer" }
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
$ArchiveName = "cxtvault-$Tag-$Target.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Tag/$ArchiveName"

Write-Host "📦 Downloading $DownloadUrl..." -ForegroundColor Cyan
$TempDir = Join-Path $env:TEMP ("cxtvault-install-" + [Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ZipFile = Join-Path $TempDir $ArchiveName

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipFile -UseBasicParsing
    
    Write-Host "📂 Extracting binary..." -ForegroundColor Cyan
    Expand-Archive -Path $ZipFile -DestinationPath $TempDir -Force

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    
    $SourceExe = Get-ChildItem -Path $TempDir -Filter "cxtvault.exe" -Recurse | Select-Object -First 1
    if (-not $SourceExe) {
        Write-Error "cxtvault.exe not found in extracted archive."
        exit 1
    }

    Copy-Item -Path $SourceExe.FullName -Destination "$InstallDir\cxtvault.exe" -Force
    # Optional alias copy
    Copy-Item -Path $SourceExe.FullName -Destination "$InstallDir\cxtv.exe" -Force -ErrorAction SilentlyContinue
    
    Write-Host ""
    Write-Host "✅ Successfully installed 'cxtvault.exe' to $InstallDir\cxtvault.exe" -ForegroundColor Green
    Write-Host ""

    # Ensure $InstallDir is in User PATH
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -split ";" -notcontains $InstallDir) {
        $NewPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:PATH = "$env:PATH;$InstallDir"
        Write-Host "✨ Added $InstallDir to your User PATH environment variable." -ForegroundColor Yellow
        Write-Host "   (Restart your terminal/IDE for PATH changes to take full effect)." -ForegroundColor Yellow
    }

    Write-Host "🚀 Run 'cxtvault --version' to verify your installation." -ForegroundColor Cyan
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
