#!/usr/bin/env node
/**
 * Source-layer guard: each named layer must not import from the layers above it.
 *
 * Two independent layers are policed here:
 *
 *  1. `src/lib/relationMetadata` — the shared relation-metadata layer, which
 *     must not climb back up into the components / stores / windows / hooks
 *     that consume it. (Query Builder is owned by the independent SQL Editor
 *     Pro repository and no longer participates in Host source-layer checks.)
 *  2. `packages/ui` — the `@datazen/ui` design system. It is the shared
 *     **leaf**: the host, every driver and every extension bundle it, so a
 *     host-owned runtime imported from a primitive breaks every consumer at
 *     once. `PathInput` importing `@tauri-apps/plugin-dialog` is exactly that,
 *     and neither the previous single rule nor the driver boundary guard
 *     (`check-driver-import-boundaries.mjs`) could see it.
 *
 * A rule names a source subtree (`from`), the subtrees it must not import from
 * (`forbidden`, matched on the *resolved* repo-relative path) and — because the
 * design system's leaks are almost always bare package specifiers rather than
 * relative climbs — the bare specifier prefixes it must not name
 * (`forbiddenPackages`, matched as prefixes). Both lists are prefix matches, so
 * a new subtree or a new `@tauri-apps/plugin-*` is covered without editing
 * this file.
 *
 * Detection walks **every string literal** in the scanned subtree, not just
 * `from '…'`: plain imports, `export … from`, dynamic `import()`,
 * `require()`, and `vi.mock()` / `vi.doMock()` all end up as literals, and
 * matching a single keyword is how violations stayed invisible. The tokenizer
 * is shared with the driver boundary guard.
 *
 * The set of files that count as source (`SCAN_EXTENSIONS`,
 * `SKIP_DIR_NAMES`) is shared with it as well, via
 * `scripts/lib/scanTargets.mjs`. Declaring the set twice is how the two guards
 * ended up disagreeing about which files exist — one reported
 * `packages/ui/dist/**` while the other ignored it, and only one of them
 * looked at `.mjs` — and a guard that quietly watches a different file set is
 * not a second opinion on the same question.
 */
import { existsSync, readFileSync, readdirSync } from 'fs';
import { resolve, dirname, extname, relative, posix } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { findCodeNeedle, lineAtOffset, scanCode } from './lib/scanSourceCode.mjs';
import {
  SCAN_EXTENSIONS,
  SKIP_DIR_NAMES,
  isSkippedPath,
  readScannedIfPresent,
} from './lib/scanTargets.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Each rule names a source subtree and the subtrees it must not import from.
 * Paths are POSIX-style, relative to the repo root.
 *
 * `blocking: false` marks a rule that is *true but not yet enforceable* — its
 * findings are reported as `ADVISORY` and the gate still exits 0. A guard that
 * fails on today's tree trains people to reach for `--no-verify`; an advisory
 * that is loudly wrong and visibly pending does not.
 *
 * @type {Array<{name: string, from: string, forbidden?: string[], forbiddenPackages?: string[], forbiddenCode?: string[], blocking?: boolean}>}
 */
export const LAYER_RULES = [
  {
    name: 'shared relation metadata must not import its consumers',
    from: 'src/lib/relationMetadata',
    forbidden: ['src/components', 'src/stores', 'src/windows', 'src/hooks'],
  },
  {
    name: 'shared design system (@datazen/ui) must stay a host-free leaf',
    from: 'packages/ui',
    // Host source, driver internals and sibling DataZen packages all sit
    // *above* the design system; reaching into any of them inverts the
    // layering every other package is built on.
    forbidden: [
      'src',
      'packages/drivers',
      'packages/driver-sdk',
      'packages/wapp-sdk',
      'packages/extension-points',
    ],
    // Bare specifiers a Tauri webview (or a host-owned store) is the only
    // place for. Matched as prefixes so a newly published plugin is covered.
    forbiddenPackages: [
      '@tauri-apps/',
      'zustand',
      '@datazen/driver-sdk',
      '@datazen/wapp-sdk',
      '@datazen/extension-points',
    ],
  },
  {
    name: 'backend-client-transport-agnostic',
    from: 'packages/backend-client/src',
    // The whole point of the package is to be the one seam that knows the
    // transport. A `@tauri-apps/` import or a raw network call inside it does
    // not make the dependency illegal — it makes the seam stop being a seam,
    // so every consumer silently inherits a desktop-only client.
    // `@tauri-apps/` is a prefix, never a glob: `forbiddenPackage` matches with
    // `String.prototype.startsWith`, so `@tauri-apps/*` would match nothing.
    forbiddenPackages: ['@tauri-apps/'],
    // `fetch(` and `XMLHttpRequest` are call syntax, not specifiers: they are
    // matched against the comment-blanked source, so the file explaining *why*
    // it must not use them does not trip the rule that forbids them.
    forbiddenCode: ['fetch(', 'XMLHttpRequest'],
    blocking: true,
  },
  {
    name: 'driver-sdk-no-direct-tauri',
    from: 'packages/driver-sdk/src',
    // Drivers are documented as portable across hosts; an `invoke` import
    // makes every driver desktop-only. Reported, but **advisory** until the
    // frontend track's P1 migration routes these through `BackendClient`:
    // three `ipc/*.ts` files import `@tauri-apps/api/core` today, and the
    // fix belongs to that migration, not to the guard.
    forbiddenPackages: ['@tauri-apps/'],
    blocking: false,
  },
];

const SOURCE_EXTENSIONS = SCAN_EXTENSIONS;

/**
 * Every source file under `dir`, recursively — read as it is found.
 *
 * The read deliberately happens here rather than in the caller. Collecting the
 * whole tree first and reading it afterwards leaves a window in which a file
 * another process creates and deletes (a test's mutation probe, a branch
 * switch, a build) makes this guard abort with ENOENT — observed on roughly
 * five runs in six, which is indistinguishable from a real violation and so
 * makes the gate unusable for signing off. Not wrapped in try/catch: a genuine
 * read failure should surface here, where the cause is obvious, rather than be
 * swallowed into a green report.
 */
function collectSourceFiles(dir) {
  const out = [];
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      // Vendored / generated trees are somebody else's source. Skipping them
      // here matters more than it looks: `packages/ui/` has no `node_modules`
      // or `dist` today, so without this a single `npm install` under the
      // design system would start failing the gate on third-party code.
      if (entry.isDirectory() && SKIP_DIR_NAMES.has(entry.name)) continue;
      const full = resolve(current, entry.name);
      if (entry.isDirectory()) {
        // Prune subtrees this repository does not own (see `SKIP_PATH_PREFIXES`).
        if (isSkippedPath(relative(ROOT, full).split(/[\\/]/).join('/'))) continue;
        walk(full);
      } else if (SOURCE_EXTENSIONS.has(extname(entry.name))) {
        const content = readScannedIfPresent(full);
        // Deleted between the directory read and this one: not in the tree, so
        // not something this guard can have an opinion about.
        if (content !== null) out.push({ path: full, content });
      }
    }
  };
  walk(resolve(ROOT, dir));
  return out;
}

/** Resolve a relative import specifier to a repo-relative POSIX path. */
function resolveSpecifier(file, specifier) {
  if (!specifier.startsWith('.')) return null;
  const target = resolve(dirname(file), specifier);
  return relative(ROOT, target).split('\\').join(posix.sep);
}

function isForbidden(target, forbidden = []) {
  return forbidden.some((prefix) => target === prefix || target.startsWith(`${prefix}/`));
}

function forbiddenPackage(specifier, prefixes = []) {
  return prefixes.find((prefix) => specifier.startsWith(prefix)) ?? null;
}

/**
 * Every needle a rule declares, as one list tagged with the channel it is
 * compared on.
 *
 * Three separate `rule.<list> ?? []` reads used to be the whole of "what this
 * rule checks", which left the guard unable to tell the difference between
 * "compared 400 files against 3 needles and they were clean" and "compared
 * nothing". Both produced an empty finding list and exit 0, and nothing on
 * stdout said which one had happened — a hard CI gate reporting success while
 * checking nothing. Collapsing the three reads into one list is what makes the
 * scan auditable: {@link collectModuleLayerFindings} can then require that
 * every needle here was actually compared, and fail loudly when one was not.
 *
 * `axis` selects the comparison, which is the guard's existing behaviour
 * unchanged: `path` and `package` needles are matched against each import
 * literal, `code` needles against the comment-blanked source.
 *
 * MUTATED, and measured: an earlier version of this file compared against
 * `rule.forbiddenPackages` directly while recording liveness from this list.
 * Rewriting the comparison's argument to `[]` — a one-token edit at the read
 * point, leaving the declaration intact — made the guard stop finding every
 * `@tauri-apps/` import, dropped the advisory finding, and still printed
 * `ok`, exit 0. The declaration and the read are the same fact; there must be
 * one copy of it.
 *
 * @param {{ forbidden?: string[], forbiddenPackages?: string[], forbiddenCode?: string[] }} rule
 * @returns {Array<{axis: 'path'|'package'|'code', value: string}>}
 */
function collectNeedles(rule) {
  const out = [];
  for (const value of rule.forbidden ?? []) out.push({ axis: 'path', value });
  for (const value of rule.forbiddenPackages ?? []) out.push({ axis: 'package', value });
  for (const value of rule.forbiddenCode ?? []) out.push({ axis: 'code', value });
  return out;
}

/**
 * Scan every rule and return the findings, without printing anything.
 *
 * Split out from {@link checkModuleLayers} so a unit test can assert on *which
 * files* a rule fired rather than on the wording of a report line: the gate's
 * contract is the exit code, but the exit code alone cannot distinguish "found
 * the three files that import `@tauri-apps/api/core`" from "found some other
 * three".
 *
 * @param {{ requireLayers?: string[] | null }} [opts]
 * @returns {{
 *   violations: Array<{rule: string, file: string, line: number, text: string, specifier: string|null, target: string}>,
 *   advisories: Array<{rule: string, file: string, line: number, text: string, specifier: string|null, target: string}>,
 *   errors: string[],
 *   vacuous: Array<{rule: string, from: string}>,
 *   examined: number,
 * }}
 */
export function collectModuleLayerFindings(opts = {}) {
  const requireLayers = opts.requireLayers ?? null;
  const violations = [];
  const advisories = [];
  const errors = [];
  const vacuous = [];
  let examined = 0;

  for (const rule of LAYER_RULES) {
    const blocking = rule.blocking !== false;
    const required = Boolean(requireLayers?.includes(rule.name));

    // "The subject does not exist" and "the subject exists and is clean" must
    // not share an exit code: the gate's whole job is to say which one it found.
    //
    // A rule graded `blocking` claims to be in force. An absent subject is a
    // contradiction of that claim rather than a pending state — there is
    // nothing to enforce — so it is an error. An advisory rule's absence is
    // *consistent* with its grade ("not in force yet"), so it stays a report,
    // and `--require-layers` still upgrades it for whoever claims it.
    if (!existsSync(resolve(ROOT, rule.from))) {
      vacuous.push({ rule: rule.name, from: rule.from });
      if (blocking) {
        errors.push(
          `${rule.name}: ${rule.from} is absent, so a blocking rule cannot hold — the gate checked nothing`,
        );
      } else if (required) {
        errors.push(
          `--require-layers ${rule.name}: ${rule.from} is absent, so the rule cannot hold`,
        );
      }
      continue;
    }

    // The one read point for what this rule will compare, and the one place
    // the scan can be shown to have happened.
    //
    // The table is split by axis *once*, here, and each axis's list is read in
    // exactly one place — the loop that compares it. Splitting at the use site
    // instead (comparing against `rule.forbidden` while recording liveness from
    // `needles`) leaves two copies of one declaration: emptying only the copy
    // the comparison reads blinds the guard while the liveness record still
    // claims the needle was compared. That was measured, not assumed — see the
    // `MUTATED` note above `collectNeedles`.
    const needles = collectNeedles(rule);
    const pathNeedles = needles.filter((n) => n.axis === 'path').map((n) => n.value);
    const packageNeedles = needles.filter((n) => n.axis === 'package').map((n) => n.value);
    const codeNeedles = needles.filter((n) => n.axis === 'code').map((n) => n.value);
    const files = collectSourceFiles(rule.from);
    const applied = new Set();

    if (needles.length === 0) {
      errors.push(
        `${rule.name}: ${rule.from} declares no forbidden path, package or code needle — the rule cannot hold`,
      );
      continue;
    }

    if (files.length === 0) {
      errors.push(
        `${rule.name}: ${rule.from} exists but holds no scannable source file, so the rule checked nothing`,
      );
      continue;
    }

    for (const { path: file, content: source } of files) {
      const scan = scanCode(source);
      const { literals } = scan;
      const lines = source.split('\n');
      const record = (finding) => (blocking ? violations : advisories).push(finding);
      for (const { value, line } of literals) {
        const target = resolveSpecifier(file, value);
        // Recorded where the comparison is *attempted*, not where it matches.
        // A rule whose needle happens to match nothing in a clean tree must not
        // be reported as "never compared" — that is the whole point of the
        // check, and conflating the two would turn every green build red.
        if (target) for (const needle of pathNeedles) applied.add(needle);
        for (const needle of packageNeedles) applied.add(needle);
        if (target && isForbidden(target, pathNeedles)) {
          record({
            rule: rule.name,
            file: relative(ROOT, file),
            line,
            text: (lines[line - 1] ?? '').trim(),
            specifier: value,
            target,
          });
          continue;
        }
        const pkg = forbiddenPackage(value, packageNeedles);
        if (pkg) {
          record({
            rule: rule.name,
            file: relative(ROOT, file),
            line,
            text: (lines[line - 1] ?? '').trim(),
            specifier: value,
            target: `${pkg}…`,
          });
        }
      }
      for (const needle of codeNeedles) {
        // Only the *blanked* source is searched: a comment that names
        // `fetch(` is prose, and prose that explains the rule must not trip it.
        // The needle must also start a whole token, or `q.refetch()` reads as a
        // call to the network — see `findCodeNeedle`.
        applied.add(needle);
        const at = findCodeNeedle(scan.code, needle);
        if (at !== -1) {
          const line = lineAtOffset(scan.code, at);
          record({
            rule: rule.name,
            file: relative(ROOT, file),
            line,
            text: (lines[line - 1] ?? '').trim(),
            specifier: null,
            target: needle,
          });
        }
      }
    }

    // Every needle the rule declares must have been put in front of at least
    // one file. A needle that never reaches the loop cannot have found
    // anything, so the rule is enforcing less than its own table claims while
    // the report says `ok` — the same defect as an absent subject, one step
    // further in. This is checked *after* the scan and against the needles the
    // loop actually consumed, so it fails if the read above ever stops feeding
    // it — which is exactly what happens when `needles` is emptied at source.
    const unapplied = needles.filter((n) => !applied.has(n.value));
    if (unapplied.length > 0) {
      errors.push(
        `${rule.name}: ${rule.from} was never compared against ` +
          `${unapplied.map((n) => `\`${n.value}\``).join(', ')} — the rule checked less than it declares`,
      );
      continue;
    }
    examined += files.length;
  }

  return { violations, advisories, errors, vacuous, examined };
}

/**
 * @param {{ root?: string, log?: (...a: unknown[]) => void, requireLayers?: string[] | null }} [opts]
 * @returns {number} 0 when clean or advisory-only, 1 on a violation, 2 on an error
 */
export function checkModuleLayers(opts = {}) {
  const log = opts.log ?? console.log;
  const { violations, advisories, errors, vacuous, examined } = collectModuleLayerFindings(opts);

  for (const v of vacuous) {
    log(`[check-module-layers] VACUOUS  ${v.rule}: ${v.from} does not exist — rule not exercised`);
  }

  if (advisories.length > 0) {
    log(`[check-module-layers] ${advisories.length} advisory finding(s) (non-blocking):`);
    for (const a of advisories) {
      log(`  ${a.file}:${a.line}  →  ${a.target}`);
      log(`    ${a.text}`);
      log(`    rule: ${a.rule} [advisory]`);
    }
  }

  if (errors.length > 0) {
    log(`[check-module-layers] ${errors.length} error(s):`);
    for (const e of errors) log(`  ${e}`);
    return 2;
  }

  if (violations.length === 0) {
    // `examined` is in the summary on purpose. A reader who sees `4 rules` and
    // no vacuous marker has to be able to tell *how much* was actually read —
    // without it, "4 rules, none vacuous" reads as "4 rules were enforced",
    // which is exactly the claim this guard previously could not support.
    const parts = [`${LAYER_RULES.length} rules`, `${examined} files examined`];
    if (vacuous.length > 0) parts.push(`${vacuous.length} vacuous`);
    if (advisories.length > 0) parts.push(`${advisories.length} advisory`);
    log(`[check-module-layers] ok (${parts.join(', ')})`);
    return 0;
  }

  log(`[check-module-layers] ${violations.length} violation(s):`);
  for (const v of violations) {
    log(`  ${v.file}:${v.line}  →  ${v.target}`);
    log(`    ${v.text}`);
    log(`    rule: ${v.rule}`);
  }
  return 1;
}

/** `--require-layers=a,b`: treat these rules as absent-subject failures. */
function parseRequireLayers(argv) {
  const arg = argv.find((a) => a.startsWith('--require-layers='));
  if (!arg) return null;
  const value = arg.slice('--require-layers='.length).trim();
  return value.length > 0 ? value.split(',').map((s) => s.trim()) : null;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exit(checkModuleLayers({ requireLayers: parseRequireLayers(process.argv.slice(2)) }));
}
