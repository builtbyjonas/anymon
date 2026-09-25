# Install the latest anymon release on Windows.
#
#   irm https://anymon.xyz/install.ps1 | iex
#
# Environment variables:
#   ANYMON_VERSION         version to install, e.g. 1.0.0 (default: latest)
#   ANYMON_INSTALL_DIR     install directory (default: %LOCALAPPDATA%\anymon)
#   ANYMON_NO_MODIFY_PATH  set to 1 to not add the install directory to PATH
#
# Works with Windows PowerShell 5.1 and PowerShell 7, verifies the download's
# SHA-256 checksum and never asks for input. Errors are reported without
# closing the current PowerShell session.

& {
    $ErrorActionPreference = 'Stop'
    # The progress bar makes Invoke-WebRequest very slow in Windows PowerShell.
    $ProgressPreference = 'SilentlyContinue'

    $repo = 'builtbyjonas/anymon'
    $docs = "https://github.com/$repo/blob/main/docs/installation.md"

    function Write-Step([string] $Message) {
        Write-Host "anymon: $Message"
    }

    function Test-SamePath([string] $A, [string] $B) {
        return $A.TrimEnd('\') -ieq $B.TrimEnd('\')
    }

    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch {
        # Not needed on PowerShell 7.
    }

    # Detect the architecture of the OS (not of the PowerShell process, which
    # may run emulated).
    $arch = $null
    try {
        $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    } catch {
        $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    }
    switch -Regex ($arch) {
        '^(x64|amd64)$' { $target = 'x86_64-pc-windows-msvc'; break }
        '^arm64$' { $target = 'aarch64-pc-windows-msvc'; break }
        default { throw "anymon: unsupported CPU architecture '$arch'; see $docs to build from source" }
    }

    $installDir = if ($env:ANYMON_INSTALL_DIR) {
        $env:ANYMON_INSTALL_DIR
    } elseif ($env:LOCALAPPDATA) {
        Join-Path $env:LOCALAPPDATA 'anymon'
    } else {
        Join-Path $HOME '.anymon'
    }

    $archive = "anymon-$target.zip"
    if ($env:ANYMON_VERSION) {
        $tag = 'v' + $env:ANYMON_VERSION.TrimStart('v')
        $url = "https://github.com/$repo/releases/download/$tag/$archive"
        $label = $tag
    } else {
        $url = "https://github.com/$repo/releases/latest/download/$archive"
        $label = 'latest release'
    }

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('anymon-' + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $zip = Join-Path $tmp $archive
        Write-Step "downloading $archive ($label)"
        try {
            Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
        } catch {
            throw "anymon: download failed: $url`n  There may be no prebuilt binary for $target in this release; see $docs"
        }

        $sumFile = "$zip.sha256"
        $hasSum = $true
        try {
            Invoke-WebRequest -Uri "$url.sha256" -OutFile $sumFile -UseBasicParsing
        } catch {
            $hasSum = $false
        }
        if ($hasSum) {
            $expected = ((Get-Content -Path $sumFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
            $actual = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
            if ($expected -ne $actual) {
                throw "anymon: checksum mismatch for $archive (expected $expected, got $actual)"
            }
        } else {
            Write-Step 'no checksum published for this release; skipping verification'
        }

        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        $src = Join-Path $tmp "anymon-$target"
        if (-not (Test-Path (Join-Path $src 'anymon.exe'))) {
            throw 'anymon: the archive does not contain anymon.exe'
        }

        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        foreach ($name in @('anymon.exe', 'anymon-shell.exe')) {
            $from = Join-Path $src $name
            if (-not (Test-Path $from)) { continue }
            $to = Join-Path $installDir $name
            $old = "$to.old"
            Remove-Item -Path $old -Force -ErrorAction SilentlyContinue
            if (Test-Path $to) {
                # A running executable cannot be overwritten, but it can be renamed.
                Move-Item -Path $to -Destination $old -Force
            }
            Copy-Item -Path $from -Destination $to -Force
            Remove-Item -Path $old -Force -ErrorAction SilentlyContinue
        }
    } finally {
        Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    $exe = Join-Path $installDir 'anymon.exe'
    $version = 'anymon'
    try { $version = (& $exe --version) | Select-Object -First 1 } catch { }
    Write-Step "installed $version to $installDir"

    # Add the install directory to the user PATH. The registry is used
    # directly so that entries such as %USERPROFILE%\bin stay unexpanded.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $userPath = [string] $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $entries = @($userPath -split ';' | Where-Object { $_ })
        $present = @($entries | Where-Object { Test-SamePath $_ $installDir }).Count -gt 0
        if ($present) {
            Write-Step "$installDir is already in your PATH"
        } elseif ($env:ANYMON_NO_MODIFY_PATH -eq '1') {
            Write-Step "add $installDir to your PATH to use anymon"
        } else {
            $key.SetValue('Path', (($entries + $installDir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
            # Setting a variable through .NET broadcasts WM_SETTINGCHANGE, so
            # new terminals pick up the new PATH.
            [Environment]::SetEnvironmentVariable('ANYMON_INSTALLER', '1', 'User')
            [Environment]::SetEnvironmentVariable('ANYMON_INSTALLER', $null, 'User')
            Write-Step "added $installDir to your user PATH; open a new terminal to use anymon everywhere"
        }
    } finally {
        $key.Close()
    }

    if (-not @($env:Path -split ';' | Where-Object { $_ -and (Test-SamePath $_ $installDir) }).Count) {
        $env:Path = "$env:Path;$installDir"
    }
    Write-Step 'get started:  anymon init  (or: anymon -e rs -- cargo run)'
}
