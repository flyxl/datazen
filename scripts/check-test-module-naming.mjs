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
 * The same reader-facing question exists on the TS side, where the repo
 * runs two runners with two disjoint filename conventions, and two more
 * rules pin them:
 *
 *   - Rule 3 — a `*.spec.{ts,tsx,js,jsx,mjs,cjs}` file anywhere in authored
 *     source. The vitest trees (a `__tests__/` directory or a colocated
 *     `*.test.ts`) carry the signal in the SUFFIX; the wdio spec trees carry
 *     it in the DIRECTORY. `.spec` belongs to neither convention, so a
 *     reader cannot tell which runner — if any — picks the file up. This is
 *     the class that produced `e2e/specs/wapps.spec.ts` (renamed to
 *     `wapps.ts`, the only `.spec` file the repository ever had).
 *   - Rule 4 — a wdio spec-tree file whose name carries a foreign runner's
 *     signal: a `test_` prefix (pytest) or a `.test.ts` suffix (vitest).
 *     Inside the wdio spec trees the directory is the test signal — like
 *     cargo's `tests/` for Rule 2 — and every sibling spec is a bare
 *     kebab-case name; a mixed signal misleads about which runner executes
 *     the file. This is the class that produced
 *     `e2e/specs/test_tester-session-fresh.ts` (renamed to
 *     `tester-session-fresh.ts`, the only prefixed name the tree ever had).
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
 * Scan scope: the Rust rules walk the workspace crates; the TS rules walk
 * every authored TS/JS file in the repository (pruned by the shared skip
 * sets — `node_modules`, `dist`, vendored subtrees — the same scoping the
 * other text guards use via scripts/lib/scanTargets.mjs). The single source
 * of truth for the Rust crate set is the `[workspace] members` list in the
 * root Cargo.toml — mirrored here (fixed members spelled out, the
 * `packages/drivers/*` glob expanded minus the workspace `exclude` entries)
 * because this guard is a pure text scanner that must not depend on a cargo
 * toolchain: the frontend CI job runs it, and that job has no Rust toolchain
 * by design (see the platform-arch comment in .github/workflows/ci.yml for
 * why cargo-dependent guards cannot live there). Consequence: adding a
 * workspace member means updating this list too; a mirrored directory that
 * has vanished is an error (exit 2), not a silently empty scan. The wdio
 * spec trees are mirrored for the same reason — resolving
 * `e2e/wdio.conf.ts`'s imports from a text scanner would couple the guard
 * to a TypeScript toolchain it must not have: the trees are `e2e/specs`
 * (the main conf's `specs` glob) plus each non-excluded
 * `packages/drivers/<driver>/e2e/` that exists (a driver without an `e2e/`
 * directory is simply not a spec tree), and a vanished `e2e/specs` is exit 2.
 *
 * `e2e/` holds TS specs today; if it ever gains Rust sources it joins the
 * Rust scan set — the check is dynamic, so Rust e2e helpers cannot silently
 * escape the guard.
 *
 * The allow-list (see below) is empty: the guard shipped inside the same
 * PR that renamed or regated all of its legacy findings, so nothing is
 * grandfathered. The mechanism remains for future justified exceptions
 * (pattern: check-id-terminology.mjs): one reason per entry, and a stale
 * entry that matches nothing fails with exit 2, so the allow-list cannot
 * silently outlive the module it exempts.
 *
 * Usage: node scripts/check-test-module-naming.mjs [repoRoot]
 * The optional argument overrides the repository root (default: this
 * script's parent directory) and exists for validating other checkouts —
 * e.g. running the guard against a pre-fix snapshot to prove it catches the
 * violation class it was written for. Exits 0 clean, 1 violations, 2 guard
 * misconfiguration (missing scan root / stale allow-list entry).
 */
import { existsSync, readdirSync } from 'fs';
import { basename, dirname, extname, join, relative, resolve } from 'path';
import {
  SCAN_EXTENSIONS,
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

/** The main wdio conf's spec glob root — the primary Rule 4 tree (mirrored,
 * not discovered: see the header's scan-scope paragraph). */
const WDIO_SPEC_TREE_ROOT = 'e2e/specs';
/** A `*.spec.*` name — a suffix from a convention this repository does not
 * have (its suffix standard is `.test.*`, and its wdio trees use bare
 * names). */
const SPEC_SUFFIX_RE = /\.spec\.(ts|tsx|js|jsx|mjs|cjs)$/;
/** A `test_` prefix — pytest's convention, foreign to every runner here. */
const SPEC_TREE_TEST_PREFIX_RE = /^test_/;
/** A `.test.ts` suffix — vitest's convention; inside a wdio spec tree it
 * misleads about which runner executes the file. */
const SPEC_TREE_TEST_SUFFIX_RE = /\.test\.ts$/;

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
 * Allow-list: justified exceptions. Empty since the PR that introduced this
 * guard: it shipped with 36 legacy findings enumerated here (15 shared
 * scaffolding files + 21 pure-test modules) so it could go green on day one
 * without freezing legacy names, and the same PR renamed or regated every
 * one of them and deleted the entries. The mechanism stays: an entry is
 * enumerated debt with a stated reason and a removal plan, MUST be deleted
 * in the same change that renames, regates or removes its module, and a
 * stale entry fails with exit 2 instead of rotting silently.
 */
const ALLOWLIST = [];

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

/** Recursively collect TS/JS source files (SCAN_EXTENSIONS) under one
 * root directory — the Rule 3 walk starts at the repository root itself,
 * so it sees exactly the authored source the other text guards see. */
function walkTs(dir, root, out) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return; // vanished between listing and walk — not part of this tree
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      const relDir = relative(root, full).split('\\').join('/');
      if (isSkippedPath(relDir)) continue;
      if (!SKIP_DIR_NAMES.has(entry.name)) walkTs(full, root, out);
      continue;
    }
    if (SCAN_EXTENSIONS.has(extname(entry.name))) out.push(full);
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

  // ── TS scan set (Rules 3–4) ────────────────────────────────────────────
  // Rule 3 looks at every authored TS/JS file; Rule 4 carves the wdio spec
  // trees out of that same walk by path prefix, so the two rules can never
  // disagree about which files exist.
  const tsFiles = [];
  walkTs(root, root, tsFiles);
  const sortedTs = tsFiles.map((f) => toRel(root, f)).sort();

  // The spec trees are mirrored, not discovered (see header). A vanished
  // `e2e/specs` would silently shrink Rule 4's scan to nothing while the
  // guard kept printing green — so it is exit 2, like a vanished member.
  if (!existsSync(resolve(root, WDIO_SPEC_TREE_ROOT))) {
    error(
      `[check-test-module-naming] wdio spec-tree root missing: ${WDIO_SPEC_TREE_ROOT}` +
        ` — the mirror of e2e/wdio.conf.ts's \`specs\` glob; update the mirror when the e2e layout changes`,
    );
    return 2;
  }
  const specTreePrefixes = [WDIO_SPEC_TREE_ROOT];
  for (const entry of readdirSync(driversFull, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const rel = `${DRIVERS_DIR}/${entry.name}`;
    if (WORKSPACE_DRIVER_EXCLUDES.has(rel)) continue; // workspace exclude
    if (existsSync(join(driversFull, entry.name, 'e2e'))) {
      specTreePrefixes.push(`${rel}/e2e`);
    }
  }

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
  /** @type {{rule: 1|2|3|4, file: string, text: string, reason: string}[]} */
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

  // ── Rule 3: *.spec.* is nobody's convention here ──────────────────────
  for (const rel of sortedTs) {
    if (!SPEC_SUFFIX_RE.test(basename(rel))) continue;
    findings.push({
      rule: 3,
      file: rel,
      text:
        'test file named *.spec.* — the repo knows .test.* (vitest trees)' +
        ' and bare names (wdio spec trees); .spec is neither',
      reason:
        'Rule 3: rename to the convention of the tree it lives in —' +
        ' *.test.ts for a vitest tree, a bare name inside a wdio spec tree',
    });
  }

  // ── Rule 4: wdio spec-tree filenames stay bare ────────────────────────
  // The directory is the test signal (cargo's tests/ analogue), so a name
  // that ALSO carries a runner signal is a mixed convention: it tells the
  // reader the wrong runner, or that the file needs one to be found.
  for (const rel of sortedTs) {
    if (!rel.endsWith('.ts')) continue; // the wdio globs collect .ts files
    if (!specTreePrefixes.some((p) => rel.startsWith(`${p}/`))) continue;
    const name = basename(rel);
    if (!SPEC_TREE_TEST_PREFIX_RE.test(name) && !SPEC_TREE_TEST_SUFFIX_RE.test(name)) {
      continue;
    }
    findings.push({
      rule: 4,
      file: rel,
      text:
        'wdio spec file carries a foreign runner signal in its name' +
        ' (test_ prefix / .test.ts suffix) — the directory already signals test',
      reason:
        'Rule 4: inside e2e/specs and packages/drivers/*/e2e every sibling spec is' +
        ' a bare name — drop the prefix/suffix',
    });
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
      ` ${gatedModCount} cfg(test)-gated modules; ${sortedTs.length} TS files scanned,` +
      ` ${specTreePrefixes.length} wdio spec trees)`,
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
