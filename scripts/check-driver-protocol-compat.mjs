#!/usr/bin/env node
/**
 * @file Driver protocol / crate compatibility gate for `packages/driver-api`.
 *
 * A driver compiled against an older `datazen-driver-api` is not wrong; it is
 * simply speaking a different contract. `platform-development-plan.md:97`
 * requires that changing a public contract be reconciled against three
 * separate numbers — the crate SemVer, `PROTOCOL_VERSION` and
 * `MIN_PROTOCOL_VERSION` — and the same document rules that
 * "协议升级与驱动发布是原子兼容门槛，不允许用下调最低版本掩盖 breaking change"
 * (a protocol upgrade and the driver release are an atomic compatibility
 * threshold; a breaking change must never be disguised by lowering the minimum
 * version).
 *
 * Those three numbers are trivially easy to forget and impossible to review by
 * eye across a large diff. This file makes the reconciliation machine-checkable.
 *
 * ## The rule table
 *
 * `CONTRACT_RULES` below is the single source of truth: which public contracts
 * are governed, and which of the three version numbers each class of change to
 * them obliges you to raise. It is data, not prose, so
 * `scripts/__tests__/check-driver-protocol-compat.test.ts` can assert the table
 * against the real source tree, and so adding a governed contract is a one-line
 * edit rather than a change buried in a checker.
 *
 * ## How a change is classified
 *
 * Rather than line-anchoring (fragile: a doc comment above a definition shifts
 * every line number), the checker resolves each governed symbol to its *span* in
 * both the base blob and the new blob, and classifies the `git diff` hunks that
 * land inside those spans. A line above `pub trait DatabaseDriver` therefore
 * never reads as a change to `DatabaseDriver`, and a change to the body always
 * does.
 *
 * ## Why a struct field addition is additive, not breaking
 *
 * Adding a field with a fail-closed `Default` does not break an existing
 * implementor: `#[derive(Default)]` fills the new field with the
 * non-supporting variant, so an un-migrated driver keeps declaring nothing.
 * That is the invariant `default_capability_set_declares_nothing()` and
 * `an_unmigrated_factory_declares_no_capabilities` already enforce in Rust, and
 * this checker treats it as the precondition that makes such an addition a
 * crate-level (minor) bump rather than a protocol bump.
 *
 * Adding a *required* trait method is different: an existing implementor that
 * does not have a default body stops compiling, which no `PROTOCOL_VERSION`
 * window can express. That is classified breaking.
 *
 * @module check/driver-protocol-compat
 */

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LOG_PREFIX = '[check-driver-protocol-compat]';

/**
 * The crate whose SemVer is one of the three numbers under reconciliation.
 * @type {string}
 */
export const GOVERNED_CRATE_MANIFEST = 'packages/driver-api/Cargo.toml';

/**
 * `PROTOCOL_VERSION` — the newest driver protocol this host understands.
 *
 * A driver declaring a higher value is refused by
 * `ProtocolCompatibility::AheadOfHost` rather than guessed at.
 * @type {string}
 */
export const PROTOCOL_VERSION_FILE = 'packages/driver-api/src/lib.rs';
/** @type {string} */
export const PROTOCOL_VERSION_SYMBOL = 'PROTOCOL_VERSION';

/**
 * `MIN_PROTOCOL_VERSION` — the oldest driver protocol this host still accepts.
 *
 * Below it a driver must be rejected, never silently degraded into
 * `ProtocolCompatibility::BelowMinimum`.
 * @type {string}
 */
export const MIN_PROTOCOL_VERSION_FILE = 'packages/driver-api/src/lib.rs';
/** @type {string} */
export const MIN_PROTOCOL_VERSION_SYMBOL = 'MIN_PROTOCOL_VERSION';

/**
 * Public contracts of `packages/driver-api` whose shape is part of the
 * driver/host ABI.
 *
 * `kind` selects how an added line inside the span is read:
 *
 * - `trait` — an added line that declares a method with no default body (ends
 *   in `;`) breaks every existing implementor, so it is breaking. An added
 *   line with a default body (`{`) is additive.
 * - `struct` / `enum` — any added line is additive, provided the fail-closed
 *   `Default` invariant holds on the Rust side.
 *
 * `anchor` must match the item's declaration line. It is matched by
 * `includes`, so it only has to be unambiguous within the file.
 *
 * @type {ReadonlyArray<{
 *   id: string,
 *   file: string,
 *   anchor: string,
 *   kind: 'trait' | 'struct' | 'enum',
 *   implementedBy?: string,
 *   why?: string,
 * }>}
 */
export const CONTRACT_RULES = Object.freeze([
  {
    id: 'database-driver',
    file: 'packages/driver-api/src/traits.rs',
    anchor: 'pub trait DatabaseDriver',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'The trait every driver implements; a signature change is a recompile requirement for the whole fleet.',
  },
  {
    id: 'key-value-driver',
    file: 'packages/driver-api/src/traits.rs',
    anchor: 'pub trait KeyValueDriver',
    kind: 'trait',
    implementedBy: 'key/value drivers',
    why: 'Same reasoning as DatabaseDriver, on the narrower KV surface.',
  },
  {
    id: 'driver-factory',
    file: 'packages/driver-api/src/factory.rs',
    anchor: 'pub trait DatabaseDriverFactory',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'Host registration goes through factories only (src-tauri DriverRegistry); changing it changes the discovery contract.',
  },
  {
    id: 'resource-provider',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub trait ResourceProvider',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'Supplies handles, namespaces and command execution to the host.',
  },
  {
    id: 'budget-port',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub trait BudgetPort',
    kind: 'trait',
    implementedBy: 'the host, consumed by drivers',
    why: 'The downlink channel for physical quota; the host implements it and drivers release against it.',
  },
  {
    id: 'resource-error',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub enum ResourceError',
    kind: 'enum',
    implementedBy: 'both sides, as an error contract',
    why: 'Error variants are matched on by the host; adding one is additive, renaming or narrowing is breaking.',
  },
  {
    id: 'driver-command-definition',
    file: 'packages/driver-api/src/command.rs',
    anchor: 'pub struct DriverCommandDefinition',
    kind: 'struct',
    implementedBy: 'drivers produce it, host dispatches it',
    why: 'Every SQL-editor action routes through execute_driver_command, so this struct is the command wire shape.',
  },
  {
    id: 'capability-set',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub struct CapabilitySet',
    kind: 'struct',
    implementedBy: 'every out-of-tree driver',
    why: 'The declaration a driver hands the host; a new domain is additive exactly because its Default is Unknown.',
  },
  {
    id: 'capability-registry',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub struct CapabilityRegistry',
    kind: 'struct',
    implementedBy: 'the host',
    why: 'The fail-closed surface that turns an Unsupported capability into an error instead of a silent no-op.',
  },
  {
    id: 'reuse-driver',
    file: 'packages/driver-api/src/reuse.rs',
    anchor: 'pub struct ReuseDriver',
    kind: 'struct',
    implementedBy: 'every out-of-tree driver',
    why: 'Constructed by every driver to satisfy the reuse contract; its fields are how reset is requested.',
  },
]);

/**
 * The reconciliation matrix, stated as data.
 *
 * `requires` names the version numbers that must have gone *up* relative to the
 * base when a change of `class` lands inside a governed span.
 *
 * @type {Readonly<{
 *   protocol: string,
 *   minProtocol: string,
 *   crateVersion: string,
 *   classes: Readonly<Record<'breaking' | 'additive' | 'cosmetic', { requires: string[], rationale: string }>>,
 * }>}
 */
export const COMPAT_MATRIX = Object.freeze({
  protocol: 'PROTOCOL_VERSION (packages/driver-api/src/lib.rs)',
  minProtocol: 'MIN_PROTOCOL_VERSION (packages/driver-api/src/lib.rs)',
  crateVersion: 'version (packages/driver-api/Cargo.toml)',
  // The rule table below is not invented here: it is the mechanical form of
  // `docs/architecture/platform/driver-capability-migration.md` §5.3
  // 「哪些改动是 breaking」, whose stated decision principle is 「老驱动在新宿主上
  // 是否仍能安全运行」. That table's six rows map onto the three classes below
  // as follows.
  //
  //   §5.3 新增 trait 方法且带 fail-closed 默认体   -> additive
  //   §5.3 新增 trait 方法没有默认体 / 改 DTO 字段必填性 -> breaking
  //   §5.3 改能力枚举取值或语义                       -> breaking
  //   §5.3 放宽已声明 unsupported 的行为              -> cosmetic (no bump)
  //   §5.3 收紧已声明 supported 的行为                 -> breaking
  //
  // §5.3's 「升 MIN + 升 PROTOCOL」 is encoded as a required `PROTOCOL_VERSION`
  // bump plus the standing `min-protocol-never-lowered` invariant, not as a
  // required MIN bump: §5.1 records `MIN = 1` against `PROTOCOL = 4` as a
  // deliberate surviving window, and §5.4's own step 1 is the PROTOCOL bump. A
  // rule that demanded a MIN bump on every breaking change would have failed
  // the crate's own 1 -> 2 -> 3 -> 4 history.
  sourceDoc: 'docs/architecture/platform/driver-capability-migration.md §5.3, §5.4',
  classes: Object.freeze({
    breaking: Object.freeze({
      requires: ['protocol'],
      rationale:
        'A removed or altered line inside a governed span changes what a driver compiled against the old crate must send or expect; no [MIN_PROTOCOL_VERSION, PROTOCOL_VERSION] window expresses that, so the protocol generation must move (migration doc §5.4 step 1).',
    }),
    additive: Object.freeze({
      requires: ['crateVersion'],
      rationale:
        'A new struct field, enum variant or defaulted trait method leaves existing implementors compiling and declaring nothing new, so the crate moves without the protocol moving (migration doc §5.3 row 1).',
    }),
    cosmetic: Object.freeze({
      requires: [],
      rationale:
        'Comment-only and whitespace-only edits carry no contract meaning and move no version number; §5.3 row 5 (放宽已声明 unsupported 的行为) is the runtime counterpart.',
    }),
  }),
});

/** @type {Readonly<Record<string, readonly string[]>>} */
export const CLASS_REQUIRES = Object.freeze(
  Object.fromEntries(
    Object.entries(COMPAT_MATRIX.classes).map(([cls, def]) => [cls, def.requires]),
  ),
);

/**
 * Invariants that hold regardless of the diff.
 *
 * - `min-protocol-never-lowered` is the plan's explicit prohibition
 *   (`platform-development-plan.md:97`): lowering the minimum widens the set of
 *   drivers the host claims to serve, which is how a breaking change gets
 *   disguised as a compat fix.
 * - `protocol-window-non-empty` catches the other way of breaking drivers by
 *   accident — raising `PROTOCOL_VERSION` past `MIN_PROTOCOL_VERSION` closes
 *   the degraded window and refuses every currently-shipping driver.
 * - `crate-version-advanced` rejects a *decrease* of the crate version, which
 *   on a published crate (`datazen-driver-api` is on crates.io) is not a
 *   rollback but an unpublishable state.
 */
export const STATIC_INVARIANTS = Object.freeze([
  Object.freeze({
    id: 'min-protocol-never-lowered',
    statement: 'MIN_PROTOCOL_VERSION must never decrease.',
    because:
      'Lowering the minimum widens the range of driver protocols this host claims to serve, which is exactly how a breaking change gets hidden behind a compat fix (platform-development-plan.md:97).',
  }),
  Object.freeze({
    id: 'protocol-window-non-empty',
    statement: 'MIN_PROTOCOL_VERSION <= PROTOCOL_VERSION.',
    because:
      'An empty window means every driver currently shipping is outside the supported range and gets refused at load.',
  }),
  Object.freeze({
    id: 'crate-version-advanced',
    statement: 'The datazen-driver-api crate version must never decrease.',
    because:
      'The crate is published; a lowered version is not a rollback but an unresolvable one.',
  }),
]);

/**
 * Read the numeric value of a `pub const NAME: u32 = N;` declaration.
 *
 * @param {string} source
 * @param {string} symbol
 * @returns {number | null} `null` when the declaration is absent or not a literal.
 */
export function readConstValue(source, symbol) {
  const pattern = new RegExp(
    `^\\s*pub\\s+const\\s+${symbol}\\s*:\\s*u32\\s*=\\s*(\\d+)\\s*;`,
    'm',
  );
  const match = pattern.exec(source);
  return match ? Number.parseInt(match[1], 10) : null;
}

/**
 * @param {string} source
 * @returns {number | null}
 */
export function readCrateVersion(source) {
  const match = /^\s*version\s*=\s*"([^"]+)"/m.exec(source);
  return match ? match[1] : null;
}

/**
 * A line carries no contract meaning if it is blank or only a `//` comment.
 *
 * Doc comments above a definition are the case this exists for: they are
 * routinely rewritten in the same commit as an unrelated change and must not
 * be read as a contract edit.
 *
 * @param {string} raw
 * @returns {boolean}
 */
export function isCosmeticLine(raw) {
  const trimmed = raw.trim();
  if (trimmed === '') return true;
  if (trimmed.startsWith('//')) return true;
  // A `/* ... */` block comment, in whole or in its continuation lines. A
  // line can only start with `*` inside such a comment in this crate's Rust
  // style, so this does not swallow a multiplication continuation.
  if (trimmed.startsWith('/*')) return true;
  if (trimmed === '*/') return true;
  if (trimmed.startsWith('*') && !trimmed.startsWith('*/')) return true;
  return false;
}

/**
 * Is this an added line that declares a required trait item (no default body)?
 *
 * Matches `fn foo(&self, ..);` and `const NAME: T;` / `type X = Y;` inside a
 * trait, i.e. items an implementor must provide. A line ending in `{` has a
 * default body and is therefore source-compatible with existing implementors.
 *
 * @param {string} raw
 * @returns {boolean}
 */
export function isRequiredTraitItemAddition(raw) {
  const trimmed = raw.trim();
  if (trimmed.endsWith(';') && !trimmed.startsWith('//')) {
    return /\b(fn|const|type)\b/.test(trimmed);
  }
  return false;
}

/**
 * Resolve the line span of a governed item.
 *
 * Finds the line containing `anchor`, then walks brace depth from that line to
 * the matching close. For a single-line item (a `const`) the span is that one
 * line.
 *
 * @param {string} source
 * @param {string} anchor
 * @returns {{ start: number, end: number } | null} zero-based, `end` inclusive.
 */
export function findItemSpan(source, anchor) {
  const lines = source.split('\n');
  const start = lines.findIndex((line) => line.includes(anchor));
  if (start === -1) return null;

  let depth = 0;
  let opened = false;
  for (let i = start; i < lines.length; i += 1) {
    const line = stripStringAndCharLiterals(lines[i]);
    for (const ch of line) {
      if (ch === '{') {
        depth += 1;
        opened = true;
      } else if (ch === '}') {
        depth -= 1;
        if (opened && depth === 0) return { start, end: i };
      }
    }
    if (!opened && i === start) {
      // Single-line item such as `pub const PROTOCOL_VERSION: u32 = 4;`
      return { start, end: start };
    }
  }
  return null;
}

/**
 * Replace the contents of string and char literals with spaces so brace
 * counting is not fooled by `"}"` or a `'{'` in a default value.
 *
 * @param {string} line
 * @returns {string}
 */
export function stripStringAndCharLiterals(line) {
  return line
    .replace(/r?#*"[^"]*"#/g, (m) => ' '.repeat(m.length))
    .replace(/'[^']*'/g, (m) => ' '.repeat(m.length));
}

/**
 * Parse a `-U0` diff into per-new-file-line records.
 *
 * The old text of a removed line is retained, because "was this line a
 * comment?" cannot be answered from the new side. Dropping it would make every
 * doc-comment rewrite inside a governed span look like a deletion, which is
 * both a false positive and a false reason.
 *
 * @param {string} diff
 * @returns {Map<string, Map<number, { added?: string, removed?: string }>>}
 *   file -> new line number -> record. `removed` carries the old text and is
 *   positioned in *new* coordinates (the line it used to occupy).
 */
export function parseDiffByFile(diff) {
  /** @type {Map<string, Map<number, { added?: string, removed?: string }>>} */
  const out = new Map();
  let file = null;
  let newLine = 0;
  // The `---`/`+++` file headers look exactly like a removal and an addition.
  // Parsed as change lines they land on line 0, which is inside the span of
  // every governed item at the top of a file, so the checker would report a
  // spurious edit to that item on every single run. Change lines exist only
  // between a hunk header and the next one.
  let inHunk = false;

  for (const line of diff.split('\n')) {
    if (line.startsWith('diff --git ')) {
      const match = /^diff --git a\/(.+?) b\/(.+)$/.exec(line);
      file = match ? match[2] : null;
      if (file && !out.has(file)) out.set(file, new Map());
      inHunk = false;
      continue;
    }
    if (line.startsWith('@@')) {
      const header = /^\@\@ -\d+(?:,\d+)? \+(\d+)/.exec(line);
      newLine = header ? Number.parseInt(header[1], 10) : 0;
      inHunk = true;
      continue;
    }
    if (!file || !inHunk) continue;

    if (line.startsWith('+')) {
      pushRecord(out, file, newLine, { added: line.slice(1) });
      newLine += 1;
    } else if (line.startsWith('-')) {
      pushRecord(out, file, newLine, { removed: line.slice(1) });
    } else if (line.startsWith(' ') || line === '') {
      newLine += 1;
    }
  }
  return out;
}

/**
 * Record a change at a new-side line, merging rather than overwriting.
 *
 * A removal and the addition that replaces it occupy the *same* new-side line
 * position. Storing one record per line and letting the later write win drops
 * the old text — which is precisely the text that says whether a deleted line
 * was a signature or a comment, and therefore whether the change was breaking.
 *
 * @param {Map<string, Map<number, unknown>>} out
 * @param {string} file
 * @param {number} line
 * @param {{ added?: string, removed?: string }} record
 */
function pushRecord(out, file, line, record) {
  let records = out.get(file);
  if (!records) {
    records = new Map();
    out.set(file, records);
  }
  const existing = records.get(line);
  records.set(line, existing ? { ...existing, ...record } : record);
}

/**
 * Quote the first few lines of a change so the report names what actually
 * moved, rather than only how much of it there was.
 *
 * @param {string[]} lines
 * @param {number} [max]
 * @returns {string}
 */
function sample(lines, max = 2) {
  const shown = lines.slice(0, max).join(' | ');
  return lines.length > max ? `${shown} | ...` : shown;
}

/**
 * Classify one governed contract.
 *
 * @param {string} baseSource contents of the file at the base ref
 * @param {string} newSource contents of the file after the change
 * @param {{ id: string, file: string, anchor: string, kind: string }} rule
 * @param {Map<number, { added?: string, removed?: string }>} records
 * @returns {{ id: string, cls: 'breaking' | 'additive' | 'cosmetic', reason: string }}
 */
export function classifyContractChange(baseSource, newSource, rule, records) {
  const oldSpan = findItemSpan(baseSource, rule.anchor);
  const newSpan = findItemSpan(newSource, rule.anchor);

  if (!oldSpan && !newSpan) {
    return { id: rule.id, cls: 'cosmetic', reason: `${rule.id}: anchor not found in either side` };
  }
  if (!oldSpan) {
    return {
      id: rule.id,
      cls: 'breaking',
      reason: `${rule.id}: ${rule.anchor} did not exist at the base ref (removed outright)`,
    };
  }
  if (!newSpan) {
    return {
      id: rule.id,
      cls: 'breaking',
      reason: `${rule.id}: ${rule.anchor} no longer resolves after the change`,
    };
  }

  let removed = 0;
  let addedMeaningful = 0;
  const meaningfulAdditions = [];
  const requiredAdditions = [];
  let tolerated = 0;

  for (const [lineNo, record] of records) {
    // A line is in scope when it falls inside the span on *either* side, which
    // is what makes an insertion at the head of an item (shifting every line
    // below it) count as a change to the item rather than to the file.
    const inScope =
      (lineNo >= oldSpan.start && lineNo <= oldSpan.end) ||
      (lineNo >= newSpan.start && lineNo <= newSpan.end);
    if (!inScope) continue;

    if (record.removed !== undefined) {
      if (!isCosmeticLine(record.removed)) {
        removed += 1;
      }
      continue;
    }
    const text = record.added ?? '';
    if (isCosmeticLine(text)) {
      tolerated += 1;
      continue;
    }
    addedMeaningful += 1;
    meaningfulAdditions.push(text.trim());
    if (rule.kind === 'trait' && isRequiredTraitItemAddition(text)) {
      requiredAdditions.push(text.trim());
    }
  }

  if (removed > 0) {
    return {
      id: rule.id,
      cls: 'breaking',
      reason: `${rule.id}: ${removed} line(s) removed from ${rule.anchor}`,
    };
  }
  if (requiredAdditions.length > 0) {
    return {
      id: rule.id,
      cls: 'breaking',
      reason: `${rule.id}: required trait item(s) added to ${rule.anchor} with no default body: ${requiredAdditions.join(' | ')}`,
    };
  }
  if (addedMeaningful > 0) {
    return {
      id: rule.id,
      cls: 'additive',
      reason: `${rule.id}: ${addedMeaningful} line(s) added to ${rule.anchor} (${tolerated} cosmetic ignored), e.g. ${sample(meaningfulAdditions)}`,
    };
  }
  return { id: rule.id, cls: 'cosmetic', reason: `${rule.id}: no non-cosmetic change` };
}

/**
 * Run git in the repository.
 *
 * @param {string[]} args
 * @param {string} cwd
 * @returns {{ status: number | null, stdout: string, stderr: string }}
 */
function git(args, cwd) {
  const res = spawnSync('git', args, { cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  return { status: res.status, stdout: res.stdout ?? '', stderr: res.stderr ?? '' };
}

/**
 * Resolve the ref the change is measured against: the merge base of HEAD and
 * `main`, overridable with `--base=<ref>`.
 *
 * The merge base rather than `main` itself matters because a branch can be
 * behind; measuring against `main` would attribute another branch's commits to
 * this change and demand version bumps for edits nobody made here.
 *
 * @param {string} cwd
 * @param {string | undefined} explicit
 * @returns {string}
 */
export function resolveBaseRef(cwd, explicit) {
  if (explicit) return explicit;
  const mergeBase = git(['merge-base', 'HEAD', 'main'], cwd);
  if (mergeBase.status === 0 && mergeBase.stdout.trim()) return mergeBase.stdout.trim();
  const head = git(['rev-parse', 'HEAD'], cwd);
  if (head.status === 0) return head.stdout.trim();
  return 'HEAD';
}

/**
 * @param {string} cwd
 * @param {string} path repo-relative
 * @param {string} ref
 * @returns {string | null} `null` when the path did not exist at `ref`.
 */
export function readBlobAtRef(cwd, path, ref) {
  const res = git(['show', `${ref}:${path}`], cwd);
  return res.status === 0 ? res.stdout : null;
}

/**
 * Evaluate the whole matrix.
 *
 * @param {{ cwd?: string, base?: string }} [options]
 * @returns {{
 *   base: string,
 *   violations: Array<{ code: string, message: string }>,
 *   findings: Array<{ id: string, cls: string, reason: string }>,
 *   before: { protocol: number | null, minProtocol: number | null, crateVersion: string | null },
 *   after: { protocol: number | null, minProtocol: number | null, crateVersion: string | null },
 * }}
 */
export function evaluate(options = {}) {
  const cwd = options.cwd ?? ROOT;
  const base = resolveBaseRef(cwd, options.base);
  const violations = [];
  const findings = [];

  const libBefore = readBlobAtRef(cwd, PROTOCOL_VERSION_FILE, base);
  const manifestBefore = readBlobAtRef(cwd, GOVERNED_CRATE_MANIFEST, base);
  const libAfter = readFileSync(join(cwd, PROTOCOL_VERSION_FILE), 'utf8');
  const manifestPath = join(cwd, GOVERNED_CRATE_MANIFEST);
  const manifestAfter = existsSync(manifestPath)
    ? readFileSync(manifestPath, 'utf8')
    : null;

  const before = {
    protocol: libBefore === null ? null : readConstValue(libBefore, PROTOCOL_VERSION_SYMBOL),
    minProtocol:
      libBefore === null ? null : readConstValue(libBefore, MIN_PROTOCOL_VERSION_SYMBOL),
    crateVersion: manifestBefore === null ? null : readCrateVersion(manifestBefore),
  };
  const after = {
    protocol: readConstValue(libAfter, PROTOCOL_VERSION_SYMBOL),
    minProtocol: readConstValue(libAfter, MIN_PROTOCOL_VERSION_SYMBOL),
    crateVersion: manifestAfter === null ? null : readCrateVersion(manifestAfter),
  };

  if (after.protocol === null || after.minProtocol === null) {
    violations.push({
      code: 'unreadable-version-constants',
      message: `${LOG_PREFIX} could not read ${PROTOCOL_VERSION_SYMBOL} / ${MIN_PROTOCOL_VERSION_SYMBOL} from ${PROTOCOL_VERSION_FILE}`,
    });
    return { base, violations, findings, before, after };
  }

  const protocolRaised = before.protocol !== null && after.protocol > before.protocol;
  const crateVersionRaised = compareCrateVersions(after.crateVersion, before.crateVersion) > 0;

  if (
    before.minProtocol !== null &&
    after.minProtocol < before.minProtocol
  ) {
    violations.push({
      code: 'min-protocol-never-lowered',
      message: `${LOG_PREFIX} MIN_PROTOCOL_VERSION was lowered ${before.minProtocol} -> ${after.minProtocol}. Lowering the minimum widens the range of driver protocols this host claims to serve; a breaking change must never be disguised by lowering the minimum (platform-development-plan.md:97).`,
    });
  }
  if (after.minProtocol > after.protocol) {
    violations.push({
      code: 'protocol-window-non-empty',
      message: `${LOG_PREFIX} MIN_PROTOCOL_VERSION (${after.minProtocol}) > PROTOCOL_VERSION (${after.protocol}); the supported window is empty and every shipping driver would be refused at load.`,
    });
  }
  if (compareCrateVersions(after.crateVersion, before.crateVersion) < 0) {
    violations.push({
      code: 'crate-version-advanced',
      message: `${LOG_PREFIX} ${GOVERNED_CRATE_MANIFEST} version was lowered ${before.crateVersion ?? '?'} -> ${after.crateVersion ?? '?'}; the crate is published, so a lowered version is not a rollback.`,
    });
  }

  const diff = git(['diff', '-U0', base], cwd);
  if (diff.status !== 0) {
    violations.push({
      code: 'diff-failed',
      message: `${LOG_PREFIX} git diff ${base} failed: ${diff.stderr.trim()}`,
    });
    return { base, violations, findings, before, after };
  }
  const byFile = parseDiffByFile(diff.stdout);

  for (const rule of CONTRACT_RULES) {
    const records = byFile.get(rule.file);
    if (!records || records.size === 0) continue;

    const newSource = readFileSync(join(cwd, rule.file), 'utf8');
    const baseSource = readBlobAtRef(cwd, rule.file, base);
    const finding = classifyContractChange(baseSource ?? '', newSource, rule, records);
    if (finding.cls === 'cosmetic') continue;

    findings.push({ ...finding, file: rule.file, why: rule.why });

    const requires = CLASS_REQUIRES[finding.cls] ?? [];
    if (requires.includes('protocol') && !protocolRaised) {
      violations.push({
        code: 'breaking-change-requires-protocol-bump',
        message: `${LOG_PREFIX} ${rule.file}: ${finding.reason}. This is a breaking change to a public contract, so ${COMPAT_MATRIX.protocol} must be raised (was ${before.protocol ?? '?'}, now ${after.protocol}). Rationale: ${rule.why}`,
      });
    }
    if (requires.includes('crateVersion') && !crateVersionRaised) {
      violations.push({
        code: 'additive-change-requires-crate-bump',
        message: `${LOG_PREFIX} ${rule.file}: ${finding.reason}. Adding to a governed contract must move ${COMPAT_MATRIX.crateVersion} (was ${before.crateVersion ?? '?'}, now ${after.crateVersion ?? '?'}). ${rule.why}`,
      });
    }
  }

  return { base, violations, findings, before, after };
}

/**
 * Numeric-aware comparison of two `major.minor.patch` strings.
 *
 * @param {string | null} left
 * @param {string | null} right
 * @returns {number} `-1`, `0` or `1`; `0` when either side is unreadable.
 */
export function compareCrateVersions(left, right) {
  if (left === null || right === null) return 0;
  const a = left.split('.').map((n) => Number.parseInt(n, 10) || 0);
  const b = right.split('.').map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    const av = a[i] ?? 0;
    const bv = b[i] ?? 0;
    if (av !== bv) return av > bv ? 1 : -1;
  }
  return 0;
}

/**
 * @returns {number} process exit code
 */
function main() {
  const baseArg = process.argv.find((a) => a.startsWith('--base='));
  const base = baseArg ? baseArg.slice('--base='.length) : undefined;

  const result = evaluate({ base });

  if (result.findings.length > 0) {
    console.log(`${LOG_PREFIX} governed contract changes vs ${result.base}:`);
    for (const f of result.findings) {
      console.log(`  [${f.cls}] ${f.reason}`);
    }
  }

  if (result.violations.length > 0) {
    console.error(`${LOG_PREFIX} compatibility reconciliation failed (base ${result.base}):`);
    for (const v of result.violations) {
      console.error(`  ${v.code}: ${v.message}`);
    }
    return 1;
  }

  console.log(
    `${LOG_PREFIX} ok (base ${result.base}; PROTOCOL_VERSION ${result.before.protocol} -> ${result.after.protocol}, MIN_PROTOCOL_VERSION ${result.before.minProtocol} -> ${result.after.minProtocol}, crate ${result.before.crateVersion} -> ${result.after.crateVersion}).`,
  );
  return 0;
}

if (process.argv[1] && import.meta.url === `file://${resolve(process.argv[1])}`) {
  process.exit(main());
}