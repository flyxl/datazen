import { describe, expect, it } from 'vitest';
import {
  GRID_ERROR_CODES,
  GRID_ERROR_NAMESPACE,
  classifyGridError,
  type GridErrorCode,
} from '../gridErrors';

describe('GRID_ERROR_CODES', () => {
  it('only registers `grid.` domain codes (never the `unknown` sentinel)', () => {
    expect(GRID_ERROR_CODES.length).toBeGreaterThan(0);
    for (const code of GRID_ERROR_CODES) {
      expect(code.startsWith(GRID_ERROR_NAMESPACE)).toBe(true);
      expect(code).not.toBe('unknown');
    }
  });

  it('is typed as the GridErrorCode union, so appenders get a compile error on typos', () => {
    const codes: readonly GridErrorCode[] = GRID_ERROR_CODES;
    expect(codes).toBe(GRID_ERROR_CODES);
  });
});

describe('classifyGridError', () => {
  it('matches every registered prefix as a prefix, not as a substring', () => {
    for (const code of GRID_ERROR_CODES) {
      expect(classifyGridError(`${code}: the human-readable reason`)).toBe(code);
    }
  });

  it('matches a bare code without a separator', () => {
    expect(classifyGridError('grid.selection.invalid')).toBe('grid.selection.invalid');
  });

  it('does not classify a longer reason that merely starts with the code text', () => {
    expect(classifyGridError('grid.selection.invalidity: not a registered code')).toBe('unknown');
  });

  it('does not classify a code mentioned inside ordinary prose', () => {
    expect(classifyGridError('unexpected grid.selection.invalid: wrapped in another message')).toBe(
      'unknown',
    );
  });

  it('picks the longest registered prefix when prefixes overlap', () => {
    expect(classifyGridError('grid.selection.outOfBounds: cell is outside the page')).toBe(
      'grid.selection.outOfBounds',
    );
  });

  it('falls back to unknown for unrelated, empty and whitespace-only messages', () => {
    expect(classifyGridError('permission denied for table users')).toBe('unknown');
    expect(classifyGridError('')).toBe('unknown');
    expect(classifyGridError('   ')).toBe('unknown');
  });

  it('tolerates surrounding whitespace but stays case-sensitive', () => {
    expect(classifyGridError('  grid.selection.stale: rows changed\n')).toBe('grid.selection.stale');
    expect(classifyGridError('GRID.SELECTION.STALE: rows changed')).toBe('unknown');
  });
});
