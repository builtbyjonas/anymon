#!/usr/bin/env bash
# Put the release binaries from the packaged archives into the npm platform
# packages (npm/<target>/bin) and sync all npm versions with Cargo.toml.
#
# Usage: .github/scripts/prepare-npm.sh [dist-dir]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dist="$(cd "${1:-dist}" && pwd)"

node "${root}/.github/scripts/update-npm-versions.js"

for pkg in "${root}"/npm/*/; do
  pkg="${pkg%/}"
  target="$(basename "$pkg")"
  [ "$target" = "anymon" ] && continue

  case "$target" in
    *windows*) archive="${dist}/anymon-${target}.zip"; exe="anymon.exe" ;;
    *) archive="${dist}/anymon-${target}.tar.gz"; exe="anymon" ;;
  esac
  if [ ! -f "$archive" ]; then
    echo "error: missing ${archive} for the npm package of ${target}" >&2
    exit 1
  fi

  tmp="$(mktemp -d)"
  case "$archive" in
    *.zip) unzip -q "$archive" -d "$tmp" ;;
    *) tar -xzf "$archive" -C "$tmp" ;;
  esac
  rm -rf "${pkg}/bin"
  mkdir -p "${pkg}/bin"
  cp "${tmp}/anymon-${target}/${exe}" "${pkg}/bin/${exe}"
  chmod 755 "${pkg}/bin/${exe}"
  rm -rf "$tmp"
  echo "prepared ${target}"
done
