$ErrorActionPreference = "Stop"

$repoOwner = "rgcsekaraa"
$repoName = "niko-cli"
$binName = "niko.exe"

Write-Host "Detecting latest version..." -ForegroundColor Cyan
$latestUrl = "https://api.github.com/repos/$repoOwner/$repoName/releases/latest"
try {
    $latestJson = Invoke-RestMethod -Uri $latestUrl
    $version = $latestJson.tag_name
} catch {
    Write-Error "Could not determine latest version."
    exit 1
}

$assetName = "niko-windows-amd64.exe"
$downloadUrl = "https://github.com/$repoOwner/$repoName/releases/download/$version/$assetName"

$installDir = "$env:LOCALAPPDATA\Programs\niko"
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

$targetFile = "$installDir\$binName"

Write-Host "Downloading niko $version..." -ForegroundColor Client
Invoke-WebRequest -Uri $downloadUrl -OutFile $targetFile

# Add to PATH if not present
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    Write-Host "Adding to PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    $env:Path += ";$installDir"
}

Write-Host "Successfully installed to $targetFile" -ForegroundColor Green
Write-Host "You may need to restart your terminal." -ForegroundColor Yellow
