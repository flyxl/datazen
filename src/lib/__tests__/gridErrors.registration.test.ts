/**
 * Guard: every error code the back end actually produces must be registered on the front
 * end, otherwise `classifyGridError` returns `unknown` and the front end cannot branch.
 *
 * The back end prefixes its refusals with `grid.<domain>.<reason>: ` (00-contracts §8.2).
 * Wave 1 produced 27 throw sites across 8 codes; the literal messages below are copied from
 * those sites so this file fails the moment a code is removed from the registry.
 */
import { describe, expect, it } from 'vitest';
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
];

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
