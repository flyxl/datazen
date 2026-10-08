#!/usr/bin/env node
/**
 * @file Driver protocol / crate compatibility gate for `packages/driver-api`.
 *
 * A driver compiled against an older `datazen-driver-api` is not wrong; it is
 * simply speaking a different contract. `platform-development-plan.md` §6
 * 「P2：Driver 固定资源与可选能力契约」 requires that changing a public
 * contract be reconciled against three separate numbers — the crate SemVer,
 * `PROTOCOL_VERSION` and `MIN_PROTOCOL_VERSION` — and the same document rules that
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
 * ## Why an added enum *variant* is breaking, and a struct field is not
 *
 * A span tells the checker that a contract was touched. It does not tell it
 * what changed, which is why an enum's variant list was invisible: every edit
 * to it diffs as one added line, the same shape as a doc comment. So for
 * `kind: 'enum'` the members are now compared across the change.
 *
 * An added variant is `breaking` unless the enum carried `#[non_exhaustive]` at
 * the base ref. `additive` means "existing implementors keep compiling and have
 * nothing new to declare", and that is false for an added variant of an
 * exhaustively matched enum: a downstream `match` has no arm for it and rustc
 * reports E0004, which is a recompile requirement rather than a version window.
 * The `#[non_exhaustive]` case is a genuine exception and is honoured — every
 * such `match` already had to carry a wildcard arm, which absorbs the new
 * variant — and the *base* ref is what decides, because the question is whether
 * drivers written against the previous crate were already forced to hold one.
 * `lib/driver-protocol-members.mjs` states the rule in full.
 *
 * Structs are not reclassified: a new public field stays additive, as described
 * above. The comparison there exists to name the field in the report.
 *
 * @module check/driver-protocol-compat
 */

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { COMPAT_MATRIX } from './lib/compatMatrix.mjs';
import { detectSourceBreak } from './lib/sourceBreak.mjs';
import { CONTRACT_RULES } from './lib/driver-protocol-rules.mjs';
import { inspectMemberDiff, stripStringAndCharLiterals } from './lib/driver-protocol-members.mjs';

// Re-exported rather than left private: the rule table and the literal stripper
// are part of this module's published surface, and every gate and test already
// imports them from here.
export { CONTRACT_RULES };
export { stripStringAndCharLiterals } from './lib/driver-protocol-members.mjs';

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
 * The table itself lives in `lib/driver-protocol-rules.mjs` — it is data, and
 * putting it in its own module keeps this file's logic readable. Re-exported
 * here under the name every gate, CLI and test already imports.
 *
 * `kind` selects how a change inside the item's span is read:
 *
 * - `trait` — an added line that declares a method with no default body (ends
 *   in `;`) breaks every existing implementor, so it is breaking. An added
 *   line with a default body (`{`) is additive.
 * - `struct` — an added line is additive, provided the fail-closed `Default`
 *   invariant holds on the Rust side. A removed line is breaking.
 * - `enum` — the *members* decide, not the lines: a removed or renamed variant
 *   is breaking, and so is an added variant unless the enum carried
 *   `#[non_exhaustive]` at the base ref. `lib/driver-protocol-members.mjs`
 *   carries the rule and its justification.
 */

/**
 * The reconciliation matrix, stated as data.
 *
 * Re-exported from `lib/` — the matrix and the gate that consumes it are one
 * subject with two files, and this module is where every gate and test has
 * always imported it from, so the import path is deliberately unchanged.
 */
export { COMPAT_MATRIX } from './lib/compatMatrix.mjs';

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
 *   (`platform-development-plan.md` §6「P2：Driver 固定资源与可选能力契约」):
 *   lowering the minimum widens the set of drivers the host claims to serve,
 *   which is how a breaking change gets disguised as a compat fix.
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
      'Lowering the minimum widens the range of driver protocols this host claims to serve, which is exactly how a breaking change gets hidden behind a compat fix (platform-development-plan.md §6「P2：Driver 固定资源与可选能力契约」: when a public contract changes, reconcile crate SemVer, PROTOCOL_VERSION and minimum compatible).',
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
 * Does `line` declare `anchor`, as opposed to merely containing it?
 *
 * `includes` alone answers the wrong question. Rust identifiers are matched as
 * plain substrings, so the anchor `pub trait KeyValueDriver` is found inside
 * `pub trait KeyValueDriverV2` — which is exactly what happens when a governed
 * trait gets renamed. The span then lands on the *new* declaration, the base
 * side has no anchor left to compare against, and a rename that breaks every
 * out-of-tree `impl` is reported as an ordinary additive change. Requiring both
 * sides of the match to be identifier boundaries makes the anchor match the
 * declaration it names and nothing that merely starts with it.
 *
 * @param {string} line
 * @param {string} anchor
 * @returns {boolean}
 */
function matchesAnchor(line, anchor) {
  const isIdentifierChar = (ch) => ch !== undefined && /[A-Za-z0-9_]/.test(ch);
  let from = 0;
  for (;;) {
    const at = line.indexOf(anchor, from);
    if (at === -1) return false;
    if (!isIdentifierChar(line[at + anchor.length]) && !isIdentifierChar(line[at - 1])) {
      return true;
    }
    from = at + 1;
  }
}

/**
 * Resolve the line span of a governed item.
 *
 * Finds the line declaring `anchor`, then walks brace depth from that line to
 * the matching close. For a single-line item (a `const`) the span is that one
 * line.
 *
 * @param {string} source
 * @param {string} anchor
 * @returns {{ start: number, end: number } | null} zero-based, `end` inclusive.
 */
export function findItemSpan(source, anchor) {
  const lines = source.split('\n');
  const start = lines.findIndex((line) => matchesAnchor(line, anchor));
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
  /** @type {Map<string, Map<number, { added?: string, removed?: string[] }>>} */
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
      pushRecord(out, file, newLine, { removed: [line.slice(1)] });
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
 * position, so the two must share one record or the old text is lost — and the
 * old text is precisely what says whether a deleted line was a signature or a
 * comment, and therefore whether the change was breaking.
 *
 * Several *removals* share that position too, and not as a curiosity: in a
 * pure-deletion hunk `newLine` never advances, so deleting ten lines emits
 * `@@ -a,10 +b,0 @@` followed by ten `-` lines that all land on key `b`. The
 * map is keyed by new-side line, so these are one key and something has to
 * merge them. Letting the last write win meant a deletion of
 * `fn driver_type(&self) -> DatabaseType;` plus the blank line after it — the
 * shape of every ordinary deletion — kept only the blank, classified the whole
 * removal as cosmetic, and let a required trait method disappear with the gate
 * reporting `ok`. Removals therefore accumulate; an addition never does,
 * because every `+` line advances `newLine` and so gets its own key.
 *
 * @param {Map<string, Map<number, unknown>>} out
 * @param {string} file
 * @param {number} line
 * @param {{ added?: string, removed?: string[] }} record
 */
function pushRecord(out, file, line, record) {
  let records = out.get(file);
  if (!records) {
    records = new Map();
    out.set(file, records);
  }
  const existing = records.get(line);
  if (!existing) {
    records.set(line, record);
    return;
  }
  const merged = { ...existing };
  if (record.added !== undefined) merged.added = record.added;
  if (record.removed !== undefined) {
    merged.removed = [...(existing.removed ?? []), ...record.removed];
  }
  records.set(line, merged);
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
 * @returns {{ id: string, cls: 'breaking' | 'additive' | 'cosmetic' | 'source-breaking',
 *   reason: string, sourceBreak?: { attributes: string[], effect: string, migration: string } }}
 *   `source-breaking` and `sourceBreak` must both appear here: the body emits
 *   them (see the `detectSourceBreak` branch below), so an annotation that
 *   lists only the three original classes describes a value the function
 *   never returns — which makes every exhaustive `switch` written against it
 *   wrong. `scripts/lib/compatMatrix.mjs` already declares the four-value
 *   union on `classes`; this annotation is the one that had drifted.
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
      // Every line the hunk removed at this key counts on its own. Counting the
      // record once instead made a multi-line deletion look like a single line,
      // which is harmless for the verdict (still non-zero) but wrong in the
      // report, and — see `pushRecord` — losing the key entirely is not.
      for (const removedText of record.removed) {
        if (!isCosmeticLine(removedText)) {
          removed += 1;
        }
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

  // The span says the contract was touched. The members say what moved inside
  // it, and for an enum that is the whole question — a variant addition diffs
  // as one added line, the same shape as a doc comment, so it would otherwise
  // fall through to `additive` here. `rule.kind` decides what is compared: a
  // struct's public field names (for the report only, never the verdict), an
  // enum's variants, where a gained variant really can break every downstream
  // `match`, or a trait's item signatures.
  const members = inspectMemberDiff(rule, baseSource, newSource, oldSpan, newSpan);

  // A trait is the one governed kind whose line range and whose contract are
  // genuinely different things. An implementor writes signatures and inherits
  // every default body, so the two facts that can force it to be edited are a
  // signature that disappeared or changed shape and a new item with no default
  // body. Everything else inside the span — bodies moving to sibling functions,
  // delegating lines replacing them, the closing brace shifting because 900
  // lines were extracted above it — leaves the signature set untouched and
  // leaves every out-of-tree implementor compiling. Reading those as removed
  // lines made a pure refactor demand a PROTOCOL_VERSION bump, which is the one
  // obligation no version window can honestly express here, so for a trait the
  // signature delta decides and the line counts are reported as evidence.
  if (rule.kind === 'trait' && members.trait !== null && (removed > 0 || addedMeaningful > 0)) {
    const { removed: gone, addedRequired, addedDefaulted, baseTotal, total } = members.trait;
    if (gone.length > 0) {
      return {
        id: rule.id,
        cls: 'breaking',
        reason: `${rule.id}: ${gone.length} trait item signature(s) removed or altered in ${rule.anchor} — an out-of-tree driver implementing them no longer compiles: ${gone.join(' | ')}`,
      };
    }
    if (addedRequired.length > 0) {
      return {
        id: rule.id,
        cls: 'breaking',
        reason: `${rule.id}: required trait item(s) added to ${rule.anchor} with no default body: ${addedRequired.join(' | ')}`,
      };
    }
    const sourceBreak = detectSourceBreak(meaningfulAdditions, rule);
    if (sourceBreak) {
      return {
        id: rule.id,
        cls: 'source-breaking',
        reason: `${rule.id}: ${sourceBreak.attributes.join(', ')} added to ${rule.anchor} — ${sourceBreak.effect}`,
        sourceBreak,
      };
    }
    // Still additive, never cosmetic: something the crate serves moved, so the
    // crate version is owed, but no implementor has to declare anything new.
    const gained =
      addedDefaulted.length > 0
        ? ` gained ${addedDefaulted.length} defaulted trait item(s) (${addedDefaulted.join(', ')}) and lost none of its ${total} item signature(s) (${baseTotal} before)`
        : ` kept all ${total} item signature(s) unchanged`;
    const moved = removed > 0 ? `, with ${removed} line(s) moved or edited inside the span` : '';
    return {
      id: rule.id,
      cls: 'additive',
      reason: `${rule.id}: ${rule.anchor}${gained}${moved} (${addedMeaningful} line(s) added, ${tolerated} cosmetic ignored), e.g. ${sample(meaningfulAdditions)}`,
    };
  }

  if (removed > 0) {
    return {
      id: rule.id,
      cls: 'breaking',
      // The verdict is unchanged; only the reason gains the member that
      // disappeared, because a variant's name is what an out-of-tree author
      // has to go and look for.
      reason: members.removalReason ?? `${rule.id}: ${removed} line(s) removed from ${rule.anchor}`,
    };
  }
  if (members.addedBreaking) {
    return { id: rule.id, cls: 'breaking', reason: members.addedBreaking };
  }
  if (requiredAdditions.length > 0) {
    return {
      id: rule.id,
      cls: 'breaking',
      reason: `${rule.id}: required trait item(s) added to ${rule.anchor} with no default body: ${requiredAdditions.join(' | ')}`,
    };
  }
  if (addedMeaningful > 0) {
    // An addition can still be a break: `#[non_exhaustive]` shows up as one
    // added line, so without this the ladder below would call it `additive`
    // and tell out-of-tree drivers they are unaffected. Checked after the
    // removal branches, because a removal is a break either way.
    const sourceBreak = detectSourceBreak(meaningfulAdditions, rule);
    if (sourceBreak) {
      return {
        id: rule.id,
        cls: 'source-breaking',
        reason: `${rule.id}: ${sourceBreak.attributes.join(', ')} added to ${rule.anchor} — ${sourceBreak.effect}`,
        sourceBreak,
      };
    }
    return {
      id: rule.id,
      cls: 'additive',
      reason: `${rule.id}: ${addedMeaningful} line(s) added to ${rule.anchor} (${tolerated} cosmetic ignored), e.g. ${sample(meaningfulAdditions)}${members.addedNote === null ? '' : ` [${members.addedNote}]`}`,
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
      message: `${LOG_PREFIX} MIN_PROTOCOL_VERSION was lowered ${before.minProtocol} -> ${after.minProtocol}. Lowering the minimum widens the range of driver protocols this host claims to serve; a breaking change must never be disguised by lowering the minimum (platform-development-plan.md §6「P2：Driver 固定资源与可选能力契约」: when a public contract changes, reconcile crate SemVer, PROTOCOL_VERSION and minimum compatible).`,
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
    const rulePath = join(cwd, rule.file);
    // A rule whose file is not in the tree cannot be judged, and that fact has
    // to be a verdict rather than silence. Two ways to get here, and both used
    // to exit 0 while claiming `ok`:
    //   * the path was renamed or deleted, so git still reports records for the
    //     old path — `readFileSync` then threw ENOENT out of `evaluate` and CI
    //     saw an unhandled exception rather than a compatibility verdict;
    //   * the path was wrong to begin with, or the file moved, so there are no
    //     records at all and `records.size === 0` skipped the rule entirely.
    //     A rule that silently stops covering its contract reports nothing when
    //     that contract is edited, which is the one outcome a compatibility
    //     gate must never produce. Both are violations, and the first is also a
    //     breaking change to a governed contract in its own right.
    if (!existsSync(rulePath)) {
      const deletedByThisChange = records !== undefined && records.size > 0;
      violations.push({
        code: 'contract-rule-file-missing',
        message: deletedByThisChange
          ? `${LOG_PREFIX} rule '${rule.id}' governs ${rule.file}, and this change removed or renamed it (${records.size} diff record(s)). A governed contract cannot disappear without ${COMPAT_MATRIX.protocol} being raised.`
          : `${LOG_PREFIX} rule '${rule.id}' governs ${rule.file}, which does not exist in the working tree. The gate therefore reads nothing for this contract and reports nothing when it changes; point the rule at the file that holds the contract.`,
      });
      continue;
    }
    if (!records || records.size === 0) continue;

    const newSource = readFileSync(rulePath, 'utf8');
    // A contract relocated into its own module has no blob at the base ref. Its
    // base side is then `previousFile`, so a pure move reconciles against the
    // signature set it had rather than reading as a contract deleted outright.
    // Only consulted when `file` itself is absent at the base ref, which is what
    // makes the record expire on its own once the base ref moves past the move.
    const baseSource =
      readBlobAtRef(cwd, rule.file, base) ??
      (rule.previousFile === undefined ? null : readBlobAtRef(cwd, rule.previousFile, base));
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
    // A source break fires this one unconditionally — unlike the two above,
    // there is no "already satisfied" state to latch onto. Raising the crate
    // version does not tell an out-of-tree author what to type, and nothing
    // else in this checker will, so the recipe is the violation.
    if (finding.sourceBreak) {
      violations.push({
        code: 'source-break-requires-out-of-tree-migration',
        message: `${LOG_PREFIX} ${rule.file}: ${finding.reason}.\n${finding.sourceBreak.migration}\n${rule.why}`,
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