#!/usr/bin/env bash
# Publish the npm packages: every platform package first, then `anymon`,
# which depends on them. Versions that already exist are skipped, so a failed
# release can simply be re-run.
#
# Environment: NODE_AUTH_TOKEN (npm token), DIST_TAG (default: latest)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tag="${DIST_TAG:-latest}"

publish() {
  local dir="$1" name version
  name="$(node -p "require('${dir}/package.json').name")"
  version="$(node -p "require('${dir}/package.json').version")"
  if [ -n "$(npm view "${name}@${version}" version 2>/dev/null || true)" ]; then
    echo "skipping ${name}@${version}: already published"
    return
  fi
  echo "publishing ${name}@${version} (tag: ${tag})"
  (cd "$dir" && npm publish --access public --tag "$tag")
}

for dir in "${root}"/npm/*/; do
  dir="${dir%/}"
  [ "$(basename "$dir")" = "anymon" ] && continue
  publish "$dir"
done
publish "${root}/npm/anymon"
