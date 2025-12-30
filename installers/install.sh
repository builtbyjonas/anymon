#!/bin/bash

# install.sh - Download and install the latest anymon release for your OS/arch

REPO="builtbyjonas/anymon"
API_URL="https://api.github.com/repos/$REPO/releases/latest"
INSTALL_DOC="https://github.com/builtbyjonas/anymon/blob/main/docs/installation.md#build-from-source"

set -euo pipefail

# Detect OS and ARCH
ios=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)

# Map arch to common names
case "$arch" in
    x86_64) arch="amd64" ;;
    aarch64) arch="arm64" ;;
    armv7l) arch="armv7" ;;
    *) arch="$arch" ;;
esac

# Determine per-user data directory (user-data/anymon)
if [ "$ios" = "linux" ]; then
    DATA_BASE="${XDG_DATA_HOME:-$HOME/.local/share}"
    INSTALL_DIR="$DATA_BASE/anymon"
elif [ "$ios" = "darwin" ]; then
    INSTALL_DIR="$HOME/Library/Application Support/anymon"
else
    INSTALL_DIR="$HOME/.local/share/anymon"
fi

mkdir -p "$INSTALL_DIR"

# Fetch latest release info
release_json=$(curl -sL "$API_URL")

# Find asset url
asset_url=$(echo "$release_json" | grep -oE '"browser_download_url":\s*"[^"]*' | cut -d '"' -f4 | grep "$ios" | grep "$arch" | head -n1)

if [ -z "$asset_url" ]; then
    echo "No prebuilt binary found for $ios/$arch."
    echo "Please build from source: $INSTALL_DOC"
    exit 1
fi

filename=$(basename "$asset_url")
outpath="$INSTALL_DIR/$filename"
echo "Downloading $filename to $outpath..."
curl -L "$asset_url" -o "$outpath"

# If archive, extract; otherwise ensure executable is placed in install dir
if [[ "$outpath" == *.zip ]]; then
    if command -v unzip >/dev/null 2>&1; then
        unzip -o "$outpath" -d "$INSTALL_DIR"
        rm -f "$outpath"
    else
        echo "Downloaded zip archive but 'unzip' is not available. Please extract $outpath into $INSTALL_DIR."
    fi
elif [[ "$outpath" == *.tar.gz || "$outpath" == *.tgz ]]; then
    tar -xzf "$outpath" -C "$INSTALL_DIR"
    rm -f "$outpath"
else
    chmod +x "$outpath"
    # rename raw binary to 'anymon' (or anymon.exe on Windows, but this script targets Unix-like shells)
    mv -f "$outpath" "$INSTALL_DIR/anymon"
fi

echo "Installed anymon to $INSTALL_DIR."

# Add the install directory to PATH if not already present
case ":$PATH:" in
    *":$INSTALL_DIR:") echo "$INSTALL_DIR is already in your PATH." ;;
    *)
        PROFILE=""
        if [ -n "${BASH_VERSION-}" ]; then
            PROFILE="$HOME/.bashrc"
        elif [ -n "${ZSH_VERSION-}" ]; then
            PROFILE="$HOME/.zshrc"
        else
            PROFILE="$HOME/.profile"
        fi
        echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$PROFILE"
        echo "Added $INSTALL_DIR to your PATH in $PROFILE. Restart your terminal or run:"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac
