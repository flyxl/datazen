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
import { spawn, execSync, execFileSync } from 'node:child_process';
import { createConnection } from 'node:net';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import os from 'node:os';

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
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
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
    log('WARNING: e2e/setup-e2e-env.sh failed. DB specs may fail; UI-only specs can still run.');
  }
}

/**
 * 清空共享夹具库（`datazen_sync_src` / `datazen_sync_tgt` / `..._mysql_*`）。
 *
 * 这些库被 data-sync / data-transfer / schema-diff 三个家族共用，此前只在整套
 * 开始与结束时各清一次：于是「某个 spec 建的表还在不在」取决于它前面跑过谁，
 * 同一个 spec 单跑绿、整包红，红点还会漂到别的 spec 头上。隔离模式在每个 spec
 * 起应用**之前**重置，让每个 spec 看到同一个起点。
 *
 * 失败不判红：重置是夹具卫生，清理失败会以 WARNING 汇总到整包结论里。
 */
const sharedFixtureResetFailures = [];

function resetSharedFixtureDbs(spec) {
  if (process.env.E2E_SKIP_SHARED_FIXTURE_RESET === '1') return;
  const script = path.join(__dirname, 'reset-sync-dbs.sh');
  if (!fs.existsSync(script)) return;
  try {
    execFileSync('bash', [script], {
      stdio: 'pipe',
      cwd: ROOT,
      env: { ...process.env, E2E_RESET_SYNC_QUIET: '1' },
    });
  } catch (error) {
    sharedFixtureResetFailures.push(spec);
    log(
      `WARNING: shared fixture reset failed before ${spec}: ${String(error.message || error)
        .split('\n')[0]
        .slice(0, 200)}`,
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
// 截图 / 录屏链路需要窗口处在系统正常的前台与合成状态（合成器才会把窗口内容
// 交给 WebDriver 的 saveScreenshot 与系统录屏），静默模式会破坏这类运行，
// 因此 e2e:shots / e2e:demo / --capture / --screenshot 默认关闭静默。
// 判定看的是参数本身，所以任何调用方式都覆盖得到。
const isCaptureRun = () =>
  args.includes('--capture') ||
  screenshotTrace ||
  args.some((a) => /demo-recording|screenshot/i.test(a));
const keepAppData = args.includes('--keep-app-data');

const portArg = args.find((a, i) => args[i - 1] === '--port');
const instancesArg = args.find((a, i) => args[i - 1] === '--instances');
const requestedInstances = instancesArg ? parseInt(instancesArg, 10) : undefined;
const WD_PORT = portArg ? parseInt(portArg, 10) : parseInt(process.env.E2E_WD_PORT || '4445', 10);

const hasExplicitSpecArg = args.some((a, i) => a === '--spec' || args[i - 1] === '--spec');
const hasExplicitSuiteArg = args.some((a, i) => a === '--suite' || args[i - 1] === '--suite');
const isFullRun =
  !hasExplicitSpecArg &&
  !hasExplicitSuiteArg &&
  !args.includes('--attach') &&
  !args.includes('--hold');

function defaultFullRunInstances() {
  const cpu = typeof os.availableParallelism === 'function' ? os.availableParallelism() : os.cpus().length;
  // Full runs default to parallel to reduce wall time, but never exceed 4 to
  // avoid CPU/memory pressure destabilizing the Tauri/WebDriver/DB stack.
  return Math.min(4, Math.max(2, Math.floor(cpu / 2) || 2));
}

let INSTANCE_COUNT;
if (isFullRun) {
  const base = Number.isFinite(requestedInstances) ? requestedInstances : defaultFullRunInstances();
  INSTANCE_COUNT = Math.max(1, Math.min(4, base));
  if (Number.isFinite(requestedInstances) && requestedInstances > 4) {
    console.warn('\x1b[33m[e2e-runner]\x1b[0m Full E2E run caps concurrency at 4 instances; using 4.');
  }
} else {
  INSTANCE_COUNT = Number.isFinite(requestedInstances) ? requestedInstances : 1;
  if (Number.isFinite(requestedInstances) && requestedInstances > 4) {
    console.warn('\x1b[33m[e2e-runner]\x1b[0m Requested instances > 4; continuing as requested for targeted runs.');
  }
}

/**
 * 每个 spec 文件独占一次应用进程（见下方 isolated 分支）。整包串跑时 spec 之间会
 * 共享运行时会话、连接池和 config_revision：前一个 spec 留下的会话会让后一个
 * spec 的 `connect` 撞上连接额度（"connection runtime operation failed"），
 * 串扰会掩盖真实的 UI 回归，所以整包验证默认走隔离模式。
 *
 * Full runs (no --spec/--suite) now default to phased parallel execution
 * (safe specs in parallel, shared-fixture suites serial) unless explicitly
 * forced back to serial isolate mode.
 */
const forceSerialIsolated = args.includes('--serial') || args.includes('--isolate-specs');
const forceNoSerialIsolated = args.includes('--no-isolate-specs') || args.includes('--parallel-full');
const isolateSpecs = forceSerialIsolated
  ? true
  : forceNoSerialIsolated
    ? false
    : process.env.E2E_ISOLATE_SPECS === '1'
      ? true
      : (!args.includes('--spec') && !args.includes('--suite') && !(isFullRun && INSTANCE_COUNT > 1));
const minimalDrivers =
  process.env.DATAZEN_DRIVERS === 'basic' || args.includes('--minimal-drivers');
const isPro =
  args.includes('--pro') || args.includes('--edition=pro') || process.env.DATAZEN_EDITION === 'pro';
if (isPro) {
  process.env.DATAZEN_EDITION = 'pro';
}
const runsProQueryBuilderSuite = args.some(
  (arg, index) => arg === '--suite' && args[index + 1] === 'pro-query-builder',
);
// The release-gallery capture drives the Builder through the same
// `window.__qbTest` bridge as the journeys, so it needs VITE_E2E just as much.
const runsProScreenshotSuite = args.some(
  (arg, index) => arg === '--suite' && args[index + 1] === 'pro-screenshots',
);
if (isPro && (runsProQueryBuilderSuite || runsProScreenshotSuite)) {
  // resolve-pro packs the extension before e2e-tauri-build sets its own env;
  // pass the test-only bridge flag into that earlier packaging step.
  process.env.VITE_E2E = '1';
}

/**
 * `--window-size=fit16x9` (or `fullscreen` / `=<w>x<h>`) pins the main window for the whole
 * run, so every `saveScreenshot` lands on one size instead of whatever geometry
 * the host monitor reports. Only screenshot runs want this — behaviour specs must
 * keep the shipped default (1280x900) or their layout assertions stop meaning
 * anything. wdio.conf.ts reads E2E_WINDOW_SIZE; `--window-size` is consumed here
 * and never forwarded to WDIO.
 *
 * `fit16x9` is the right value for the release gallery: the window is sized to the
 * largest 16:9 viewport the display can show, so the whole app is always in
 * frame and the capture needs no cropping. Resizing to 1920x1080 for the video is
 * a pure pixel step, done afterwards by scripts/normalize-screenshots.mjs.
 */
/**
 * `--attach` skips launching the app and drives an instance that is already
 * running on `--port`, so a hand-sized window survives into the capture.
 * See the guard further down where the app processes are started.
 */
const windowSizeArg = args.find((a) => a.startsWith('--window-size='));
if (windowSizeArg) {
  const value = windowSizeArg.slice('--window-size='.length);
  if (value !== 'fullscreen' && value !== 'fit16x9' && !/^\d+\s*[x×]\s*\d+$/.test(value)) {
    console.error(`[e2e] --window-size must be fullscreen, fit16x9 or <w>x<h>, got "${value}"`);
    process.exit(1);
  }
  process.env.E2E_WINDOW_SIZE = value;
  log(`Window pinned to ${value} for this run (screenshots only)`);
}

/**
 * `--capture` re-admits the specs wdio.conf.ts normally excludes
 * (zz-screenshots / demo-recording / zz-diag). They are excluded by default
 * because they are slow, need seeded demo data and rewrite files in
 * e2e/screenshots/site/. It pairs with --window-size: the whole point of a
 * capture run is a uniform set, which needs both.
 */
if (args.includes('--capture')) {
  process.env.E2E_CAPTURE = '1';
}
if (screenshotTrace) {
  process.env.E2E_SCREENSHOT = '1';
  fs.mkdirSync(path.join(__dirname, 'screenshots'), { recursive: true });
}
const proFlag = isPro ? '--edition=pro ' : '';
const e2eProArg = isPro ? ' --edition=pro' : '';
/** Inject drivers then build with webdriver + plugin Cargo features (see scripts/e2e-tauri-build.mjs). */
const BUILD_CMD = minimalDrivers
  ? `node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs ${proFlag}--drivers=basic -- node scripts/e2e-tauri-build.mjs${e2eProArg}`
  : `node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs ${proFlag}-- node scripts/e2e-tauri-build.mjs${e2eProArg}`;
const wdioArgs = [];
{
  const filtered = args.filter(
    (a, i) =>
      a !== '--skip-build' &&
      a !== '--minimal-drivers' &&
      a !== '--minimal-plugins' &&
      a !== '--screenshot' &&
      a !== '--capture' &&
      a !== '--keep-app-data' &&
      a !== '--isolate-specs' &&
      a !== '--no-isolate-specs' &&
      !a.startsWith('--spec-timeout=') &&
      a !== '--attach' &&
      a !== '--hold' &&
      a !== '--pro' &&
      a !== '--edition=pro' &&
      !a.startsWith('--edition=') &&
      a !== '--port' &&
      args[i - 1] !== '--port' &&
      a !== '--instances' &&
      args[i - 1] !== '--instances' &&
      !a.startsWith('--window-size=') &&
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

/**
 * The executable inside the bundle is named after the binary target
 * (`src-tauri/Cargo.toml`), not after the product. The DDD refactor renamed the
 * crate to `datazen_desktop`, so resolve both names instead of hard-coding the
 * old one and silently falling back to a stale `target/debug/datazen` left over
 * from an earlier checkout.
 */
const APP_BINARY_NAMES = ['datazen_desktop', 'datazen'];

function getAppBinaryPath() {
  if (process.platform === 'win32') {
    return path.join(ROOT, 'target/debug', `${APP_BINARY_NAMES[0]}.exe`);
  }
  // macOS: always prefer the Tauri debug .app when present.
  // Bare `cargo build` overwrites the unbundled binary and often breaks
  // embedded frontend assets ("asset not found: index.html"), while the
  // .app from `pnpm tauri build --debug` stays intact.
  const bundles = ['DataZen.app', 'DataZen Pro.app'];
  if (process.platform === 'darwin') {
    for (const bundle of bundles) {
      for (const name of APP_BINARY_NAMES) {
        const candidate = path.join(
          ROOT,
          `target/debug/bundle/macos/${bundle}/Contents/MacOS/${name}`,
        );
        if (fs.existsSync(candidate)) return candidate;
      }
    }
  }
  for (const name of APP_BINARY_NAMES) {
    const candidate = path.join(ROOT, 'target/debug', name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return path.join(ROOT, 'target/debug', APP_BINARY_NAMES[0]);
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
    log('WARNING: binary is older than dist/index.html. Embedded assets may be stale.');
    log(`Rebuild with: ${BUILD_CMD}`);
  }
}

/**
 * The app-data directory each instance owns.
 *
 * The port is part of the name whenever the run is not on the default port:
 * two runs on different ports are two runs, and sharing `.app-data` between
 * them would have one wipe the other's settings mid-spec. `--instances` keeps
 * the older `.app-data-<i>` names so existing scratch dirs stay where they are.
 */
function resolveDataDirs(count) {
  const suffix = WD_PORT === 4445 ? '' : `-port${WD_PORT}`;
  if (count <= 1) {
    return [path.join(ROOT, 'e2e', `.app-data${suffix}`)];
  }
  return Array.from(
    { length: count },
    (_, i) => path.join(ROOT, 'e2e', `.app-data${suffix}-${i}`),
  );
}

function resolvePorts(basePort, count) {
  return Array.from({ length: count }, (_, i) => basePort + i);
}

function startAppInstance({ binaryPath, dataDir, port, workerIndex, onOutput }) {
  log(`Starting app instance on port ${port}: ${binaryPath}`);
  log(`App data isolation: DATAZEN_DATA_DIR=${dataDir}`);
  const workerSchema = `e2e_worker_${workerIndex ?? 0}`;
  // 静默模式（macOS）：默认开启，避免 E2E 抢走开发者正在用的键盘焦点。
  // 截图 / 录屏链路需要窗口处于系统正常的前台与合成状态，默认关闭；
  // 任何情况下都可用 DATAZEN_E2E_QUIET=0/1 显式覆盖。
  const quiet = process.env.DATAZEN_E2E_QUIET ?? (isCaptureRun() ? '0' : '1');
  log(`macOS quiet mode: DATAZEN_E2E_QUIET=${quiet}`);
  const proc = spawn(binaryPath, [], {
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: false,
    cwd: ROOT,
    env: {
      ...process.env,
      DATAZEN_DATA_DIR: dataDir,
      // 每次 E2E 都是全新的 data dir，本地没有 `.key` 兜底；走系统钥匙串会在
      // macOS 上弹出「找不到钥匙串 login / 还原为默认」模态框并阻塞 security
      // CLI，钥匙串搜索列表一旦异常（Module Directory Service error）必弹。
      // 主密钥改落临时目录里的 `.key`，E2E 也不该碰用户真实钥匙串。
      DATAZEN_KEYRING: process.env.DATAZEN_KEYRING ?? 'file',
      DATAZEN_E2E_QUIET: quiet,
      TAURI_WEBDRIVER_PORT: String(port),
      E2E_WD_PORT: String(port),
      E2E_WORKER_INDEX: String(workerIndex ?? 0),
      E2E_WORKER_SCHEMA: workerSchema,
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

/**
 * `--attach` drives an app instance someone else launched and sized by hand.
 *
 * Capture runs need a window geometry that is stable for the whole run, and the
 * only reliable way to get one is to let a human set it once on a real display.
 * That conflicts with the normal flow, where this script starts (and later kills)
 * the app itself — resizing a window that gets recreated per run is pointless.
 * So `--attach` leaves the app alone and only runs WDIO against
 * `E2E_WD_PORT`, keeping the data dir intact because the running instance owns
 * whatever is already seeded there.
 */
const attachMode = args.includes('--attach');
const holdMode = args.includes('--hold');

if (holdMode) {
  if (attachMode) die('--hold and --attach are mutually exclusive');
  if (INSTANCE_COUNT > 1) die('--hold opens a single window; use --instances 1');
}

if (attachMode) {
  if (INSTANCE_COUNT > 1) {
    die('--attach drives a single already-running instance; use --instances 1');
  }
  log(`Attach mode: driving the app already listening on ${WD_PORT}.`);
} else {
  for (const dir of dataDirs) {
    resetAppDataDir(dir, keepAppData);
  }
}

if (INSTANCE_COUNT > 1) {
  log(`Multi-instance mode: ${INSTANCE_COUNT} app processes on ports ${wdPorts.join(', ')}`);
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
const appProcesses = [];

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
  runEnvTeardown();
  process.exit(130);
});
process.on('SIGTERM', () => {
  cleanup();
  runEnvTeardown();
  process.exit(143);
});

async function startAndWaitManagedInstances(ports, dirs, label = 'app') {
  const procs = attachMode
    ? []
    : ports.map((port, i) =>
        startAppInstance({
          binaryPath: appBinary,
          dataDir: dirs[i],
          port,
          workerIndex: i,
          onOutput: onAppOutput,
        }),
      );
  appProcesses.push(...procs);
  try {
    await Promise.all(ports.map((port) => waitForPort(port)));
  } catch (err) {
    for (const p of procs) {
      try { p.kill('SIGTERM'); } catch {}
    }
    for (const p of procs) {
      const idx = appProcesses.indexOf(p);
      if (idx >= 0) appProcesses.splice(idx, 1);
    }
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
        `WebDriver port(s) did not open: ${ports.join(', ')}`,
        'Common causes:',
        '  1. Binary built WITHOUT --features webdriver',
        '  2. Used cargo build instead of `pnpm tauri build --debug --features webdriver`',
        `  3. Another process already holds one of ${ports.join(', ')}`,
        'See docs/development/e2e-testing.md',
      ].join('\n'),
    );
  }
  if (sawAssetMissing) {
    for (const p of procs) {
      try { p.kill('SIGTERM'); } catch {}
    }
    cleanup();
    die(
      [
        'App reported "asset not found: index.html" — frontend is broken.',
        `Rebuild with: ${BUILD_CMD}`,
        'Do NOT use: cargo build -p datazen_desktop --features webdriver',
        'See docs/development/e2e-testing.md',
      ].join('\n'),
    );
  }
  if (procs.length > 1) {
    log(`${label}: all ${procs.length} WebDriver plugin(s) ready on ports ${ports.join(', ')}.`);
  } else if (procs.length === 1) {
    log(`${label}: WebDriver plugin ready on port ${ports[0]}.`);
  } else if (attachMode) {
    log(`${label}: attach mode (no app process started).`);
  }
  return procs;
}

const usesPhasedParallelFullRun = isFullRun && INSTANCE_COUNT > 1 && !forceSerialIsolated;
const usesManagedMultiInstance = INSTANCE_COUNT > 1 && !usesPhasedParallelFullRun;
const usesManagedSerialLifecycle = isolateSpecs || usesPhasedParallelFullRun;
// Modes that need a pre-started app here: --hold, or standard single-instance run.
const needsPrestartedSingleApp =
  !attachMode &&
  !usesPhasedParallelFullRun &&
  !usesManagedMultiInstance &&
  !usesManagedSerialLifecycle &&
  !holdMode
    ? false
    : holdMode;

if (holdMode) {
  await startAndWaitManagedInstances([WD_PORT], [dataDirs[0]], 'hold');
  log('Hold mode: app is up. Size the window, then run the capture with --attach.');
  await new Promise((resolve) => {
    appProcesses.forEach((p) => p.on('close', resolve));
    process.on('SIGINT', resolve);
    process.on('SIGTERM', resolve);
  });
  cleanup();
  runEnvTeardown();
  process.exit(0);
}

// For the plain single-instance, non-isolated, non-managed run we start the app now.
// All other modes manage app lifecycle themselves (phased parallel, multi-instance helper,
// isolated serial per-spec) to avoid duplicate instances / port conflicts.
if (
  !usesPhasedParallelFullRun &&
  !usesManagedMultiInstance &&
  !isolateSpecs &&
  !holdMode
) {
  // Single-instance standard WDIO run (or attach mode)
  await startAndWaitManagedInstances([WD_PORT], [dataDirs[0]], 'single');
}

// Step 3: Run WDIO
if (screenshotTrace) {
  log('Screenshot trace enabled (E2E_SCREENSHOT=1) → e2e/screenshots/<spec>/');
}
log('Running E2E tests...');

function collectFullRunSpecs() {
  const capture = Boolean(process.env.E2E_CAPTURE);
  const excluded = capture
    ? []
    : [
        /\/specs\/zz-screenshots\.ts$/,
        /\/specs\/demo-recording[^/]*\.ts$/,
        /\/specs\/zz-diag\.ts$/,
        /\/specs\/[^/]*screenshot[^/]*\.ts$/,
      ];
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!entry.name.endsWith('.ts') || entry.name.endsWith('.d.ts')) continue;
      const rel = path.relative(ROOT, full);
      if (excluded.some((re) => re.test(rel))) continue;
      const source = fs.readFileSync(full, 'utf8');
      if (!/\b(describe|it)\s*\(/.test(source)) continue;
      out.push(rel);
    }
  };
  walk(path.join(ROOT, 'e2e', 'specs'));
  return out.sort();
}

function normalizeSpecPath(spec) {
  return spec.replace(/\\/g, '/').replace(/^\.\//, '');
}

function parseSuiteFromConfig(suiteName) {
  const confPath = path.join(ROOT, 'e2e', 'wdio.conf.ts');
  const confText = fs.readFileSync(confPath, 'utf8');
  const re = new RegExp(`['"]?${suiteName}['"]?\\s*:\\s*\\[([^\\]]+)\\]`, 's');
  const m = confText.match(re);
  if (!m) {
    die(`Suite "${suiteName}" not found in wdio.conf.ts`);
  }
  const files = [];
  for (const line of m[1].split('\n')) {
    const fm = line.match(/'([^']+\.ts)'/);
    if (fm) files.push(normalizeSpecPath(fm[1].replace(/^\.\//, 'e2e/')));
  }
  return files;
}

const SHARED_FIXTURE_SERIAL_SUITES = ['data-sync', 'data-transfer', 'schema-diff'];
const SERIAL_ONLY_FULL_RUN_SPECS = new Set(
  SHARED_FIXTURE_SERIAL_SUITES.flatMap((s) => parseSuiteFromConfig(s)),
);

async function runMultiInstanceSpecs(specList, instanceCount, options = {}) {
  const {
    skipSharedFixtureReset = false,
    label = 'multi-instance',
  } = options;

  const ports = resolvePorts(WD_PORT, instanceCount);
  const dirs = resolveDataDirs(instanceCount);
  if (!attachMode) {
    for (const dir of dirs) resetAppDataDir(dir, keepAppData);
  }

  /** @type {import('node:child_process').ChildProcess[]} */
  const procs = attachMode
    ? []
    : ports.map((port, i) =>
        startAppInstance({
          binaryPath: appBinary,
          dataDir: dirs[i],
          port,
          workerIndex: i,
          onOutput: onAppOutput,
        }),
      );
  appProcesses.push(...procs);

  try {
    await Promise.all(ports.map((port) => waitForPort(port)));
  } catch (err) {
    for (const p of procs) {
      try { p.kill('SIGTERM'); } catch {}
    }
    throw err;
  }

  log(
    `[${label}] ${specList.length} spec(s) across ${instanceCount} process(es) on ports ${ports.join(', ')}`,
  );

  const chunks = Array.from({ length: instanceCount }, () => []);
  specList.forEach((spec, i) => chunks[i % instanceCount].push(spec));
  log(`[${label}] split: ${chunks.map((c) => c.length).join(' / ')}`);

  const wdioProcesses = chunks.map((specChunk, i) => {
    if (specChunk.length === 0) return null;
    const workerSchema = `e2e_worker_${i}`;
    const env = {
      ...process.env,
      DATAZEN_DATA_DIR: dirs[i],
      E2E_WD_PORT: String(ports[i]),
      E2E_WORKER_INDEX: String(i),
      E2E_WORKER_SCHEMA: workerSchema,
      ...(skipSharedFixtureReset ? { E2E_SKIP_SHARED_FIXTURE_RESET: '1' } : {}),
    };
    const specArgs = [];
    for (const s of specChunk) specArgs.push('--spec', s);
    const filteredWdioArgs = wdioArgs.filter(
      (a, idx) =>
        a !== '--spec' &&
        wdioArgs[idx - 1] !== '--spec' &&
        a !== '--suite' &&
        wdioArgs[idx - 1] !== '--suite',
    );
    const proc = spawn('npx', ['wdio', 'run', 'e2e/wdio.conf.ts', ...filteredWdioArgs, ...specArgs], {
      stdio: ['inherit', 'pipe', 'pipe'],
      cwd: ROOT,
      env,
    });
    const prefix = `\x1b[3${2 + (i % 6)}m[worker-${i}]\x1b[0m `;
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

  // Stop only this phase's app processes
  for (const p of procs) {
    if (!p.killed) {
      try { p.kill('SIGTERM'); } catch {}
    }
  }
  for (const p of procs) {
    const idx = appProcesses.indexOf(p);
    if (idx >= 0) appProcesses.splice(idx, 1);
  }

  const maxCode = exitCodes.length ? Math.max(...exitCodes) : 0;
  const passed = exitCodes.filter((c) => c === 0).length;
  log(
    `[${label}] ${passed}/${exitCodes.length} worker(s) passed` +
      (maxCode === 0 ? '' : ` (codes: ${exitCodes.join(', ')})`),
  );
  return { maxCode, exitCodes };
}

async function runIsolatedSerialSpecs(specList, options = {}) {
  const { label = 'serial-isolated', resetSharedFixtures = true } = options;

  // Single app instance for serial isolated mode (current behavior)
  const dataDir = dataDirs[0];
  const port = WD_PORT;

  // Ensure data dir exists/reset for this phase start
  if (!attachMode) resetAppDataDir(dataDir, keepAppData);

  // Local helpers mirroring previous isolateSpecs implementation
  const portOpen = () =>
    new Promise((resolve) => {
      const sock = createConnection({ port, host: '127.0.0.1' }, () => {
        sock.destroy();
        resolve(true);
      });
      sock.on('error', () => resolve(false));
    });

  const killPortHolders = () => {
    try {
      const out = execSync(`lsof -nP -iTCP:${port} -sTCP:LISTEN -t`, {
        stdio: ['ignore', 'pipe', 'ignore'],
      })
        .toString()
        .trim();
      for (const pid of out.split('\n').filter(Boolean)) {
        log(`port ${port} still held by pid ${pid}; killing it`);
        try { process.kill(Number(pid), 'SIGKILL'); } catch {}
      }
    } catch {}
  };

  const stopAppInstance = async (proc) => {
    if (!proc || proc.exitCode !== null) return;
    const closed = new Promise((resolve) =>
      proc.once('close', (code, signal) => resolve({ code, signal })),
    );
    try { proc.kill('SIGTERM'); } catch {}
    const outcome = await Promise.race([
      closed,
      new Promise((resolve) => setTimeout(() => resolve(null), 8000)),
    ]);
    if (outcome === null) {
      log('app did not exit within 8s of SIGTERM; sending SIGKILL');
      try { proc.kill('SIGKILL'); } catch {}
      await closed;
    }
    const started = Date.now();
    while (Date.now() - started < 15000) {
      if (!(await portOpen())) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    log(`port ${port} is still listening 15s after the app exited`);
    killPortHolders();
    await new Promise((resolve) => setTimeout(resolve, 500));
  };

  const startAppVerified = async (phaseLabel) => {
    for (let attempt = 1; attempt <= 3; attempt++) {
      const proc = startAppInstance({
        binaryPath: appBinary,
        dataDir,
        port,
        workerIndex: 0,
        onOutput: (chunk) => {
          if (chunk.toString().includes('Failed to bind to address')) proc.__bindFailed = true;
          onAppOutput(chunk);
        },
      });
      appProcesses.push(proc);
      try {
        await waitForPort(port, '127.0.0.1', 30000);
      } catch (err) {
        log(`${phaseLabel}: app did not open port ${port}: ${err.message}`);
        await stopAppInstance(proc);
        const idx = appProcesses.indexOf(proc);
        if (idx >= 0) appProcesses.splice(idx, 1);
        continue;
      }
      await new Promise((resolve) => setTimeout(resolve, 400));
      if (!proc.__bindFailed) return proc;
      log(
        `${phaseLabel}: port ${port} was still owned by a previous app (attempt ${attempt}); ` +
          'killing the leftovers and retrying',
      );
      killPortHolders();
      await stopAppInstance(proc);
      const idx = appProcesses.indexOf(proc);
      if (idx >= 0) appProcesses.splice(idx, 1);
    }
    die(`${phaseLabel}: could not obtain port ${port} for a fresh app process`);
    return null;
  };

  const otherWdioArgs = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--spec') { i++; continue; }
    otherWdioArgs.push(wdioArgs[i]);
  }

  const baseEnv = {
    ...process.env,
    DATAZEN_DATA_DIR: dataDir,
    E2E_WD_PORT: String(port),
    E2E_WORKER_INDEX: '0',
    E2E_WORKER_SCHEMA: process.env.E2E_WORKER_SCHEMA || 'e2e_worker_0',
    E2E_SHARED_FIXTURE_RESET: resetSharedFixtures ? 'runner' : 'runner',
  };

  const SPEC_TIMEOUT_MS = Number(
    process.env.E2E_SPEC_TIMEOUT_MS || args.find((a) => a.startsWith('--spec-timeout='))?.split('=')[1],
  ) || 4 * 60 * 1000;

  const runWdioOnce = (spec) =>
    new Promise((resolve) => {
      const proc = spawn(
        'npx',
        ['wdio', 'run', 'e2e/wdio.conf.ts', ...otherWdioArgs, '--spec', spec],
        { stdio: ['ignore', 'pipe', 'pipe'], cwd: ROOT, env: baseEnv },
      );
      let timedOut = false;
      let zeroTests = false;
      let tail = '';
      const forward = (chunk, stream) => {
        const text = chunk.toString();
        stream.write(chunk);
        tail = (tail + text).slice(-4000);
        if (/Spec Files:\s+0 passed, 0 total/.test(tail)) zeroTests = true;
      };
      proc.stdout.on('data', (chunk) => forward(chunk, process.stdout));
      proc.stderr.on('data', (chunk) => forward(chunk, process.stderr));
      const timer = setTimeout(() => {
        timedOut = true;
        log(`spec timed out after ${Math.round(SPEC_TIMEOUT_MS / 1000)}s, killing: ${spec}`);
        try { proc.kill('SIGKILL'); } catch {}
      }, SPEC_TIMEOUT_MS);
      proc.on('close', (code) => {
        clearTimeout(timer);
        if (timedOut) return resolve({ code: 1, skipped: false });
        resolve({ code: code ?? 1, skipped: zeroTests && code !== 0 });
      });
    });

  log(`[${label}] ${specList.length} spec file(s), one app process each`);
  const failures = [];
  const skipped = [];
  for (let i = 0; i < specList.length; i++) {
    const spec = specList[i];
    log(`[${label}] [${i + 1}/${specList.length}] ${spec}`);
    if (i > 0 && !attachMode) resetAppDataDir(dataDir, false);
    if (resetSharedFixtures) resetSharedFixtureDbs(spec);
    const proc = await startAppVerified(spec);
    let result = { code: 1, skipped: false };
    try {
      result = await runWdioOnce(spec);
    } finally {
      await stopAppInstance(proc);
      const idx = appProcesses.indexOf(proc);
      if (idx >= 0) appProcesses.splice(idx, 1);
    }
    if (result.skipped) skipped.push(spec);
    else if (result.code !== 0) failures.push(spec);
    const status = result.skipped ? 'SKIP' : result.code === 0 ? 'PASS' : 'FAIL';
    log(`[${label}] [${i + 1}/${specList.length}] ${status} ${spec}`);
  }

  if (skipped.length > 0) log(`[${label}] ${skipped.length} spec file(s) had no runnable case (skipped)`);
  if (sharedFixtureResetFailures.length > 0) {
    log(
      `[${label}] ${sharedFixtureResetFailures.length} spec file(s) ran without a clean shared fixture DB ` +
        '(resets failed; see WARNING lines above)',
    );
  }
  return { failures, skipped };
}

function resolveExplicitSpecList() {
  // --suite support for targeted runs in non-full modes: resolve via config
  const suiteNames = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--suite' && wdioArgs[i + 1]) {
      suiteNames.push(wdioArgs[i + 1]);
      i++;
    }
  }
  if (suiteNames.length > 0) {
    const out = [];
    for (const name of suiteNames) out.push(...parseSuiteFromConfig(name));
    return [...new Set(out)];
  }
  const fromArgs = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--spec' && wdioArgs[i + 1]) fromArgs.push(normalizeSpecPath(wdioArgs[i + 1]));
  }
  if (fromArgs.length > 0) return [...new Set(fromArgs)];
  // default: all specs (respecting excludes)
  const capture = Boolean(process.env.E2E_CAPTURE);
  const excluded = capture
    ? []
    : [
        /\/specs\/zz-screenshots\.ts$/,
        /\/specs\/demo-recording[^/]*\.ts$/,
        /\/specs\/zz-diag\.ts$/,
        /\/specs\/[^/]*screenshot[^/]*\.ts$/,
      ];
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) { walk(full); continue; }
      if (!entry.name.endsWith('.ts') || entry.name.endsWith('.d.ts')) continue;
      const rel = path.relative(ROOT, full);
      if (excluded.some((re) => re.test(rel))) continue;
      out.push(rel);
    }
  };
  walk(path.join(ROOT, 'e2e', 'specs'));
  return out.sort();
}

if (isFullRun && INSTANCE_COUNT > 1 && !forceSerialIsolated) {
  // Full E2E: parallel-safe specs run in parallel (<=4 workers), shared-fixture
  // suites (data-sync/data-transfer/schema-diff) run serially to avoid conflicts.
  const allSpecs = collectFullRunSpecs();
  const parallelSpecs = allSpecs.filter((s) => !SERIAL_ONLY_FULL_RUN_SPECS.has(normalizeSpecPath(s)));
  const serialSpecs = allSpecs.filter((s) => SERIAL_ONLY_FULL_RUN_SPECS.has(normalizeSpecPath(s)));

  log(
    `Full E2E (phased): ${allSpecs.length} spec(s) total → ` +
      `${parallelSpecs.length} parallel-safe + ${serialSpecs.length} serial-only ` +
      `(instances=${INSTANCE_COUNT}, cap=4)`,
  );

  let overallFailed = false;

  if (parallelSpecs.length > 0) {
    // Parallel phase does not depend on shared sync/transfer/diff fixture DBs.
    const { maxCode } = await runMultiInstanceSpecs(parallelSpecs, INSTANCE_COUNT, {
      skipSharedFixtureReset: true,
      label: 'phase-parallel',
    });
    if (maxCode !== 0) overallFailed = true;
  }

  if (serialSpecs.length > 0) {
    // Run setup scripts for transfer/schema-diff environments if present? The
    // dedicated package scripts do this; full run goes through setup-e2e-env.sh
    // already. Ensure serial phase uses clean isolated app per spec.
    const { failures } = await runIsolatedSerialSpecs(serialSpecs, {
      label: 'phase-serial',
      resetSharedFixtures: true,
    });
    if (failures.length > 0) {
      overallFailed = true;
      log(`Serial-only phase failures:`);
      for (const spec of failures) log(`  - ${spec}`);
    }
  }

  cleanup();
  runEnvTeardown();
  if (overallFailed) {
    log('Full E2E completed with failures.');
    process.exit(1);
  }
  log('Full E2E passed (parallel + serial phases).');
  process.exit(0);
}

if (INSTANCE_COUNT > 1) {
  // Targeted multi-instance run (suite/spec) — preserve existing behavior.
  let allSpecs = [];
  const suiteNames = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--suite' && wdioArgs[i + 1]) {
      suiteNames.push(wdioArgs[i + 1]);
      i++;
    }
  }

  if (suiteNames.length > 0) {
    for (const name of suiteNames) allSpecs.push(...parseSuiteFromConfig(name));
    log(`Resolved ${suiteNames.join('+')} suite(s): ${allSpecs.length} specs`);
  } else {
    const specIdx = wdioArgs.indexOf('--spec');
    if (specIdx >= 0 && wdioArgs[specIdx + 1]) {
      allSpecs = wdioArgs[specIdx + 1].split(',').map((s) => normalizeSpecPath(s.trim()));
    } else {
      allSpecs = resolveExplicitSpecList();
    }
  }

  const { maxCode, exitCodes } = await runMultiInstanceSpecs(allSpecs, INSTANCE_COUNT, {
    skipSharedFixtureReset: false,
    label: 'multi-instance',
  });
  cleanup();
  runEnvTeardown();
  if (maxCode === 0) {
    log('All tests passed!');
    process.exit(0);
  }
  log(`Some workers failed (codes: ${exitCodes.join(', ')})`);
  process.exit(maxCode);
}

if (isolateSpecs) {
  // Isolated serial mode (targeted runs, or forced serial full run)
  let specs;
  if (isFullRun) {
    specs = collectFullRunSpecs();
  } else {
    // Targeted: respect --spec args as given; if none, fall back to suite/all resolution
    const fromArgs = [];
    for (let i = 0; i < wdioArgs.length; i++) {
      if (wdioArgs[i] === '--spec' && wdioArgs[i + 1]) fromArgs.push(normalizeSpecPath(wdioArgs[i + 1]));
    }
    if (fromArgs.length > 0) {
      specs = [...new Set(fromArgs)];
    } else if (hasExplicitSuiteArg) {
      const suiteNames = [];
      for (let i = 0; i < wdioArgs.length; i++) {
        if (wdioArgs[i] === '--suite' && wdioArgs[i + 1]) { suiteNames.push(wdioArgs[i + 1]); i++; }
      }
      const out = [];
      for (const name of suiteNames) out.push(...parseSuiteFromConfig(name));
      specs = [...new Set(out)];
    } else {
      specs = collectFullRunSpecs();
    }
  }

  const { failures, skipped } = await runIsolatedSerialSpecs(specs, {
    label: 'isolated',
    resetSharedFixtures: true,
  });
  cleanup();
  runEnvTeardown();
  if (failures.length === 0) {
    log(`All ${specs.length} isolated spec files passed!`);
    process.exit(0);
  }
  log(`${specs.length - failures.length}/${specs.length} isolated spec files passed; failing:`);
  for (const spec of failures) log(`  - ${spec}`);
  process.exit(1);
}

// Single-instance: standard WDIO run (targeted suites/specs when not isolated)
{
  const wdioEnv = {
    ...process.env,
    DATAZEN_DATA_DIR: dataDirs[0],
    E2E_WD_PORT: String(WD_PORT),
    E2E_WORKER_INDEX: '0',
    E2E_WORKER_SCHEMA: process.env.E2E_WORKER_SCHEMA || 'e2e_worker_0',
  };

  const wdio = spawn('npx', ['wdio', 'run', 'e2e/wdio.conf.ts', ...wdioArgs], {
    stdio: 'inherit',
    cwd: ROOT,
    env: wdioEnv,
  });

  const exitCode = await new Promise((resolve) => {
    wdio.on('close', (code) => resolve(code ?? 1));
  });

  cleanup();
  runEnvTeardown();
  log(exitCode === 0 ? 'All tests passed!' : `Tests failed (exit code ${exitCode})`);
  process.exit(exitCode);
}
