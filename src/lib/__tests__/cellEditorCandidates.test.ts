import { describe, expect, it } from 'vitest';
import {
  CANDIDATE_VALUE_LIMIT,
  buildDistinctCandidatesSql,
} from '../cellEditorCandidates';

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

  it('refuses an empty table name', () => {
    expect(buildDistinctCandidatesSql({ table: '', column: 'status' })).toBeNull();
  });
});
