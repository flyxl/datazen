#!/usr/bin/env node
/**
 * One-command E2E runner:
 *   1. Build the app with `pnpm tauri build --debug --features webdriver`
 *      (skippable via --skip-build ONLY if that exact build already exists)
 *   2. Start the Tauri app binary (embedded webdriver plugin listens on 4445)
 *   3. Run WDIO tests (forwards extra args like --spec)
 *   4. Kill the app on exit
 *
 * NEVER use bare `cargo build --features webdriver` for E2E — it often produces
 * a binary that fails at runtime with: asset not found: index.html
 * because the Tauri CLI (beforeBuildCommand + asset embedding) was skipped.
 *
 * See docs/development/e2e-testing.md for the full agent playbook.
 */
import { spawn, execSync } from 'node:child_process';
import { createConnection } from 'node:net';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const DIST_INDEX = path.join(ROOT, 'dist', 'index.html');

/** Load e2e/.env into process.env without overriding existing vars. */
function loadDotEnv(filePath) {
  if (!fs.existsSync(filePath)) return;
  for (const raw of fs.readFileSync(filePath, 'utf8').split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const eq = line.indexOf('=');
    if (eq <= 0) continue;
    const key = line.slice(0, eq).trim();
    let val = line.slice(eq + 1).trim();
    if (
      (val.startsWith('"') && val.endsWith('"')) ||
      (val.startsWith("'") && val.endsWith("'"))
    ) {
      val = val.slice(1, -1);
    }
    if (process.env[key] === undefined) process.env[key] = val;
  }
}
loadDotEnv(path.join(__dirname, '.env'));

function runEnvSetup() {
  const script = path.join(__dirname, 'setup-e2e-env.sh');
  if (!fs.existsSync(script)) return;
  log('Preparing E2E databases (e2e/setup-e2e-env.sh)...');
  try {
    execSync(`bash "${script}"`, { stdio: 'inherit', cwd: ROOT, env: process.env });
  } catch {
    log(
      'WARNING: e2e/setup-e2e-env.sh failed. DB specs may fail; UI-only specs can still run.',
    );
  }
}

const args = process.argv.slice(2);
const skipBuild = args.includes('--skip-build');
const minimalDrivers =
  process.env.DATAZEN_DRIVERS === 'basic' || args.includes('--minimal-drivers');
/** Inject drivers then build with webdriver + plugin Cargo features (see scripts/e2e-tauri-build.mjs). */
const BUILD_CMD = minimalDrivers
  ? 'node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs'
  : 'node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs -- node scripts/e2e-tauri-build.mjs';
const wdioArgs = [];
{
  const filtered = args.filter(
    (a) => a !== '--skip-build' && a !== '--minimal-drivers' && a !== '--minimal-plugins' && a !== '--',
  );
  for (let i = 0; i < filtered.length; i++) {
    if (filtered[i] === '--spec' && filtered[i + 1]) {
      for (const s of filtered[i + 1].split(',')) {
        wdioArgs.push('--spec', s.trim());
      }
      i++;
    } else {
      wdioArgs.push(filtered[i]);
    }
  }
}

function log(msg) {
  console.log(`\x1b[36m[e2e-runner]\x1b[0m ${msg}`);
}

function die(msg) {
  console.error(`\x1b[31m[e2e-runner]\x1b[0m ${msg}`);
  process.exit(1);
}

function waitForPort(port, host = '127.0.0.1', timeoutMs = 15000) {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const tryConnect = () => {
      const sock = createConnection({ port, host }, () => {
        sock.destroy();
        resolve();
      });
      sock.on('error', () => {
        if (Date.now() - start > timeoutMs) {
          reject(new Error(`Port ${port} not ready after ${timeoutMs}ms`));
        } else {
          setTimeout(tryConnect, 300);
        }
      });
    };
    tryConnect();
  });
}

function getAppBinaryPath() {
  if (process.platform === 'win32') {
    return path.join(ROOT, 'target/debug/datazen.exe');
  }
  // macOS: always prefer the Tauri debug .app when present.
  // Bare `cargo build` overwrites target/debug/datazen and often breaks
  // embedded frontend assets ("asset not found: index.html"), while the
  // .app from `pnpm tauri build --debug` stays intact.
  const appBundleBin = path.join(
    ROOT,
    'target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen',
  );
  if (process.platform === 'darwin' && fs.existsSync(appBundleBin)) {
    return appBundleBin;
  }
  return path.join(ROOT, 'target/debug/datazen');
}

function assertFrontendDistPresent() {
  if (!fs.existsSync(DIST_INDEX)) {
    die(
      [
        'Missing dist/index.html — frontend assets were never built.',
        `Fix: run \`${BUILD_CMD}\` (do not use bare cargo build).`,
        'See docs/development/e2e-testing.md',
      ].join('\n'),
    );
  }
}

function assertBinaryReady(binaryPath) {
  if (!fs.existsSync(binaryPath)) {
    die(
      [
        `E2E binary not found: ${binaryPath}`,
        `Fix: run \`${BUILD_CMD}\` then re-run, or omit --skip-build.`,
        'See docs/development/e2e-testing.md',
      ].join('\n'),
    );
  }

  assertFrontendDistPresent();

  const binM = fs.statSync(binaryPath).mtimeMs;
  const distM = fs.statSync(DIST_INDEX).mtimeMs;
  if (binM + 1000 < distM) {
    log(
      'WARNING: binary is older than dist/index.html. Embedded assets may be stale.',
    );
    log(`Rebuild with: ${BUILD_CMD}`);
  }
}

// Step 1: Build
if (!skipBuild) {
  log(`Building app with webdriver feature via Tauri CLI...`);
  if (minimalDrivers) {
    log('Using basic driver set (DATAZEN_DRIVERS=basic / --minimal-drivers).');
  }
  log(`Command: ${BUILD_CMD}`);
  try {
    execSync(BUILD_CMD, {
      stdio: 'inherit',
      cwd: ROOT,
    });
  } catch {
    process.exit(1);
  }
} else {
  log('Skipping build (--skip-build). Binary MUST come from a prior Tauri webdriver build.');
}

runEnvSetup();

const appBinary = getAppBinaryPath();
assertBinaryReady(appBinary);

// Step 2: Start the Tauri app (webdriver plugin on port 4445)
// Isolate app data: the webdriver binary would otherwise read/write the real
// production directory (~/Library/Application Support/com.tbeasy.datazen) and
// connection-wiping specs would destroy real user data. Requires a binary built
// with DATAZEN_DATA_DIR support in Store::default_app_data_dir / Store::init.
const isolatedDataDir = path.join(ROOT, 'e2e', '.app-data');
log(`Starting app: ${appBinary}`);
log(`App data isolation: DATAZEN_DATA_DIR=${isolatedDataDir}`);
let sawAssetMissing = false;
const app = spawn(appBinary, [], {
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: false,
  cwd: ROOT,
  env: { ...process.env, DATAZEN_DATA_DIR: isolatedDataDir },
});

function onAppOutput(chunk) {
  const text = chunk.toString();
  process.stderr.write(chunk);
  if (/asset not found:\s*index\.html/i.test(text)) {
    sawAssetMissing = true;
  }
}

app.stdout.on('data', (d) => process.stdout.write(d));
app.stderr.on('data', onAppOutput);

function cleanup() {
  if (!app.killed) {
    log('Stopping app...');
    try {
      app.kill('SIGTERM');
    } catch {
      /* ignore */
    }
  }
}
process.on('exit', cleanup);
process.on('SIGINT', () => {
  cleanup();
  process.exit(130);
});
process.on('SIGTERM', () => {
  cleanup();
  process.exit(143);
});

try {
  await waitForPort(4445);
  log('WebDriver plugin is ready on port 4445.');
} catch (err) {
  console.error(err.message);
  if (sawAssetMissing) {
    die(
      [
        'App failed with "asset not found: index.html".',
        'Cause: binary was likely built with bare `cargo build`, which skips Tauri asset embedding.',
        `Fix: ${BUILD_CMD}`,
        'Then: pnpm e2e:skip-build -- --spec <your-spec>',
        'See docs/development/e2e-testing.md',
      ].join('\n'),
    );
  }
  die(
    [
      'WebDriver port 4445 did not open.',
      'Common causes:',
      '  1. Binary built WITHOUT --features webdriver',
      '  2. Used cargo build instead of `pnpm tauri build --debug --features webdriver`',
      '  3. Another process already holds 4445',
      'See docs/development/e2e-testing.md',
    ].join('\n'),
  );
}

if (sawAssetMissing) {
  cleanup();
  die(
    [
      'App reported "asset not found: index.html" — frontend is broken.',
      `Rebuild with: ${BUILD_CMD}`,
      'Do NOT use: cargo build -p datazen --features webdriver',
      'See docs/development/e2e-testing.md',
    ].join('\n'),
  );
}

// Step 3: Run WDIO
log('Running E2E tests...');
const wdio = spawn(
  'npx',
  ['wdio', 'run', 'e2e/wdio.conf.ts', ...wdioArgs],
  { stdio: 'inherit', cwd: ROOT },
);

const exitCode = await new Promise((resolve) => {
  wdio.on('close', (code) => resolve(code ?? 1));
});

// Step 4: Cleanup
cleanup();
log(exitCode === 0 ? 'All tests passed!' : `Tests failed (exit code ${exitCode})`);
process.exit(exitCode);
