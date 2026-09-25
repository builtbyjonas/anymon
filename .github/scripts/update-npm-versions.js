#!/usr/bin/env node
// Keeps the npm packages in sync with the Rust workspace:
// - sets the version of every npm/*/package.json to the [workspace.package]
//   version in Cargo.toml,
// - lists every platform package as an optional dependency of `anymon`.
//
// Usage: node .github/scripts/update-npm-versions.js [--print]
//   --print  only print the workspace version

'use strict';

const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..', '..');
const npmDir = path.join(root, 'npm');

function workspaceVersion() {
  const toml = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
  const section = toml.split(/^\[workspace\.package\]\s*$/m)[1];
  const match = section && section.split(/^\[/m)[0].match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error('no version in [workspace.package] of Cargo.toml');
  return match[1];
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function writeJson(file, value) {
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
}

function main() {
  const version = workspaceVersion();
  if (process.argv.includes('--print')) {
    console.log(version);
    return;
  }

  const platforms = [];
  for (const dir of fs.readdirSync(npmDir).sort()) {
    const file = path.join(npmDir, dir, 'package.json');
    if (!fs.existsSync(file) || dir === 'anymon') continue;
    const pkg = readJson(file);
    pkg.version = version;
    writeJson(file, pkg);
    platforms.push(pkg.name);
  }

  const mainFile = path.join(npmDir, 'anymon', 'package.json');
  const main = readJson(mainFile);
  main.version = version;
  main.optionalDependencies = Object.fromEntries(platforms.map((name) => [name, version]));
  writeJson(mainFile, main);

  console.log(`npm packages set to ${version} (${platforms.length} platform packages)`);
}

main();
