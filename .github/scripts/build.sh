#!/usr/bin/env bash
# Build the release binaries for one target into target/<target>/release/.
#
# Linux targets are cross-compiled with cargo-zigbuild (https://github.com/rust-cross/cargo-zigbuild):
# - *-linux-musl builds are fully static and run on any Linux distribution.
# - *-linux-gnu builds link against glibc 2.17, so they also run on old
#   distributions such as CentOS 7 or Debian 8.
#
# Usage: .github/scripts/build.sh <target>
set -euo pipefail

target="${1:?usage: build.sh <target>}"
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

case "$target" in
  *-linux-gnu | *-linux-musl)
    if ! command -v cargo-zigbuild >/dev/null 2>&1; then
      echo "cargo-zigbuild is required to build $target (cargo install cargo-zigbuild)" >&2
      exit 1
    fi
    zig_target="$target"
    if [[ "$target" == *-linux-gnu ]]; then
      zig_target="${target}.2.17"
    fi
    cargo zigbuild --release --locked --target "$zig_target"
    ;;
  *)
    cargo build --release --locked --target "$target"
    ;;
esac

ls -l "target/${target}/release/"anymon*
