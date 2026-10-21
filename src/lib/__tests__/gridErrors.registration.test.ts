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
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { GRID_ERROR_CODES, classifyGridError } from '../gridErrors';

/** Real messages, copied verbatim from the back-end producers. */
const PRODUCED_MESSAGES: readonly { code: string; message: string }[] = [
  {
    code: 'grid.commit.stalePlan',
    message:
      'grid.commit.stalePlan: Row change plan fingerprint is stale or was modified',
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
    message: 'grid.cell.noRowIdentity: UPDATE requires the table\'s primary-key identity',
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
      'grid.filter.unsupportedOperator: this database does not support the \'regex\' filter on column "name"; try \'like\' or \'contains\' instead',
  },
];

const REPO_ROOT = resolve(__dirname, '../../..');

/** The Rust trees that raise `grid.*` codes. */
const SCAN_ROOTS = ['packages/feature-core/src', 'src-tauri/src'];

/** A code literal as it appears in a Rust string. */
const CODE_PATTERN = /"grid\.[a-z]+\.[a-zA-Z]+/g;

/**
 * Codes that are registered although nothing raises them yet.
 *
 * Each entry is a decision, not a leftover: the front end branches on the
 * code, the back end does not raise it. When the producer lands, the entry
 * has to leave this list — the test below fails until it does.
 */
const PRODUCER_PENDING: readonly string[] = [
  'grid.selection.invalid',
  'grid.selection.outOfBounds',
  'grid.selection.stale',
];

function rustFiles(root: string): string[] {
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
  walk(join(REPO_ROOT, root));
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
  for (const root of SCAN_ROOTS) {
    for (const file of rustFiles(root)) {
      const source = readFileSync(file, 'utf-8');
      for (const match of source.matchAll(CODE_PATTERN)) {
        // The pattern starts at the opening quote; the code itself does not.
        codes.add(match[0].slice(1));
      }
    }
  }
  return codes;
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
  it('registers every code the back-end sources raise', () => {
    const registered = new Set<string>(GRID_ERROR_CODES);
    const missing = [...producedCodes()]
      .filter((code) => !registered.has(code))
      .sort();
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
