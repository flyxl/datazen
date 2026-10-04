#!/usr/bin/env node
/**
 * Run `cargo test` for the platform core crates.
 *
 * Closes the gap at platform-development-plan.md:269 — `datazen-runtime` was
 * linked by CI (`cargo test --lib -p datazen --lib`) but never tested, so the
 * one crate the server will be built on could compile clean and still be wrong.
 *
 * ## Why this is a script and not a list in ci.yml
 *
 * Precedent: ci.yml:96 carries the comment 「驱动集合只写在
 * `run-unit-driver-set.mjs`，不要内联到这里」 for the driver test set. The same
 * argument applies with more force here, because the platform crates are *being
 * written right now* by another track — `packages/application` and
 * `packages/platform-api` are landing in a parallel branch. A `-p` list in
 * ci.yml would have to be edited the day they merge; a list derived from the
 * same LAYERS table the boundary guard uses is edited never, and cannot drift
 * from the guard's idea of what a core crate is.
 *
 * ## Why an unclassified member stops the run instead of reddening it at the end
 *
 * `lib/cargoWorkspace.mjs` classifies a member by the directory it lives in, and
 * a member matching no layer path is an **error on purpose**: a member this
 * classifier cannot place is a member no rule covers. The sibling gate enforces
 * that by folding `index.unclassified` into its exit code (`errors` at :367-368
 * → `failed = violations.length + errors.length` at :532-534). This script used
 * to print the identical ERROR line and then fall through — the message went to
 * stderr while execution continued and ended in `return 0` (`--dry-run`, :117-120)
 * or in cargo's own code (:128-129). Measured at c3cf61fef on a workspace with
 * one unclassifiable member: the guard exited 1, this runner exited 0 on both
 * the `--dry-run` path and the real `cargo test` path.
 *
 * **The choice here is to abort before testing, not to aggregate into the final
 * code**, for one reason. The crate list this script tests is derived from the
 * same classification, so once a member is unclassifiable that list is known to
 * be incomplete — the unclassified member may itself be a `runtime`-layer crate
 * that belongs in the tested set. Aggregating would still print
 * `PASS — N core crate(s) tested`, a coverage claim the same run has just
 * disproved, and that line is the one a reader skims. Failing before the claim
 * is made cannot mislead anyone.
 *
 * Nothing is lost by stopping: the observable CI contract is identical either
 * way (same stderr, same exit 1), the fix for an unclassified member is a
 * one-line `LAYERS` edit rather than a debugging session, and there is no reason
 * to spend cargo's time proving the rest of the set is green before saying so.
 * `check-platform-crate-boundaries.mjs` already takes the opposite-but-equally-
 * deliberate route — it aggregates, because it has more than one rule to report
 * — and it aborts early on its own input defect (`aborted: no spec`). The rule
 * followed by both is the one that matters: never emit a green result over a
 * workspace the classifier could not fully account for.
 *
 * Placing the abort during discovery, before the dry-run branch, is deliberate:
 * `--dry-run` must not be a bypass, and dry-run exists to show the plan, not to
 * skip the plan's own validity check.
 *
 * @example
 * node scripts/run-platform-crate-tests.mjs --dry-run
 */

import { spawnSync } from 'child_process';

import {
  buildWorkspaceIndex,
  layerById,
  runCargoMetadata,
} from './lib/cargoWorkspace.mjs';

export const PREFIX = '[platform-crate-tests]';

/**
 * Layers whose crates must be unit-tested in CI. A crate appears here by
 * virtue of the directory it lives in, not by its name.
 */
export const TESTED_LAYERS = Object.freeze(['runtime', 'application', 'platform-api', 'server']);

/**
 * Cargo target selectors that must also be tested, per crate, **on top of `--lib`**.
 *
 * ## Why this table exists
 *
 * `cargo test --lib` cannot reach either CM-60 entry point, and that is a
 * structural fact about cargo rather than an oversight anyone can spot by
 * reading a CI log:
 *
 * - `packages/runtime/tests/cm60_pressure_drain.rs` is an **integration** test
 *   target. `--lib` selects the library's unit-test binary and nothing else, so
 *   the whole §11.4 pressure half — the resource-pressure and drain assertions
 *   CM-60 is written as — was never compiled in CI at all.
 * - `packages/runtime/src/bin/cm60-bench/` is a **binary** target. Its unit
 *   tests live in the bin's own test binary, again outside `--lib`, and they
 *   additionally **have to** be built `--release`: `cm60-bench/main.rs` refuses
 *   to run under `debug_assertions` and exits 2, which is a deliberate guard
 *   against a debug-profile benchmark quietly becoming the measurement.
 *
 * A test that CI never compiles is not a test, and the failure mode is silent
 * in the worst way: the green `cargo test --lib` line is exactly the line a
 * reader skims.
 *
 * ## Why `release` is per-entry instead of a global `--release`
 *
 * `--release` is a profile flag for the **whole invocation**, not for the target
 * it happens to sit next to. Folding `['--bin','cm60-bench','--release']` into
 * the shared argv would silently rebuild every core crate's test suite in
 * release, roughly doubling the compile time of the gate this script exists to
 * keep cheap — and the cost would be invisible in the diff, because the flag
 * looks like it belongs to the bin target. Release extras therefore get their
 * own invocation, built by {@link buildCargoArgv}.
 *
 * The actual benchmark **run** (as opposed to its unit tests) is not here:
 * running it is a separate CI step in `.github/workflows/ci.yml`, because it
 * needs `--out target/bench` and a `rustc --version` captured into
 * `DZ_CM60_RUSTC_VERSION`, and because a benchmark that takes ~100 s should be
 * visible in the CI UI as its own step rather than hidden inside a test script.
 *
 * ## Adding an entry
 *
 * A target named here but absent from the workspace makes cargo fail with a
 * target-not-found error, which is the behaviour we want: the table cannot rot
 * into a no-op without turning the gate red. Entries for a crate that was not
 * discovered are skipped instead — no crate, no target.
 */
export const EXTRA_TARGETS = Object.freeze({
  'datazen-runtime': Object.freeze([
    // §11.4 pressure half: resource pressure + drain, deterministic, no timing.
    Object.freeze({ release: false, args: Object.freeze(['--test', 'cm60_pressure_drain']) }),
    // §11.3 latency half: the bench binary's own unit tests, release only.
    Object.freeze({ release: true, args: Object.freeze(['--bin', 'cm60-bench']) }),
  ]),
});

/**
 * Build the full list of `cargo` invocations for a discovered crate set.
 *
 * Pure and exported so the argv can be asserted on directly. Two invocations
 * at most: everything shareable goes into the debug one, and every `release`
 * entry goes into a single `--release` one. Returns an empty list only when
 * there is nothing at all to run, which `runCli` already guards against.
 *
 * @param {ReadonlyArray<{ name: string }>} crates
 * @returns {string[][]}
 */
export function buildCargoArgv(crates) {
  if (crates.length === 0) return [];
  const names = crates.map((c) => c.name);
  const shared = [];
  const releaseEntries = [];
  for (const name of names) {
    for (const entry of EXTRA_TARGETS[name] ?? []) {
      if (entry.release) releaseEntries.push([name, entry.args]);
      else shared.push(...entry.args);
    }
  }
  return [
    // Unconditional: the whole-crate `--lib` run is this script's reason to
    // exist. It must not become conditional on the extras table being non-empty
    // — that coupling is what makes a table edit able to delete the gate.
    ['test', '--lib', ...names.flatMap((n) => ['-p', n]), ...shared],
    ...(releaseEntries.length > 0
      ? [
          [
            'test',
            '--release',
            ...releaseEntries.flatMap(([name, args]) => ['-p', name, ...args]),
          ],
        ]
      : []),
  ];
}

/**
 * Pure with respect to policy: it reports what the shared classifier found and
 * makes no decision about it. `unclassified` is returned, not thrown and not
 * swallowed, because this function is exported — callers that read it must keep
 * seeing the same shape regardless of what `runCli` does with the value.
 *
 * @param {{ root: string, env?: NodeJS.ProcessEnv, requireLayers?: string[], log?: (s: string) => void }} options
 */
export function discoverCoreCrates({ root, env, log = () => {} }) {
  const metadata = runCargoMetadata({ cwd: root, env, log });
  const index = buildWorkspaceIndex(metadata);
  const unclassified = index.unclassified.map((u) => u.reason);
  const found = new Map();
  for (const layerId of TESTED_LAYERS) {
    for (const member of index.members.values()) {
      if (member.layer === layerId) found.set(member.name, member);
    }
  }
  return { crates: [...found.values()], unclassified, index };
}

function parseArgs(argv) {
  const args = { root: process.cwd(), requireLayers: [], dryRun: false };
  for (const arg of argv) {
    if (arg.startsWith('--root=')) args.root = arg.slice('--root='.length);
    else if (arg.startsWith('--require-layers='))
      args.requireLayers = arg
        .slice('--require-layers='.length)
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
    else if (arg === '--dry-run') args.dryRun = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  const fromEnv = process.env.ARCH_GUARD_REQUIRED_LAYERS;
  if (fromEnv) args.requireLayers.push(...fromEnv.split(',').map((s) => s.trim()).filter(Boolean));
  return args;
}

export function runCli({ argv = process.argv.slice(2), env = process.env } = {}) {
  const out = (s) => process.stdout.write(`${s}\n`);
  const err = (s) => process.stderr.write(`${s}\n`);

  let args;
  try {
    args = parseArgs(argv);
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }

  let discovered;
  try {
    discovered = discoverCoreCrates({ root: args.root, env, log: out });
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }
  const { crates, unclassified } = discovered;
  if (unclassified.length > 0) {
    for (const reason of unclassified)
      err(`${PREFIX} ERROR unclassified workspace member: ${reason}`);
    err(
      `${PREFIX} ABORT — ${unclassified.length} member(s) match no layer path, so the crate set ` +
        `below is provably incomplete and testing it would report a coverage claim this run ` +
        `cannot support. Classify them in LAYERS (scripts/lib/cargoWorkspace.mjs) and re-run.`,
    );
    return 1;
  }

  for (const crate of crates)
    out(`${PREFIX} discovered ${crate.name} (${layerById(crate.layer).path}/)`);

  if (crates.length === 0) {
    err(`${PREFIX} no core crate found — nothing to test`);
    return 1;
  }

  const missing = args.requireLayers.filter(
    (layerId) => !crates.some((c) => c.layer === layerId),
  );
  if (missing.length > 0) {
    err(
      `${PREFIX} ERROR required layer(s) have no crate: ${missing.join(', ')}. ` +
        `Add --require-layers only for layers that must exist at this gate.`,
    );
    return 1;
  }

  const invocations = buildCargoArgv(crates);
  for (const argvList of invocations) out(`${PREFIX} cargo ${argvList.join(' ')}`);
  if (args.dryRun) {
    out(
      `${PREFIX} DRY RUN — ${crates.length} crate(s) selected, ` +
        `${invocations.length} cargo invocation(s)`,
    );
    return 0;
  }

  for (const argvList of invocations) {
    const result = spawnSync('cargo', argvList, { cwd: args.root, stdio: 'inherit', env });
    if (result.error) {
      err(`${PREFIX} could not run cargo: ${result.error.message}`);
      return 2;
    }
    const code = result.status ?? 1;
    out(`${PREFIX} ${code === 0 ? 'PASS' : 'FAIL'} — cargo ${argvList.join(' ')}`);
    // Fail fast: a later invocation passing says nothing about an earlier one.
    if (code !== 0) return code;
  }
  out(`${PREFIX} PASS — ${crates.length} core crate(s) tested`);
  return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exitCode = runCli();
}