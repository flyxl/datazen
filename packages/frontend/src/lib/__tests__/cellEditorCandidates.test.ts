import { describe, expect, it } from 'vitest';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';
import {
  CANDIDATE_VALUE_LIMIT,
  buildDistinctCandidatesSql,
} from '../cellEditorCandidates';

/**
 * `DatabaseType` is a **codegen** union: it only contains the drivers the build
 * injects. The per-PR CI typechecks the `basic` set (postgres, mysql, sqlite,
 * redis) while the release typecheck injects every path driver, so a bare
 * `'sqlserver'` literal compiles locally and fails on CI — which is exactly how
 * this file broke the gate once. Naming a driver outside `basic` therefore goes
 * through an explicit assertion: the host's dialect table carries the family, so
 * the shape such a driver will get is still pinned here.
 */
const ORACLE = 'oracle' as unknown as DatabaseType;
const SQLSERVER = 'sqlserver' as unknown as DatabaseType;

/**
 * Whether this build injected the SQL Server driver. `DB_REGISTRY` is generated
 * codegen, so the check is a membership test rather than a property read: under the
 * `basic` driver set the key simply is not there.
 */
const SQLSERVER_INJECTED = 'sqlserver' in DB_REGISTRY;

describe('buildDistinctCandidatesSql', () => {
  it('quotes identifiers per dialect', () => {
    expect(
      buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: 'postgresql' }),
    ).toBe('SELECT DISTINCT "status" FROM "users" LIMIT 200');
    expect(buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: 'mysql' })).toBe(
      'SELECT DISTINCT `status` FROM `users` LIMIT 200',
    );
    // The quote character comes from the driver's own registry entry, so the
    // host never hard-codes a dialect.
    expect(buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: 'sqlite' })).toBe(
      'SELECT DISTINCT "status" FROM "users" LIMIT 200',
    );
  });

  it('caps the rows the way the dialect caps them', () => {
    // `LIMIT` is a syntax error on SQL Server and on Oracle 12c+: SQL Server
    // wants `TOP n` inside the select list, Oracle wants
    // `FETCH FIRST n ROWS ONLY` at the end. The shape comes from the dialect's
    // own declaration, so the host never spells it — and it is host-side, so it
    // does not depend on which drivers this build injected.
    expect(
      buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: SQLSERVER }),
    ).toMatch(/^SELECT DISTINCT TOP 200 /);
    expect(buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: ORACLE })).toBe(
      'SELECT DISTINCT "status" FROM "users" FETCH FIRST 200 ROWS ONLY',
    );
    // The two dialects that do accept `LIMIT` keep it.
    expect(
      buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: 'postgresql' }),
    ).toContain('LIMIT 200');
    expect(buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: 'sqlite' })).toContain(
      'LIMIT 200',
    );
  });

  it('quotes a column name that carries the quote character itself', () => {
    const sql = buildDistinctCandidatesSql({
      table: 'users',
      column: 'we"ird',
      dialect: 'postgresql',
    });
    expect(sql).toBe('SELECT DISTINCT "we""ird" FROM "users" LIMIT 200');
    const mysql = buildDistinctCandidatesSql({
      table: 'users',
      column: 'we`ird',
      dialect: 'mysql',
    });
    expect(mysql).toBe('SELECT DISTINCT `we``ird` FROM `users` LIMIT 200');
  });

  it('qualifies the table with its schema when the dialect has one', () => {
    expect(
      buildDistinctCandidatesSql({
        table: 'users',
        column: 'status',
        schema: 'public',
        dialect: 'postgresql',
      }),
    ).toBe('SELECT DISTINCT "status" FROM "public"."users" LIMIT 200');
  });

  it('always limits to 200 rows and stays a single statement', () => {
    const sql = buildDistinctCandidatesSql({ table: 'users', column: 'status' });
    expect(sql).toContain(`LIMIT ${CANDIDATE_VALUE_LIMIT}`);
    expect(sql?.split(';')).toHaveLength(1);
    expect(sql).not.toContain(';');
    // A larger limit is capped, a smaller one is honoured.
    expect(buildDistinctCandidatesSql({ table: 't', column: 'c', limit: 5000 })).toContain(
      'LIMIT 200',
    );
    expect(buildDistinctCandidatesSql({ table: 't', column: 'c', limit: 10 })).toContain(
      'LIMIT 10',
    );
  });

  it('refuses an empty column name', () => {
    expect(buildDistinctCandidatesSql({ table: 'users', column: '' })).toBeNull();
    expect(buildDistinctCandidatesSql({ table: 'users', column: '   ' })).toBeNull();
  });

  // The bracket style is the **driver's** metadata (`quoteChar`), read through
  // `DB_REGISTRY`; a build that does not inject the driver falls back to `"`. Pin the
  // real statement only where the driver exists — skipped is honest, silently weaker
  // is not.
  it.skipIf(!SQLSERVER_INJECTED)(
    'quotes identifiers with the SQL Server bracket style',
    () => {
      expect(
        buildDistinctCandidatesSql({ table: 'users', column: 'status', dialect: SQLSERVER }),
      ).toBe('SELECT DISTINCT TOP 200 [status] FROM [users]');
    },
  );

  it('refuses an empty table name', () => {
    expect(buildDistinctCandidatesSql({ table: '', column: 'status' })).toBeNull();
  });
});
