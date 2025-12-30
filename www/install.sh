#!/bin/bash

# install.sh - Download and install the latest anymon release for your OS/arch

REPO="builtbyjonas/anymon"
API_URL="https://api.github.com/repos/$REPO/releases/latest"
INSTALL_DOC="https://github.com/builtbyjonas/anymon/blob/main/docs/installation.md#build-from-source"

set -e

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

# Fetch latest release info
release_json=$(curl -sL "$API_URL")

# Find asset name
asset_url=$(echo "$release_json" | grep -oE '"browser_download_url":\s*"[^"]*' | cut -d '"' -f4 | grep "$ios" | grep "$arch" | head -n1)

if [ -z "$asset_url" ]; then
    echo "No prebuilt binary found for $ios/$arch."
    echo "Please build from source: $INSTALL_DOC"
    exit 1
fi

filename=$(basename "$asset_url")
echo "Downloading $filename..."
curl -L "$asset_url" -o "$filename"

chmod +x "$filename"
echo "Downloaded and made $filename executable."

# Add the current directory to PATH if not already present
CUR_DIR=$(pwd)
case ":$PATH:" in
    *":$CUR_DIR:"*)
        echo "$CUR_DIR is already in your PATH." ;;
    *)
        # Try to add to shell profile
        PROFILE=""
        if [ -n "$BASH_VERSION" ]; then
            PROFILE="$HOME/.bashrc"
        elif [ -n "$ZSH_VERSION" ]; then
            PROFILE="$HOME/.zshrc"
        else
            PROFILE="$HOME/.profile"
        fi
        echo "export PATH=\"$CUR_DIR:\$PATH\"" >> "$PROFILE"
        echo "Added $CUR_DIR to your PATH in $PROFILE. Restart your terminal or run:"
        echo "  export PATH=\"$CUR_DIR:\$PATH\"" ;;
esac
