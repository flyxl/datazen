import { describe, expect, it } from 'vitest';
import { inferDefaultSchema, inferDefaultTable } from '../sqlEditorDefaults';
import type { TableInfo } from '../../types';

describe('inferDefaultTable', () => {
  it('reads an unqualified FROM table', () => {
    expect(inferDefaultTable('SELECT * FROM users WHERE ')).toBe('users');
  });

  it('reads schema.table and returns the table segment', () => {
    expect(inferDefaultTable('SELECT * FROM public.users')).toBe('users');
  });

  it('uses the last FROM/JOIN table', () => {
    expect(inferDefaultTable('SELECT * FROM users u JOIN orders o ON u.id = o.user_id')).toBe(
      'orders',
    );
  });

  it('handles quoted identifiers', () => {
    expect(inferDefaultTable('SELECT * FROM "User"')).toBe('User');
    expect(inferDefaultTable('SELECT * FROM "public"."Order Item"')).toBe('Order Item');
  });

  it('returns undefined when there is no FROM', () => {
    expect(inferDefaultTable('SELECT 1')).toBeUndefined();
  });
});

describe('inferDefaultSchema', () => {
  const t = (name: string, schema: string | null): TableInfo => ({
    name,
    tableType: 'table',
    schema,
    rowCount: null,
  });

  it('prefers public when present', () => {
    expect(inferDefaultSchema([t('u', 'audit'), t('v', 'public')])).toBe('public');
  });

  it('uses the most common schema otherwise', () => {
    expect(inferDefaultSchema([t('a', 'sales'), t('b', 'sales'), t('c', 'hr')])).toBe('sales');
  });

  it('ignores path-nav sentinels and missing schemas', () => {
    expect(inferDefaultSchema([t('hive', 'CATALOG'), t('u', null)])).toBeUndefined();
  });
});
