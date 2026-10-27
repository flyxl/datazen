#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * Run `cargo test` for the platform core crates.
 *
 * Closes the gap at platform-development-plan.md:269 — the shared runtime was
 * shared `datazen` crate was linked by CI but never tested, so the
 * one crate the server will be built on could compile clean and still be wrong.
 *
 * ## Why this is a script and not a list in ci.yml
 *
 * Precedent: ci.yml:96 carries the comment 「驱动集合只写在
 * `run-unit-driver-set.mjs`，不要内联到这里」 for the driver test set. The same
 * argument applies with more force here, because the platform crates are *being
 * written right now* by another track — `modules` is now the single shared
 * business crate. A `-p` list in
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
  'infra',
  'server',
]);

/**
 * Cargo target selectors that must also be tested, per crate, in a profile the
 * default invocation does not build.
 *
 * ## Why this table is empty
 *
 * Its last entry — the CM-60 release run (`--release -p datazen --features
 * execution,jobs,test-harness --bin cm60-bench`) — was removed on 2026-10-27
 * by maintainer decision（CI 不做性能测试），together with the ci.yml step that
 * ran the bench itself and uploaded its artifacts. What that entry added over
 * the selector-free invocation was only a **release-profile re-run** of the
 * bench binary's 53 unit tests:
 *
 * - those tests are synthetic-arithmetic assertions (e.g. `Percentiles::of` on
 *   hand-constructed samples proving the p95 bookkeeping), not wall-clock
 *   measurements — the release profile carries no extra signal for them;
 * - the no-selector invocation below already compiles the bin as a test target
 *   (its `required-features` of execution/jobs/test-harness are all in the
 *   named feature list) and runs those 53 tests in debug.
 *
 * What the entry cost was the single most expensive leg in the rust job: the
 * only place in PR CI that compiled datazen's dev-dependencies in the release
 * profile, with `lto = true`, `codegen-units = 1`, `opt-level = "z"` (the
 * workspace `[profile.release]`) over the whole closure.
 *
 * The mechanism is retained — not deleted — because "a target whose assertions
 * refuse the debug profile" is a structural category that may recur, and the
 * target-not-found failure mode below is what keeps a future table honest.
 *
 * ## Adding an entry
 *
 * A target named here but absent from the workspace makes cargo fail with a
 * target-not-found error, which is the behaviour we want: the table cannot rot
 * into a no-op without turning the gate red. Entries for a crate that was not
 * discovered are skipped instead — no crate, no target.
 */
export const EXTRA_TARGETS = Object.freeze({});

/**
 * Features the selector-free invocation deliberately does NOT name.
 *
 * `default` is handled structurally by `--no-default-features` (see {@link
 * buildCargoArgv}); this table covers the rest. Every entry must stay an
 * **empty cfg switch** — an exclusion is only valid while enabling the feature
 * changes nothing the test build compiles — so {@link buildCargoArgv}
 * verifies, per crate, that a named exclusion which exists in the metadata
 * feature table still maps to an empty array, and aborts otherwise. A feature
 * that grows real dependencies makes the exclusion stale silently otherwise:
 * the tests stay green while the leg quietly stops compiling code
 * `--all-features` used to compile.
 */
export const EXCLUDED_TEST_FEATURES = Object.freeze({
  webdriver:
    'E2E-only fault-injection seam (D5, modules/Cargo.toml: `webdriver = []`). ' +
    'Its gates in modules/src are `cfg(any(test, all(debug_assertions, feature = "webdriver")))`, ' +
    'so every cargo-test build compiles the seam in via cfg(test) with the feature off; ' +
    'the feature itself is only needed by the WDIO E2E binaries ' +
    '(pnpm tauri:build:webdriver), which CI never builds (maintainer decision ' +
    '2026-10-27: CI 不做性能测试、不需要 WebDriver、不跑 E2E). Excluding it lets this ' +
    'leg compile datazen with the exact feature set the datazen_desktop host leg uses, ' +
    'so the ~209k-line lib is compiled once per CI run instead of twice.',
});

/**
 * Feature profiles that must be exercised in addition to each crate's default
 * unit-test invocation. D1's settings slice is optional by design, so a default
 * test alone would compile only the empty module root and miss the real service.
 */
export const FEATURE_TESTS = Object.freeze({
  'datazen': Object.freeze([
    'settings',
    'ai',
    // `connection` exposes DTOs that use execution receipts, while execution
    // itself depends on connection. Test the smallest valid profile instead
    // of pretending that `connection` is a standalone Cargo configuration.
    'connection,execution',
    'named_views',
    'monitor',
    'dashboard',
    'tunnel',
  ]),
});

/**
 * Verify the exclusion table still describes no-op switches, and that every
 * crate carries the feature table the argv needs. Throws (rather than
 * returning a degraded argv) so both this module's direct callers and `runCli`
 * fail closed; `runCli` maps the throw to exit 2.
 *
 * @param {ReadonlyArray<{ name: string, features?: object }>} crates
 */
function assertFeatureAssumptions(crates) {
  for (const crate of crates) {
    if (typeof crate.features !== 'object' || crate.features === null) {
      throw new Error(
        `crate ${crate.name} carries no feature table — buildCargoArgv refuses to guess ` +
          `what to enable. discoverCoreCrates attaches it from cargo metadata; a hand-built ` +
          `crate list must carry it too (see the test suite's fixtures).`,
      );
    }
    for (const [feature, reason] of Object.entries(EXCLUDED_TEST_FEATURES)) {
      const enables = crate.features[feature];
      if (Array.isArray(enables) && enables.length > 0) {
        throw new Error(
          `EXCLUDED_TEST_FEATURES is stale: ${crate.name}'s "${feature}" now enables ` +
            `[${enables.join(', ')}]. The exclusion was only valid while it was an empty cfg ` +
            `switch. Reason recorded at exclusion time: ${reason}`,
        );
      }
    }
  }
}

/**
 * Build the full list of `cargo` invocations for a discovered crate set.
 *
 * Pure and exported so the argv can be asserted on directly. It returns a
 * no-selector run that reaches every target of every crate, and one invocation
 * for each required feature profile. Returns an empty list only when there is
 * nothing at all to run, which `runCli` already guards against.
 *
 * ## Why the no-selector run is `--no-default-features --features <explicit
 * list>` and not `--all-features` (changed 2026-10-27)
 *
 * The list names every feature of every tested crate except `default` and the
 * {@link EXCLUDED_TEST_FEATURES} table. For datazen that is 27 of its 29
 * features, and — this is the point — **exactly the feature set the
 * datazen_desktop host leg compiles datazen with** (ci.yml appends
 * `datazen/test-harness` to the host step for this reason): the host's
 * dependency closure over modules/Cargo.toml plus test-harness equals
 * all-features minus {default, webdriver}. That equality is not a hope — the
 * test suite parses both real manifests and pins it, so a datazen feature
 * added without being wired into the host goes red here instead of silently
 * un-sharing the compile.
 *
 * Before this change the leg was `--all-features`, whose set differs from the
 * host's by exactly those two empty switches — and because a different feature
 * set is a different compilation unit, datazen's ~209k-line lib was compiled
 * **twice** per CI run (once for the host leg, once here) for want of two
 * no-op flags. Naming the list keeps the alignment auditable, and the
 * exclusion table's emptiness check in {@link assertFeatureAssumptions} is
 * what stops it from rotting silently.
 *
 * `default` is excluded structurally (not via the table) because the host
 * consumes datazen through the workspace template's `default-features = false`
 * — `--no-default-features` mirrors that, and naming `default` back in would
 * re-diverge the two sets.
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
 * So: keep the no-selector call selector-free, keep the feature-profile legs,
 * and let EXTRA_TARGETS stay empty until a target that refuses the debug
 * profile earns its way back in.
 *
 * @param {ReadonlyArray<{ name: string, features?: object }>} crates
 * @returns {string[][]}
 */
export function buildCargoArgv(crates) {
  if (crates.length === 0) return [];
  assertFeatureAssumptions(crates);
  const names = crates.map((c) => c.name);
  // Package-qualified (`datazen/ai`, not bare `ai`): tested crates are selected
  // together in one invocation, and a bare name would silently apply to
  // whichever crate defines it — ambiguous the day two tested crates grow a
  // same-named feature. Metadata key order (manifest declaration order) is
  // preserved, so the argv is deterministic and diffable.
  const namedFeatures = [];
  for (const crate of crates) {
    for (const feature of Object.keys(crate.features)) {
      if (feature === 'default') continue;
      if (feature in EXCLUDED_TEST_FEATURES) continue;
      namedFeatures.push(`${crate.name}/${feature}`);
    }
  }
  const featureInvocations = [];
  for (const name of names) {
    for (const features of FEATURE_TESTS[name] ?? []) {
      featureInvocations.push([
        'test',
        '--no-default-features',
        '--features',
        features,
        '--lib',
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
    // Unconditional, and reads no table for its target selection: this is the
    // invocation the gate's "a table edit cannot delete the run" invariant
    // rests on. It reaches lib unit tests, bin unit tests, every tests/*.rs
    // integration binary and the lib doc tests, with no target list
    // maintained on this side — and its feature list is the host-aligned set
    // documented above.
    [
      'test',
      '--no-default-features',
      ...(namedFeatures.length > 0 ? ['--features', namedFeatures.join(',')] : []),
      ...names.flatMap((n) => ['-p', n]),
    ],
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
  // The feature table travels with the crate: buildCargoArgv names features
  // explicitly (see its doc comment) and refuses a crate without one.
  const featuresById = new Map(metadata.packages.map((p) => [p.id, p.features]));
  const found = new Map();
  for (const layerId of TESTED_LAYERS) {
    for (const member of index.members.values()) {
      if (member.layer === layerId)
        found.set(member.name, { ...member, features: featuresById.get(member.id) });
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

  let invocations;
  try {
    invocations = buildCargoArgv(crates);
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }
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
