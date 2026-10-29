import { describe, expect, it, vi } from 'vitest';
import { EditorState } from '@codemirror/state';
import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { schemaCompletionSource, sql, type SQLConfig } from '@codemirror/lang-sql';
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

  // The three cases above hand-build `type: 'type'` / `type: 'property'`
  // options, so they agree with the filter by construction and can never catch
  // a mismatch with the real producer. These two run the real
  // `schemaCompletionSource` from `@codemirror/lang-sql` and feed its output
  // through the filter unchanged, which is the only way the contract between
  // the producer and the filter is actually checked.
  describe('against the real @codemirror/lang-sql producer', () => {
    const sqlConfig: SQLConfig = {
      defaultSchema: 'public',
      schema: {
        public: {
          customers: ['id', 'name'],
          products: ['id', 'price'],
        },
      },
    };

    const runSource = (doc: string) => {
      // The SQL language must be installed: lang-sql reads the syntax tree to
      // decide what kind of name is expected here, and returns null without it.
      const state = EditorState.create({
        doc,
        extensions: [sql()],
        selection: { anchor: doc.length },
      });
      const result = schemaCompletionSource(sqlConfig)({
        state,
        pos: doc.length,
        explicit: false,
      } as CompletionContext);
      expect(result).not.toBe(null);
      return (result as CompletionResult).options;
    };

    // The hand-built cases at the top of this describe block cannot catch a
    // mismatch with the real producer: they construct `type: 'type'` and
    // `type: 'property'` options themselves, so they agree with the filter by
    // construction. These run the actual `schemaCompletionSource` from
    // `@codemirror/lang-sql` and feed its untouched output through the filter,
    // which is the only way the producer/filter contract is really checked.
    it('keeps the tables the real producer emits in FROM context', () => {
      const options = runSource('SELECT * FROM cust');
      expect(options.map((o) => o.label)).toEqual(['public', 'customers', 'products']);
      // The real producer's marker, recorded so a future change to lang-sql
      // that renames it shows up here instead of silently emptying every
      // completion popup.
      expect(Array.from(new Set(options.map((o) => String(o.type))))).toEqual(['type']);
      expect(filterCompletionsByKind(options, 'table').map((o) => o.label)).toEqual([
        'public',
        'customers',
        'products',
      ]);
    });

    it('keeps the columns the real producer emits', () => {
      // Columns are offered once the statement qualifies them. In a bare
      // `WHERE ` the source still answers with tables — that is lang-sql
      // behaviour, not something this filter decides.
      const options = runSource('SELECT * FROM customers WHERE customers.');
      // `type: 'property'` is the column contract. If a lang-sql upgrade
      // renames it, this fails here instead of silently emptying every column
      // popup in the product.
      expect(options.map((o) => o.type)).toEqual(['property', 'property']);
      expect(filterCompletionsByKind(options, 'column').map((o) => o.label)).toEqual([
        'id',
        'name',
      ]);
    });
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

    const result = contextualSource(mockContext) as unknown as { options: Completion[] };
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

    const result = contextualSource(mockContext) as unknown as { options: Completion[] };
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
