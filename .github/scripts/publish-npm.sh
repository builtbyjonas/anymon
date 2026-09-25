#!/usr/bin/env bash
# Publish the npm packages with trusted publishing (OIDC) and provenance:
# every platform package first, then `anymon`, which depends on them.
# Versions that already exist are skipped, so a failed release can simply be
# re-run.
#
# Runs in GitHub Actions with `id-token: write` and npm >= 11.5.1; no npm
# token is used. Every package must already exist on npm with this
# repository's release workflow configured as its trusted publisher (see
# .github/scripts/setup-npm-trust.sh).
#
# Environment: DIST_TAG (default: latest)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tag="${DIST_TAG:-latest}"

packages=()
for dir in "${root}"/npm/*/; do
  dir="${dir%/}"
  [ "$(basename "$dir")" = "anymon" ] && continue
  packages+=("$dir")
done
packages+=("${root}/npm/anymon")

field() {
  node -p "require('${1}/package.json').${2}"
}

# Trusted publishing only works for packages that already exist, so check all
# of them before publishing anything.
missing=()
for dir in "${packages[@]}"; do
  name="$(field "$dir" name)"
  if [ -z "$(npm view "$name" name 2>/dev/null || true)" ]; then
    missing+=("$name")
  fi
done
if [ "${#missing[@]}" -gt 0 ]; then
  for name in "${missing[@]}"; do
    echo "::error::${name} does not exist on npm yet. Trusted publishing can only be configured for existing packages; run .github/scripts/setup-npm-trust.sh once (see docs/development.md)."
  done
  exit 1
fi

for dir in "${packages[@]}"; do
  name="$(field "$dir" name)"
  version="$(field "$dir" version)"
  if [ -n "$(npm view "${name}@${version}" version 2>/dev/null || true)" ]; then
    echo "skipping ${name}@${version}: already published"
    continue
  fi
  echo "publishing ${name}@${version} (tag: ${tag})"
  (cd "$dir" && npm publish --access public --provenance --tag "$tag")
done
