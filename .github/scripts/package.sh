#!/usr/bin/env bash
# Package the release binaries of one target into an archive plus a SHA-256
# checksum file:
#
#   dist/anymon-<target>.tar.gz          (Linux, macOS)
#   dist/anymon-<target>.zip             (Windows)
#   dist/anymon-<target>.<ext>.sha256
#
# The archive contains a single directory `anymon-<target>/` with the
# binaries, the README and the licenses.
#
# Usage: .github/scripts/package.sh <target> [out-dir]
set -euo pipefail

target="${1:?usage: package.sh <target> [out-dir]}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(mkdir -p "${2:-dist}" && cd "${2:-dist}" && pwd)"
name="anymon-${target}"
bin_dir="${root}/target/${target}/release"

ext=""
case "$target" in
  *windows*) ext=".exe" ;;
esac

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
mkdir -p "${staging}/${name}"
for bin in anymon anymon-shell; do
  cp "${bin_dir}/${bin}${ext}" "${staging}/${name}/"
done
cp "${root}/README.md" "${root}/MIT-LICENSE.md" "${root}/APACHE-LICENSE.md" "${staging}/${name}/"

cd "$staging"
case "$target" in
  *windows*)
    archive="${name}.zip"
    7z a -tzip "${out}/${archive}" "${name}" >/dev/null
    ;;
  *)
    archive="${name}.tar.gz"
    tar -czf "${out}/${archive}" "${name}"
    ;;
esac

cd "$out"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$archive" > "${archive}.sha256"
else
  shasum -a 256 "$archive" > "${archive}.sha256"
fi

echo "packaged ${out}/${archive}"
cat "${archive}.sha256"
