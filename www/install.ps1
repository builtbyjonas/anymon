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
Write-Host "Downloading $filename..."
Invoke-WebRequest -Uri $url -OutFile $filename

Write-Host "Downloaded $filename."

# Add the current directory to the user's PATH if not already present
$currentDir = (Get-Location).Path
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$currentDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$currentDir", "User")
    Write-Host "Added $currentDir to your user PATH. You may need to restart your terminal or log out/in for changes to take effect."
} else {
    Write-Host "$currentDir is already in your user PATH."
}
