import { describe, expect, it } from 'vitest';
import { inferDefaultSchema, inferDefaultTable, tablesReferencedInSql } from '../sqlEditorDefaults';
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

  it('uses the last segment of a multi-level path', () => {
    expect(inferDefaultTable('SELECT * FROM hive.snap.afi_credit_r_cibi_report_trade')).toBe(
      'afi_credit_r_cibi_report_trade',
    );
  });
});

describe('tablesReferencedInSql', () => {
  it('returns every FROM/JOIN table once, last path segment only', () => {
    expect(
      tablesReferencedInSql(
        'SELECT * FROM hive.snap.orders o JOIN hive.snap.users u ON o.uid = u.id',
      ),
    ).toEqual(['orders', 'users']);
  });

  it('is empty when there is no relation', () => {
    expect(tablesReferencedInSql('SELECT 1')).toEqual([]);
    expect(tablesReferencedInSql('')).toEqual([]);
  });

  it('still extracts an in-progress identifier (store must filter unknown names)', () => {
    expect(tablesReferencedInSql('SELECT * FROM hive.snap.wb_d')).toEqual(['wb_d']);
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
