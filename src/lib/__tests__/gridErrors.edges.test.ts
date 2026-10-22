/**
 * Supplemental classifier coverage (00-contracts §8.1).
 *
 * The main file covers the registered prefixes. This one pins the two contract obligations
 * the UI depends on: an unregistered prefix falls back to `unknown` (so the caller can show
 * the raw message) and no prototypal/non-string input can crash the classifier.
 */
import { describe, expect, it } from 'vitest';
import { classifyGridError, GRID_ERROR_CODES, GRID_ERROR_NAMESPACE } from '../gridErrors';

describe('classifyGridError contract edges', () => {
  it('keeps every registered code inside the grid namespace and excludes the sentinel', () => {
    expect(GRID_ERROR_NAMESPACE).toBe('grid.');
    for (const code of GRID_ERROR_CODES) {
      expect(code.startsWith(GRID_ERROR_NAMESPACE)).toBe(true);
      expect(code).not.toBe('unknown');
    }
  });

  it('returns unknown for non-string runtime input instead of throwing', () => {
    // The signature is `string`, but IPC payloads are not typed at the boundary.
    expect(classifyGridError(undefined as unknown as string)).toBe('unknown');
    expect(classifyGridError(null as unknown as string)).toBe('unknown');
    expect(classifyGridError({ message: 'grid.cell.readOnly: x' } as unknown as string)).toBe(
      'unknown',
    );
  });

  it('does not treat a longer reason as a registered code and is case-sensitive', () => {
    expect(classifyGridError('grid.cell.readOnlyBecause: typo')).toBe('unknown');
    expect(classifyGridError('Grid.Cell.ReadOnly: casing')).toBe('unknown');
    expect(classifyGridError('grid.cell.readOnly')).toBe('grid.cell.readOnly');
    expect(classifyGridError('grid.paging.cursorTooDeep: beyond the limit')).toBe(
      'grid.paging.cursorTooDeep',
    );
  });
});
