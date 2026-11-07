/** @vitest-environment node */
/**
 * Unit tests for `scripts/check-test-module-naming.mjs`, the test-module
 * naming guard.
 *
 * The guard has no virtual `files` option — it walks a real directory tree —
 * so every fixture below is a real tree staged under `os.tmpdir()`. Three
 * properties of the guard shape those trees:
 *
 *   - it scans only its hardcoded workspace-member mirror (six fixed member
 *     directories plus `crates/drivers/*`, minus the workspace excludes), so
 *     each fixture pre-creates every member directory and `crates/drivers/`
 *     — a missing one is exit 2, not an empty scan — and every Rust fixture
 *     file must sit under one of those paths or the guard cannot see it;
 *   - the TS rules (3–4) add one more mirrored root: `e2e/specs` must exist
 *     or the run is exit 2, so the skeleton stages it too — a driver
 *     `e2e/` directory is optional and only counts as a spec tree when
 *     present (the driver fixtures rely on that: staging
 *     `crates/drivers/redis/e2e/…` is what bumps the tree count to 2);
 *   - the ok line reports how many files and gated modules the run actually
 *     saw on BOTH scanners, so the clean fixtures assert those numbers: a
 *     "clean" verdict must not be fakeable by a scan that found nothing (the
 *     same stance the panic-policy suite takes for its test-module case).
 *
 * Findings are asserted through the guard's injectable `error` option
 * (deterministic under vitest) rather than by spying on `process.stderr`;
 * exit codes carry most cases, and the error channel is inspected where a
 * finding must name an exact file.
 *
 * Skipped coverage: the stale-allow-list exit-2 path needs a non-empty
 * ALLOWLIST const, which is module-private and empty since the rename batch,
 * so it is not exercisable from this suite — the vanished-member and
 * vanished-spec-tree fixtures cover the exit-2 misconfiguration plumbing.
 */
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { afterAll, describe, expect, it } from 'vitest';
import { checkTestModuleNaming } from '../check-test-module-naming.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/**
 * The guard's hardcoded workspace-member mirror: every fixture must create all
 * of these (plus the `crates/drivers` glob root and the `e2e/specs` spec-tree
 * root) or the run is exit 2.
 */
const WORKSPACE_MEMBER_DIRS = [
  'crates/datazen-desktop',
  'crates/datazen-driver-api',
  'crates/datazen-driver-support',
  'crates/datazen-infra',
  'crates/datazen',
  'crates/datazen-server',
];

/** Fixture roots staged so far; removed once in `afterAll`. */
const fixtureRoots: string[] = [];

/** Stage a real fixture tree: the full member skeleton plus the given files. */
function makeFixtureRoot(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'check-test-module-naming-'));
  fixtureRoots.push(root);
  for (const dir of [...WORKSPACE_MEMBER_DIRS, 'crates/drivers', 'e2e/specs']) {
    mkdirSync(join(root, dir), { recursive: true });
  }
  for (const [rel, text] of Object.entries(files)) {
    const full = join(root, rel);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, text);
  }
  return root;
}

/** Run the guard on a staged root, capturing both output channels. */
function runGuard(root: string): { code: number; out: string; err: string } {
  const out: string[] = [];
  const err: string[] = [];
  const code = checkTestModuleNaming({
    root,
    log: (msg) => out.push(String(msg)),
    error: (msg) => err.push(String(msg)),
  });
  return { code, out: out.join('\n'), err: err.join('\n') };
}

afterAll(() => {
  for (const root of fixtureRoots) rmSync(root, { recursive: true, force: true });
});

describe('Rule 1 — gated module names must signal "test"', () => {
  it('flags a cfg(test)-gated bare-named module and names the misclassified file', () => {
    // The incident class that motivated the guard: `ipc_contract_guards.rs`
    // was 100% test code behind a gate, invisible to triage.
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': '#[cfg(test)]\nmod ipc_contract_guards;\n',
      'crates/datazen/src/ipc_contract_guards.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    // The finding names the misclassified FILE (what a reader misclassifies)…
    expect(result.err).toContain('crates/datazen/src/ipc_contract_guards.rs:1');
    // …and the declaration site — the line a fix would edit.
    expect(result.err).toContain('mod ipc_contract_guards;');
    expect(result.err).toContain('crates/datazen/src/lib.rs:2');
    expect(result.err).toContain('without any "test" signal');
  });

  it('accepts the same module once its name carries the test signal', () => {
    const root = makeFixtureRoot({
      'crates/datazen-desktop/src/lib.rs': '#[cfg(test)]\nmod ipc_contract_guards_tests;\n',
      'crates/datazen-desktop/src/ipc_contract_guards_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    // Both numbers are asserted so a scan that saw nothing cannot fake the pass.
    expect(result.out).toContain(
      'ok (2 Rust files scanned, 1 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });

  it('judges a #[path] redirect by the path basename, not the mod name', () => {
    // The repo's `#[cfg(test)] #[path = "backup_tests.rs"] mod tests;` pattern:
    // the basename carries the signal, the mod name does not have to.
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': '#[cfg(test)]\n#[path = "backup_tests.rs"]\nmod tests;\n',
      'crates/datazen/src/backup_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    expect(result.out).toContain(
      'ok (2 Rust files scanned, 1 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });

  it('falls back to the mod name when #[path] points at a mod.rs', () => {
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': '#[cfg(test)]\n#[path = "helper_suite/mod.rs"]\nmod helpers;\n',
      'crates/datazen/src/helper_suite/mod.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    // A `mod.rs` basename carries no signal, so NAME decides — and `helpers`
    // has none: the violation names the redirected file.
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen/src/helper_suite/mod.rs:1');
    expect(result.err).toContain('#[path = "helper_suite/mod.rs"] mod helpers;');
  });

  it('treats cfg(all(test, feature = "…")) as gated', () => {
    const root = makeFixtureRoot({
      'crates/datazen-server/src/lib.rs':
        '#[cfg(all(test, feature = "slow"))]\nmod plain_helpers;\n',
      'crates/datazen-server/src/plain_helpers.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    // Only a recognized gate can produce this Rule 1 finding: if cfg(all(…))
    // stopped counting as test-gated, the declaration would be skipped.
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen-server/src/plain_helpers.rs:1');
  });

  it('does not treat feature = "integration_tests" / "testing" as a test gate', () => {
    // The bare-token rule: `test` must appear as a word, not inside a feature
    // string. Both spellings would gate under a naive substring match, which
    // would silently ship these test files in the product binary.
    const root = makeFixtureRoot({
      'crates/datazen-driver-api/src/lib.rs': [
        '#[cfg(feature = "integration_tests")]',
        'mod report_tests;',
        '#[cfg(feature = "testing")]',
        'mod audit_tests;',
      ].join('\n'),
      'crates/datazen-driver-api/src/report_tests.rs': 'use super::*;\n',
      'crates/datazen-driver-api/src/audit_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen-driver-api/src/report_tests.rs:1');
    expect(result.err).toContain('crates/datazen-driver-api/src/audit_tests.rs:1');
    expect(result.err).toContain('compiled into the product binary');
  });

  it('treats cfg(any(test, feature = "…")) as gated', () => {
    const root = makeFixtureRoot({
      'crates/datazen-driver-support/src/lib.rs': [
        '#[cfg(any(test, feature = "extra"))]',
        'mod misc_tests;',
      ].join('\n'),
      'crates/datazen-driver-support/src/misc_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    expect(result.out).toContain(
      'ok (2 Rust files scanned, 1 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });

  it('leaves an ungated declaration of an unsignaled name alone', () => {
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': 'mod plain_helpers;\n',
      'crates/datazen/src/plain_helpers.rs': 'pub fn helper() {}\n',
    });
    const result = runGuard(root);
    // Scoping pin: Rule 1 covers gated decls only, so a gateless
    // `mod plain_helpers;` is production code as far as this guard is
    // concerned — nothing to flag.
    expect(result.code).toBe(0);
    expect(result.out).toContain(
      'ok (2 Rust files scanned, 0 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });
});

describe('Rule 2 — test-named files must not reach the product binary', () => {
  it('flags a *_tests.rs declared without a cfg(test) gate', () => {
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': 'mod backup_tests;\n',
      'crates/datazen/src/backup_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen/src/backup_tests.rs:1');
    expect(result.err).toContain('compiled into the product binary');
    // The declaring site is part of the finding: that is where the gate belongs.
    expect(result.err).toContain('crates/datazen/src/lib.rs (mod)');
  });

  it('flags an orphan *_tests.rs that no declaration reaches', () => {
    const root = makeFixtureRoot({
      'crates/datazen-infra/src/floating_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen-infra/src/floating_tests.rs:1');
    expect(result.err).toContain('orphan');
  });

  it('exempts a *_tests.rs under a crate-level tests/ directory', () => {
    // cargo only builds a `tests/` target for `cargo test`: the directory is
    // its own gate, no declaration needed.
    const root = makeFixtureRoot({
      'crates/datazen/tests/foo_tests.rs': 'use datazen_modules::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    expect(result.out).toContain(
      'ok (1 Rust files scanned, 0 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });

  it('accepts include!-reached tests when the gate is direct or inherited', () => {
    // The driver-api pattern: `resource_adapter_output_tests.rs` reaches the
    // compiler through include!. Here `nested_tests.rs` is included by an
    // UNGATED include! — clean only because its including file is itself
    // test-only (transitive gating), and `direct_tests.rs` is gated in place.
    const root = makeFixtureRoot({
      'crates/drivers/redis/src/lib.rs': [
        '#[cfg(test)]',
        'mod suite_tests;',
        '// A second gated channel: textual inclusion straight from the crate root.',
        '#[cfg(test)]',
        'include!("direct_tests.rs");',
      ].join('\n'),
      'crates/drivers/redis/src/suite_tests.rs': [
        '// Gating is inherited from the declaring module tree — not restated here.',
        'include!("nested_tests.rs");',
      ].join('\n'),
      'crates/drivers/redis/src/direct_tests.rs': 'use super::*;\n',
      'crates/drivers/redis/src/nested_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    // If include! were not a declaration channel, nested_tests.rs and
    // direct_tests.rs would be orphans; if gating were not transitive,
    // nested_tests.rs would ship in the product binary.
    expect(result.out).toContain(
      'ok (4 Rust files scanned, 1 cfg(test)-gated modules; 0 TS files scanned, 1 wdio spec trees)',
    );
  });

  it('flags a *_tests.rs whose declaring file sits in an ungated tree', () => {
    // Transitivity must not over-pardon: support.rs is plain production code,
    // so its child test file compiles into the product binary.
    const root = makeFixtureRoot({
      'crates/datazen/src/lib.rs': 'mod support;\n',
      'crates/datazen/src/support.rs': 'mod deep_tests;\n',
      'crates/datazen/src/support/deep_tests.rs': 'use super::*;\n',
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/datazen/src/support/deep_tests.rs:1');
    expect(result.err).toContain('crates/datazen/src/support.rs (mod)');
  });
});

describe('Rule 3 — *.spec.* is nobody’s convention here', () => {
  it('flags a .spec file wherever it appears, in any scanned extension', () => {
    // The incident class: `e2e/specs/wapps.spec.ts` was the only .spec file
    // the repository ever had — a suffix from a convention nobody here uses.
    const root = makeFixtureRoot({
      'packages/frontend/src/commands/wapps.spec.ts': "import { it } from 'vitest';\n",
      'scripts/probe.spec.mjs': "console.log('x');\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('packages/frontend/src/commands/wapps.spec.ts:1');
    expect(result.err).toContain('scripts/probe.spec.mjs:1');
    // The finding must say what the conventions ARE, not just that this is
    // not one of them — that is what makes it actionable.
    expect(result.err).toContain('.test.* (vitest trees)');
    expect(result.err).toContain('bare names (wdio spec trees)');
  });

  it('accepts the same files once they carry their tree’s own signal', () => {
    const root = makeFixtureRoot({
      // vitest tree: the suffix is the signal.
      'packages/frontend/src/commands/wapps.test.ts': "import { it } from 'vitest';\n",
      // wdio spec tree: the directory is the signal, the name stays bare.
      'e2e/specs/wapps.ts': "describe('wapps', () => {});\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    expect(result.out).toContain(
      'ok (0 Rust files scanned, 0 cfg(test)-gated modules; 2 TS files scanned, 1 wdio spec trees)',
    );
  });
});

describe('Rule 4 — wdio spec-tree filenames stay bare', () => {
  it('flags a pytest-style test_ prefix inside e2e/specs (the incident file)', () => {
    // `e2e/specs/test_tester-session-fresh.ts` was the only prefixed name in
    // a 128-file tree of bare kebab-case siblings.
    const root = makeFixtureRoot({
      'e2e/specs/test_tester-session-fresh.ts': "import { expect } from '@wdio/globals';\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('e2e/specs/test_tester-session-fresh.ts:1');
    expect(result.err).toContain('foreign runner signal');
  });

  it('flags a vitest-style .test.ts suffix inside a driver e2e tree', () => {
    const root = makeFixtureRoot({
      'crates/drivers/redis/e2e/sync-wave-one.test.ts': "describe('sync', () => {});\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(1);
    expect(result.err).toContain('crates/drivers/redis/e2e/sync-wave-one.test.ts:1');
  });

  it('keeps bare names in the spec trees clean — the directory is the signal', () => {
    const root = makeFixtureRoot({
      'e2e/specs/tester-session-fresh.ts': "import { expect } from '@wdio/globals';\n",
      'crates/drivers/redis/e2e/redis-topology.ts': "describe('topology', () => {});\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    // Staging the driver e2e/ directory is what bumps the spec-tree count
    // from 1 to 2 — this asserts the driver trees really joined the scan.
    expect(result.out).toContain(
      'ok (0 Rust files scanned, 0 cfg(test)-gated modules; 2 TS files scanned, 2 wdio spec trees)',
    );
  });

  it('does not police .test.ts outside the spec trees — vitest trees own that suffix', () => {
    // e2e/contract is a vitest tree (its config includes
    // `e2e/contract/**/*.test.ts`), not a wdio spec tree: the suffix there is
    // the convention, and Rule 4 must not reach outside its trees.
    const root = makeFixtureRoot({
      'packages/frontend/src/lib/windowCapabilities.test.ts': "import { it } from 'vitest';\n",
      'e2e/contract/__tests__/fixtures.test.ts': "import { it } from 'vitest';\n",
    });
    const result = runGuard(root);
    expect(result.code).toBe(0);
    expect(result.out).toContain('; 2 TS files scanned, 1 wdio spec trees)');
  });
});

describe('guard plumbing', () => {
  it('exits 2 when a mirrored workspace member directory has vanished', () => {
    // All members but `server` exist, so the walk really reaches the sixth
    // entry: the guard must fail loudly rather than scan five sixths of the
    // workspace and call the remainder clean.
    const root = mkdtempSync(join(tmpdir(), 'check-test-module-naming-'));
    fixtureRoots.push(root);
    for (const dir of WORKSPACE_MEMBER_DIRS) {
      if (dir !== 'crates/datazen-server') mkdirSync(join(root, dir), { recursive: true });
    }
    mkdirSync(join(root, 'crates/drivers'), { recursive: true });
    mkdirSync(join(root, 'e2e/specs'), { recursive: true });
    const result = runGuard(root);
    expect(result.code).toBe(2);
    expect(result.err).toContain('workspace member directory missing: crates/datazen-server');
  });

  it('exits 2 when the mirrored wdio spec-tree root has vanished', () => {
    // The workspace skeleton is complete, but `e2e/specs` is gone: Rule 4's
    // scan would silently shrink to the driver trees while the guard kept
    // printing green — the same silent-empty-scan failure mode as a vanished
    // member, and the same exit 2.
    const root = mkdtempSync(join(tmpdir(), 'check-test-module-naming-'));
    fixtureRoots.push(root);
    for (const dir of WORKSPACE_MEMBER_DIRS) {
      mkdirSync(join(root, dir), { recursive: true });
    }
    mkdirSync(join(root, 'crates/drivers'), { recursive: true });
    const result = runGuard(root);
    expect(result.code).toBe(2);
    expect(result.err).toContain('wdio spec-tree root missing: e2e/specs');
  });

  it('passes on the real repository tree and prints the ok line', () => {
    // No probe lock needed: the sibling mutation probes plant .ts files under
    // packages/desktop/src and packages/frontend/src with convention-shaped names
    // (a `test/` directory, vendored node_modules), never a *.spec.* name and
    // never inside a wdio spec tree — no overlap with Rules 3–4, and the Rust
    // rules still only see .rs files.
    const result = runGuard(REPO_ROOT);
    expect(result.err).toBe('');
    expect(result.code).toBe(0);
    // Counts drift as the repo grows, so pin the shape, not the numbers.
    expect(result.out).toMatch(
      /ok \(\d+ Rust files scanned, \d+ cfg\(test\)-gated modules; \d+ TS files scanned, \d+ wdio spec trees\)/,
    );
  });
});
