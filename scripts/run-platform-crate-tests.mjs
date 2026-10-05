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
 * ## D2 — `retry: 2` at vitest.config.ts:75 can report a first-run failure as PASS
 *
 * **Registered here, NOT fixed.** The number `D2` is stable: it names this
 * finding wherever it is cited, including in ledger files that do not survive
 * the merge.
 *
 * **Conclusion.** `vitest.config.ts:75` sets `retry: 2` at the Root level of
 * the `test:` block opened at :28 — same indent as `testTimeout` at :63 — and
 * the `include:` list at :76-86 covers, at any depth under
 * `scripts/__tests__/`, every `*.test.{ts,mjs}` file, so this script's own test
 * file is collected under it. (The glob is spelled out in words because writing
 * it verbatim would end this block comment.)
 * Both CI unit steps
 * inherit that setting: `ci.yml:83` runs `pnpm test:unit`, whose script is
 * literally `vitest run`, and `ci.yml:139` runs `pnpm test:unit:driver-set`,
 * which spawns `node_modules/vitest/vitest.mjs run` at
 * `scripts/run-unit-driver-set.mjs:113`; `vitest.config.ts` is the only
 * **git-tracked** vitest config (`git ls-files` matches exactly one; a second
 * one sits on disk at `packages/pro-extensions/sql-editor-pro/vitest.config.ts`
 * but `.gitignore:71` excludes the whole directory and CI never builds it).
 * A test that fails on its first attempt and passes on a
 * retry is reported PASSED and the step exits 0. vitest.config.ts:70-73 states
 * this cost in the config's own words: 「an all-green run is NOT evidence that
 * no intermittent failure occurred… that inference is invalid by
 * construction」.
 *
 * **Why it is not fixed in this change.** Two reasons, both deliberate.
 * First, scope: `vitest.config.ts` is pre-existing and is not among the files
 * this work touches — `scripts/run-platform-crate-tests.mjs` and
 * `scripts/__tests__/run-platform-crate-tests.test.ts` are, and the fix would
 * not be. Second, it is not a free win to remove: the value was chosen for the
 * flake class measured at vitest.config.ts:64-69 — contention that spans a
 * stretch of tests, where one immediate re-run need not land after the
 * contention passes — and `ci.yml:124-125` already commits that number as the
 * absorber for first-real-runner flakiness, explicitly 「neither is a licence to
 * widen either number」. Retiring it needs its own measured decision covering
 * both files, not a drive-by inside an unrelated change.
 *
 * **How to reproduce.** The static half is a read of `vitest.config.ts:75`
 * together with the Root-level nesting described above. The behavioural
 * recipe — give a test in this directory a first-run-only failure and confirm
 * the process still exits 0 — was **not executed** in this change; it is
 * offered as the check to run before acting, not as a measurement reported
 * here.
 *
 * **Impact.** While D2 is open, any conclusion of the form 「the run was green,
 * therefore the bypass is closed」 is unsupported on its own. The bypass proofs
 * in this script's test suite stand as assertions; what D2 removes is the claim
 * that a green exit code by itself certifies those assertions were evaluated
 * and passed on the attempt a reader assumes.
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
 * ## Why the integration binaries also do not need to be listed here
 *
 * A no-selector invocation runs every integration binary. It is unconditional
 * in `buildCargoArgv` — a literal element of the returned array, not guarded by
 * this table — which is the property that makes them safe to leave unnamed.
 *
 * Had this table been the place to close the integration-binary gap, every
 * future `tests/*.rs` would have to be added here too — and the day someone
 * forgets, the test still "exists", it just never compiles in CI, which is the
 * failure mode this section exists to prevent. Cargo already knows the target
 * list; this file must not keep a second copy of it.
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
 * Pure and exported so the argv can be asserted on directly. Up to three
 * invocations: the `--lib` run, a no-selector run that reaches every
 * integration binary, and a single `--release` one for the release extras.
 * Returns an empty list only when there is nothing at all to run, which
 * `runCli` already guards against.
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
    // Unconditional, and for the same reason. `--lib` selects the library's own
    // unit-test target **only**: every `tests/*.rs` integration binary is a
    // separate target that `--lib` silently skips, and `--test <name>` names
    // exactly one of them. A script that reads as "tests the platform core
    // crates" while naming targets one at a time is the same overstatement this
    // file already guards against twice elsewhere ("PASS — N core crate(s)
    // tested" over an incomplete set), so the full target set runs too.
    //
    // The exact argv matters, and the rejected alternatives are recorded here
    // because each one is defensible-looking and wrong:
    //   * **Replacing** the `--lib` invocation with `['test','--tests',…]`.
    //     Measured on this tree, `--tests` is strictly stronger than `--lib`:
    //     same targets, same tests, except that `--tests` also drops the doc
    //     tests. So the swap would not open a hole in the library, it would
    //     trade doc-test coverage for a re-run of targets already covered — and
    //     `--tests` is fine as an *addition*. Either way the `--lib` call above
    //     stays exactly as it was.
    //   * **Replacing** it with a bare no-selector call (i.e. dropping `--lib`
    //     entirely) would cover everything — but it would also delete the
    //     invariant the `--lib` branch is there to hold, and make the debug gate
    //     depend on the extras table staying shaped the way it is today.
    //   * Naming one `--test <name>` per binary would avoid the re-run, but it
    //     puts a target list in this file that `Cargo.toml` owns: a new
    //     `tests/*.rs` would join the crate silently and stay untested. Cargo
    //     already knows the list; the gate should not keep a second one.
    // So: keep `--lib`, keep the extras, **add** a no-selector call. What it
    // repeats from the first is only the two targets that call names, and it
    // emits no Doc-tests block, because `--lib` selects the library's unit-test
    // target and nothing else. The doctests are new coverage here, not a repeat
    // of anything above. That redundancy is still paid for deliberately: it is
    // bounded and compile-shared, while a drifting duplicate target list is not.
    //
    // It also picks up `cm60-bench`'s own unit tests in **debug**, which the
    // `--release` call below runs properly and which is the run that counts —
    // `cm60-bench/main.rs` exits 2 under `debug_assertions` by design. Measured
    // green in debug, so it is recorded rather than worked around: suppressing
    // it would mean putting a `--bin` exclusion into the one call whose whole
    // property is "names no target", and a debug-profile unit test failing later
    // is a real signal.
    ['test', ...names.flatMap((n) => ['-p', n])],
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