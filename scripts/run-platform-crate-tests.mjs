#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
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
export const TESTED_LAYERS = Object.freeze([
  'modules',
  'runtime',
  // 机制内核（job / budget / sql_guard / schema_cache / driver_registry）：从 `runtime`
  // 分出后**仍是 core crate**，它不进这张表就会从 CI 的测试集里安静消失
  // ——正是本脚本存在的理由（见文件头的 platform-development-plan.md:269 那条）。
  'kernel',
  'tunnel',
  'application',
  'platform-api',
  'infra',
  'server',
  // The Workspace App kernel used to be the standalone `workspace-apps` layer.
  // It now lives in `feature-core` as the `feature_core::workspace_apps`
  // module, so its unit tests travel with that layer.
  'feature-core',
  'schema-diff',
  'migration-common',
  'data-sync',
  'data-transfer',
]);

/**
 * Cargo target selectors that must also be tested, per crate, in a profile the
 * default invocation does not build.
 *
 * ## Why this table now only holds `release: true` entries
 *
 * `cargo test -p <crate>` with no target selector already builds and runs,
 * per the Cargo Book's target-selection rules for `cargo test`:
 *
 *   - the lib target **as a unit test** (its `test = true` default),
 *   - every bin target as unit tests (`test = true` default),
 *   - every `tests/*.rs` integration binary, and
 *   - the lib's doc tests.
 *
 * A debug-profile extra bolted onto that run can therefore only ever RE-RUN a
 * target the no-selector invocation already covers. The one thing the default
 * invocation structurally cannot reach is a target whose assertions refuse the
 * debug profile:
 *
 * - `packages/runtime/src/bin/cm60-bench/` is a **binary** target whose unit
 *   tests live in the bin's own test binary — that much `cargo test -p …`
 * *does* reach — but `cm60-bench/main.rs` **has to** be built and run
 * `--release`: it exits 2 under `debug_assertions`, which is a deliberate
 * guard against a debug-profile benchmark quietly becoming the measurement.
 *   A profile flag is a property of the whole invocation, so the release run
 *   gets its own invocation built by {@link buildCargoArgv} rather than a flag
 *   folded onto the shared argv.
 *
 * The debug `--test cm60_pressure_drain` entry this table used to carry was
 * removed for the reason above: the §11.4 pressure half is an integration
 * binary, the no-selector call compiles and runs every integration binary
 * unconditionally, and the old placement only guaranteed it executed twice.
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
    // §11.3 latency half: the bench binary's own unit tests, release only.
    Object.freeze({ release: true, args: Object.freeze(['--bin', 'cm60-bench']) }),
  ]),
});

/**
 * Feature profiles that must be exercised in addition to each crate's default
 * unit-test invocation. D1's settings slice is optional by design, so a default
 * test alone would compile only the empty module root and miss the real service.
 */
export const FEATURE_TESTS = Object.freeze({
  'datazen-modules': Object.freeze([
    'settings',
    'ai',
    'connection',
    'named_views',
    // D6 Monitor's domain-only feature and the dashboard/application closure
    // are both exercised; the latter is the real Desktop scheduler build.
    'monitor',
    'dashboard',
  ]),
});

/**
 * Build the full list of `cargo` invocations for a discovered crate set.
 *
 * Pure and exported so the argv can be asserted on directly. It returns a
 * no-selector run that reaches every target of every crate, one invocation for
 * each required feature profile, and a single `--release` one for release extras. Returns an empty list
 * only when there is nothing at all to run, which `runCli` already guards
 * against.
 *
 * ## Why there is no `--lib` (or `--tests`) invocation any more
 *
 * The previous shape was [`--lib` + extras, no-selector, `--release` extras].
 * The first element was a strict subset of the second — `cargo test --lib`
 * selects only the lib's own unit-test target, while the no-selector call
 * builds and runs that same target plus bins, integration binaries and doc
 * tests — so every lib test executed twice per CI run. Measured on the
 * current main, before this change: **2,537 test functions** re-executed
 * across the tested layers, plus the cm60 pressure binary, per run.
 *
 * The redundancy was recorded as deliberate at the time ("bounded and
 * compile-shared"), but the invariant it was defending — "the lib run must
 * not become conditional on the extras table" — is held by the no-selector
 * call itself, which is unconditional and reads no table. What removing the
 * `--lib` call changes in coverage: nothing. What it changes in the gate's
 * resistance to table edits: nothing, for the same reason.
 *
 * The rejected alternatives, recorded because each looks like a tidy-up:
 *   * **`--tests` instead of the no-selector call** would keep every `test =
 *     true` target (the lib included — the Cargo Book's target-selection
 *     section: "--tests … by default this includes the library and binaries
 *     built as unittests, and integration tests") but silently drop the lib's
 *     **doc tests**, which only the no-selector form runs. An earlier
 *     revision of the comment here believed `--tests` "drops the library's
 *     own unit tests"; that was a factual error, and the guard that encoded
 *     it is gone with it.
 *   * **Naming one `--test <name>` per integration binary** would put a
 *     target list in this file that `Cargo.toml` owns: a new `tests/*.rs`
 *     would join the crate silently and stay untested. Cargo already knows
 *     the list; the gate should not keep a second one.
 * So: keep the no-selector call exactly as it was, keep the release extras,
 * and drop the invocation that only re-ran what the second one covers.
 *
 * @param {ReadonlyArray<{ name: string }>} crates
 * @returns {string[][]}
 */
export function buildCargoArgv(crates) {
  if (crates.length === 0) return [];
  const names = crates.map((c) => c.name);
  const featureInvocations = [];
  for (const name of names) {
    for (const features of FEATURE_TESTS[name] ?? []) {
      featureInvocations.push([
        'test',
        '--no-default-features',
        '--features',
        features,
        '-p',
        name,
      ]);
    }
  }
  const releaseEntries = [];
  for (const name of names) {
    for (const entry of EXTRA_TARGETS[name] ?? []) {
      if (entry.release) releaseEntries.push([name, entry.args]);
    }
  }
  return [
    // Unconditional, and reads no table: this is the invocation the gate's
    // "a table edit cannot delete the run" invariant now rests on. It reaches
    // lib unit tests, bin unit tests, every tests/*.rs integration binary and
    // the lib doc tests, with no target list maintained on this side.
    ['test', ...names.flatMap((n) => ['-p', n])],
    ...featureInvocations,
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

if (isDirectInvocation(import.meta.url)) {
  process.exitCode = runCli();
}
