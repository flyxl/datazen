import { describe, expect, it, vi } from 'vitest';
import type { Completion, CompletionContext } from '@codemirror/autocomplete';
import {
  filterCompletionsByKind,
  filterKeywordsByKind,
  contextualKeywordCompletion,
  inferSqlCompletionKind,
  lastBareSqlKeyword,
} from '../sqlCompletionContext';

describe('inferSqlCompletionKind', () => {
  it('treats WHERE as column context', () => {
    expect(inferSqlCompletionKind('SELECT * FROM product WHERE p')).toBe('column');
  });

  it('treats SELECT list as column context', () => {
    expect(inferSqlCompletionKind('SELECT p')).toBe('column');
  });

  it('treats FROM with unfinished table name as table context', () => {
    expect(inferSqlCompletionKind('SELECT * FROM p')).toBe('table');
    expect(inferSqlCompletionKind('SELECT * FROM ec')).toBe('table');
  });

  it('treats FROM with completed table name and subsequent typing as any context (allowing WHERE, JOIN, alias)', () => {
    // When table name is completed (followed by space), user is typing alias or WHERE / JOIN
    expect(inferSqlCompletionKind('SELECT * FROM er_customers ')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers WHE')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers ec ')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers ec WHE')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM "public"."er_customers" WHE')).toBe('any');
  });

  it('treats multi-table FROM with comma as table context when typing next table', () => {
    expect(inferSqlCompletionKind('SELECT * FROM er_customers, ')).toBe('table');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers, ord')).toBe('table');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers, orders ')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM er_customers, orders WHE')).toBe('any');
  });

  it('does not filter after a qualified dot', () => {
    expect(inferSqlCompletionKind('SELECT * FROM product.')).toBe('any');
    expect(inferSqlCompletionKind('SELECT * FROM public.p')).toBe('any');
  });

  it('treats AND after WHERE as column context', () => {
    expect(inferSqlCompletionKind('SELECT * FROM product WHERE price > 1 AND p')).toBe('column');
  });

  it('treats INSERT INTO ( as column context', () => {
    expect(inferSqlCompletionKind('INSERT INTO product (p')).toBe('column');
  });
});

describe('lastBareSqlKeyword', () => {
  it('ignores keywords inside strings', () => {
    expect(lastBareSqlKeyword("SELECT * FROM t WHERE name = 'from' AND p")).toBe('and');
  });
});

describe('filterCompletionsByKind', () => {
  const options: Completion[] = [
    { label: 'price', type: 'property' },
    { label: 'product', type: 'type' },
    { label: 'public', type: 'type' },
  ];

  it('keeps only columns in WHERE context', () => {
    expect(filterCompletionsByKind(options, 'column').map((o) => o.label)).toEqual(['price']);
  });

  it('keeps only tables/schemas in FROM context and boosts their weight', () => {
    const filtered = filterCompletionsByKind(options, 'table');
    expect(filtered.map((o) => o.label)).toEqual(['product', 'public']);
    for (const item of filtered) {
      expect(item.boost).toBe(10);
    }
  });

  it('boosts column completions in column context', () => {
    const filtered = filterCompletionsByKind(options, 'column');
    expect(filtered.map((o) => o.label)).toEqual(['price']);
    expect(filtered[0]?.boost).toBe(5);
  });
});

describe('filterKeywordsByKind in table context', () => {
  const mixedKeywords: Completion[] = [
    { label: 'PG_EXCEPTION_HINT', type: 'keyword' },
    { label: 'PG_EXCEPTION_DETAIL', type: 'keyword' },
    { label: 'OCCURRENCES_REGEX', type: 'keyword' },
    { label: 'PERCENTILE_DISC', type: 'keyword' },
    { label: 'PARAMETER_SPECIFIC_NAME', type: 'keyword' },
    { label: 'SELECT', type: 'keyword' },
    { label: 'LATERAL', type: 'keyword' },
    { label: 'VALUES', type: 'keyword' },
  ];

  it('filters out system exception / diagnostic keywords and only keeps valid subquery/table keywords', () => {
    const filtered = filterKeywordsByKind(mixedKeywords, 'table');
    const labels = filtered.map((k) => k.label);
    expect(labels).not.toContain('PG_EXCEPTION_HINT');
    expect(labels).not.toContain('PG_EXCEPTION_DETAIL');
    expect(labels).not.toContain('OCCURRENCES_REGEX');
    expect(labels).not.toContain('PERCENTILE_DISC');
    expect(labels).not.toContain('PARAMETER_SPECIFIC_NAME');

    expect(labels).toEqual(['SELECT', 'LATERAL', 'VALUES']);
    // Allowed keywords in table context should have negative boost so actual tables rank higher
    for (const k of filtered) {
      expect(k.boost).toBeLessThan(0);
    }
  });

  it('does not filter keywords when context is any', () => {
    const filtered = filterKeywordsByKind(mixedKeywords, 'any');
    expect(filtered).toHaveLength(mixedKeywords.length);
  });
});

describe('contextualKeywordCompletion', () => {
  it('filters keywords in FROM context via CompletionContext', () => {
    const rawSource = vi.fn().mockReturnValue({
      from: 14,
      options: [
        { label: 'PG_EXCEPTION_HINT', type: 'keyword' },
        { label: 'SELECT', type: 'keyword' },
      ],
    });

    const contextualSource = contextualKeywordCompletion(rawSource);
    const mockContext = {
      state: {
        sliceDoc: (from: number, to: number) => 'SELECT * FROM ec'.slice(from, to),
      },
      pos: 16,
    } as unknown as CompletionContext;

    const result = contextualSource(mockContext) as { options: Completion[] };
    expect(result).not.toBeNull();
    expect(result.options.map((o) => o.label)).toEqual(['SELECT']);
    expect(result.options[0]?.boost).toBeLessThan(0);
  });

  it('does NOT filter keywords like WHERE when table is completed and typing subsequent clause', () => {
    const rawSource = vi.fn().mockReturnValue({
      from: 26,
      options: [
        { label: 'WHERE', type: 'keyword' },
        { label: 'JOIN', type: 'keyword' },
        { label: 'ORDER BY', type: 'keyword' },
      ],
    });

    const contextualSource = contextualKeywordCompletion(rawSource);
    const mockContext = {
      state: {
        sliceDoc: (from: number, to: number) => 'SELECT * FROM er_customers WHE'.slice(from, to),
      },
      pos: 29,
    } as unknown as CompletionContext;

    const result = contextualSource(mockContext) as { options: Completion[] };
    expect(result).not.toBeNull();
    const labels = result.options.map((o) => o.label);
    expect(labels).toContain('WHERE');
    expect(labels).toContain('JOIN');
    expect(labels).toContain('ORDER BY');
  });

  it('returns null when after an alias dot', () => {
    const rawSource = vi.fn();
    const contextualSource = contextualKeywordCompletion(rawSource);
    const mockContext = {
      state: {
        sliceDoc: (from: number, to: number) => 'SELECT eo.'.slice(from, to),
      },
      pos: 10,
    } as unknown as CompletionContext;

    const result = contextualSource(mockContext);
    expect(result).toBeNull();
    expect(rawSource).not.toHaveBeenCalled();
  });
});
