/**
 * Guard: every error code the back end actually produces must be registered on the front
 * end, otherwise `classifyGridError` returns `unknown` and the front end cannot branch.
 *
 * The back end prefixes its refusals with `grid.<domain>.<reason>: ` (00-contracts §8.2).
 * Wave 1 produced 27 throw sites across 8 codes; the literal messages below are copied from
 * those sites so this file fails the moment a code is removed from the registry.
 *
 * The hand-copied list above only proves the codes that were known when it was written. It
 * cannot see a code a later track adds, and every track so far has added one: `grid.fk.*`,
 * `grid.cell.*`, `grid.commit.*`, `grid.insert.*` — and then the front end did not register
 * it, so the message arrived unclassified. The source scan below closes that hole: it reads
 * the Rust sources that raise the codes and compares the two sets, so the leak is caught the
 * moment it is written rather than the next time somebody remembers.
 */
import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { GRID_ERROR_CODES, classifyGridError } from '../gridErrors';

/** Real messages, copied verbatim from the back-end producers. */
const PRODUCED_MESSAGES: readonly { code: string; message: string }[] = [
  {
    code: 'grid.commit.stalePlan',
    message: 'grid.commit.stalePlan: Row change plan fingerprint is stale or was modified',
  },
  {
    code: 'grid.commit.stalePlan',
    message: 'grid.commit.stalePlan: Database context changed; preview the changes again',
  },
  {
    code: 'grid.commit.affectedMismatch',
    message: 'grid.commit.affectedMismatch: INSERT affected 2 rows; refusing ambiguous write',
  },
  {
    code: 'grid.commit.conflictingIntents',
    message:
      'grid.commit.conflictingIntents: a row marked for delete cannot also carry column writes',
  },
  {
    code: 'grid.cell.noRowIdentity',
    message: "grid.cell.noRowIdentity: UPDATE requires the table's primary-key identity",
  },
  {
    code: 'grid.cell.ambiguousIdentity',
    message: 'grid.cell.ambiguousIdentity: Row changes contain duplicate primary-key identity',
  },
  {
    code: 'grid.cell.readOnly',
    message: 'grid.cell.readOnly: Connection is read-only; row changes are not allowed',
  },
  {
    code: 'grid.insert.unknownColumn',
    message: "grid.insert.unknownColumn: INSERT writes column(s) that do not exist on 'users'",
  },
  {
    code: 'grid.filter.incomplete',
    message: 'grid.filter.incomplete: column "id" needs a value (operator \'eq\')',
  },
  {
    code: 'grid.filter.unsupportedOperator',
    message:
      "grid.filter.unsupportedOperator: this database does not support the 'regex' filter on column \"name\"; try 'like' or 'contains' instead",
  },
];

const REPO_ROOT = resolve(__dirname, '../../..');
const WORKSPACE_MANIFEST = join(REPO_ROOT, 'Cargo.toml');

/**
 * The Rust trees that may raise a `grid.*` code: every source tree of every
 * workspace member.
 *
 * The list is read from the workspace manifest instead of being written out
 * here, so a new crate is covered the day it is added — a hand-written list
 * would rot into the same hole this guard exists to close. A member's `src`
 * and `tests` directories and its build script are all scanned; the `tests`
 * directory is where the integration tests live, and a code that only appears
 * there is still a code the front end has to know.
 *
 * **Two boundaries of this derivation, both deliberate:**
 *
 * * The manifest's `exclude` list is **not** read. A driver that is excluded
 *   from the workspace but still on disk would be scanned anyway, so its
 *   codes would be required to be registered even though nothing ships them.
 *   Today the excluded drivers (`kiwi`, `superset`) are not on disk at all,
 *   so this has no effect.
 * * The member patterns are expanded by hand: only a trailing `/*` is
 *   understood. A different glob shape would silently scan less — which is
 *   why the test below pins a **lower bound** on the number of files the
 *   scan reaches.
 */
function scanRoots(): string[] {
  const manifest = readFileSync(WORKSPACE_MANIFEST, 'utf-8');
  const members = manifest.match(/^members\s*=\s*\[([\s\S]*?)\]/m)?.[1] ?? '';
  const roots = new Set<string>();
  for (const quoted of members.match(/"[^"]+"/g) ?? []) {
    const member = quoted.slice(1, -1);
    for (const path of expandMember(member)) {
      for (const directory of ['src', 'tests']) {
        const full = join(REPO_ROOT, path, directory);
        if (existsSync(full)) roots.add(full);
      }
      const buildScript = join(REPO_ROOT, path, 'build.rs');
      if (existsSync(buildScript)) roots.add(buildScript);
    }
  }
  return [...roots];
}

/** Expand one workspace member pattern (`packages/drivers/*` is a glob). */
function expandMember(member: string): string[] {
  if (!member.endsWith('/*')) return [member];
  const parent = join(REPO_ROOT, member.slice(0, -1));
  if (!existsSync(parent)) return [];
  return readdirSync(parent, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => `${member.slice(0, -1)}${entry.name}`);
}

/**
 * A code literal as it appears in a Rust string.
 *
 * **The scan covers literals only.** A code assembled at run time —
 * `format!("grid.{}.something", domain)` — has no literal to find, and the
 * guard does not see it. The repository has no such spelling today (every
 * `grid.` occurrence outside a literal is in a doc comment), but the boundary
 * is worth stating: the guard proves that every *literal* code is registered,
 * not that every possible code is.
 */
const CODE_PATTERN = /"grid\.[a-z]+\.[a-zA-Z]+/g;

/**
 * Codes that are registered although nothing raises them yet.
 *
 * Each entry is a decision, not a leftover: the front end branches on the
 * code, the back end does not raise it. When the producer lands, the entry
 * has to leave this list — the test below fails until it does.
 *
 * A code that will **never** have a producer does not belong here either:
 * "this database cannot estimate row counts" is a fact the front end knows
 * on its own (the driver's declaration, the answer in `totalRowsKind`), so the
 * grid renders it directly instead of waiting for a message that would never
 * arrive. It is an i18n key, not an error code.
 */
const PRODUCER_PENDING: readonly string[] = [
  // DB-05: the typed editor refuses a value it cannot parse (05 §8.2 row 2).
  // The refusal happens in the **front end** — the invalid value never
  // reaches the back end, so there is no Rust producer to wait for. The code
  // is nevertheless the contract's: the grid classifies the refusal with it,
  // and the reason it carries is the one the editor refused for.
  'grid.cell.invalidValue',
  // DB-03: the clipboard is a **front-end** concern. Every one of these codes is
  // produced by `gridClipboard.ts` / `useGridClipboard.ts` while deciding whether a
  // paste may be planned at all — the clipboard text never reaches Rust, so no
  // producer will ever appear for them (03 §3.3).
  'grid.paste.empty',
  'grid.paste.tooLarge',
  'grid.paste.raggedRow',
  'grid.paste.unclosedQuote',
  'grid.paste.outOfBounds',
  'grid.paste.noWritableCell',
  'grid.paste.notNullEmpty',
  'grid.paste.identityChanged',
  'grid.paste.crossTable',
  'grid.paste.columnMismatch',
  'grid.clipboard.writeFailed',
  'grid.clipboard.readFailed',
  // DB-20: "no table references this row" is the **front end's** answer to
  // the reverse catalog — an empty catalog is a successful scan, not a Rust
  // rejection, so no producer on the host will ever raise it.
  'grid.fk.noReferences',
];

function rustFiles(root: string): string[] {
  if (!existsSync(root)) return [];
  if (!statSync(root).isDirectory()) {
    return root.endsWith('.rs') ? [root] : [];
  }
  const found: string[] = [];
  const walk = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const full = join(directory, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (entry.name.endsWith('.rs')) found.push(full);
    }
  };
  walk(root);
  return found;
}

/**
 * Every `grid.<domain>.<reason>` code the scanned Rust sources mention.
 *
 * Inline `#[cfg(test)]` modules live in the same files and are scanned with
 * them: a code that only a test raises still has to be a registered code.
 */
function producedCodes(): Set<string> {
  const codes = new Set<string>();
  for (const file of scannedRustFiles()) {
    const source = readFileSync(file, 'utf-8');
    for (const match of source.matchAll(CODE_PATTERN)) {
      // The pattern starts at the opening quote; the code itself does not.
      codes.add(match[0].slice(1));
    }
  }
  return codes;
}

/** Every Rust file the scan reaches. */
function scannedRustFiles(): string[] {
  const files: string[] = [];
  for (const root of scanRoots()) {
    files.push(...rustFiles(root));
  }
  return files;
}

describe('grid error code registration', () => {
  it('classifies every message the back end actually produces', () => {
    for (const { code, message } of PRODUCED_MESSAGES) {
      expect(classifyGridError(message)).toBe(code);
    }
  });

  it('registers each code the back end produces', () => {
    const produced = [...new Set(PRODUCED_MESSAGES.map((entry) => entry.code))];
    for (const code of produced) {
      expect(GRID_ERROR_CODES).toContain(code);
    }
  });

  it('does not fall back to unknown for a real back-end message', () => {
    for (const { message } of PRODUCED_MESSAGES) {
      expect(classifyGridError(message)).not.toBe('unknown');
    }
  });
});

describe('the scan of the back-end sources', () => {
  it('reaches the whole workspace', () => {
    // The one way this guard can weaken silently is the **derivation** of the
    // scan roots: the glob only understands a trailing `/*`, and the manifest
    // only the first line-initial `members = [`. Neither breaks loudly today,
    // and a broken derivation would leave the guard scanning less than it
    // thinks — the same kind of silence this guard was written to end.
    //
    // 1423 is the measured count on 2026-10-22. The repository only ever
    // grows, so this number can only go up; if it goes **down**, the
    // derivation broke and the scan is no longer covering what it claims.
    const scanned = scannedRustFiles();
    expect(scanned.length).toBeGreaterThanOrEqual(1423);
  });

  it('registers every code the back-end sources raise', () => {
    const registered = new Set<string>(GRID_ERROR_CODES);
    const missing = [...producedCodes()].filter((code) => !registered.has(code)).sort();
    expect(missing).toEqual([]);
  });

  it('has a producer for every registered code, or lists it as pending', () => {
    const produced = producedCodes();
    const pending = new Set(PRODUCER_PENDING);
    const withoutProducer = GRID_ERROR_CODES.filter(
      (code) => !produced.has(code) && !pending.has(code),
    );
    expect(withoutProducer).toEqual([]);
  });

  it('keeps the pending list honest', () => {
    // A code that has a producer does not belong on the pending list any
    // more: the list is the record of what is still missing, and a stale
    // entry would hide the fact.
    const produced = producedCodes();
    expect(PRODUCER_PENDING.filter((code) => produced.has(code))).toEqual([]);
  });
});
