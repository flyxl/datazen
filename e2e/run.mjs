#!/usr/bin/env node
/**
 * One-command E2E runner:
 *   1. Build the app with --features webdriver (skippable via --skip-build)
 *   2. Start the Tauri app binary (the embedded webdriver plugin listens on 4445)
 *   3. Run WDIO tests (forwards extra args like --spec)
 *   4. Kill the app on exit
 */
import { spawn, execSync } from 'node:child_process';
import { createConnection } from 'node:net';
import path from 'node:path';
import url from 'node:url';

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));

const args = process.argv.slice(2);
const skipBuild = args.includes('--skip-build');
const wdioArgs = [];
{
  const filtered = args.filter((a) => a !== '--skip-build' && a !== '--');
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
  const target = path.resolve(__dirname, '../src-tauri/target');
  if (process.platform === 'win32') {
    return path.join(target, 'debug/datazen.exe');
  }
  return path.join(target, 'debug/datazen');
}

// Step 1: Build
if (!skipBuild) {
  log('Building app with webdriver feature...');
  try {
    execSync('pnpm tauri build --debug --features webdriver', {
      stdio: 'inherit',
      cwd: process.cwd(),
    });
  } catch {
    process.exit(1);
  }
}

// Step 2: Start the Tauri app (webdriver plugin on port 4445)
const appBinary = getAppBinaryPath();
log(`Starting app: ${appBinary}`);
const app = spawn(appBinary, [], {
  stdio: ['ignore', 'pipe', 'pipe'],
  detached: false,
});

app.stdout.on('data', (d) => process.stdout.write(d));
app.stderr.on('data', (d) => process.stderr.write(d));

function cleanup() {
  if (!app.killed) {
    log('Stopping app...');
    app.kill('SIGTERM');
  }
}
process.on('exit', cleanup);
process.on('SIGINT', () => { cleanup(); process.exit(130); });
process.on('SIGTERM', () => { cleanup(); process.exit(143); });

try {
  await waitForPort(4445);
  log('WebDriver plugin is ready on port 4445.');
} catch (err) {
  console.error(err.message);
  cleanup();
  process.exit(1);
}

// Step 3: Run WDIO
log('Running E2E tests...');
const wdio = spawn(
  'npx',
  ['wdio', 'run', 'e2e/wdio.conf.ts', ...wdioArgs],
  { stdio: 'inherit', cwd: process.cwd() },
);

const exitCode = await new Promise((resolve) => {
  wdio.on('close', (code) => resolve(code ?? 1));
});

// Step 4: Cleanup
cleanup();
log(exitCode === 0 ? 'All tests passed!' : `Tests failed (exit code ${exitCode})`);
process.exit(exitCode);
