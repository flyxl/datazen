import { describe, it, expect } from 'vitest';
import {
  extractQualifierParts,
  detectAliasDotContext,
  inferSqlCompletionKind,
  filterCompletionsByKind,
} from '../../../../lib/sqlCompletionContext';

describe('sqlCompletionContext (alias-aware)', () => {
  describe('extractQualifierParts', () => {
    it('extracts single qualifier', () => {
      expect(extractQualifierParts('SELECT o.')).toEqual(['o']);
    });

    it('extracts multi-part qualifier', () => {
      expect(extractQualifierParts('SELECT public.users.')).toEqual(['public', 'users']);
    });

    it('returns empty for no qualifier', () => {
      expect(extractQualifierParts('SELECT * FROM')).toEqual([]);
    });

    it('handles quoted qualifiers', () => {
      expect(extractQualifierParts('SELECT "myTable".')).toEqual(['myTable']);
    });

    it('handles backtick qualifiers', () => {
      expect(extractQualifierParts('SELECT `myTable`.')).toEqual(['myTable']);
    });
  });

  describe('detectAliasDotContext', () => {
    it('detects unique alias', () => {
      const result = detectAliasDotContext('SELECT o.');
      expect(result.kind).toBe('unique');
      expect(result.qualifier).toBe('o');
    });

    it('detects multi-part as none (schema.table)', () => {
      const result = detectAliasDotContext('SELECT public.users.');
      expect(result.kind).toBe('none');
    });

    it('returns none for no qualifier', () => {
      const result = detectAliasDotContext('SELECT *');
      expect(result.kind).toBe('none');
    });
  });

  describe('inferSqlCompletionKind (backward compat)', () => {
    it('returns any for alias dot pattern', () => {
      expect(inferSqlCompletionKind('SELECT o.')).toBe('any');
    });

    it('returns table for FROM keyword', () => {
      expect(inferSqlCompletionKind('SELECT * FROM ')).toBe('table');
    });

    it('returns column for SELECT keyword', () => {
      expect(inferSqlCompletionKind('SELECT ')).toBe('column');
    });

    it('returns column when typing partial identifier after SELECT', () => {
      expect(inferSqlCompletionKind('SELECT col1')).toBe('column');
      expect(inferSqlCompletionKind('select col1')).toBe('column');
    });

    it('returns column for WHERE keyword', () => {
      expect(inferSqlCompletionKind('SELECT 1 WHERE ')).toBe('column');
    });
  });

  describe('filterCompletionsByKind', () => {
    const options = [
      { label: 'users', type: 'type' },
      { label: '"col1"', type: 'property', filterText: 'col1' },
      { label: '"col2"', type: 'property', filterText: 'col2' },
    ];

    it('keeps only property (column) completions in column context', () => {
      const filtered = filterCompletionsByKind(options, 'column');
      expect(filtered).toHaveLength(2);
      expect(filtered.every((o) => o.type === 'property')).toBe(true);
    });

    it('keeps only type (table) completions in table context', () => {
      const filtered = filterCompletionsByKind(options, 'table');
      expect(filtered).toHaveLength(1);
      expect(filtered[0]!.label).toBe('users');
    });
  });
});
