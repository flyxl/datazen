#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * ci-union-typecheck.mjs — typecheck the driver union once per release.
 *
 * release.yml runs this from the `union-typecheck` job, inside a
 * `with-driver-inject --drivers=all,kiwi,superset` boundary. That set is the
 * superset of every variant (all path drivers plus the two git drivers the
 * Akulaku SKU needs), so one `tsc --noEmit` here is a strict superset of the 11
 * per-variant typechecks it replaces — and the variant matrix skips its own via
 * DATAZEN_CI_TYPECHECK_ONCE=1.
 *
 * The check is target independent, so one runner covers all four
 * (platform, target) pairs. Nothing here is compiled: codegen + the compilers
 * need `generated.ts` and the TypeScript sources, not a Rust artifact, so the job
 * needs no Rust toolchain and no `dist/`.
 *
 * **It runs `tsrs` — the same engine the local gate runs** (user decision):
 * one compiler everywhere, so a green local run and a green release run mean
 * the same thing. The GA compiler (`tsc`) is no longer part of the *typecheck*
 * step; it still runs inside `pnpm build` (`tsc --noEmit` is part of that
 * script), so a diagnostic only the GA compiler reports is still caught by the
 * release build jobs. That is deliberate double coverage, not an oversight.
 *
 * `tsrs` has no win32 binary. Where it cannot run, the step **falls back to
 * `tsc` and says so** (never silently) — the gate must always have a compiler
 * behind it.
 *
 * This used to also warm the shared cargo cache (`ci-driver-warmup.mjs`, with a
 * `warm-driver-deps` job ahead of the matrix). That was removed after
 * measurement showed it cost ~21 min of wall clock and saved nothing — see
 * docs/development/ci-test-matrix.md §6.1.
 */

import { spawnSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { probeEngine } from './typecheck.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const DRIVER_FEATURES_FILE = '.driver-features.json';
const TSRS = join('node_modules', 'tsrs', 'bin', 'tsrs');
const TSC = join('node_modules', 'typescript', 'bin', 'tsc');

/**
 * Codegen + typecheck commands run once over the union driver set.
 * `generate-builtin-locales` / `generate-menu-labels` mirror the codegen half
 * of `pnpm build`; `tsc` is invoked through its local entry point so the
 * command is identical on Windows, macOS and Linux (no `pnpm.cmd` shell
 * re-parse).
 */
export function planTypecheckCommands(root = ROOT) {
  return [
    { cmd: process.execPath, args: [join(root, 'scripts', 'generate-builtin-locales.mjs')] },
    { cmd: process.execPath, args: [join(root, 'scripts', 'generate-menu-labels.mjs')] },
  ];
}

/**
 * The compiler step.
 *
 * `tsrs` is the engine the whole pipeline uses (local gate and release gate
 * alike). It resolves a platform binary at launch time, so its availability is
 * a property of the machine, not of the repository: probing it is the only
 * honest way to know. Where it cannot run, the step falls back to the GA
 * compiler and **says so** — a gate must always have a compiler behind it.
 */
export function planEngineCommand(root = ROOT) {
  const tsrs = probeEngine(root, 'tsrs');
  if (tsrs.available) {
    return {
      command: { cmd: process.execPath, args: [join(root, TSRS), '--noEmit'] },
      engine: 'tsrs',
      reason: null,
    };
  }
  const tsc = probeEngine(root, 'tsc');
  if (tsc.available) {
    return {
      command: { cmd: process.execPath, args: [join(root, TSC), '--noEmit'] },
      engine: 'tsc',
      reason: tsrs.reason,
    };
  }
  return { command: null, engine: null, reason: 'neither tsrs nor tsc is installed' };
}

/**
 * The injected driver feature set. Doubles as a precondition: if
 * `with-driver-inject` did not run, the union was never generated and
 * typechecking it would silently cover the wrong (empty) driver set.
 */
export function readDriverFeatures(root = ROOT) {
  const path = join(root, DRIVER_FEATURES_FILE);
  if (!existsSync(path)) {
    throw new Error(
      `missing ${DRIVER_FEATURES_FILE} — run with-driver-inject before the union typecheck`,
    );
  }
  const { features } = JSON.parse(readFileSync(path, 'utf-8'));
  return features ?? [];
}

export function runTypecheck({ root = ROOT, log = console.log } = {}) {
  const features = readDriverFeatures(root);
  log(`[ci-union-typecheck] typechecking ${features.length} injected driver features`);

  for (const { cmd, args } of planTypecheckCommands(root)) {
    const result = spawnSync(cmd, args, { cwd: root, stdio: 'inherit', shell: false });
    if (result.status !== 0) {
      return { status: result.status ?? 1 };
    }
  }

  const engine = planEngineCommand(root);
  if (!engine.command) {
    log(`[ci-union-typecheck] no compiler available: ${engine.reason}`);
    return { status: 1 };
  }
  if (engine.reason) {
    log(`[ci-union-typecheck] tsrs is unavailable here (${engine.reason}) — falling back to ${engine.engine}`);
  }
  const result = spawnSync(engine.command.cmd, engine.command.args, {
    cwd: root,
    stdio: 'inherit',
    shell: false,
  });
  if (result.status !== 0) {
    return { status: result.status ?? 1 };
  }
  return { status: 0 };
}

function main() {
  let result;
  try {
    result = runTypecheck();
  } catch (err) {
    console.error('[ci-union-typecheck]', err instanceof Error ? err.message : err);
    process.exit(1);
  }
  process.exit(result.status);
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
