#!/usr/bin/env node
/**
 * Platform crate dependency boundary gate — F-01..F-07 of
 * `docs/architecture/platform/shared-boundaries-and-ports.md` §2.4.
 *
 * The requirement this implements (platform-development-plan.md:80):
 * 「加入 CI 架构检查：对 server crate 的 normal/build 依赖闭包验证不含 Tauri
 * crate 和 UI runtime，并验证 server 独立构建；依赖检查失败即阻断合并。」
 *
 * ## What it actually prevents
 *
 * One failure, concretely: the `server` / `platform-api` / `application` crates
 * are being written so the same Rust logic can run behind a headless HTTP
 * process instead of a webview. The moment one of them takes a `tauri` /
 * `tauri-plugin-*` / host-crate edge — usually innocently, to reuse one helper
 * or one command struct — `cargo build` for the headless target starts pulling
 * in webkit2gtk / wry / the windows crate, which cannot link on a server
 * image, and the failure surfaces as a broken *deploy*, weeks later, as an
 * unreadable link error on a crate the author never touched. The same edge in
 * the other direction (a driver reaching into `datazen` or `runtime`) does the
 * mirror-image damage: F-01 exists because it turns a host-only build into one
 * that drags the whole desktop shell into every driver, so "宿主无关的构建被拖垮".
 *
 * ## Why this is not a hardcoded crate list
 *
 * Layers come from directory paths (see `lib/cargoWorkspace.mjs`), the
 * forbidden workspace crates come from `cargo metadata`'s own package names for
 * those layers, and the third-party families (`tauri*`, HTTP transports,
 * `react*`) come out of the **spec table itself** by parsing §2.4's rows. Three
 * consequences:
 *
 *  1. a crate dropped under `packages/platform-api/`, `packages/application/` or
 *     `server/` is covered with no edit here and no edit to ci.yml;
 *  2. editing §2.4 to add a family (say `yew` to F-05) arms it here;
 *  3. a workspace member matching no layer is an **error** — the one thing that
 *     must never happen is a green gate over a crate the gate does not know.
 *
 * ## Independent sources
 *
 *  (1) Cargo's feature-resolved dependency graph, (2) the filesystem path of
 *  each member's `manifest_path`, (3) the spec's own §2.4 table, cross-checked
 *  both ways. Honest limit, stated because it matters: **the rule table itself
 *  is hand-written**, so a *coordinated* wrong edit of the guard and the spec
 *  still passes. What the cross-check does buy is that a spec edit alone, or a
 *  guard edit alone, is caught. See `README`-free note in
 * `docs/development/ci-test-matrix.md`.
 */

import { existsSync, readFileSync, readdirSync } from 'fs';
import { join, resolve } from 'path';

import {
  LAYERS,
  buildWorkspaceIndex,
  layerById,
  manifestDir,
  matchesCrateFamily,
  normalBuildClosure,
  runCargoMetadata,
} from './lib/cargoWorkspace.mjs';
import { SCAN_EXTENSIONS, SKIP_DIR_NAMES, isSkippedPath } from './lib/scanTargets.mjs';
import { scanCode } from './lib/scanSourceCode.mjs';

export const SPEC_PATH = 'docs/architecture/platform/shared-boundaries-and-ports.md';
export const PREFIX = '[check-platform-arch]';

/**
 * `forbiddenCrates: 'spec'` means "take the crate names straight out of this
 * rule's own §2.4 row", which is why F-03 needs no `axum`/`actix-web`/`warp`/
 * `tonic` literal here. `subjectsFromSpec` means "assert the layer ids I
 * declare against the paths the spec row names" — a two-way check, so a spec
 * row that gains or loses a subject path turns the gate red until the two are
 * reconciled.
 *
 * `forbiddenLayers` names layers, never crate names: their package names are
 * read from `cargo metadata`, so renaming or adding a crate in one of those
 * layers needs no edit here. `allowedLayers` is the inverted form used by F-01
 * and F-04, where a deny-list would quietly permit any layer added later.
 *
 * The cross-check against §2.4 is deliberately *not* uniform, and here is why:
 * the table writes layer paths (`packages/application`) in some cells and crate
 * names (`datazen-runtime`) in others, so only some rows can be compared both
 * ways. What is checked, per rule:
 *
 *   F-01  token presence + allow-list inversion      (row names crates, not paths)
 *   F-02  token presence + subject two-way           (row also names `datazen`)
 *   F-03  token presence + subject two-way + crate-family two-way
 *   F-04  token presence + allow-list inversion      (row names forbidden paths)
 *   F-05  token presence + subject two-way + crate-family two-way
 *   F-06  token presence + subject two-way
 *   F-07  token presence + subject two-way
 *
 * "Two-way" means *equal on both sides*, so a §2.4 row edited without the guard
 * goes red and the guard edited without §2.4 goes red too.
 */
const RULES = Object.freeze([
  {
    id: 'F-01',
    subjects: ['driver'],
    // F-01's row names its forbidden crates (`datazen`, `datazen-runtime`, …) as
    // crate *names*, not as paths, so there is no subject/path two-way check to
    // run against it. It gets a stronger check instead: an allow-list.
    //
    // `allowedLayers` inverts the rule on purpose. Written as a deny-list, a
    // layer added to LAYERS six months from now would be *permitted* for drivers
    // until somebody remembered to add it here — the exact silent-pass failure
    // this guard exists to prevent. Written this way, a new layer is forbidden
    // for drivers the day it is declared, and relaxing it is a deliberate edit.
    allowedLayers: ['driver', 'driver-api'],
    forbiddenCrates: [],
    specTokens: ['packages/drivers/*'],
    note: 'drivers stay host-free and platform-free',
  },
  {
    id: 'F-02',
    subjects: ['application', 'runtime', 'platform-api'],
    subjectsFromSpec: true,
    forbiddenLayers: ['host'],
    forbiddenCrates: ['tauri'],
    // One-way: the row also names `datazen`, which the host layer already covers.
    specSubsetOnly: true,
    specTokens: ['tauri'],
    note: 'core platform crates stay webview-free',
  },
  {
    id: 'F-03',
    subjects: ['application', 'runtime'],
    subjectsFromSpec: true,
    forbiddenLayers: [],
    forbiddenCrates: 'spec',
    // Two-way: these are the row's own crate tokens, so adding `rocket` to §2.4
    // arms `rocket` here with no edit.
    specTokens: ['axum'],
    specCrates: ['axum', 'actix-web', 'warp', 'tonic'],
    note: 'no HTTP/transport framework in the business core',
  },
  {
    id: 'F-04',
    subjects: ['platform-api'],
    // Same allow-list inversion as F-01; §2.3 draws platform-api depending on
    // the driver API and the AI API and nothing else.
    allowedLayers: ['platform-api', 'driver-api', 'ai-api'],
    forbiddenCrates: [],
    specTokens: ['packages/platform-api', 'src-tauri'],
    note: 'platform-api points at ports, not at the implementation',
  },
  {
    id: 'F-05',
    subjects: ['application', 'runtime', 'platform-api'],
    subjectsFromSpec: true,
    forbiddenLayers: [],
    forbiddenCrates: 'spec',
    // `@tauri-apps/api` is parsed as a *frontend* token and has no meaning on
    // the Rust side; §7's source scan is where the frontend half lives.
    specTokens: ['@tauri-apps/api'],
    specCrates: ['react'],
    note: 'no UI runtime markers on the Rust side',
  },
  {
    id: 'F-06',
    subjects: ['server'],
    subjectsFromSpec: true,
    forbiddenLayers: ['host'],
    forbiddenCrates: ['tauri'],
    specSubsetOnly: true,
    specTokens: ['tauri'],
    note: 'the headless server links no webview',
  },
  {
    id: 'F-07',
    subjects: ['backend-client'],
    subjectsFromSpec: true,
    forbiddenLayers: [],
    // F-07's subject is TypeScript, not Cargo: forbidden *literals* in source.
    tsForbid: ['@tauri-apps/', 'fetch(', 'XMLHttpRequest'],
    specTokens: ['packages/backend-client', '@tauri-apps/', 'fetch(', 'XMLHttpRequest'],
    note: 'the browser client stays transport-agnostic',
  },
]);

/**
 * Layers the spec makes subjects of some rule. Until they exist the rules
 * covering them are **vacuous** — reported loudly, exit 0 — because refusing to
 * pass on a repo that has not started the platform work yet would block every
 * unrelated PR. `--require-layers=a,b` (or `ARCH_GUARD_REQUIRED_LAYERS`)
 * converts vacuous into error, and that is the lever to arm at the P1 exit gate
 * once the crates land.
 */
export const DEFAULT_REQUIRED_LAYERS = [];

function specLayerId(pathToken) {
  const normalised = pathToken.replace(/\/\*$/, '/*');
  const layer = LAYERS.find(
    (l) => l.path === normalised || (normalised.endsWith('/*') && l.path === normalised),
  );
  if (layer) return layer.id;
  // `src-tauri` never appears as a F-row subject, but keep the miss loud rather
  // than silently mapping it onto something plausible.
  return { unclassified: `spec names \`${pathToken}\`, which matches no LAYERS entry` };
}

/**
 * Split one §2.4 row's backticked tokens into the three kinds the rules need:
 * workspace paths (→ layer), third-party crate names (→ forbidden families),
 * and frontend/identifier tokens (F-05's `@tauri-apps/api`, F-07's `fetch(`),
 * which have no meaning on the Rust side.
 */
export function parseSpecRow(row) {
  const paths = [];
  const crates = [];
  const frontend = [];
  for (const match of row.matchAll(/`([^`]+)`/g)) {
    const token = match[1].trim();
    if (token === '') continue;
    if (/^(packages\/|src-tauri$|server$)/.test(token)) paths.push(token);
    else if (token.includes('@') || token.includes('(') || token.includes('.')) frontend.push(token);
    else crates.push(token);
  }
  return { paths, crates, frontend };
}

function findSpecRow(specText, ruleId) {
  for (const line of specText.split('\n')) {
    if (new RegExp(`^\\|\\s*${ruleId}\\s*\\|`).test(line.trim())) return line;
  }
  return null;
}

/**
 * Cross-check the hand-written rule table against the spec table.
 *
 * Returns violations rather than throwing, so the caller reports them in the
 * guard's normal output format. Fails closed: a row that cannot be found, a
 * path with no layer, or a two-way subject mismatch all become red.
 */
export function checkSpecConsistency(specText, rules = RULES) {
  const violations = [];
  for (const rule of rules) {
    const row = findSpecRow(specText, rule.id);
    if (row === null) {
      violations.push(
        `${rule.id}: no row found in ${SPEC_PATH} §2.4. The rule table and the spec ` +
          `have diverged; the guard can no longer claim to implement F-01..F-07.`,
      );
      continue;
    }
    for (const token of rule.specTokens ?? []) {
      if (!row.includes(token)) {
        violations.push(
          `${rule.id}: ${SPEC_PATH} §2.4 no longer mentions \`${token}\`, which this guard enforces.`,
        );
      }
    }
    if (!rule.subjectsFromSpec) continue;

    const parsed = parseSpecRow(row);
    const specLayers = [];
    for (const token of parsed.paths) {
      const id = specLayerId(token);
      if (typeof id === 'string') {
        if (!specLayers.includes(id)) specLayers.push(id);
      } else {
        violations.push(`${rule.id}: ${id.unclassified}.`);
      }
    }
    const mine = [...rule.subjects].sort();
    const theirs = [...specLayers].sort();
    if (mine.join(',') !== theirs.join(',')) {
      violations.push(
        `${rule.id}: subjects disagree with ${SPEC_PATH} §2.4 — guard has ` +
          `[${mine.join(', ')}], spec row has [${theirs.join(', ')}]. ` +
          `Reconcile the guard and the spec.`,
      );
    }
    if (rule.forbiddenCrates === 'spec') {
      const mineCrates = [...rule.specCrates].sort();
      const theirsCrates = [...parsed.crates].sort();
      if (mineCrates.join(',') !== theirsCrates.join(',')) {
        violations.push(
          `${rule.id}: crate families disagree with ${SPEC_PATH} §2.4 — guard expects ` +
            `[${mineCrates.join(', ')}], spec row has [${theirsCrates.join(', ')}].`,
        );
      }
    }
  }
  return violations;
}

/** F-07: walk `layer.path`, feed each source file to the shared tokenizer. */
function checkTsLayer(root, layer, rule, errors, violations) {
  const dir = resolve(root, layer.path);
  if (!existsSync(dir)) return 0;
  let files = 0;
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const rel = manifestDir(join(current, entry.name), root);
      if (entry.isDirectory()) {
        if (SKIP_DIR_NAMES.has(entry.name) || isSkippedPath(rel)) continue;
        walk(join(current, entry.name));
        continue;
      }
      const ext = entry.name.slice(entry.name.lastIndexOf('.'));
      if (!SCAN_EXTENSIONS.has(ext)) continue;
      files += 1;
      const { code, literals } = scanCode(readFileSync(join(current, entry.name), 'utf8'));
      for (const forbidden of rule.tsForbid) {
        // A bare `fetch(` is a call, so it is matched against the blanked-out
        // source (comments and string bodies are already gone); a bare
        // `@tauri-apps/` would also match inside a comment, so it is matched
        // against the literal list instead.
        const hit = forbidden.endsWith('(')
          ? code.includes(forbidden)
            ? { line: code.slice(0, code.indexOf(forbidden)).split('\n').length }
            : undefined
          : literals.find((l) => l.value.includes(forbidden));
        if (hit) {
          violations.push(
            `${rule.id}  ${rel}:${hit.line} contains \`${forbidden}\` — ${layer.path} must reach ` +
              `the backend through the platform client, not through Tauri IPC or the network directly.`,
          );
        }
      }
    }
  };
  try {
    walk(dir);
  } catch (cause) {
    errors.push(`${rule.id}: could not scan ${layer.path}: ${cause.message}`);
  }
  return files;
}

/**
 * Run every rule.
 *
 * @param {{
 *   root: string,
 *   metadata?: object,
 *   specText?: string,
 *   env?: NodeJS.ProcessEnv,
 *   requireLayers?: string[],
 *   log?: (s: string) => void,
 *   error?: (s: string) => void,
 * }} options
 */
export function checkPlatformCrateBoundaries(options) {
  const { root, requireLayers = DEFAULT_REQUIRED_LAYERS, log = () => {}, error = console.error } = options;
  const violations = [];
  const errors = [];
  const vacuous = [];
  const advisories = [];
  const evaluated = [];

  const metadata = options.metadata ?? runCargoMetadata({ cwd: root, env: options.env, log, error });
  let specText = options.specText;
  if (specText === undefined) {
    const specFile = resolve(root, SPEC_PATH);
    if (!existsSync(specFile)) {
      errors.push(`missing ${SPEC_PATH}; the guard cannot verify it implements F-01..F-07`);
      return { violations, errors, vacuous, advisories, evaluated, summary: 'aborted: no spec' };
    }
    specText = readFileSync(specFile, 'utf8');
  }
  violations.push(...checkSpecConsistency(specText));

  const index = buildWorkspaceIndex(metadata);
  for (const bad of index.unclassified) {
    errors.push(`unclassified workspace member ${bad.name ?? bad.id} (${bad.relDir ?? '?'}): ${bad.reason}`);
  }

  const membersByLayer = new Map();
  for (const member of index.members.values()) {
    if (!membersByLayer.has(member.layer)) membersByLayer.set(member.layer, []);
    membersByLayer.get(member.layer).push(member);
  }

  // Package name → layer, for the layer-derived half of every forbidden set.
  const nameToLayer = new Map();
  for (const member of index.members.values()) nameToLayer.set(member.name, member.layer);

  const required = new Set(requireLayers);

  for (const rule of RULES) {
    const subjects = [];
    for (const layerId of rule.subjects) {
      const layer = layerById(layerId);
      const found = membersByLayer.get(layerId) ?? [];
      if (found.length === 0) {
        const absent = `${layer?.path ?? layerId}`;
        if (required.has(layerId)) {
          errors.push(
            `${rule.id}: required layer \`${absent}\` has no workspace member. ` +
              `${required.has(layerId) ? 'ARCH_GUARD_REQUIRED_LAYERS' : ''}`,
          );
        } else {
          vacuous.push({ rule: rule.id, layer: layerId, path: absent });
        }
        continue;
      }
      subjects.push(...found);
    }

    const forbiddenNames = new Set();
    if (rule.allowedLayers) {
      // Fail-closed inversion: everything not explicitly allowed is forbidden.
      const allowed = new Set(rule.allowedLayers);
      for (const layer of LAYERS) {
        if (layer.kind !== 'rust' || allowed.has(layer.id)) continue;
        for (const member of membersByLayer.get(layer.id) ?? []) forbiddenNames.add(member.name);
      }
    } else {
      for (const member of index.members.values()) {
        if (rule.forbiddenLayers.includes(member.layer)) forbiddenNames.add(member.name);
      }
    }

    if (rule.id === 'F-07') {
      const layer = layerById(rule.subjects[0]);
      const files = checkTsLayer(root, layer, rule, errors, violations);
      if (files > 0) evaluated.push({ rule: rule.id, subjects: [`${layer.path}/ (${files} file(s))`] });
      continue;
    }

    const families = rule.forbiddenCrates === 'spec' ? parseSpecRow(findSpecRow(specText, rule.id) ?? '').crates : rule.forbiddenCrates;
    if (subjects.length === 0) continue;
    evaluated.push({ rule: rule.id, subjects: subjects.map((s) => s.name) });

    for (const member of subjects) {
      const closure = normalBuildClosure(index, member.id);
      for (const pkgId of closure) {
        if (pkgId === member.id) continue;
        const pkg = index.byId.get(pkgId);
        if (!pkg) continue;
        const viaLayer = forbiddenNames.has(pkg.name);
        const viaFamily = families.some((f) => matchesCrateFamily(pkg.name, f.replace(/\*$/, '')));
        if (!viaLayer && !viaFamily) continue;
        const why = viaFamily ? 'crate family' : `${pkg.name}'s layer (${nameToLayer.get(pkg.name) ?? 'external'})`;
        violations.push(
          `${rule.id}  ${member.name} (${member.relDir}/Cargo.toml) → ${pkg.name} in its ` +
            `normal+build closure [${why}]: ${rule.note}.`,
        );
      }
    }
  }

  // Advisory, non-blocking. Same precedent as R3 in
  // check-driver-import-boundaries.mjs: a real finding that no F-row names, so
  // blocking on it would be enforcing a rule the spec has not adopted.
  //
  // Finding, measured at 6175f1c1f: `packages/drivers/redis/Cargo.toml:31`
  // declares `[build-dependencies] tauri-plugin = { version = "2", features =
  // ["build"] }`, so the redis driver drags Tauri into its build graph. F-01's
  // consequence column describes exactly this ("宿主无关的构建被拖垮"), but F-01's
  // own cell names workspace crates only. Escalated to the platform owner rather
  // than enforced here; the fix is an edit to a driver manifest, which is
  // outside this guard's write scope. Delete this block in the same commit that
  // removes the build-dependency, or promote the family into F-01's
  // `forbiddenCrates`.
  for (const member of membersByLayer.get('driver') ?? []) {
    const closure = normalBuildClosure(index, member.id);
    const tauri = [...closure]
      .map((id) => index.byId.get(id))
      .filter((pkg) => pkg && matchesCrateFamily(pkg.name, 'tauri'))
      .map((pkg) => pkg.name);
    if (tauri.length === 0) continue;
    advisories.push(
      `F-01?  ${member.name} (${member.relDir}/Cargo.toml) reaches ${tauri.sort().join(', ')} ` +
        `in its normal+build closure. Not blocking: no F-row forbids tauri* for drivers.`,
    );
  }

  // `evaluated.length` counts only rules that found at least one subject, so a
  // bare count reads stronger than the evidence: measured at c3cf61fef this
  // guard says "1 rule(s) evaluated over 16 crate(s)" while 11 rule×subject
  // combos print VACUOUS, i.e. 6 of the 7 F-rows produced no verdict at all.
  // "PASS — …" then invites reading the spec as fully checked when the truth is
  // "everything that could be checked was checked". The vacuous count is
  // already computed and already printed per line; naming it here costs nothing
  // and is the only place a reader is told the denominator.
  const subjectSlots = evaluated.reduce((n, e) => n + e.subjects.length, 0);
  const summary =
    `${index.members.size} workspace member(s) classified, ${evaluated.length} rule(s) evaluated over ` +
    `${subjectSlots} crate(s), ${vacuous.length} rule×subject combo(s) vacuous: ` +
    `${violations.length} violation(s), ${errors.length} error(s), ${advisories.length} advisory(ies)`;
  return { violations, errors, vacuous, advisories, evaluated, summary, index };
}

function parseArgs(argv) {
  const args = { requireLayers: [...DEFAULT_REQUIRED_LAYERS], root: process.cwd() };
  for (const arg of argv) {
    if (arg.startsWith('--root=')) args.root = resolve(arg.slice('--root='.length));
    else if (arg.startsWith('--require-layers='))
      args.requireLayers = arg
        .slice('--require-layers='.length)
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
    else if (arg === '--help') args.help = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  const envRequired = process.env.ARCH_GUARD_REQUIRED_LAYERS;
  if (envRequired) args.requireLayers.push(...envRequired.split(',').map((s) => s.trim()).filter(Boolean));
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
  if (args.help) {
    out(
      `${PREFIX} usage: node scripts/check-platform-crate-boundaries.mjs ` +
        `[--root=<dir>] [--require-layers=<layerId,...>]\n` +
        `${PREFIX} layers: ${LAYERS.map((l) => `${l.id}=${l.path}`).join('  ')}`,
    );
    return 0;
  }

  let result;
  try {
    result = checkPlatformCrateBoundaries({ ...args, env, log: out, error: err });
  } catch (cause) {
    err(`${PREFIX} ${cause.message}`);
    return 2;
  }

  for (const layer of result.vacuous)
    out(`${PREFIX} VACUOUS  ${layer.rule}: no \`${layer.path}\` yet — rule armed, nothing to check`);
  for (const line of result.advisories) out(`${PREFIX} ADVISORY ${line}`);
  for (const line of result.violations) err(`${PREFIX} VIOLATION ${line}`);
  for (const line of result.errors) err(`${PREFIX} ERROR    ${line}`);
  for (const rule of result.evaluated) out(`${PREFIX} ok ${rule.rule}: ${rule.subjects.join(', ')}`);

  const failed = result.violations.length + result.errors.length;
  out(`${PREFIX} ${failed === 0 ? 'PASS' : 'FAIL'} — ${result.summary}`);
  return failed === 0 ? 0 : 1;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exitCode = runCli();
}