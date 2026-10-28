#!/usr/bin/env node
/**
 * Driver ↔ host import boundary guard (Wave 4, track `import-guard`).
 *
 * Waves 1–3 cleared every reference from driver frontends back into host code
 * (`docs/development/driver-api-dependency-boundary.md` §2.1). That kind of
 * decoupling is trivially re-introduced by one autocomplete keystroke in a new
 * MR, so this script turns the written contract into a build-time gate.
 *
 * Rules (see §2.1.2 / §2.4.2 / §2.2 of the contract):
 *
 *   R1  Driver packages must not reference host `src/**`.
 *       Scans **every string-literal specifier** in `packages/drivers/**`
 *       (`.ts/.tsx/.js/.jsx/.mjs/.cjs`) — plain `import`, `export … from`,
 *       dynamic `import()`, `vi.mock()` / `vi.doMock()`, `require()` and any
 *       helper that takes a module path all end up as string literals, so
 *       nothing depends on the keyword in front of them. Matching only
 *       `from '…'` — the way the earlier `grep -rn "from '\.\./.*src/"`
 *       baseline was collected — silently misses the mock/require shapes
 *       (that is exactly how 8 `vi.mock('<rel>/src/hooks/useI18n')` violations
 *       stayed invisible during Wave 2). A literal is a violation when it is
 *       relative and — once resolved against the importing file — lands inside
 *       the host `src/` directory. Bare package specifiers are the sanctioned
 *       surface and are left to the bundler/TypeScript.
 *
 *   R2  Only the host may call `setLocale()` (§2.4.2).
 *       Scans `packages/**` — except `packages/ui/src/i18n.ts`, which owns the
 *       single i18n runtime (§2.4.1) and its own unit test, which has to call
 *       it (see `R2_FILE_CARVEOUTS`). Comment text, string literals and
 *       type/interface member declarations (`setLocale(locale: string): void;`)
 *       are not calls and never count.
 *
 *   R3  Host code must not reach into driver internals (symmetry, §2.2 row 4).
 *       Scans `src/**` for relative specifiers resolving into
 *       `packages/drivers/**`, skipping the gitignored codegen registries
 *       `src/extensions/generated*.ts` — importing drivers there is the
 *       sanctioned mechanism.
 *       **Advisory (non-blocking), coordinator ruling recorded**: the four
 *       host test-harness findings were ruled legitimate and moved to
 *       `ALLOWLIST` (vitest setupFile + the runtime registration path those
 *       suites assert). One finding remains and is DELIBERATELY not exempted:
 *       `src/windows/connection/DocumentConnectionView.tsx` imports six symbols
 *       from `packages/drivers/mongodb/ui/mongodbFind`, a real host→driver
 *       internals break. `blocking` stays `false` because fixing it is a
 *       feature-sized refactor (route the document view through the driver SDK /
 *       generated registry), and flipping it now would hand the team a gate that
 *       is red on every PR until that refactor lands. Flip `RULES.R3.blocking`
 *       to `true` in the same commit that moves those imports behind the SDK.
 *
 *   R4  The shared design system must stay host-free and runtime-free.
 *       Scans **every string literal** in `packages/ui/**`: no
 *       `@tauri-apps/*` plugin, no `zustand`, and no relative climb into the
 *       host `src/**`, a driver package or a sibling DataZen package.
 *       `@datazen/ui` is bundled by the host, by every driver and by every
 *       extension, several of which run without a Tauri webview — a host
 *       runtime imported from a primitive breaks all of them at once, and the
 *       existing rules could not see it: R1 only covers `packages/drivers/**`,
 *       R2 only looks for `setLocale()`, R3 only looks at `src/**`. This rule
 *       is what the `PathInput` → `@tauri-apps/plugin-dialog` import tripped.
 *
 * Exemptions live in `ALLOWLIST` below: exact `(rule, file, specifier)` triples
 * with a reason and the milestone that owns them — no directory or glob
 * wildcards. Entries that stop matching (file gone, or violation fixed) are
 * reported as expired exemptions so the list cannot silently rot.
 *
 * Blocking scope (BUG-008 ruling): a rule with `blocking: true` only fails the
 * gate for **source this repository tracks**. A local full checkout may carry
 * gitignored external trees — git-driver clones under `packages/drivers/<id>/`
 * (`.gitignore` `/packages/drivers/*` + per-builtin un-ignores) and Pro EPs
 * under `packages/pro-extensions/` (each its own git repo). Those are not this
 * repo's code, so findings there are downgraded to **advisory** (still listed
 * file:line, counted, but exit code stays 0) and flagged as external drift to
 * be fixed in that repository — never absorbed into `ALLOWLIST`. Classification
 * runs `git check-ignore` **per violating file only** (single-digit calls),
 * never on the walk hot path. If git is unavailable or errors, the file is
 * treated as tracked (fail-safe: the gate stays strict).
 *
 * Exit codes: 0 clean (external-tree advisories do not count) · 1 blocking
 * violation(s) or expired exemption(s) · 2 the guard could not scan anything
 * (wrong repo root / broken checkout).
 */
import { execFileSync } from 'child_process';
import { readdirSync, existsSync, readFileSync } from 'fs';
import { dirname, join, posix, relative, resolve, sep } from 'path';
import { fileURLToPath } from 'url';
import { scanCode } from './lib/scanSourceCode.mjs';
import {
  SCAN_EXTENSIONS,
  SKIP_DIR_NAMES,
  isSkippedPath,
  readScannedIfPresent,
} from './lib/scanTargets.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Gitignored codegen. `src/extensions/generated*.ts` is the *sanctioned* place
 * where host code imports driver entry modules (contract §2.2 row 4), so it is
 * out of R3's scope — it is regenerated by `scripts/resolve-drivers.mjs`.
 */
export const SKIPPED_CODEGEN_FILES = new Set([
  'src/extensions/generated.ts',
  'src/extensions/generated-locales.ts',
  'src/extensions/generated-pro.ts',
]);

export const HOST_SRC_DIR = 'src';
export const DRIVER_DIR = 'packages/drivers';
/**
 * Shared design system. Every driver, extension and the host itself bundle
 * `@datazen/ui`, so a host-only runtime that leaks in here breaks *all*
 * consumers at once (and the ones that run without a Tauri webview break
 * hardest). R4 is what keeps that package a leaf: it may depend on React and
 * on itself, on nothing else that the host owns.
 */
export const UI_DIR = 'packages/ui';
/**
 * Bare specifier prefixes that only make sense inside a Tauri webview, against
 * a host-owned store, or above the design system in the dependency graph. They
 * are matched as prefixes (not exact names) so a newly published
 * `@tauri-apps/plugin-*` is covered the day it appears.
 *
 * Kept in sync with the `packages/ui` entry of `LAYER_RULES` in
 * `check-module-layers.mjs`. "Kept in sync" is a statement about these two
 * lists, not a safety net: the two guards are deliberately redundant, and
 * neither is a fallback for the other. If R4 is deleted from this script, this
 * one goes quiet while the other keeps working, and vice versa — the protection
 * against that is the mutation tests in `scripts/__tests__/`, not the other
 * script. What the two guards *do* share is the scan-target set
 * (`scripts/lib/scanTargets.mjs`) and the tokenizer, so they cannot drift on
 * *which files* or *how* they are read — only on rule logic, which the tests
 * cover.
 */
export const R4_FORBIDDEN_PACKAGES = [
  '@tauri-apps/',
  'zustand',
  '@datazen/driver-sdk',
  '@datazen/wapp-sdk',
  '@datazen/extension-points',
];
/**
 * Owner package of the single i18n runtime (§2.4.1). R2 exempts **only** the
 * definition file and that file's own unit test — never the rest of the
 * package, so a `setLocale()` call sneaking into another `@datazen/ui` component
 * still fails the gate.
 */
export const I18N_OWNER_DIR = 'packages/ui';
export const R2_FILE_CARVEOUTS = new Set([
  'packages/ui/src/i18n.ts',
  'packages/ui/src/__tests__/i18n.test.tsx',
]);

/**
 * R6 transition ledger (form-split F2): production `src/` files that still
 * import `@tauri-apps/*` **outside** the desktop zones. Every entry is a
 * data-plane invoke wrapper or a chrome touchpoint on the F4 facade-migration
 * list (plan §4.3 "~23 只碰 invoke" + §4.2 stragglers). Findings in these
 * files are downgraded to advisory so the gate goes live WITHOUT handing the
 * team a permanently red CI — the same governance pattern as R3's
 * DocumentConnectionView ledger. Entries are removed as F4 migrates each
 * file onto the backend-client facade; the set itself is deleted when the
 * last entry goes (F6's `packages/frontend` must be host-runtime-free).
 */
export const R6_LEGACY_TAURI_FILES = new Set([
  'src/commands/ai.ts',
  'src/commands/artifact.ts',
  'src/commands/backup.ts',
  'src/commands/connection.ts',
  'src/commands/context.ts',
  'src/commands/dashboard.ts',
  'src/commands/database.ts',
  'src/commands/file.ts',
  'src/commands/history.ts',
  'src/commands/query.ts',
  'src/commands/sample.ts',
  'src/commands/schemaDiff.ts',
  'src/commands/sync.ts',
  'src/commands/transfer.ts',
  'src/commands/transferJobs.ts',
  'src/commands/tunnel.ts',
  'src/commands/wapps.ts',
  'src/components/MenuBar.tsx',
  'src/lib/fetchRelationDdl.ts',
  'src/lib/sqlFileExecution.ts',
  'src/lib/surfaceBgCache.ts',
  'src/lib/wappBridge.ts',
  'src/windows/backup/BackupWindow.tsx',
  'src/windows/connection/QueryPanel.tsx',
  'src/windows/data-sync/useDataSyncEndpoints.ts',
  'src/windows/data-sync/useDataSyncProfiles.ts',
  'src/windows/data-transfer/DataTransferWindow.tsx',
  'src/windows/onboarding/BrandSidebar.tsx',
  'src/windows/onboarding/OnboardingWindow.tsx',
  'src/windows/schema-diff/useSchemaDiffEndpoints.ts',
  'src/windows/settings/SettingsContent.tsx',
  'src/windows/workspace/WorkspaceView.tsx',
]);

/**
 * R7 sanctioned bridges (form-split F2): the assembly root plus the six thin
 * re-export shims whose entire purpose is to keep legacy import paths alive
 * until F6. Both lists self-eliminate — main.tsx becomes `apps/desktop/src/`
 * and the shims die with the `src/` → `packages/frontend/` move.
 */
export const R7_SANCTIONED_BRIDGES = new Set([
  'src/main.tsx',
  'src/lib/windowManager.ts',
  'src/lib/crossWindowBus.ts',
  'src/lib/pathPicker.ts',
  'src/lib/updater.ts',
  'src/platform/tauriBackendTransport.ts',
  'src/hooks/usePlatform.ts',
  // The fallback shim keeps legacy `src/platform/tauriBackendTransport` importers
  // (tests + the transitional driver-sdk lazy bind) working until F6.
]);

/**
 * Rule metadata. `blocking: false` keeps a rule visible in the report without
 * failing CI (only R3 qualifies today, see header note).
 */
export const RULES = {
  R1: {
    id: 'R1',
    name: 'driver package must not reference host src/**',
    blocking: true,
    doc: 'docs/development/driver-api-dependency-boundary.md §2.1.2',
  },
  R2: {
    id: 'R2',
    name: 'only the host may call setLocale()',
    blocking: true,
    doc: 'docs/development/driver-api-dependency-boundary.md §2.4.2',
  },
  R3: {
    id: 'R3',
    name: 'host must not import driver internals (codegen aside)',
    blocking: false,
    doc: 'docs/development/driver-api-dependency-boundary.md §2.2',
  },
  R4: {
    id: 'R4',
    name: 'shared design system must stay host-free and runtime-free',
    blocking: true,
    doc: 'docs/architecture/frontend/components.md §9',
  },
  R6: {
    id: 'R6',
    name: 'src outside the desktop zones must not import @tauri-apps/*',
    blocking: true,
    doc: 'docs/development/frontend-form-split-plan.md §6.1 rule 1',
  },
  R7: {
    id: 'R7',
    name: 'src may import platform/desktop entities only via sanctioned bridges',
    blocking: true,
    doc: 'docs/development/frontend-form-split-plan.md §6.1 rule 2',
  },
  R8: {
    id: 'R8',
    name: 'driver packages must not import @tauri-apps/* (events ride the backend-client rail)',
    blocking: false,
    doc: 'docs/development/frontend-form-split-plan.md §6.2 (R4 scope extension, drivers face)',
  },
};

/**
 * Allow-list: the two coordinator-ruled host-integration fixtures in the redis
 * driver UI test. Matched on exact (rule, file, specifier); an entry that stops
 * matching is reported as an expired exemption.
 */
export const ALLOWLIST = [
  {
    rule: 'R1',
    file: 'packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx',
    specifier: '../../../../../src/components/ui/WebContextMenu',
    reason:
      'driver↔host integration fixture: renders the real host WebContextMenuHost to prove showNativeContextMenu() lands in the host web menu',
    milestone:
      'Wave 4 import-guard (coordinator ruling); remove when the host menu mount is contributed via an SDK bridge',
  },
  {
    rule: 'R1',
    file: 'packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx',
    specifier: '../../../../../src/stores/contextMenuStore',
    reason:
      'same fixture asserts against the host contextMenuStore (importing it is what triggers bindContextMenuBridge)',
    milestone:
      'Wave 4 import-guard (coordinator ruling); remove together with the WebContextMenuHost exemption above',
  },
  // ── R3 test-harness ruling ────────────────────────────────────────────────
  // The four R3 findings that are NOT DocumentConnectionView.tsx are test
  // scaffolding, not host product code. `src/test/driverUiSetup.ts` is the
  // setupFile of `vitest.drivers.config.ts` (line 24) — it must import driver
  // entry modules for driver UI suites to get real strings, which the file's
  // own header states. `locales.test.ts` imports a driver locale entry to
  // MIRROR what `src/extensions/generated.ts` does at runtime; the host never
  // aggregates driver keys itself. Test-only access to a driver entry is the
  // same sanctioned mechanism R3 already exempts in `SKIPPED_CODEGEN_FILES`.
  // Baseline after this ruling: 1 finding, DocumentConnectionView.tsx (real
  // production break, deliberately NOT exempted — see RULES.R3 doc comment).
  {
    rule: 'R3',
    file: 'src/locales/locales.test.ts',
    specifier: '../../packages/drivers/redis/locales',
    reason:
      'test mirrors the runtime registration path: imports the driver locale entry the way generated.ts loads driver UI meta, to assert the shared registry sees driver packs',
    milestone: 'Wave 4 import-guard (coordinator ruling); remove when the registry contract can be asserted without the driver entry import',
  },
  {
    rule: 'R3',
    file: 'src/locales/locales.test.ts',
    specifier: '../../packages/drivers/mongodb/locales',
    reason: 'same registry-contract assertion as the redis pack above',
    milestone:
      'Wave 4 import-guard (coordinator ruling); remove together with the redis locale-pack exemption',
  },
  {
    rule: 'R3',
    file: 'src/test/driverUiSetup.ts',
    specifier: '../../packages/drivers/redis/ui/shared/meta',
    reason:
      'vitest.drivers.config.ts setupFile (line 24); driver UI suites need the driver entry registered to render with real strings',
    milestone: 'Wave 4 import-guard (coordinator ruling); remove only if the drivers vitest config can register driver entries without host-side imports',
  },
  {
    rule: 'R3',
    file: 'src/test/driverUiSetup.ts',
    specifier: '../../packages/drivers/mongodb/ui/meta',
    reason: 'same vitest.drivers.config.ts setupFile contract as the redis entry above',
    milestone:
      'Wave 4 import-guard (coordinator ruling); remove together with the redis driver-ui entry exemption',
  },
];

/** POSIX-relative repo path for messages and allow-list matching. */
function toPosix(p) {
  return p.split(sep).join('/');
}

/**
 * Resolve a relative specifier against the importing file (repo-relative POSIX
 * paths). Absolute / bare specifiers return `null` — package names are the
 * sanctioned import surface (contract §2.1.1) and are checked by TypeScript.
 */
export function resolveSpecifier(fileRel, specifier) {
  if (!specifier.startsWith('.')) return null;
  return posix.resolve('/', posix.dirname(fileRel), specifier).slice(1);
}

/** Which rules apply to a repo-relative POSIX path. */
export function rulesForFile(rel) {
  const ext = rel.slice(rel.lastIndexOf('.'));
  if (!SCAN_EXTENSIONS.has(ext)) return [];
  if (SKIPPED_CODEGEN_FILES.has(rel)) return [];
  const rules = [];
  if (rel.startsWith(`${DRIVER_DIR}/`)) rules.push(RULES.R1);
  if (rel.startsWith('packages/') && !R2_FILE_CARVEOUTS.has(rel)) rules.push(RULES.R2);
  if (rel.startsWith(`${HOST_SRC_DIR}/`)) rules.push(RULES.R3);
  if (rel.startsWith(`${UI_DIR}/`)) rules.push(RULES.R4);
  // ── Form-split F2 zone rules (plan §6.1) ────────────────────────────────
  // Tests may probe/mock the zones freely (the vitest alias for
  // `@tauri-apps/api/window` is a test asset); production code may not.
  if (isHostProductionFile(rel)) {
    const inPlatformZone = rel.startsWith('src/platform/desktop/');
    const inShellZone = rel.startsWith('src/shell/desktop/');
    if (!inPlatformZone && !inShellZone) {
      // R6 applies everywhere (ledgered files get their findings downgraded to
      // advisory in classification — visibility without a red CI).
      rules.push(RULES.R6);
      if (!R7_SANCTIONED_BRIDGES.has(rel)) rules.push(RULES.R7);
    }
  }
  // Drivers are a shared two-form layer (plan §6.1 rule 3): host runtime
  // imports are forbidden. Advisory until the F4 PubSubPanel fix lands, then
  // flip `RULES.R8.blocking` in the same commit.
  if (rel.startsWith(`${DRIVER_DIR}/`) && !isDriverTestFile(rel)) rules.push(RULES.R8);
  return rules;
}

/** Host production file = under `src/` and not a test scaffold. */
function isHostProductionFile(rel) {
  if (!rel.startsWith('src/')) return false;
  if (rel.includes('/__tests__/')) return false;
  if (rel.startsWith('src/test/')) return false;
  return true;
}

/** Driver e2e/webdriver bridges are test scaffolding, not driver UI. */
function isDriverTestFile(rel) {
  return rel.includes('/e2e/') || rel.includes('/__tests__/');
}

/**
 * Does this specifier reach outside the design system?
 *
 *  - a bare prefix from {@link R4_FORBIDDEN_PACKAGES} (Tauri plugins, zustand);
 *  - or a relative climb out of `packages/ui/` — into the host `src/`, into a
 *    driver package, or into a sibling DataZen package. Sibling climbs are
 *    included on purpose: the design system is the shared *leaf*, so even a
 *    "harmless" `@datazen/driver-sdk` type import would invert the layering
 *    every other package is built on.
 *
 * @param {string} rel repo-relative POSIX path of the importing file
 * @param {string} specifier
 * @returns {string|null} human-readable reason, or null when the specifier is fine
 */
export function uiPurityBreach(rel, specifier) {
  const bare = R4_FORBIDDEN_PACKAGES.find((p) => specifier.startsWith(p));
  if (bare) return `imports the host-only runtime package '${bare}…'`;
  const target = resolveSpecifier(rel, specifier);
  if (!target) return null;
  if (target === HOST_SRC_DIR || target.startsWith(`${HOST_SRC_DIR}/`)) {
    return `reaches into the host ${target}`;
  }
  if (target.startsWith('packages/') && !target.startsWith(`${UI_DIR}/`)) {
    return `reaches outside the design system (${target})`;
  }
  return null;
}

/** Typed parameter list (`name:`) or `function setLocale(` ⇒ declaration, not a call. */
const SETLOCALE_DECLARATION =
  /(?:\bfunction\s+setLocale\s*\()|(?:\bsetLocale\s*\(\s*[A-Za-z_$][\w$]*\s*:)/;
const SETLOCALE_CALL = /\bsetLocale\s*\(/;

/**
 * Find every boundary finding in one file, before allow-list suppression.
 *
 * @param {string} rel repo-relative POSIX path
 * @param {string} source file content
 * @returns {Array<{ rule: object, file: string, line: number, text: string, specifier: string|null, detail: string }>}
 */
export function inspectSource(rel, source) {
  const rules = rulesForFile(rel);
  if (rules.length === 0) return [];
  const { code, literals } = scanCode(source);
  const lines = source.split('\n');
  const findings = [];
  const at = (line) => (lines[line - 1] ?? '').trim();

  for (const rule of rules) {
    if (rule === RULES.R1) {
      for (const { value, line } of literals) {
        // Only *relative* specifiers can climb into the host: bare package
        // names are the sanctioned surface (§2.1.1) and are resolved by the
        // bundler, and a data string such as 'src/assets/x.svg' is not an
        // import (there is no tsconfig `baseUrl` that would make it one).
        const target = resolveSpecifier(rel, value);
        if (!target) continue;
        if (!(target === HOST_SRC_DIR || target.startsWith(`${HOST_SRC_DIR}/`))) continue;
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail: `resolves to host ${target}`,
        });
      }
    }

    if (rule === RULES.R2) {
      code.split('\n').forEach((codeLine, idx) => {
        if (!SETLOCALE_CALL.test(codeLine)) return;
        if (SETLOCALE_DECLARATION.test(codeLine)) return;
        findings.push({
          rule,
          file: rel,
          line: idx + 1,
          text: at(idx + 1),
          specifier: null,
          detail: 'setLocale() may only be called from host src/** (src/lib/localeSync.ts)',
        });
      });
    }

    if (rule === RULES.R3) {
      for (const { value, line } of literals) {
        const target = resolveSpecifier(rel, value);
        if (!target || !target.startsWith(`${DRIVER_DIR}/`)) continue;
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail: `reaches into driver internals (${target})`,
        });
      }
    }

    if (rule === RULES.R4) {
      // Same literal surface as R1/R3 — plain, dynamic, `require()` and
      // `vi.mock()` shapes alike — so a Tauri import cannot hide behind the
      // keyword in front of it.
      for (const { value, line } of literals) {
        const detail = uiPurityBreach(rel, value);
        if (!detail) continue;
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail,
        });
      }
    }

    if (rule === RULES.R6) {
      for (const { value, line } of literals) {
        if (!value.startsWith('@tauri-apps/')) continue;
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail: `imports the host runtime '${value}' outside the desktop zones (ruling D2/plan §6.1 rule 1)`,
        });
      }
    }

    if (rule === RULES.R7) {
      for (const { value, line } of literals) {
        const target = resolveSpecifier(rel, value);
        if (!target) continue;
        if (!(target === 'src/platform/desktop' || target.startsWith('src/platform/desktop/'))) {
          continue;
        }
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail: `imports the desktop zone entity ${target} — shared code may only see platform/contracts.ts ports (ruling #7)`,
        });
      }
    }

    if (rule === RULES.R8) {
      for (const { value, line } of literals) {
        if (!value.startsWith('@tauri-apps/')) continue;
        findings.push({
          rule,
          file: rel,
          line,
          text: at(line),
          specifier: value,
          detail: `driver layer imports the host runtime '${value}' — events ride the backend-client rail (ruling D2/plan §6.1 rule 3)`,
        });
      }
    }
  }
  return findings;
}

/**
 * Recursively collect scannable files below `dir` (missing dir ⇒ no files),
 * reading each one as it is found.
 *
 * Reading here rather than after the walk closes the enumerate-then-read
 * window: another process creating and deleting a source file mid-walk used to
 * abort the whole guard with ENOENT — seen on five runs in six, so a red gate
 * could not be told apart from a genuine boundary violation. Not wrapped in
 * try/catch, so a real I/O failure still surfaces at the point of the read.
 */
function walk(dir, root, out) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    // A missing optional dir is legitimate (e.g. a clone without drivers
    // resolved). Anything else — permissions, I/O — is a real fault and must
    // not quietly shrink the scan into a false "clean".
    if (e.code === 'ENOENT') return out;
    throw e;
  }
  for (const entry of entries) {
    if (SKIP_DIR_NAMES.has(entry.name)) continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      // Prune whole subtrees this repository does not own (see
      // `SKIP_PATH_PREFIXES`) before descending, so none of it is ever read.
      if (isSkippedPath(toPosix(relative(root, full)))) continue;
      walk(full, root, out);
      continue;
    }
    if (!entry.isFile()) continue;
    const rel = toPosix(relative(root, full));
    const ext = rel.slice(rel.lastIndexOf('.'));
    if (!SCAN_EXTENSIONS.has(ext)) continue;
    if (SKIPPED_CODEGEN_FILES.has(rel)) continue;
    const content = readScannedIfPresent(full);
    // Deleted between the directory read and this one: not in the tree, so
    // not something this guard can have an opinion about.
    if (content !== null) out.push({ rel, content });
  }
  return out;
}

/**
 * Real (git-tracked-source) file reader used when no virtual tree is given.
 *
 * `listSources` returns content alongside each path so the caller never
 * re-reads by path; `read` exists only for paths named by config (the
 * allowlist), not for the scan itself.
 */
function createFsAdapter(root) {
  return {
    listSources() {
      const out = [];
      for (const dir of new Set([HOST_SRC_DIR, 'packages'])) walk(resolve(root, dir), root, out);
      return out.sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
    },
    has(rel) {
      return existsSync(resolve(root, rel));
    },
    read(rel) {
      return readFileSync(resolve(root, rel), 'utf-8');
    },
  };
}

/** Suffix that makes external-tree downgrades self-explanatory in the report. */
export const EXTERNAL_ADVISORY_NOTE =
  'external (untracked) repo — contract drift to be fixed in that repo, not here';

/**
 * Per-file "is this path git-ignored (i.e. external code this repo does not
 * track)?" predicate backed by `git check-ignore`. Invoked **only** after a
 * blocking-rule violation is found (single-digit call counts), never while
 * walking the tree. `git check-ignore` consults the index first, so tracked
 * files exit non-zero even if a pattern would match them; any git failure
 * (missing git, symlinked path, non-repo) is treated as "tracked" so the gate
 * can never be loosened by a broken environment.
 *
 * @param {string} root repository root the predicate runs in
 * @returns {(rel: string) => boolean}
 */
export function createGitIgnorePredicate(root) {
  const cache = new Map();
  return (rel) => {
    const cached = cache.get(rel);
    if (cached !== undefined) return cached;
    let ignored = false;
    try {
      execFileSync('git', ['check-ignore', '-q', '--', rel], { cwd: root, stdio: 'ignore' });
      ignored = true;
    } catch {
      ignored = false; // exit 1 = not ignored · exit ≥128 = fail-safe to tracked
    }
    cache.set(rel, ignored);
    return ignored;
  };
}

/**
 * @param {{
 *   root?: string,
 *   files?: Record<string, string>,           // virtual file tree (tests)
 *   allowlist?: Array<{ rule: string, file: string, specifier: string }>,
 *   checkExpiredAllowlist?: boolean,
 *   isIgnored?: (rel: string) => boolean,     // tracking-scope override (tests)
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} exit code (see header)
 */
export function checkDriverImportBoundaries(opts = {}) {
  const root = opts.root ?? ROOT;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);
  const allowlist = opts.allowlist ?? ALLOWLIST;
  const checkExpired = opts.checkExpiredAllowlist ?? true;
  // Virtual trees carry no git state: default to "everything tracked" unless a
  // test injects its own predicate. Real scans classify via `git check-ignore`.
  const isIgnored =
    opts.isIgnored ?? (opts.files === undefined ? createGitIgnorePredicate(root) : () => false);

  const adapter =
    opts.files === undefined
      ? createFsAdapter(root)
      : {
          listSources: () =>
            Object.keys(opts.files)
              .sort()
              .map((rel) => ({ rel, content: opts.files[rel] })),
          has: (rel) => Object.prototype.hasOwnProperty.call(opts.files, rel),
          read: (rel) => opts.files[rel],
        };

  const sources = adapter.listSources();
  if (sources.length === 0) {
    error(
      `[check-driver-import-boundaries] no source files scanned below ${HOST_SRC_DIR}/ or packages/ — refusing to report success (root=${root})`,
    );
    return 2;
  }

  const blocked = [];
  const advisory = [];
  const usedEntries = new Set();
  const ledgeredSeen = new Set();
  let allowed = 0;

  for (const { rel, content } of sources) {
    for (const finding of inspectSource(rel, content)) {
      const exempt = allowlist.findIndex(
        (entry, idx) =>
          entry.rule === finding.rule.id &&
          entry.file === finding.file &&
          entry.specifier === finding.specifier,
      );
      if (exempt >= 0) {
        usedEntries.add(exempt);
        allowed += 1;
        continue;
      }
      // R6 transition ledger: F4-owned files stay visible but advisory.
      if (finding.rule.id === 'R6' && R6_LEGACY_TAURI_FILES.has(finding.file)) {
        ledgeredSeen.add(finding.file);
        advisory.push({ ...finding, ledgered: true });
        continue;
      }
      // BUG-008: findings in files this repo does not track (gitignored
      // external trees: git-driver clones, staged Pro EPs) never block the
      // gate — they are reported as advisories to be fixed upstream.
      if (finding.rule.blocking && isIgnored(finding.file)) {
        advisory.push({ ...finding, external: true });
        continue;
      }
      (finding.rule.blocking ? blocked : advisory).push(finding);
    }
  }

  const expired = checkExpired
    ? allowlist
        .map((entry, idx) => ({ entry, idx }))
        .filter(({ idx }) => !usedEntries.has(idx))
        .map(({ entry }) => ({
          fileMissing: !adapter.has(entry.file),
          entry,
        }))
    : [];

  const tag = '[check-driver-import-boundaries]';

  if (allowed > 0) log(`${tag} ${allowed} allow-listed reference(s) skipped`);
  for (const finding of blocked) {
    error(`${tag} ${finding.rule.id} ${finding.file}:${finding.line}: ${finding.detail}`);
    error(`    ${finding.text}`);
  }
  for (const finding of advisory) {
    const note = finding.external
      ? ` · ${EXTERNAL_ADVISORY_NOTE}`
      : finding.ledgered
        ? ' · R6 transition ledger (F4 facade migration burns it down)'
        : '';
    log(
      `${tag} ${finding.rule.id} (advisory) ${finding.file}:${finding.line}: ${finding.detail}${note}`,
    );
  }
  // Stale ledger hygiene: an entry whose file no longer produces findings is
  // dead weight — surface it so the set cannot silently rot (same property the
  // ALLOWLIST expiry check buys).
  for (const ledgerFile of R6_LEGACY_TAURI_FILES) {
    if (ledgeredSeen.has(ledgerFile)) continue;
    if (!adapter.has(ledgerFile)) continue; // deleted file: cleanup hint below
    log(
      `${tag} R6 (ledger) ${ledgerFile}: no @tauri-apps findings left — remove the entry`,
    );
  }
  for (const { entry, fileMissing } of expired) {
    const why = fileMissing
      ? 'the file no longer exists'
      : 'no matching violation was found — the reference has been decoupled';
    error(`${tag} expired exemption: ${entry.rule} ${entry.file} → '${entry.specifier}' (${why})`);
  }

  const scanned = `${sources.length} file(s) scanned`;
  const advisoryNote = advisory.length > 0 ? ` · ${advisory.length} advisory finding(s)` : '';
  if (blocked.length > 0 || expired.length > 0) {
    error(
      `${tag} FAILED: ${blocked.length} violation(s)${expired.length ? ` + ${expired.length} expired exemption(s)` : ''} (${scanned}${advisoryNote})`,
    );
    for (const ruleId of ['R1', 'R2', 'R4', 'R6', 'R7']) {
      if (blocked.some((f) => f.rule.id === ruleId)) error(`    see ${RULES[ruleId].doc}`);
    }
    return 1;
  }
  log(`${tag} ok (${scanned} · 0 blocking violation(s)${advisoryNote})`);
  return 0;
}

/**
 * CLI entry. Kept separate from the module-load guard so it can be unit tested.
 * Supports `--root=<dir>` to run the guard against another checkout.
 *
 * @param {{ argv?: string[], log?: Function, error?: Function }} [opts]
 * @returns {number} exit code
 */
export function runCli(opts = {}) {
  const argv = opts.argv ?? process.argv;
  const rest = { ...opts };
  delete rest.argv;
  const rootArg = argv.find((arg) => arg.startsWith('--root='));
  if (rootArg) rest.root = resolve(rootArg.slice('--root='.length));
  return checkDriverImportBoundaries(rest);
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('check-driver-import-boundaries.mjs')) {
  process.exitCode = runCli();
}
