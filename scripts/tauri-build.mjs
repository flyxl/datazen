#!/usr/bin/env node
/**
 * tauri-build.mjs — unified build wrapper supporting --edition and --drivers.
 *
 * Automatically:
 *  1. Generates builtin locales and menu labels.
 *  2. Resolves selected drivers (--drivers=all|basic|<list> or DATAZEN_DRIVERS).
 *  3. Injects Pro extensions if --edition=pro / --pro (default) or falls back to community.
 *  4. Invokes Tauri build with correct configuration and feature flags.
 *  5. Automatically restores Cargo.toml and generated files on completion.
 *
 * Usage:
 *   pnpm tauri:build                                 # Pro edition, all native drivers (default)
 *   pnpm tauri:build --drivers=basic                 # Pro edition, 4 core drivers (fast)
 *   pnpm tauri:build --edition=community             # Community edition, all drivers
 *   pnpm tauri:build --edition=community --drivers=basic # Community minimal
 *   pnpm tauri:build --edition=pro --drivers=postgres,mysql # Custom drivers with Pro
 *   pnpm tauri:build --target=x86_64-pc-windows-msvc # Pass any target or tauri args
 *
 * Environment variables:
 *   DATAZEN_EDITION="pro" | "community" (defaults to "pro")
 *   DATAZEN_DRIVERS="all" | "basic" | "<comma-list>" (defaults to "all")
 */

import { spawnSync, execSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

export function parseBuildArgs(argv = process.argv.slice(2), env = process.env) {
  let edition = env.DATAZEN_EDITION || 'pro';
  let drivers = env.DATAZEN_DRIVERS || 'all';
  let proPath = env.DATAZEN_PRO_PATH || null;
  let proGit = env.DATAZEN_PRO_GIT || null;

  const extraArgs = [];

  for (const arg of argv) {
    if (arg === '--pro' || arg === '--edition=pro') {
      edition = 'pro';
    } else if (arg === '--community' || arg === '--edition=community') {
      edition = 'community';
    } else if (arg.startsWith('--edition=')) {
      edition = arg.slice('--edition='.length);
    } else if (arg.startsWith('--drivers=')) {
      drivers = arg.slice('--drivers='.length);
    } else if (arg.startsWith('--pro-path=')) {
      proPath = arg.slice('--pro-path='.length);
    } else if (arg.startsWith('--pro-git=')) {
      proGit = arg.slice('--pro-git='.length);
    } else {
      extraArgs.push(arg);
    }
  }

  return { edition, drivers, proPath, proGit, extraArgs };
}

export function buildCommandString({ edition, drivers, proPath, proGit, extraArgs }) {
  const injectFlags = [
    `--drivers=${drivers}`,
    `--edition=${edition}`,
    proPath ? `--pro-path=${proPath}` : null,
    proGit ? `--pro-git=${proGit}` : null,
  ]
    .filter(Boolean)
    .join(' ');

  const ciBuildFlags = [`--edition=${edition}`, ...extraArgs].join(' ');

  return `node scripts/with-driver-inject.mjs ${injectFlags} -- node scripts/ci-tauri-build.mjs ${ciBuildFlags}`.trim();
}

export function runTauriBuild(opts = parseBuildArgs()) {
  console.log(`[tauri:build] configuration: edition="${opts.edition}", drivers="${opts.drivers}"`);

  // 1. Generate builtin locales & menu labels
  execSync('node scripts/generate-builtin-locales.mjs', { cwd: ROOT, stdio: 'inherit' });
  execSync('node scripts/generate-menu-labels.mjs', { cwd: ROOT, stdio: 'inherit' });

  // 2. Delegate to with-driver-inject + ci-tauri-build
  const cmd = buildCommandString(opts);
  console.log(`[tauri:build] executing: ${cmd}`);

  const result = spawnSync(cmd, {
    cwd: ROOT,
    stdio: 'inherit',
    shell: true,
    env: {
      ...process.env,
      DATAZEN_EDITION: opts.edition,
      DATAZEN_DRIVERS: opts.drivers,
    },
  });

  return result.status ?? 1;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
if (isMain) {
  const code = runTauriBuild();
  process.exit(code);
}
