# install.ps1 - Download and install the latest anymon release for your OS/arch

$repo = "builtbyjonas/anymon"
$apiUrl = "https://api.github.com/repos/$repo/releases/latest"
$installDoc = "https://github.com/builtbyjonas/anymon/blob/main/docs/installation.md#build-from-source"

# Detect OS and ARCH
$os = "windows"
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $arch = "amd64" }
    "ARM64" { $arch = "arm64" }
    default { $arch = $arch.ToLower() }
}

# Install directory: use %LOCALAPPDATA%/anymon if available, otherwise %APPDATA%/anymon
$installDir = if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "anymon" } else { Join-Path $env:APPDATA "anymon" }
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

# Fetch latest release info
try {
    $release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing
} catch {
    Write-Host "Failed to fetch release info."
    exit 1
}

# Find asset URL
$asset = $release.assets | Where-Object { $_.browser_download_url -match $os -and $_.browser_download_url -match $arch } | Select-Object -First 1

if (-not $asset) {
    Write-Host "No prebuilt binary found for $os/$arch."
    Write-Host "Please build from source: $installDoc"
    exit 1
}

$filename = $asset.name
$url = $asset.browser_download_url
$downloadPath = Join-Path $installDir $filename
Write-Host "Downloading $filename to $downloadPath..."
Invoke-WebRequest -Uri $url -OutFile $downloadPath -UseBasicParsing

# If archive, extract; otherwise ensure executable is placed in install dir
if ($downloadPath -like "*.zip") {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($downloadPath, $installDir)
    Remove-Item $downloadPath -Force
} elseif ($downloadPath -like "*.tar.gz" -or $downloadPath -like "*.tgz") {
    # Use tar if available
    if (Get-Command tar -ErrorAction SilentlyContinue) {
        tar -xzf $downloadPath -C $installDir
        Remove-Item $downloadPath -Force
    } else {
        Write-Host "Downloaded tar archive but 'tar' is not available to extract. Please extract $downloadPath manually to $installDir."
    }
} else {
    # raw binary: ensure it's named anymon.exe
    $target = Join-Path $installDir (if ($os -eq "windows") { "anymon.exe" } else { "anymon" })
    Move-Item -Path $downloadPath -Destination $target -Force
}

Write-Host "Installation completed to $installDir."

# Add the install directory to the user's PATH if not already present
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$installDir*") {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $installDir } else { "$userPath;$installDir" }
    [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    Write-Host "Added $installDir to your user PATH. You may need to restart your terminal or log out/in for changes to take effect."
} else {
    Write-Host "$installDir is already in your user PATH."
}
