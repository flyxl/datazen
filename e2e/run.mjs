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
/**
 * 每个 spec 文件独占一次应用进程（见下方 isolated 分支）。整包串跑时 spec 之间会
 * 共享运行时会话、连接池和 config_revision：前一个 spec 留下的会话会让后一个
 * spec 的 `connect` 撞上连接额度（"connection runtime operation failed"），
 * 串扰会掩盖真实的 UI 回归，所以整包验证默认走隔离模式。
 */
const isolateSpecs = args.includes('--isolate-specs')
  ? true
  : args.includes('--no-isolate-specs')
    ? false
    : process.env.E2E_ISOLATE_SPECS === '1'
      ? true
      : !args.includes('--spec') && !args.includes('--suite');
const portArg = args.find((a, i) => args[i - 1] === '--port');
const instancesArg = args.find((a, i) => args[i - 1] === '--instances');
const WD_PORT = portArg ? parseInt(portArg, 10) : parseInt(process.env.E2E_WD_PORT || '4445', 10);
const INSTANCE_COUNT = instancesArg ? parseInt(instancesArg, 10) : 1;
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
 * site/assets/screenshots/. It pairs with --window-size: the whole point of a
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
  const proAppBundleBin = path.join(
    ROOT,
    'target/debug/bundle/macos/DataZen Pro.app/Contents/MacOS/datazen',
  );
  if (process.platform === 'darwin') {
    if (fs.existsSync(appBundleBin)) return appBundleBin;
    if (fs.existsSync(proAppBundleBin)) return proAppBundleBin;
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
const appProcesses = attachMode
  ? []
  : wdPorts.map((port, i) =>
      startAppInstance({
        binaryPath: appBinary,
        dataDir: dataDirs[i],
        port,
        workerIndex: i,
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
  runEnvTeardown();
  process.exit(130);
});
process.on('SIGTERM', () => {
  cleanup();
  runEnvTeardown();
  process.exit(143);
});

try {
  await Promise.all(wdPorts.map((port) => waitForPort(port)));

  /**
   * `--hold` opens the app and stops there, leaving the window up so a human can
   * size it by hand, then drives it with a follow-up `--attach` run.
   *
   * The screenshot geometry is not something a spec can pick: the webdriver
   * captures 1:1 in CSS pixels and ignores devicePixelRatio, so the only way to
   * land on a window that is both fully visible and the right density is to set
   * it once, by eye, on the real display.
   */
  if (holdMode) {
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
  let allSpecs = [];

  /**
   * Parse suite definitions from wdio.conf.ts by reading the file and extracting
   * the named suite's file list via regex. Returns absolute spec paths.
   */
  function parseSuiteFromConfig(suiteName) {
    const confPath = path.join(ROOT, 'e2e', 'wdio.conf.ts');
    const confText = fs.readFileSync(confPath, 'utf8');
    // Match: suiteName: [\n  './specs/...', ...]
    const re = new RegExp(`['"]?${suiteName}['"]?\\s*:\\s*\\[([^\\]]+)\\]`, 's');
    const m = confText.match(re);
    if (!m) {
      die(`Suite "${suiteName}" not found in wdio.conf.ts`);
    }
    const files = [];
    for (const line of m[1].split('\n')) {
      const fm = line.match(/'([^']+\.ts)'/);
      if (fm) files.push(fm[1].replace(/^\.\//, 'e2e/'));
    }
    return files;
  }

  // Collect suite args (--suite may appear multiple times)
  const suiteNames = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--suite' && wdioArgs[i + 1]) {
      suiteNames.push(wdioArgs[i + 1]);
      i++;
    }
  }

  if (suiteNames.length > 0) {
    for (const name of suiteNames) {
      allSpecs.push(...parseSuiteFromConfig(name));
    }
    log(`Resolved ${suiteNames.join('+')} suite(s): ${allSpecs.length} specs`);
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

  const chunks = Array.from({ length: INSTANCE_COUNT }, () => []);
  allSpecs.forEach((spec, i) => chunks[i % INSTANCE_COUNT].push(spec));

  log(
    `Splitting ${allSpecs.length} specs across ${INSTANCE_COUNT} processes: ` +
      `${chunks.map((c) => c.length).join(' / ')}`,
  );

  const wdioProcesses = chunks.map((specChunk, i) => {
    if (specChunk.length === 0) return null;
    const workerSchema = `e2e_worker_${i}`;
    const env = {
      ...process.env,
      DATAZEN_DATA_DIR: dataDirs[i],
      E2E_WD_PORT: String(wdPorts[i]),
      E2E_WORKER_INDEX: String(i),
      E2E_WORKER_SCHEMA: workerSchema,
    };
    const specArgs = [];
    for (const s of specChunk) {
      specArgs.push('--spec', s);
    }
    // Remove --spec and --suite from original wdioArgs since we're providing explicit spec list
    const filteredWdioArgs = wdioArgs.filter(
      (a, idx) =>
        a !== '--spec' &&
        wdioArgs[idx - 1] !== '--spec' &&
        a !== '--suite' &&
        wdioArgs[idx - 1] !== '--suite',
    );
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
      (maxCode === 0
        ? 'All tests passed!'
        : `Some workers failed (codes: ${exitCodes.join(', ')})`),
  );
  process.exit(maxCode);
} else if (isolateSpecs) {
  // 隔离模式：一个 spec 文件 = 一次应用进程 + 一次 WDIO 会话。
  //
  // 串跑（同一进程跑全部 spec）会把前一个 spec 的运行时会话、连接池、
  // config_revision 和 WebDriver 会话留给下一个 spec：seeded 连接在 spec 之间
  // 被改写后，旧会话仍占着连接额度，后一个 spec 的 `connect` 直接收到
  // "connection runtime operation failed"，失败被记在无辜的 spec 头上。
  // 隔离后失败只属于当前 spec，整包结果才有判读价值。
  const collectIsolatedSpecs = () => {
    const fromArgs = [];
    for (let i = 0; i < wdioArgs.length; i++) {
      if (wdioArgs[i] === '--spec' && wdioArgs[i + 1]) fromArgs.push(wdioArgs[i + 1]);
    }
    if (fromArgs.length > 0) return [...new Set(fromArgs)];
    // 只收「真的有用例」的 spec 文件，规则与 wdio.conf.ts 的 exclude 对齐：
    // 截图/录屏 spec 是刻意不进默认套件的（会重写 site/ 下已提交的截图二进制，
    // 还需要人工摆好的窗口与种子数据），把它们的「0 个用例」当成失败毫无意义。
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
        // 纯 helper 文件（journeys/*Helpers.ts 这类）不是 spec，跑它只会得到
        // 「0 个用例」；按内容判别比按文件名猜更可靠。
        const source = fs.readFileSync(full, 'utf8');
        if (!/\b(describe|it)\s*\(/.test(source)) continue;
        out.push(rel);
      }
    };
    walk(path.join(ROOT, 'e2e', 'specs'));
    return out.sort();
  };

  const otherWdioArgs = [];
  for (let i = 0; i < wdioArgs.length; i++) {
    if (wdioArgs[i] === '--spec') {
      i++;
      continue;
    }
    otherWdioArgs.push(wdioArgs[i]);
  }

  const baseEnv = {
    ...process.env,
    DATAZEN_DATA_DIR: dataDirs[0],
    E2E_WD_PORT: String(WD_PORT),
    E2E_WORKER_INDEX: '0',
    E2E_WORKER_SCHEMA: process.env.E2E_WORKER_SCHEMA || 'e2e_worker_0',
  };

  // 单个 spec 的超时上限：挂了就杀掉，记 FAIL 继续下一个，否则一个卡死的 spec
  // 会把整包验证拖成无限等待。
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
      // 有些 spec 文件只在专项开关下才有用例（例如截图 spec）。WDIO 对「一个用例
      // 都没跑」仍返回非零，那等于把「跳过」记成「失败」，会把整包结论搞脏。
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
        try {
          proc.kill('SIGKILL');
        } catch {
          /* already gone */
        }
      }, SPEC_TIMEOUT_MS);
      proc.on('close', (code) => {
        clearTimeout(timer);
        if (timedOut) return resolve({ code: 1, skipped: false });
        resolve({ code: code ?? 1, skipped: zeroTests && code !== 0 });
      });
    });

  /**
   * SIGTERM → （必要时）SIGKILL，并等到端口真的释放。
   *
   * 关键不是「发了信号」，而是「下一个 spec 起来时 4445 已经没有别人在听」：
   * 旧进程只要还占着端口，新进程的 WebDriver 线程就会 bind 失败（AddrInUse），
   * 于是 WDIO 连到的是**上一个 spec 的旧进程**——串扰原样回来，而且失败会被
   * 记在当前这个无辜的 spec 头上。
   */
  const portOpen = () =>
    new Promise((resolve) => {
      const sock = createConnection({ port: WD_PORT, host: '127.0.0.1' }, () => {
        sock.destroy();
        resolve(true);
      });
      sock.on('error', () => resolve(false));
    });

  const killPortHolders = () => {
    // 兜底：进程杀了但端口仍被占时，把监听者找出来杀掉。lsof 不在也只是一条日志。
    try {
      const out = execSync(`lsof -nP -iTCP:${WD_PORT} -sTCP:LISTEN -t`, {
        stdio: ['ignore', 'pipe', 'ignore'],
      })
        .toString()
        .trim();
      for (const pid of out.split('\n').filter(Boolean)) {
        log(`port ${WD_PORT} still held by pid ${pid}; killing it`);
        try {
          process.kill(Number(pid), 'SIGKILL');
        } catch {
          /* already gone */
        }
      }
    } catch {
      /* lsof missing or nothing to kill */
    }
  };

  const stopAppInstance = async (proc) => {
    if (!proc || proc.exitCode !== null) return;
    const closed = new Promise((resolve) =>
      proc.once('close', (code, signal) => resolve({ code, signal })),
    );
    try {
      proc.kill('SIGTERM');
    } catch {
      /* already gone */
    }
    const outcome = await Promise.race([
      closed,
      new Promise((resolve) => setTimeout(() => resolve(null), 8000)),
    ]);
    if (outcome === null) {
      log('app did not exit within 8s of SIGTERM; sending SIGKILL');
      try {
        proc.kill('SIGKILL');
      } catch {
        /* already gone */
      }
      await closed;
    }
    const started = Date.now();
    while (Date.now() - started < 15000) {
      if (!(await portOpen())) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
    log(`port ${WD_PORT} is still listening 15s after the app exited`);
    killPortHolders();
    await new Promise((resolve) => setTimeout(resolve, 500));
  };

  /**
   * 起一个应用进程，并确认 WebDriver 端口真的是**这个**进程在听。
   *
   * 旧进程没退干净时，新进程会 bind 失败而旧进程继续服务；这种错位必须在这里
   * 被抓住并重试，不能让整个 spec 悄悄跑在上一轮的进程上。
   */
  const startAppVerified = async (label) => {
    for (let attempt = 1; attempt <= 3; attempt++) {
      const proc = startAppInstance({
        binaryPath: appBinary,
        dataDir: dataDirs[0],
        port: WD_PORT,
        workerIndex: 0,
        onOutput: (chunk) => {
          if (chunk.toString().includes('Failed to bind to address')) proc.__bindFailed = true;
          onAppOutput(chunk);
        },
      });
      appProcesses.push(proc);
      try {
        await waitForPort(WD_PORT, '127.0.0.1', 30000);
      } catch (err) {
        log(`${label}: app did not open port ${WD_PORT}: ${err.message}`);
        await stopAppInstance(proc);
        continue;
      }
      await new Promise((resolve) => setTimeout(resolve, 400));
      if (!proc.__bindFailed) return proc;
      log(
        `${label}: port ${WD_PORT} was still owned by a previous app (attempt ${attempt}); ` +
          'killing the leftovers and retrying',
      );
      killPortHolders();
      await stopAppInstance(proc);
    }
    die(`${label}: could not obtain port ${WD_PORT} for a fresh app process`);
    return null;
  };

  const specs = collectIsolatedSpecs();
  log(`Isolated mode: ${specs.length} spec file(s), one app process each`);
  const failures = [];
  const skipped = [];
  for (let i = 0; i < specs.length; i++) {
    const spec = specs[i];
    log(`[${i + 1}/${specs.length}] ${spec}`);
    if (i > 0) resetAppDataDir(dataDirs[0], false);
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
    const label = result.skipped ? 'SKIP' : result.code === 0 ? 'PASS' : 'FAIL';
    log(`[${i + 1}/${specs.length}] ${label} ${spec}`);
  }

  cleanup();
  runEnvTeardown();
  if (skipped.length > 0) log(`${skipped.length} spec file(s) had no runnable case (skipped)`);
  if (failures.length === 0) {
    log(`All ${specs.length} isolated spec files passed!`);
    process.exit(0);
  }
  log(`${specs.length - failures.length}/${specs.length} isolated spec files passed; failing:`);
  for (const spec of failures) log(`  - ${spec}`);
  process.exit(1);
} else {
  // Single-instance: standard WDIO run
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
