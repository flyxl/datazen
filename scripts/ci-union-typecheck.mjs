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
 * **Both TypeScript 7 engines run here** (user decision): the authoritative
 * `tsc` (7.0.2 GA) *and* the forward-looking `tsrs` (7.1-dev). The local
 * gate (`pnpm typecheck`) runs `tsrs` alone for speed; the release gate is
 * where both get compared against the same union, so a `tsrs`-only diagnostic
 * that the GA compiler disagrees with is caught **before** a release, not after.
 *
 * `tsrs` has no win32 binary, so on a platform where it cannot run the step is
 * **announced and skipped** (never silently) — the authoritative `tsc` has
 * already run by then, so the gate still has a compiler behind it.
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
import { engineBinPath, probeEngine } from './typecheck.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const DRIVER_FEATURES_FILE = '.driver-features.json';
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
    { cmd: process.execPath, args: [join(root, TSC), '--noEmit'] },
  ];
}

/**
 * The forward-looking engine's step, when it can run here.
 *
 * `tsrs` resolves a platform binary at launch time, so its availability is a
 * property of the machine, not of the repository: probing it is the only honest
 * way to know. When it cannot run, the caller **announces** the skip and moves
 * on — the authoritative `tsc` has already run, so the gate is not left
 * without a compiler.
 */
export function planTsrsCommand(root = ROOT) {
  const probe = probeEngine(root, 'tsrs');
  if (!probe.available) {
    return { command: null, reason: probe.reason };
  }
  const bin = engineBinPath(root, 'tsrs');
  return {
    command: { cmd: process.execPath, args: [bin, '--noEmit'] },
    reason: null,
  };
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

  // The forward-looking engine, on the same union. Both compilers have now
  // seen the same sources, which is the point: a diagnostic only one of them
  // reports is exactly what a release gate should catch.
  const tsrs = planTsrsCommand(root);
  if (!tsrs.command) {
    log(`[ci-union-typecheck] SKIPPING tsrs: ${tsrs.reason}`);
    return { status: 0 };
  }
  const tsrsResult = spawnSync(tsrs.command.cmd, tsrs.command.args, {
    cwd: root,
    stdio: 'inherit',
    shell: false,
  });
  if (tsrsResult.status !== 0) {
    return { status: tsrsResult.status ?? 1 };
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
