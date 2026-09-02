#!/usr/bin/env node
/**
 * One-command E2E runner:
 *   1. Build the app with `pnpm tauri build --debug --features webdriver`
 *      (skippable via --skip-build ONLY if that exact build already exists)
 *   2. Start the Tauri app binary (embedded webdriver plugin listens on 4445)
 *   3. Run WDIO tests (forwards extra args like --spec)
 *   4. Kill the app on exit
 *
 * Multi-instance mode (`--instances N`, N > 1):
 *   - Starts N app processes on consecutive ports (WD_PORT, WD_PORT+1, …)
 *   - Isolates app data in e2e/.app-data-0 … e2e/.app-data-(N-1)
 *   - Passes E2E_INSTANCES / E2E_WD_PORTS / E2E_DATA_DIRS to WDIO for parallel workers
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

function runEnvTeardown() {
  if (process.env.E2E_SKIP_TEARDOWN === '1') {
    log('Skipping DB teardown (E2E_SKIP_TEARDOWN=1)');
    return;
  }
  const script = path.join(__dirname, 'teardown-e2e-env.sh');
  if (!fs.existsSync(script)) return;
  log('Resetting E2E databases (e2e/teardown-e2e-env.sh)...');
  try {
    execSync(`bash "${script}"`, { stdio: 'inherit', cwd: ROOT, env: process.env });
  } catch {
    log('WARNING: e2e/teardown-e2e-env.sh failed; ephemeral DB objects may remain.');
  }
}

function resetAppDataDir(dir, keep) {
  fs.mkdirSync(dir, { recursive: true });
  if (keep) {
    log(`Keeping existing isolated app data: ${dir}`);
    return;
  }
  log(`Resetting isolated app data: ${dir}`);
  for (const entry of fs.readdirSync(dir)) {
    fs.rmSync(path.join(dir, entry), { recursive: true, force: true });
  }
}

const args = process.argv.slice(2);
const skipBuild = args.includes('--skip-build');
const screenshotTrace = args.includes('--screenshot');
const keepAppData = args.includes('--keep-app-data');
const portArg = args.find((a, i) => args[i - 1] === '--port');
const instancesArg = args.find((a, i) => args[i - 1] === '--instances');
const WD_PORT = portArg
  ? parseInt(portArg, 10)
  : parseInt(process.env.E2E_WD_PORT || '4445', 10);
const INSTANCE_COUNT = instancesArg ? parseInt(instancesArg, 10) : 1;
const minimalDrivers =
  process.env.DATAZEN_DRIVERS === 'basic' || args.includes('--minimal-drivers');
if (screenshotTrace) {
  process.env.E2E_SCREENSHOT = '1';
  fs.mkdirSync(path.join(__dirname, 'screenshots'), { recursive: true });
}
/** Inject drivers then build with webdriver + plugin Cargo features (see scripts/e2e-tauri-build.mjs). */
const BUILD_CMD = minimalDrivers
  ? 'node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs'
  : 'node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs -- node scripts/e2e-tauri-build.mjs';
const wdioArgs = [];
{
  const filtered = args.filter(
    (a, i) =>
      a !== '--skip-build' &&
      a !== '--minimal-drivers' &&
      a !== '--minimal-plugins' &&
      a !== '--screenshot' &&
      a !== '--keep-app-data' &&
      a !== '--port' &&
      args[i - 1] !== '--port' &&
      a !== '--instances' &&
      args[i - 1] !== '--instances' &&
      a !== '--',
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

function resolveDataDirs(count) {
  if (count <= 1) {
    return [path.join(ROOT, 'e2e', '.app-data')];
  }
  return Array.from({ length: count }, (_, i) =>
    path.join(ROOT, 'e2e', `.app-data-${i}`),
  );
}

function resolvePorts(basePort, count) {
  return Array.from({ length: count }, (_, i) => basePort + i);
}

function startAppInstance({ binaryPath, dataDir, port, onOutput }) {
  log(`Starting app instance on port ${port}: ${binaryPath}`);
  log(`App data isolation: DATAZEN_DATA_DIR=${dataDir}`);
  const proc = spawn(binaryPath, [], {
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: false,
    cwd: ROOT,
    env: {
      ...process.env,
      DATAZEN_DATA_DIR: dataDir,
      TAURI_WEBDRIVER_PORT: String(port),
      E2E_WD_PORT: String(port),
    },
  });
  proc.stdout.on('data', (d) => process.stdout.write(d));
  proc.stderr.on('data', onOutput);
  return proc;
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

if (!Number.isFinite(INSTANCE_COUNT) || INSTANCE_COUNT < 1) {
  die('--instances must be a positive integer');
}

runEnvSetup();

const appBinary = getAppBinaryPath();
assertBinaryReady(appBinary);

const dataDirs = resolveDataDirs(INSTANCE_COUNT);
const wdPorts = resolvePorts(WD_PORT, INSTANCE_COUNT);

for (const dir of dataDirs) {
  resetAppDataDir(dir, keepAppData);
}

if (INSTANCE_COUNT > 1) {
  log(
    `Multi-instance mode: ${INSTANCE_COUNT} app processes on ports ${wdPorts.join(', ')}`,
  );
} else {
  log(`Starting app: ${appBinary}`);
  log(`WebDriver port: ${WD_PORT} (TAURI_WEBDRIVER_PORT / E2E_WD_PORT)`);
  log(`App data isolation: DATAZEN_DATA_DIR=${dataDirs[0]}`);
}

let sawAssetMissing = false;
function onAppOutput(chunk) {
  const text = chunk.toString();
  process.stderr.write(chunk);
  if (/asset not found:\s*index\.html/i.test(text)) {
    sawAssetMissing = true;
  }
}

/** @type {import('node:child_process').ChildProcess[]} */
const appProcesses = wdPorts.map((port, i) =>
  startAppInstance({
    binaryPath: appBinary,
    dataDir: dataDirs[i],
    port,
    onOutput: onAppOutput,
  }),
);

function cleanup() {
  for (const proc of appProcesses) {
    if (!proc.killed) {
      try {
        proc.kill('SIGTERM');
      } catch {
        /* ignore */
      }
    }
  }
  if (appProcesses.some((p) => !p.killed)) {
    log('Stopping app process(es)...');
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
  await Promise.all(wdPorts.map((port) => waitForPort(port)));
  if (INSTANCE_COUNT > 1) {
    log(`All ${INSTANCE_COUNT} WebDriver plugin(s) ready on ports ${wdPorts.join(', ')}.`);
  } else {
    log(`WebDriver plugin is ready on port ${WD_PORT}.`);
  }
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
      `WebDriver port(s) did not open: ${wdPorts.join(', ')}`,
      'Common causes:',
      '  1. Binary built WITHOUT --features webdriver',
      '  2. Used cargo build instead of `pnpm tauri build --debug --features webdriver`',
      `  3. Another process already holds one of ${wdPorts.join(', ')}`,
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
if (screenshotTrace) {
  log('Screenshot trace enabled (E2E_SCREENSHOT=1) → e2e/screenshots/<spec>/');
}
log('Running E2E tests...');

if (INSTANCE_COUNT > 1) {
  // Multi-instance: launch N independent WDIO processes, each with its own port/dataDir
  // and a round-robin slice of the spec files. This avoids WDIO's capability duplication.
  // Resolve spec files to split across workers.
  // If --suite is given, we can't easily parse wdio.conf.ts suites here,
  // so we fall back to passing --suite to a single WDIO process (no split).
  const hasSuiteArg = wdioArgs.includes('--suite');
  let allSpecs = [];
  let useSpecSplit = true;

  if (hasSuiteArg) {
    // Suite mode: can't split specs, run single WDIO with all instances sharing work
    // is impossible — warn and fall back to single instance behavior
    log(
      'Warning: --suite with --instances is not supported (cannot split suite specs). ' +
        'Running with instance 0 only. Use explicit --spec list for true parallelism.',
    );
    useSpecSplit = false;
  } else {
    const specIdx = wdioArgs.indexOf('--spec');
    if (specIdx >= 0 && wdioArgs[specIdx + 1]) {
      allSpecs = wdioArgs[specIdx + 1].split(',').map((s) => s.trim());
    } else {
      // Recursively find all .ts spec files
      const specsDir = path.join(ROOT, 'e2e', 'specs');
      const walk = (dir) => {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        const files = [];
        for (const entry of entries) {
          const full = path.join(dir, entry.name);
          if (entry.isDirectory()) {
            files.push(...walk(full));
          } else if (entry.name.endsWith('.ts')) {
            files.push(path.relative(ROOT, full));
          }
        }
        return files;
      };
      allSpecs = walk(specsDir)
        .filter(
          (f) =>
            !f.includes('zz-screenshots') &&
            !f.includes('demo-recording') &&
            !f.includes('zz-diag'),
        )
        .sort();
    }
  }

  const effectiveInstances = useSpecSplit ? INSTANCE_COUNT : 1;
  const chunks = Array.from({ length: effectiveInstances }, () => []);
  allSpecs.forEach((spec, i) => chunks[i % effectiveInstances].push(spec));

  if (useSpecSplit) {
    log(
      `Splitting ${allSpecs.length} specs across ${effectiveInstances} processes: ` +
        `${chunks.map((c) => c.length).join(' / ')}`,
    );
  }

  const wdioProcesses = chunks.map((specChunk, i) => {
    if (specChunk.length === 0) return null;
    const env = {
      ...process.env,
      DATAZEN_DATA_DIR: dataDirs[i],
      E2E_WD_PORT: String(wdPorts[i]),
    };
    const specArgs = [];
    if (useSpecSplit) {
      for (const s of specChunk) {
        specArgs.push('--spec', s);
      }
      // Remove --spec from original wdioArgs since we're providing our own
      var filteredWdioArgs = wdioArgs.filter(
        (a, idx) => a !== '--spec' && wdioArgs[idx - 1] !== '--spec',
      );
    } else {
      // Pass original args as-is (suite mode)
      var filteredWdioArgs = [...wdioArgs];
    }
    const proc = spawn(
      'npx',
      ['wdio', 'run', 'e2e/wdio.conf.ts', ...filteredWdioArgs, ...specArgs],
      {
        stdio: ['inherit', 'pipe', 'pipe'],
        cwd: ROOT,
        env,
      },
    );
    const prefix = `\x1b[3${2 + i}m[worker-${i}]\x1b[0m `;
    proc.stdout.on('data', (chunk) => {
      for (const line of chunk.toString().split('\n')) {
        if (line) process.stdout.write(prefix + line + '\n');
      }
    });
    proc.stderr.on('data', (chunk) => {
      for (const line of chunk.toString().split('\n')) {
        if (line) process.stderr.write(prefix + line + '\n');
      }
    });
    return proc;
  });

  const exitCodes = await Promise.all(
    wdioProcesses
      .filter(Boolean)
      .map((proc) => new Promise((resolve) => proc.on('close', (code) => resolve(code ?? 1)))),
  );

  cleanup();
  runEnvTeardown();
  const maxCode = Math.max(...exitCodes);
  const passed = exitCodes.filter((c) => c === 0).length;
  log(
    `${passed}/${exitCodes.length} workers passed. ` +
      (maxCode === 0 ? 'All tests passed!' : `Some workers failed (codes: ${exitCodes.join(', ')})`),
  );
  process.exit(maxCode);
} else {
  // Single-instance: standard WDIO run
  const wdioEnv = {
    ...process.env,
    DATAZEN_DATA_DIR: dataDirs[0],
    E2E_WD_PORT: String(WD_PORT),
  };

  const wdio = spawn(
    'npx',
    ['wdio', 'run', 'e2e/wdio.conf.ts', ...wdioArgs],
    {
      stdio: 'inherit',
      cwd: ROOT,
      env: wdioEnv,
    },
  );

  const exitCode = await new Promise((resolve) => {
    wdio.on('close', (code) => resolve(code ?? 1));
  });

  cleanup();
  runEnvTeardown();
  log(exitCode === 0 ? 'All tests passed!' : `Tests failed (exit code ${exitCode})`);
  process.exit(exitCode);
}
