# install.ps1 - Download and install the latest anymon release for your OS/arch

$ErrorActionPreference = 'Stop'

$repo = "builtbyjonas/anymon"
$apiUrl = "https://api.github.com/repos/$repo/releases/latest"
$installDoc = "https://github.com/builtbyjonas/anymon/blob/main/docs/installation.md#build-from-source"

# Detect OS and ARCH
$os = "windows"
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $arch = "x86_64" }
    "ARM64" { $arch = "aarch64" }
    "X86"   { $arch = "i386" }
    default { $arch = $arch.ToLower() }
}

# Install directory: use %LOCALAPPDATA%/anymon if available, otherwise %APPDATA%/anymon
$installDir = if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "anymon" } else { Join-Path $env:APPDATA "anymon" }
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

# Logging: keep a transcript so errors are preserved when the window closes
$logPath = Join-Path $installDir "install-log.txt"
try {
    Start-Transcript -Path $logPath -Append -ErrorAction SilentlyContinue | Out-Null
} catch {
    Write-Host "Warning: unable to start transcript logging: $_"
}

# Wrap the main install flow so we can surface and log errors
try {

# Fetch latest release info
$release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing

# Find asset URL
$asset = $release.assets | Where-Object { $_.browser_download_url -match $os -and $_.browser_download_url -match $arch } | Select-Object -First 1

# If no matching asset
if (-not $asset) {
    Write-Host "No prebuilt binary found for $os/$arch."
    Write-Host "Please build from source: $installDoc"
    throw "no-asset"
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

    # If extraction produced a single subdirectory (e.g. anymon-x86_64-pc-windows-msvc),
    # move its contents up so $installDir directly contains the executables.
    $children = Get-ChildItem -Path $installDir -Force | Where-Object { $_.Name -ne "install-log.txt" }
    if ($children.Count -eq 1 -and $children[0].PSIsContainer) {
        $sub = $children[0].FullName
        Get-ChildItem -Path $sub -Force | ForEach-Object {
            $dest = Join-Path $installDir $_.Name
            if (Test-Path $dest) { Remove-Item -Path $dest -Recurse -Force }
            Move-Item -Path $_.FullName -Destination $dest -Force
        }
        # Remove the now-empty subdirectory
        Remove-Item -Path $sub -Recurse -Force
    }

} elseif ($downloadPath -like "*.tar.gz" -or $downloadPath -like "*.tgz") {
    # Use tar if available
    if (Get-Command tar -ErrorAction SilentlyContinue) {
        tar -xzf $downloadPath -C $installDir
        Remove-Item $downloadPath -Force

        # Flatten single-folder extraction like above
        $children = Get-ChildItem -Path $installDir -Force | Where-Object { $_.Name -ne "install-log.txt" }
        if ($children.Count -eq 1 -and $children[0].PSIsContainer) {
            $sub = $children[0].FullName
            Get-ChildItem -Path $sub -Force | ForEach-Object {
                $dest = Join-Path $installDir $_.Name
                if (Test-Path $dest) { Remove-Item -Path $dest -Recurse -Force }
                Move-Item -Path $_.FullName -Destination $dest -Force
            }
            Remove-Item -Path $sub -Recurse -Force
        }
    } else {
        Write-Host "Downloaded tar archive but 'tar' is not available to extract. Please extract $downloadPath manually to $installDir."
    }
} else {
    # raw binary: ensure it's named anymon.exe
    $target = Join-Path $installDir (if ($os -eq "windows") { "anymon.exe" } else { "anymon" })
    Move-Item -Path $downloadPath -Destination $target -Force
}

# Success
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

} catch {
    # Log the error both to host and to the transcript/log file
    Write-Host "ERROR: $($_.Exception.Message)"
    try {
        Add-Content -Path $logPath -Value ("`n[$(Get-Date -Format o)] ERROR: $($_.Exception.ToString())`n") -ErrorAction SilentlyContinue
    } catch {
        Write-Host "Failed to write to log: $_"
    }
    exit 1
} finally {
    # Stop transcript if it was started
    try { Stop-Transcript -ErrorAction SilentlyContinue } catch {}
    Write-Host "Log file: $logPath"
    Write-Host "Press Enter to close this window and view messages."
    Read-Host | Out-Null
}
