#!/usr/bin/env node
/**
 * ci-tauri-build.mjs — run `pnpm tauri build` using .plugin-features.json.
 *
 * Used by CI inside with-plugin-inject so we never nest `bash -c` through
 * Node spawn (that loses the -c script argument on Windows).
 *
 * Usage:
 *   node scripts/ci-tauri-build.mjs --target=x86_64-pc-windows-msvc
 */

import { readFileSync, existsSync } from 'fs';
import { spawnSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

export const UPDATER_CONFIG = { bundle: { createUpdaterArtifacts: true } };

export function pnpmBin() {
  return process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
}

export function buildTauriArgs({ target = null, updater = false, features = [] } = {}) {
  const args = ['tauri', 'build'];
  if (target) {
    args.push('--target', target);
  }
  if (updater) {
    args.push('--config', JSON.stringify(UPDATER_CONFIG));
  }
  if (Array.isArray(features) && features.length > 0) {
    args.push('-f', features.join(','));
  }
  return args;
}

function main() {
  const targetArg = process.argv.find((a) => a.startsWith('--target='));
  const target = targetArg ? targetArg.slice('--target='.length) : null;

  const featuresPath = resolve(ROOT, '.plugin-features.json');
  if (!existsSync(featuresPath)) {
    console.error('[ci-tauri-build] missing .plugin-features.json — run resolve-drivers first');
    process.exit(1);
  }

  const { features } = JSON.parse(readFileSync(featuresPath, 'utf-8'));
  const args = buildTauriArgs({
    target,
    updater: process.argv.includes('--updater'),
    features,
  });

  console.log(`[ci-tauri-build] ${pnpmBin()} ${args.join(' ')}`);
  const result = spawnSync(pnpmBin(), args, {
    cwd: ROOT,
    stdio: 'inherit',
    // Do not use a shell: `shell: true` strips quotes from `--config '{"bundle":...}'`,
    // and Tauri then rejects the unquoted object as invalid JSON.
    shell: false,
    env: process.env,
    windowsHide: true,
  });
  process.exit(result.status ?? 1);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
