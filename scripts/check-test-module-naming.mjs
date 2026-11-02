#!/usr/bin/env node
/**
 * Test-module naming guard.
 *
 * The repository's convention is that a standalone Rust test module is named
 * `tests.rs`, `*_tests.rs`, or lives in a `tests/` directory (integration
 * targets, which cargo only builds for `cargo test`). Two confusions break
 * that convention, and this guard fails on both directions:
 *
 *   - Rule 1 — a cfg(test)-gated external `mod NAME;` whose name carries no
 *     "test" signal. Reader-facing misclassification: review effort, grep
 *     scopes and "is this production code?" triage all skip a file that is
 *     in fact 100% test code. This is the class that produced
 *     `ipc_contract_guards.rs` (renamed to `ipc_contract_guards_tests.rs`,
 *     the incident that motivated this guard).
 *   - Rule 2 — the inverse confusion, which is worse: a `tests.rs` /
 *     `*_tests.rs` file that is NOT behind a cfg(test) gate, i.e. test code
 *     compiled into the product binary.
 *
 * What the guard accepts as "signals test":
 *   - Rule 1: the module NAME contains the substring "test" (`tests`,
 *     `*_tests`, `command_test_host`, `testing` all qualify). If the
 *     declaration redirects the module with `#[path = "..."]`, the name rule
 *     applies to the path's basename without `.rs` (the repo's
 *     `#[cfg(test)] #[path = "backup_tests.rs"] mod tests;` pattern passes on
 *     the path, not the mod name); a `mod.rs` basename falls back to NAME.
 *   - Rule 2: the file is inside a `tests/` directory (cargo's integration
 *     target convention is its own gate), or it is reached by a declaration
 *     that is cfg(test)-gated.
 *
 * Gating detection (both rules): walk backwards from the `mod`/`include!`
 * line over contiguous attribute lines (`#[...]`) and `//` comment lines; the
 * declaration is test-gated iff one of those attributes is a cfg group
 * containing the bare token `test` — `#[cfg(test)]` and
 * `#[cfg(all(test, x))]` count, `#[cfg(feature = "integration_tests")]` and
 * `"testing"` do not (the token must be bare, not inside a feature string).
 *
 * Deviations from a naive "look at the attribute above the mod" scan, both
 * made because Rust's gating is transitive and the guard must not cry wolf:
 *   - a declaration inside a file that is itself test-only (reached from a
 *     gated declaration, recursively, or inside a `tests/` dir) does not need
 *     to restate its parent's `#[cfg(test)]`. The guard computes the
 *     transitive test-only set; without this, `app_db_job_org_tests.rs` and
 *     friends — declared gateless inside gated parents — would be false
 *     Rule 2 violations.
 *   - `include!("..._tests.rs")` counts as a declaration channel for Rule 2:
 *     textual inclusion into a test-only file is how
 *     `packages/driver-api/src/resource_adapter_output_tests.rs` reaches the
 *     compiler, and it is exactly as test-only as its including file.
 *
 * Scan scope: the Rust workspace crates. The single source of truth for that
 * set is the `[workspace] members` list in the root Cargo.toml — mirrored
 * here (fixed members spelled out, the `packages/drivers/*` glob expanded
 * minus the workspace `exclude` entries) because this guard is a pure text
 * scanner that must not depend on a cargo toolchain: the frontend CI job runs
 * it, and that job has no Rust toolchain by design (see the platform-arch
 * comment in .github/workflows/ci.yml for why cargo-dependent guards cannot
 * live there). Consequence: adding a workspace member means updating this
 * list too; a mirrored directory that has vanished is an error (exit 2), not
 * a silently empty scan.
 *
 * `e2e/` holds TS specs today; if it ever gains Rust sources it joins the
 * scan set — the check is dynamic, so Rust e2e helpers cannot silently
 * escape the guard.
 *
 * Findings that predate the guard are listed in ALLOWLIST below with one
 * justification each (pattern: check-id-terminology.mjs). A stale entry that
 * matches nothing fails with exit 2, so the allow-list cannot silently
 * outlive the module it exempts.
 *
 * Usage: node scripts/check-test-module-naming.mjs [repoRoot]
 * The optional argument overrides the repository root (default: this
 * script's parent directory) and exists for validating other checkouts —
 * e.g. running the guard against a pre-fix snapshot to prove it catches the
 * violation class it was written for. Exits 0 clean, 1 violations, 2 guard
 * misconfiguration (missing scan root / stale allow-list entry).
 */
import { existsSync, readdirSync } from 'fs';
import { basename, dirname, join, relative, resolve } from 'path';
import {
  SKIP_DIR_NAMES,
  isSkippedPath,
  readScannedIfPresent,
} from './lib/scanTargets.mjs';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Mirror of the `[workspace] members` list in the root Cargo.toml (see the
 * header comment for why it is hardcoded rather than read via cargo).
 */
const WORKSPACE_MEMBER_DIRS = [
  'apps/desktop/src-tauri',
  'packages/driver-api',
  'packages/driver-support',
  'packages/infra',
  'modules',
  'server',
];
// `packages/drivers/*` from the same members list, minus the workspace
// `exclude` entries (`packages/drivers/kiwi`, `packages/drivers/superset`).
const DRIVERS_DIR = 'packages/drivers';
const WORKSPACE_DRIVER_EXCLUDES = new Set([
  'packages/drivers/kiwi',
  'packages/drivers/superset',
]);

/** External module declaration, any visibility, `;`-terminated (inline
 * `mod NAME { ... }` blocks are not declarations of a separate file). */
const MOD_DECL_RE = /^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/;
/** `include!("….rs")` — textual inclusion, declaration-equivalent for Rule 2. */
const INCLUDE_RS_RE = /^\s*include!\s*\(\s*"([^"]+\.rs)"\s*\)\s*;?/;
/** A cfg group: `#[cfg(...)]` up to the first `)`. */
const CFG_GROUP_RE = /#\[\s*cfg\(([^)]*)\)/;
/** Bare `test` token inside a cfg group — rejects "integration_tests" (the
 * `_` before "test" is a word character, so no boundary) and "testing". */
const BARE_TEST_TOKEN_RE = /\btest\b/;
/** `#[path = "..."]` module redirection. */
const PATH_ATTR_RE = /#\[\s*path\s*=\s*"([^"]*)"\s*\]/;
/** Crate/module-root files: a child `mod x;` in these resolves relative to
 * the file's own directory, not a subdirectory named after the file. */
const ROOT_FILE_NAMES = new Set(['mod.rs', 'lib.rs', 'main.rs', 'build.rs']);

/**
 * Allow-list: findings that predate this guard (introduced at f4ae70c, the
 * commit whose review found `ipc_contract_guards.rs`). The guard's job is to
 * keep NEW modules on the convention, not to freeze legacy names in place —
 * every entry is enumerated debt with a stated reason, and every entry MUST
 * be deleted in the same change that renames, regates or removes its module
 * (a stale entry fails with exit 2 instead of rotting silently).
 */
const ALLOWLIST = [
  // ── Rule 1 · shared test scaffolding ──────────────────────────────────
  // These modules EXPORT fixtures/doubles for other test modules; the name
  // describes the role in the test tree, not the word "test". Gating is
  // correct — only the name predates the convention.
  { rule: 1, file: 'apps/desktop/src-tauri/src/commands/data_transfer/plan_scope.rs', why: 'test-only fixture: the desktop plan-registry scope the transfer tests assemble' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/commands/sync/jobs_contract.rs', why: 'shared sync-job test fixture library (pub pair()/gated_pair() helpers)' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/store/history_db/fixtures.rs', why: 'history-db test fixture data (#[path]-redirected from history_db.rs)' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/testing/app_state.rs', why: 'test-harness AppState fixture (src/testing/)' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/testing/resource_provider.rs', why: 'test-harness resource-provider double (src/testing/)' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/testing/mock_ai_provider.rs', why: 'AI provider mock for AI command tests' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/testing/ai_wiremock.rs', why: 'AI wire-level mock server for AI command tests' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/harness/mod.rs', why: 'shared recording-port harness for connection tests' },
  { rule: 1, file: 'modules/src/execution/runtime/gateway/facade_support.rs', why: 'shared fixtures for facade_tests + cancel_event_tests' },
  { rule: 1, file: 'modules/src/execution/runtime/resource/harness.rs', why: 'shared resource test double (recording ports, injectable faults)' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/harness.rs', why: 'shared tunnel test double (recording ports, injectable faults)' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/source_scan.rs', why: 'source-scan primitives shared by the tunnel audit gates' },
  { rule: 1, file: 'modules/src/use_case_contracts.rs', why: 'WR-C3 dual-transport shared test specs (IPC + HTTP legs share the spec)' },
  { rule: 1, file: 'modules/src/migration/data_transfer/sql_structure_fixtures.rs', why: 'SQL structure fixture data for data-transfer tests' },
  { rule: 1, file: 'packages/drivers/redis/src/live_server.rs', why: 'shared live-Redis harness (#[ignore]-gated live tests)' },
  // ── Rule 1 · legacy pure-test modules ─────────────────────────────────
  // Same class as the ipc_contract_guards incident: 100% test code behind a
  // name with no test signal. Renaming them is debt, not part of the guard's
  // introduction; rename when next touched, then delete the entry.
  { rule: 1, file: 'apps/desktop/src-tauri/src/commands/export/live_acceptance.rs', why: 'export live-acceptance tests; legacy name predates the guard' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/commands/sync/jobs_cancel_contract.rs', why: 'sync job-cancellation contract tests; legacy name predates the guard' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/mcp/contract.rs', why: 'MCP external-contract golden snapshot tests; legacy name predates the guard' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/platform/scope_gate.rs', why: 'org-scope shape gate: self-proving scan tests with kill test; legacy name' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/platform/org_literal_gate.rs', why: 'org-id literal gate: self-proving scan tests with kill test; legacy name' },
  { rule: 1, file: 'apps/desktop/src-tauri/src/workflow/live_acceptance.rs', why: 'workflow live-acceptance tests; legacy name predates the guard' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/fake_resource/catalog_guard.rs', why: 'fake-resource catalog guard tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/fake_resource/close_cases.rs', why: 'fake-resource close-order case tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/fake_resource/close_unconfirmed.rs', why: 'fake-resource unconfirmed-close tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/harness/cm73_threads.rs', why: 'CM73 real-thread eviction regression tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/connection/testing/harness/cm74_release_order.rs', why: 'CM74 release-order regression tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/resource/journey.rs', why: 'pool-return / handle-close journey tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/resource/journey_ledger.rs', why: 'resource journey ledger tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/resource/journey_rotation.rs', why: 'resource journey rotation tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/form_neutrality.rs', why: 'kernel form-neutrality gate (scan tests); legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/journey_failure.rs', why: 'tunnel failure-journey tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/journey_return.rs', why: 'tunnel return-journey tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/journey_sharing.rs', why: 'tunnel sharing-journey tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/journey_single_counter.rs', why: 'tunnel single-counter journey tests; legacy name' },
  { rule: 1, file: 'modules/src/execution/runtime/tunnel/single_counter_audit/mod.rs', why: 'one-count iron-law audit gate (dir module with kill_tests); legacy name' },
  { rule: 1, file: 'packages/drivers/redis/src/connect/tls_journey.rs', why: 'real Redis TLS handshake journey tests; legacy name' },
  // ── Rule 2 · known pre-existing finding ───────────────────────────────
  {
    rule: 2,
    file: 'apps/desktop/src-tauri/src/commands/named_views_error_tests.rs',
    why: 'KNOWN pre-existing defect, not a convention exemption: declared in commands/mod.rs with only #[path] and no #[cfg(test)], so this test file is compiled into the product binary. Fix (add the gate) is pending; tracked here so the guard ships green while still catching every new violation of either rule. Delete this entry together with the fix.',
  },
];

/** Recursively collect .rs files under one root directory. */
function walkRust(dir, root, out) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return; // vanished between listing and walk — not part of this tree
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      // Prune subtrees this repository does not own (see SKIP_PATH_PREFIXES)
      // and vendored/generated directories (see SKIP_DIR_NAMES).
      const relDir = relative(root, full).split('\\').join('/');
      if (isSkippedPath(relDir)) continue;
      if (!SKIP_DIR_NAMES.has(entry.name)) walkRust(full, root, out);
      continue;
    }
    if (entry.name.endsWith('.rs')) out.push(full);
  }
}

/** Repo-relative POSIX path of an absolute path. */
function toRel(root, full) {
  return relative(root, full).split('\\').join('/');
}

/** Join and normalize repo-relative POSIX paths (`'a/b'` + `'../c'` ->
 * `'a/c'`). Declaration targets are resolved in repo-relative space so the
 * result is directly comparable against the scan set — resolve() would
 * contaminate it with the process CWD. */
function joinRel(base, relPath) {
  const out = [];
  for (const seg of `${base}/${relPath}`.split('/')) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') {
      out.pop();
      continue;
    }
    out.push(seg);
  }
  return out.join('/');
}

/**
 * Resolve the file an external `mod` declaration in `declFile` points at.
 *
 * Rust's rules, simplified to what this tree uses:
 *   - `#[path = "P"]` is relative to the directory of the declaring file;
 *   - a plain `mod x;` in a module-root file (`mod.rs` / `lib.rs` /
 *     `main.rs` / `build.rs`) resolves to `<dir>/x.rs` or `<dir>/x/mod.rs`;
 *   - a plain `mod x;` in any other file `foo.rs` resolves to
 *     `<dir>/foo/x.rs` or `<dir>/foo/x/mod.rs` (the 2018 subdirectory
 *     convention).
 *
 * A declaration nested inside an inline `mod y { ... }` block (one exists:
 * `traits.rs`'s `mod structure_defaults_tests { mod cases; }`) would need the
 * inline nesting chain to resolve; this resolver returns null for it, which
 * drops the edge instead of misattaching it — `cases.rs` is not test-named,
 * so no rule depends on it today.
 *
 * @param {string} relDeclFile repo-relative path of the declaring file
 * @param {string} name module name from the declaration
 * @param {string|null} pathAttr `#[path]` value, if any
 * @param {Set<string>} relSet all repo-relative .rs paths in the scan set
 * @returns {string|null} repo-relative target path, or null if unknown
 */
function resolveModTarget(relDeclFile, name, pathAttr, relSet) {
  const dir = dirname(relDeclFile);
  if (pathAttr) return joinRel(dir, pathAttr);
  const base = ROOT_FILE_NAMES.has(basename(relDeclFile))
    ? dir
    : `${dir}/${basename(relDeclFile).replace(/\.rs$/, '')}`;
  const asFile = `${base}/${name}.rs`;
  if (relSet.has(asFile)) return asFile;
  const asDir = `${base}/${name}/mod.rs`;
  if (relSet.has(asDir)) return asDir;
  return null;
}

/**
 * @param {{
 *   root?: string,
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} 0 clean, 1 violations, 2 guard misconfiguration
 */
export function checkTestModuleNaming(opts = {}) {
  const root = opts.root ?? ROOT;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  // ── collect the scan set ────────────────────────────────────────────────
  const rustFiles = [];
  for (const rel of WORKSPACE_MEMBER_DIRS) {
    const full = resolve(root, rel);
    if (!existsSync(full)) {
      error(
        `[check-test-module-naming] workspace member directory missing: ${rel}` +
          ` — WORKSPACE_MEMBER_DIRS mirrors the [workspace] members list of` +
          ` the root Cargo.toml; update the mirror when the workspace changes`,
      );
      return 2;
    }
    walkRust(full, root, rustFiles);
  }
  const driversFull = resolve(root, DRIVERS_DIR);
  if (!existsSync(driversFull)) {
    error(`[check-test-module-naming] workspace members glob root missing: ${DRIVERS_DIR}`);
    return 2;
  }
  for (const entry of readdirSync(driversFull, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const rel = `${DRIVERS_DIR}/${entry.name}`;
    if (WORKSPACE_DRIVER_EXCLUDES.has(rel)) continue; // workspace exclude
    walkRust(join(driversFull, entry.name), root, rustFiles);
  }
  // e2e/ joins the scan set only when it actually holds Rust sources (it
  // holds TS specs today; the check is dynamic so Rust e2e helpers cannot
  // silently escape — see header).
  const e2eRust = [];
  walkRust(resolve(root, 'e2e'), root, e2eRust);
  if (e2eRust.length > 0) rustFiles.push(...e2eRust);

  const relSet = new Set(rustFiles.map((f) => toRel(root, f)));
  const sortedRels = [...relSet].sort();

  // ── collect external declarations (mod + include!) ─────────────────────
  /** @type {{file: string, line: number, kind: 'mod'|'include', name: string|null,
   *          pathAttr: string|null, gated: boolean, target: string|null}[]} */
  const decls = [];
  for (const rel of sortedRels) {
    const text = readScannedIfPresent(resolve(root, rel));
    if (text === null) continue; // vanished mid-scan — see readScannedIfPresent
    const lines = text.split('\n');
    lines.forEach((line, i) => {
      const isMod = MOD_DECL_RE.test(line);
      const isInclude = !isMod && INCLUDE_RS_RE.test(line);
      if (!isMod && !isInclude) return;
      // Walk back over the contiguous attribute/comment run that gates this
      // declaration. Doc comments (`///`) are skipped like any other comment:
      // to rustc they are attributes too, so a gate above them still binds to
      // the same item, and order within the run must not matter.
      const attrLines = [];
      for (let j = i - 1; j >= 0; j -= 1) {
        const t = lines[j].trim();
        if (t.startsWith('#[')) {
          attrLines.push(t);
          continue;
        }
        if (t.startsWith('//')) continue;
        break;
      }
      let gated = false;
      for (const attr of attrLines) {
        const cfg = attr.match(CFG_GROUP_RE);
        if (cfg && BARE_TEST_TOKEN_RE.test(cfg[1])) {
          gated = true;
          break;
        }
      }
      if (isMod) {
        const name = line.match(MOD_DECL_RE)[1];
        const pathAttr = (attrLines.map((a) => a.match(PATH_ATTR_RE)).find(Boolean) ?? [])[1] ?? null;
        decls.push({
          file: rel,
          line: i + 1,
          kind: 'mod',
          name,
          pathAttr,
          gated,
          target: resolveModTarget(rel, name, pathAttr, relSet),
        });
      } else {
        const path = line.match(INCLUDE_RS_RE)[1];
        decls.push({
          file: rel,
          line: i + 1,
          kind: 'include',
          name: null,
          pathAttr: path,
          gated,
          target: joinRel(dirname(rel), path),
        });
      }
    });
  }

  // ── transitive test-only set ────────────────────────────────────────────
  // A file is test-only iff it sits in a `tests/` directory (integration
  // target) or is reached by a gated declaration — directly, or from a file
  // that is itself test-only (Rust's gating is transitive; a child module
  // does not restate its parent's #[cfg(test)]).
  const incoming = new Map(); // target rel -> [{from, gated, kind}]
  for (const d of decls) {
    if (!d.target || !relSet.has(d.target)) continue;
    if (!incoming.has(d.target)) incoming.set(d.target, []);
    incoming.get(d.target).push({ from: d.file, gated: d.gated, kind: d.kind });
  }
  const testOnlyMemo = new Map();
  function isTestOnly(rel) {
    if (testOnlyMemo.has(rel)) return testOnlyMemo.get(rel);
    testOnlyMemo.set(rel, false); // cycle guard (include! could loop)
    let result = false;
    if (rel.split('/').slice(0, -1).includes('tests')) {
      result = true; // cargo integration-target directory
    } else {
      for (const edge of incoming.get(rel) ?? []) {
        if (edge.gated || isTestOnly(edge.from)) {
          result = true;
          break;
        }
      }
    }
    testOnlyMemo.set(rel, result);
    return result;
  }

  // ── Rule 1: gated module names must signal "test" ──────────────────────
  /** @type {{rule: 1|2, file: string, text: string, reason: string}[]} */
  const findings = [];
  for (const d of decls) {
    if (d.kind !== 'mod' || !d.gated) continue;
    let effectiveName = d.name;
    if (d.pathAttr) {
      const pathBase = basename(d.pathAttr).replace(/\.rs$/, '');
      effectiveName = pathBase === 'mod' ? d.name : pathBase;
    }
    if (effectiveName.includes('test')) continue;
    // Report the misnamed FILE (that is what a reader misclassifies); the
    // declaration site — the line to edit — goes into the message.
    findings.push({
      rule: 1,
      file: d.target ?? `${d.file}#${d.name}`,
      text:
        `cfg(test)-gated \`${d.pathAttr ? `#[path = "${d.pathAttr}"] ` : ''}mod ${d.name};\`` +
        ` at ${d.file}:${d.line} names its module without any "test" signal`,
      reason:
        'Rule 1: standalone test modules are tests.rs / *_tests.rs (or live in a tests/ dir);' +
        ' rename the module — or its #[path] target — to carry "test"',
    });
  }

  // ── Rule 2: test-named files must not reach the product binary ────────
  for (const rel of sortedRels) {
    const parts = rel.split('/');
    const stem = parts[parts.length - 1].replace(/\.rs$/, '');
    if (stem !== 'tests' && !stem.endsWith('_tests')) continue;
    if (parts.slice(0, -1).includes('tests')) continue; // tests/ dir is its own gate
    const edges = incoming.get(rel) ?? [];
    const gated = edges.some((e) => e.gated || isTestOnly(e.from));
    if (gated) continue;
    if (edges.length === 0) {
      findings.push({
        rule: 2,
        file: rel,
        text: 'test-named file outside a tests/ dir with no mod/include! declaration reaching it (orphan)',
        reason:
          'Rule 2: a tests.rs / *_tests.rs file must be declared behind #[cfg(test)] —' +
          ' an undeclared file is dead weight or an accident',
      });
    } else {
      const sites = edges.map((e) => `${e.from} (${e.kind})`).join(', ');
      findings.push({
        rule: 2,
        file: rel,
        text: `test-named file is compiled into the product binary (declared at ${sites} without a cfg(test) gate)`,
        reason:
          'Rule 2: gate the declaring module with #[cfg(test)] so test code stays out of shipped builds',
      });
    }
  }

  // ── allow-list + staleness ─────────────────────────────────────────────
  const matched = new Set();
  const violations = findings.filter((f) => {
    const hit = ALLOWLIST.findIndex(
      (e) => e.rule === f.rule && e.file === f.file,
    );
    if (hit >= 0) {
      matched.add(hit);
      return false;
    }
    return true;
  });
  const stale = ALLOWLIST.filter((_, i) => !matched.has(i));
  if (stale.length > 0) {
    error(`[check-test-module-naming] ${stale.length} stale allow-list entr(y/ies):`);
    for (const e of stale) {
      error(`  rule ${e.rule} ${e.file} -> ${e.why}`);
    }
    error(
      'An entry that matches no finding means its module was renamed, regated or' +
        ' removed — delete the entry in the same change.',
    );
    return 2;
  }

  const gatedModCount = decls.filter((d) => d.kind === 'mod' && d.gated).length;
  if (violations.length > 0) {
    error(`[check-test-module-naming] ${violations.length} violation(s):`);
    for (const v of violations) {
      error(`  ${v.file}:1: ${v.text}\n    -> ${v.reason}`);
    }
    return 1;
  }
  const skipped = ALLOWLIST.length;
  if (skipped > 0) {
    log(`[check-test-module-naming] ${skipped} allow-listed legacy finding(s) skipped`);
  }
  log(
    `[check-test-module-naming] ok (${sortedRels.length} Rust files scanned,` +
      ` ${gatedModCount} cfg(test)-gated modules)`,
  );
  return 0;
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('check-test-module-naming.mjs')) {
  // Optional argument: repo root override, for validating other checkouts
  // (pre-fix snapshots, worktrees) — see header.
  const rootArg = process.argv[2];
  process.exitCode = checkTestModuleNaming({ root: rootArg ? resolve(rootArg) : undefined });
}
