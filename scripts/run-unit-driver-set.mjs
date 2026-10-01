#!/usr/bin/env node
/**
 * Run the Host unit suite (vitest) under an explicit driver set.
 *
 * Why this exists: the frontend codegen files (`src/extensions/generated.ts`
 * and friends) are gitignored, so "which driver set am I type-checking and
 * testing" is invisible in CI logs and silently depends on whatever ran
 * last. `pnpm test:unit` therefore only ever exercised `basic`, and a driver
 * form that `basic` does not register (e.g. `sqlserver`) escaped Host unit
 * coverage entirely.
 *
 * Deliberately NOT a pnpm `pre`/`post` hook pair:
 *   - pnpm >= 7 does not run `pre<script>` / `post<script>` unless
 *     `enable-pre-post-scripts=true`, so the `pretest:unit` / `posttest:unit`
 *     entries in package.json are inert today (measured: the driver set
 *     survives `pnpm test:unit` untouched).
 *   - Every `pre<script>` hook pnpm does run would have to force *one* set,
 *     which is exactly the hardcoding this script removes.
 * So the set is resolved here, in-process, and is the single place that
 * decides it.
 *
 * Driver set resolution order: --drivers=<set> flag > DATAZEN_UNIT_DRIVERS >
 * DEFAULT_DRIVER_SET. Default is `all` (all registered path drivers, no git
 * drivers) because that is the widest set CI can afford: `all` resolves to
 * path drivers only, so it needs no deploy key and no network — unlike the
 * `all,kiwi,superset` union typechecked by release.yml.
 *
 * Usage:
 *   node scripts/run-unit-driver-set.mjs
 *   node scripts/run-unit-driver-set.mjs --drivers=postgres,mysql
 *   DATAZEN_UNIT_DRIVERS=all node scripts/run-unit-driver-set.mjs src/foo.test.ts
 *
 * Extra argv is forwarded to vitest verbatim.
 */

import { spawnSync } from 'child_process';
import { join, resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const ROOT = resolve(__dirname, '..');

const RESOLVE_DRIVERS = join('scripts', 'resolve-drivers.mjs');
const VITEST = join('node_modules', 'vitest', 'vitest.mjs');

/**
 * Widest driver set CI can run without credentials. `all` expands to every
 * registered *path* driver only — git drivers (kiwi, superset, olap) are
 * excluded by resolve-drivers, so no clone and no deploy key is required.
 */
export const DEFAULT_DRIVER_SET = 'all';

/**
 * @param {string[]} [argv]
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {{ drivers: string, vitestArgs: string[] }}
 */
export function parseUnitDriverSetArgs(argv = process.argv.slice(2), env = process.env) {
  const vitestArgs = [];
  // An explicitly supplied set (flag or env) is authoritative even when blank,
  // so a typo fails loudly instead of quietly falling back to `basic` and
  // reporting green for the wrong driver set.
  const requested = argv.find((a) => a.startsWith('--drivers='))?.slice('--drivers='.length) ?? env.DATAZEN_UNIT_DRIVERS;
  const drivers = requested === undefined ? DEFAULT_DRIVER_SET : requested.trim();
  for (const arg of argv) {
    if (!arg.startsWith('--drivers=')) vitestArgs.push(arg);
  }
  if (!drivers) {
    throw new Error(
      'Empty driver set. Pass --drivers=<set> or set DATAZEN_UNIT_DRIVERS; ' +
        'use --drivers=basic to fall back to the four core drivers.',
    );
  }
  return { drivers, vitestArgs };
}

/**
 * Order matters: codegen first, so vitest imports the driver set it is about
 * to be judged against. Every command goes through `process.execPath` with
 * `shell: false`, so the plan is byte-identical on Windows, macOS and Linux
 * (no pnpm.cmd re-parse, no shell quoting of the driver set).
 *
 * @param {{ drivers: string, vitestArgs?: string[], root?: string }} options
 * @returns {{ cmd: string, args: string[] }[]}
 */
export function planDriverSetCommands(options) {
  const root = options.root ?? ROOT;
  const vitestArgs = options.vitestArgs ?? [];
  return [
    // `--codegen-only`: this gate needs the frontend driver set only. Skip the
    // full path so Cargo.toml is never touched and no stash/restore is needed
    // (driver-file-stash.mjs owns Cargo.toml / Cargo.lock, not generated.ts).
    { cmd: process.execPath, args: [join(root, RESOLVE_DRIVERS), '--codegen-only', `--drivers=${options.drivers}`] },
    { cmd: process.execPath, args: [join(root, VITEST), 'run', ...vitestArgs] },
  ];
}

/**
 * @param {{ drivers: string, vitestArgs?: string[], root?: string }} options
 * @returns {number} vitest's exit code
 */
export function runUnitDriverSet(options) {
  const root = options.root ?? ROOT;
  const commands = planDriverSetCommands({ ...options, root });
  console.log(`[unit-driver-set] driver set: ${options.drivers}`);
  for (const { cmd, args } of commands) {
    const result = spawnSync(cmd, args, { cwd: root, stdio: 'inherit', shell: false });
    if (result.error) throw result.error;
    if (result.status !== 0) return result.status ?? 1;
  }
  return 0;
}

function main() {
  const { drivers, vitestArgs } = parseUnitDriverSetArgs();
  process.exit(runUnitDriverSet({ drivers, vitestArgs }));
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}