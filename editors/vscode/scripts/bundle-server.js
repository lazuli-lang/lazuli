// Copy the freshly-built lazuli release binary into editors/vscode/server/
// so `vsce package` ships it inside the .vsix. Run `cargo build --release
// -p lazuli_cli` first; this script is a thin copy + sanity check.
//
// Usage:
//   node scripts/bundle-server.js                       (default win32-x64)
//   node scripts/bundle-server.js --target linux-x64    (future)

const fs = require('fs');
const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..', '..', '..');
const RELEASE_DIR = path.join(REPO_ROOT, 'target', 'release');
const SERVER_DIR = path.resolve(__dirname, '..', 'server');

function exeNameForPlatform() {
  return process.platform === 'win32' ? 'lazuli.exe' : 'lazuli';
}

function main() {
  const exeName = exeNameForPlatform();
  const src = path.join(RELEASE_DIR, exeName);
  if (!fs.existsSync(src)) {
    console.error(`ERROR: ${src} not found. Build it first:`);
    console.error('  cargo build --release -p lazuli_cli');
    process.exit(1);
  }

  fs.mkdirSync(SERVER_DIR, { recursive: true });
  const dst = path.join(SERVER_DIR, exeName);
  fs.copyFileSync(src, dst);

  const stats = fs.statSync(dst);
  const sizeMb = (stats.size / 1024 / 1024).toFixed(1);
  console.log(`bundled ${dst} (${sizeMb} MB)`);
}

main();
