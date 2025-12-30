#!/usr/bin/env node

const { platform, arch } = process;
const path = require('path');
const fs = require('fs');

const targets = {
  'linux-x64': '@anymon/x86_64-unknown-linux-gnu',
  'linux-arm64': '@anymon/aarch64-unknown-linux-gnu',
  'darwin-x64': '@anymon/x86_64-apple-darwin',
  'darwin-arm64': '@anymon/aarch64-apple-darwin',
  'win32-x64': '@anymon/x86_64-pc-windows-msvc',
  'win32-arm64': '@anymon/aarch64-pc-windows-msvc',
};

function getTarget() {
  let key = platform + '-' + arch;
  return targets[key];
}

const targetPkg = getTarget();
if (!targetPkg) {
  console.error(`Unsupported platform/arch: ${platform}/${arch}`);
  process.exit(1);
}

let binName = platform === 'win32' ? 'anymon.exe' : 'anymon';
let binPath = path.join(
  __dirname,
  '..',
  '..',
  targetPkg,
  'bin',
  binName
);

if (!fs.existsSync(binPath)) {
  console.error(`Binary not found for your platform: ${binPath}`);
  process.exit(1);
}

const { spawn } = require('child_process');
const child = spawn(binPath, process.argv.slice(2), { stdio: 'inherit' });
child.on('exit', code => process.exit(code));
