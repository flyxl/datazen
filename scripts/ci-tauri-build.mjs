#!/usr/bin/env node
/**
 * ci-tauri-build.mjs — run the Tauri CLI using .driver-features.json.
 *
 * Used by CI inside with-driver-inject so we never nest `bash -c` through
 * Node spawn (that loses the -c script argument on Windows).
 *
 * Invokes `node node_modules/@tauri-apps/cli/tauri.js` directly. Going through
 * `pnpm.cmd` on Windows re-parses argv in cmd.exe and strips quotes from
 * `--config '{"bundle":...}'`.
 *
 * Usage:
 *   node scripts/ci-tauri-build.mjs --target=x86_64-pc-windows-msvc
 */

import { readFileSync, existsSync, writeFileSync, mkdirSync } from 'fs';
import { spawnSync } from 'child_process';
import { createRequire } from 'module';
import { tmpdir } from 'os';
import { resolve, dirname, join } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const require = createRequire(import.meta.url);

export const UPDATER_CONFIG = { bundle: { createUpdaterArtifacts: true } };

export const PRO_CONFIG = {};

/** Relative paths that must exist under the staged Pro extension tree. */
export const REQUIRED_PRO_STAGED_PATHS = [
  'manifest.json',
  'dist/index.esm.js',
  'signature.sig',
];

export function builtinEpStagingDir(root = ROOT) {
  return join(root, 'src-tauri', 'resources', 'builtin-ep', 'sql-editor-pro');
}

/**
 * Pre-flight check for Pro edition builds: verify the staged builtin-ep tree
 * exists *before* the ~10 min Tauri build starts. Returns the list of missing
 * relative paths (empty = ready). Emits a `::notice::` line per missing file
 * so failures are visible in check-run annotations without admin log access.
 */
export function checkProStagingReady({ root = ROOT, log = console.log } = {}) {
  const staging = builtinEpStagingDir(root);
  const missing = REQUIRED_PRO_STAGED_PATHS.filter(
    (rel) => !existsSync(join(staging, rel)),
  );
  if (missing.length > 0) {
    log(
      `::notice::[pro-staging] missing staged files under ${staging}: ${missing.join(', ')}`,
    );
  } else {
    log(`[pro-staging] staged tree ready at ${staging}`);
  }
  return missing;
}

export function resolveTauriCli(root = ROOT) {
  return require.resolve('@tauri-apps/cli/tauri.js', { paths: [root] });
}

export function writeUpdaterConfigFile(dir = join(tmpdir(), 'datazen-ci-tauri')) {
  return writeTauriConfigFile({ updater: true, isPro: false, dir });
}

export function writeTauriConfigFile({
  updater = false,
  isPro = false,
  dir = join(tmpdir(), 'datazen-ci-tauri'),
} = {}) {
  mkdirSync(dir, { recursive: true });
  const file = join(dir, `config-${isPro ? 'pro' : 'base'}-${updater ? 'updater' : 'plain'}.json`);
  const config = {};
  if (updater) {
    Object.assign(config, UPDATER_CONFIG);
  }
  if (isPro) {
    Object.assign(config, PRO_CONFIG);
  }
  writeFileSync(file, `${JSON.stringify(config)}\n`);
  return file;
}

export function buildTauriArgs({
  target = null,
  updater = false,
  edition = 'community',
  features = [],
  configPath = null,
  updaterConfigPath = null,
  extraArgs = [],
} = {}) {
  const args = ['build'];
  if (target) {
    args.push('--target', target);
  }
  const isPro = edition === 'pro';
  if (updater || isPro) {
    args.push(
      '--config',
      configPath ?? updaterConfigPath ?? writeTauriConfigFile({ updater, isPro }),
    );
  }
  if (Array.isArray(features) && features.length > 0) {
    args.push('-f', features.join(','));
  }
  if (Array.isArray(extraArgs) && extraArgs.length > 0) {
    args.push(...extraArgs);
  }
  return args;
}

export function spawnTauri(args, { cwd = ROOT, env = process.env, log = console.log } = {}) {
  const cli = resolveTauriCli(cwd);
  log(`[ci-tauri-build] ${process.execPath} ${cli} ${args.join(' ')}`);
  const result = spawnSync(process.execPath, [cli, ...args], {
    cwd,
    stdio: 'inherit',
    shell: false,
    env,
    windowsHide: true,
  });
  if (result.error) {
    console.error('[ci-tauri-build] spawn failed:', result.error);
  }
  return result;
}

function main() {
  const argv = process.argv.slice(2);
  const targetArg = argv.find((a) => a.startsWith('--target='));
  const target = targetArg ? targetArg.slice('--target='.length) : null;
  const isPro =
    argv.includes('--pro') ||
    argv.includes('--edition=pro') ||
    process.env.DATAZEN_EDITION === 'pro';
  const edition = isPro ? 'pro' : 'community';

  const knownPrefixes = ['--target=', '--edition='];
  const knownFlags = new Set(['--pro', '--community', '--updater']);
  const extraArgs = argv.filter((a) => {
    if (knownFlags.has(a)) return false;
    if (knownPrefixes.some((p) => a.startsWith(p))) return false;
    return true;
  });

  const featuresPath = resolve(ROOT, '.driver-features.json');
  if (!existsSync(featuresPath)) {
    console.error('[ci-tauri-build] missing .driver-features.json — run resolve-drivers first');
    process.exit(1);
  }

  const { features } = JSON.parse(readFileSync(featuresPath, 'utf-8'));

  // Fail fast: a missing staged tree means the .deb/.app would ship without
  // the Pro extension — better to stop here than after a 10-min build.
  if (edition === 'pro') {
    const missing = checkProStagingReady();
    if (missing.length > 0) {
      console.error(
        `[ci-tauri-build] pro staging incomplete, missing: ${missing.join(', ')} ` +
          `(expected under ${builtinEpStagingDir()})`,
      );
      process.exit(1);
    }
  }

  const args = buildTauriArgs({
    target,
    updater: argv.includes('--updater'),
    edition,
    features,
    extraArgs,
  });
  const result = spawnTauri(args);
  process.exit(result.status ?? 1);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
