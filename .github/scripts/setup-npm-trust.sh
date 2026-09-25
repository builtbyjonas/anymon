#!/usr/bin/env bash
# One-time setup of npm trusted publishing (OIDC) for every anymon package.
#
# Run this locally as an npm maintainer of `anymon` and the `@anymon` scope,
# logged in with `npm login`, with two-factor authentication enabled and
# npm >= 11.15.0 (`npm install -g npm@latest`):
#
#   .github/scripts/setup-npm-trust.sh              # any environment
#   .github/scripts/setup-npm-trust.sh --env stable # only from `stable`
#
# For each package in npm/:
# 1. Packages that do not exist on npm yet get a placeholder `0.0.0` release,
#    because npm only allows configuring trusted publishing for existing
#    packages. The next release replaces it as `latest`.
# 2. The release workflow of builtbyjonas/anymon is added as the package's
#    trusted publisher.
#
# Restricting to an environment (--env) makes npm reject publishes from any
# other environment, e.g. pre-releases published from `next`.
set -euo pipefail

repo="builtbyjonas/anymon"
workflow="release.yml"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

env_args=()
if [ "${1:-}" = "--env" ]; then
  env_args=(--env "${2:?usage: setup-npm-trust.sh [--env <github-environment>]}")
fi

if ! user="$(npm whoami 2>/dev/null)"; then
  echo "error: not logged in to npm; run \`npm login\` first" >&2
  exit 1
fi
echo "npm user: ${user}, npm $(npm --version)"

failed=()
for dir in "${root}"/npm/*/; do
  dir="${dir%/}"
  name="$(node -p "require('${dir}/package.json').name")"

  if [ -z "$(npm view "$name" name 2>/dev/null || true)" ]; then
    echo "==> ${name}: creating placeholder 0.0.0"
    tmp="$(mktemp -d)"
    # The script is JavaScript; the backticks inside it are literal text.
    # shellcheck disable=SC2016
    node -e '
      const fs = require("fs");
      const [src, dest] = process.argv.slice(1);
      const pkg = JSON.parse(fs.readFileSync(src, "utf8"));
      const placeholder = {
        name: pkg.name,
        version: "0.0.0",
        description: "Placeholder release. Install the `anymon` package instead.",
        homepage: pkg.homepage,
        repository: pkg.repository,
        license: pkg.license,
      };
      fs.writeFileSync(dest + "/package.json", JSON.stringify(placeholder, null, 2) + "\n");
      fs.writeFileSync(dest + "/README.md", "# " + pkg.name + "\n\nPart of [anymon](https://github.com/builtbyjonas/anymon). Install `anymon` instead.\n");
    ' "${dir}/package.json" "$tmp"
    if ! (cd "$tmp" && npm publish --access public); then
      failed+=("${name} (placeholder)")
      rm -rf "$tmp"
      continue
    fi
    rm -rf "$tmp"
  fi

  echo "==> ${name}: trusting ${repo} ${workflow}"
  if ! npm trust github "$name" --file "$workflow" --repo "$repo" --allow-publish --yes ${env_args[@]+"${env_args[@]}"}; then
    failed+=("${name} (trust)")
  fi
done

if [ "${#failed[@]}" -gt 0 ]; then
  echo
  echo "Failed for: ${failed[*]}"
  echo "Configure these on npmjs.com instead: package -> Settings -> Trusted publisher -> GitHub Actions"
  echo "(organization: builtbyjonas, repository: anymon, workflow: ${workflow})."
  exit 1
fi
echo
echo "Done. The release workflow can now publish every package without a token."
