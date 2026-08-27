#!/usr/bin/env node
/**
 * Ensure gitignored driver codegen files exist.
 *
 * Files:
 *   src/plugins/generated.ts
 *   src/plugins/generated-locales.ts
 *   src-tauri/src/driver_init.rs
 *   src-tauri/capabilities/default.json (merged from default_host.json + plugins)
 *
 * Used by `pnpm install` (prepare) and `pnpm build` so tsc / rust-analyzer /
 * beforeBuildCommand work on a fresh clone without injecting Cargo.toml.
 *
 * If all files exist, skip (keeps the last `tauri:dev --drivers=...`
 * selection). Pass `--force` to regenerate. Driver set follows
 * `--drivers=...` / DATAZEN_DRIVERS / default `basic`.
 */

import { existsSync } from 'fs';
import { execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { FULLY_GENERATED_MANAGED } from './driver-deinject.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

const GENERATED_FILES = [
  ...FULLY_GENERATED_MANAGED,
  'src-tauri/capabilities/default.json',
];

/**
 * @param {string} [root]
 * @returns {string[]}
 */
export function missingGeneratedFiles(root = ROOT) {
  return GENERATED_FILES.filter((rel) => !existsSync(resolve(root, rel)));
}

/**
 * @param {string} [root]
 * @param {boolean} [force]
 */
export function shouldGenerate(root = ROOT, force = false) {
  if (force) return true;
  return missingGeneratedFiles(root).length > 0;
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

  if (!shouldGenerate(root, force)) {
    log('[ensure-generated] driver codegen files already present; skip');
    return { generated: false, missing: [] };
  }

  if (missing.length > 0) {
    log(`[ensure-generated] missing ${missing.join(', ')}; generating`);
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
    });
  runResolve(resolveArgs);
  return { generated: true, missing };
}

function main() {
  runEnsureGeneratedDrivers();
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
