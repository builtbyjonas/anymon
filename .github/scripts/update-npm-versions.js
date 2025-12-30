// update-npm-versions.js
// Updates all npm package.json versions and the global anymon package.json optionalDependencies to match the top-level version.

const fs = require('fs');
const path = require('path');

const npmDir = path.join(__dirname, '..', '..', 'npm');
const globalPkgPath = path.join(npmDir, 'anymon', 'package.json');
const workspaceToml = path.join(__dirname, '..', '..', 'Cargo.toml');

function getWorkspaceVersion() {
  const toml = fs.readFileSync(workspaceToml, 'utf8');
  const match = toml.match(/version\s*=\s*"([^"]+)"/);
  return match ? match[1] : null;
}

function updatePackageJsonVersion(pkgPath, newVersion) {
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  pkg.version = newVersion;
  fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
}

function updateGlobalOptionalDeps(globalPkgPath, newVersion) {
  const pkg = JSON.parse(fs.readFileSync(globalPkgPath, 'utf8'));
  if (pkg.optionalDependencies) {
    for (const dep in pkg.optionalDependencies) {
      pkg.optionalDependencies[dep] = newVersion;
    }
  }
  pkg.version = newVersion;
  fs.writeFileSync(globalPkgPath, JSON.stringify(pkg, null, 2) + '\n');
}

function main() {
  const newVersion = getWorkspaceVersion();
  if (!newVersion) {
    console.error('Could not determine workspace version');
    process.exit(1);
  }
  // Update all npm/*/package.json
  fs.readdirSync(npmDir).forEach(dir => {
    const pkgPath = path.join(npmDir, dir, 'package.json');
    if (fs.existsSync(pkgPath)) {
      updatePackageJsonVersion(pkgPath, newVersion);
    }
  });
  // Update global anymon package optionalDependencies
  updateGlobalOptionalDeps(globalPkgPath, newVersion);
}

main();
