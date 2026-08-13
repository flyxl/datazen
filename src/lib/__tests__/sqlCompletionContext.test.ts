import { describe, expect, it } from 'vitest';
import type { Completion } from '@codemirror/autocomplete';
import {
  filterCompletionsByKind,
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

  it('treats FROM as table context', () => {
    expect(inferSqlCompletionKind('SELECT * FROM p')).toBe('table');
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

  it('keeps only tables/schemas in FROM context', () => {
    expect(filterCompletionsByKind(options, 'table').map((o) => o.label)).toEqual([
      'product',
      'public',
    ]);
  });
});
