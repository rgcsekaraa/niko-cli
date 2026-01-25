# Niko CLI Installer for Windows
# Usage: iwr -useb https://get.niko.dev/install.ps1 | iex

$ErrorActionPreference = "Stop"

$REPO = "rgcsekaraa/niko-cli"
$BINARY_NAME = "niko"
$INSTALL_DIR = "$env:LOCALAPPDATA\niko\bin"

function Write-Banner {
    Write-Host @"
  _   _ _ _
 | \ | (_) | _____
 |  \| | | |/ / _ \
 | |\  | |   < (_) |
 |_| \_|_|_|\_\___/

"@ -ForegroundColor Cyan
    Write-Host "Natural Language to Shell Commands" -ForegroundColor White
    Write-Host ""
}

function Get-Architecture {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "amd64" }
        "ARM64" { return "arm64" }
        default { return "unknown" }
    }
}

function Get-LatestVersion {
    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest"
        return $response.tag_name
    } catch {
        return "latest"
    }
}

function Install-Niko {
    Write-Banner

    $arch = Get-Architecture
    if ($arch -eq "unknown") {
        Write-Host "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" -ForegroundColor Red
        exit 1
    }

    Write-Host "Detected: windows/$arch" -ForegroundColor Cyan

    Write-Host "Fetching latest version..."
    $version = Get-LatestVersion
    Write-Host "Latest version: $version" -ForegroundColor Cyan

    $filename = "$BINARY_NAME-windows-$arch.exe"
    $url = "https://github.com/$REPO/releases/download/$version/$filename"

    # Create install directory
    if (-not (Test-Path $INSTALL_DIR)) {
        New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
    }

    $installPath = Join-Path $INSTALL_DIR "$BINARY_NAME.exe"

    Write-Host "Downloading $filename..." -ForegroundColor Cyan
    try {
        Invoke-WebRequest -Uri $url -OutFile $installPath
    } catch {
        Write-Host "Failed to download binary" -ForegroundColor Red
        Write-Host ""
        Write-Host "You can install manually:"
        Write-Host "  go install github.com/$REPO/cmd/niko@latest"
        exit 1
    }

    # Add to PATH if not already there
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$INSTALL_DIR*") {
        Write-Host "Adding to PATH..." -ForegroundColor Cyan
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$INSTALL_DIR", "User")
        $env:Path = "$env:Path;$INSTALL_DIR"
    }

    # Verify installation
    try {
        $versionOutput = & $installPath version 2>&1
        Write-Host "Niko $versionOutput installed successfully!" -ForegroundColor Green
    } catch {
        Write-Host "Installation completed but verification failed" -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "Get started (it's that simple):" -ForegroundColor Green
    Write-Host ""
    Write-Host '  niko "list all files"'
    Write-Host ""
    Write-Host "First run will auto-download Ollama + AI model (~1GB)." -ForegroundColor Yellow
    Write-Host "After that, it works offline - no API keys needed!" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "More examples:"
    Write-Host '  niko "find files larger than 100MB"'
    Write-Host '  niko "show running processes"'
    Write-Host '  niko "compress folder to zip"'
    Write-Host ""
    Write-Host "Documentation: https://github.com/$REPO" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "NOTE: Restart your terminal for PATH changes to take effect" -ForegroundColor Yellow
}

Install-Niko
