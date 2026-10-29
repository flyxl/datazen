#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * Ensure gitignored driver codegen files exist.
 *
 * Files:
 *   src/extensions/generated.ts
 *   apps/desktop/src-tauri/src/driver_init.rs
 *   apps/desktop/src-tauri/capabilities/default.json (merged from default_host.json + drivers)
 *
 * Used by `pnpm install` (prepare) and `pnpm build` so tsc / rust-analyzer /
 * beforeBuildCommand work on a fresh clone without injecting Cargo.toml.
 *
 * If all files exist, skip (keeps the last `tauri:dev --drivers=...`
 * selection). Pass `--force` to regenerate. Driver set follows
 * `--drivers=...` / DATAZEN_DRIVERS / default `basic`.
 */

import { existsSync, readFileSync } from 'fs';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { FULLY_GENERATED_MANAGED } from './driver-deinject.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

const GENERATED_FILES = [
  ...FULLY_GENERATED_MANAGED,
  'apps/desktop/src-tauri/capabilities/default.json',
];

/**
 * Exports a codegen file must contain to be considered current.
 *
 * Codegen files are gitignored and this script is a no-op when they already
 * exist, so changing a template (adding an export) would otherwise leave every
 * existing checkout compiling against a stale file — `import { X } from
 * '../extensions/generated'` fails with "has no exported member" and the only
 * fix is remembering to delete the file by hand. Markers make that self-healing:
 * a codegen file that predates an export it is expected to provide is
 * regenerated on the next `pnpm install` / `pnpm build`.
 */
export const REQUIRED_CODEGEN_MARKERS = {
  'packages/frontend/src/extensions/generated.ts': [
    'export const DRIVER_PROTOCOL_VERSION',
    'export const DATAZEN_VARIANT',
    'export const DATAZEN_UPDATER_CHANNEL',
  ],
};

/**
 * @param {string} [root]
 * @returns {string[]}
 */
export function missingGeneratedFiles(root = ROOT) {
  return GENERATED_FILES.filter((rel) => !existsSync(resolve(root, rel)));
}

/**
 * Existing codegen files that are missing an export they must provide.
 * @param {string} [root]
 * @returns {string[]}
 */
export function staleGeneratedFiles(root = ROOT) {
  return Object.entries(REQUIRED_CODEGEN_MARKERS)
    .filter(([rel, markers]) => {
      const abs = resolve(root, rel);
      if (!existsSync(abs)) return false;
      const content = readFileSync(abs, 'utf-8');
      return markers.some((marker) => !content.includes(marker));
    })
    .map(([rel]) => rel);
}

/**
 * @param {string} [root]
 * @param {boolean} [force]
 */
export function shouldGenerate(root = ROOT, force = false) {
  if (force) return true;
  return missingGeneratedFiles(root).length > 0 || staleGeneratedFiles(root).length > 0;
}

/**
 * @param {{
 *   argv?: string[],
 *   root?: string,
 *   log?: (...args: unknown[]) => void,
 *   runResolve?: (args: string) => void,
 * }} [options]
 */
export function runEnsureGeneratedDrivers(options = {}) {
  const argv = options.argv ?? process.argv.slice(2);
  const root = options.root ?? ROOT;
  const log = options.log ?? console.log.bind(console);
  const force = argv.includes('--force');
  const extra = argv.filter((a) => a !== '--force').join(' ');
  const missing = missingGeneratedFiles(root);
  const stale = staleGeneratedFiles(root);

  if (!shouldGenerate(root, force)) {
    log('[ensure-generated] driver codegen files already present; skip');
    return { generated: false, missing: [], stale: [] };
  }

  if (missing.length > 0) {
    log(`[ensure-generated] missing ${missing.join(', ')}; generating`);
  } else if (stale.length > 0) {
    log(`[ensure-generated] stale ${stale.join(', ')} (missing required export); regenerating`);
  } else {
    log('[ensure-generated] --force; regenerating driver codegen files');
  }

  const resolveArgs = `--codegen-only${extra ? ` ${extra}` : ''}`;
  const runResolve =
    options.runResolve ??
    ((args) => {
      execSync(`node scripts/resolve-drivers.mjs ${args}`, {
        cwd: root,
        stdio: 'inherit',
        env: process.env,
      });
      execSync(`node scripts/resolve-pro.mjs --codegen-only`, {
        cwd: root,
        stdio: 'inherit',
        env: process.env,
      });
    });
  runResolve(resolveArgs);
  return { generated: true, missing, stale };
}

function main() {
  runEnsureGeneratedDrivers();
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
