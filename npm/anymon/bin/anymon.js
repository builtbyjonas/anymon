#!/usr/bin/env node
'use strict';

// Launches the native anymon binary that npm installed as an optional
// dependency for this platform.

const { spawn } = require('child_process');
const path = require('path');

const PACKAGES = {
  'darwin-arm64': ['@anymon/aarch64-apple-darwin'],
  'darwin-x64': ['@anymon/x86_64-apple-darwin'],
  'linux-arm64': ['@anymon/aarch64-unknown-linux-gnu', '@anymon/aarch64-unknown-linux-musl'],
  'linux-x64': ['@anymon/x86_64-unknown-linux-gnu', '@anymon/x86_64-unknown-linux-musl'],
  'win32-arm64': ['@anymon/aarch64-pc-windows-msvc', '@anymon/x86_64-pc-windows-msvc'],
  'win32-x64': ['@anymon/x86_64-pc-windows-msvc'],
};

function isMusl() {
  if (process.platform !== 'linux') return false;
  try {
    const report = process.report && process.report.getReport();
    const header = report && (typeof report === 'string' ? JSON.parse(report) : report).header;
    return !(header && header.glibcVersionRuntime);
  } catch {
    return false;
  }
}

function candidates() {
  const key = `${process.platform}-${process.arch}`;
  const packages = (PACKAGES[key] || []).slice();
  // Prefer the musl build on musl systems (Alpine) and the glibc build elsewhere.
  if (isMusl()) packages.sort((a, b) => b.endsWith('-musl') - a.endsWith('-musl'));
  return packages;
}

function findBinary() {
  const exe = process.platform === 'win32' ? 'anymon.exe' : 'anymon';
  for (const pkg of candidates()) {
    try {
      const manifest = require.resolve(`${pkg}/package.json`);
      return path.join(path.dirname(manifest), 'bin', exe);
    } catch {
      // Not installed (npm skips packages for other platforms); try the next.
    }
  }
  return null;
}

const binary = process.env.ANYMON_BINARY || findBinary();
if (!binary) {
  const expected = candidates();
  console.error(`anymon: no prebuilt binary is installed for ${process.platform}-${process.arch}.`);
  if (expected.length) {
    console.error(`Reinstall anymon, or install ${expected[0]} directly. If you use --no-optional or`);
    console.error('--omit=optional, allow optional dependencies for anymon.');
  } else {
    console.error('This platform has no prebuilt binary. Build from source:');
    console.error('https://github.com/builtbyjonas/anymon/blob/main/docs/installation.md');
  }
  process.exit(1);
}

const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit', windowsHide: false });

// anymon handles Ctrl-C itself and exits once its tasks have stopped, so the
// launcher only forwards signals and then mirrors the exit status.
const forward = (signal) => {
  if (child.exitCode === null && child.signalCode === null) child.kill(signal);
};
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    if (signal !== 'SIGINT') forward(signal);
  });
}

child.on('error', (err) => {
  console.error(`anymon: failed to start ${binary}: ${err.message}`);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code === null ? 1 : code);
});
