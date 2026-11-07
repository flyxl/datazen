import { describe, expect, it } from 'vitest';
import { sqlFunctionCompletions } from '../sqlCompletions';

describe('sqlFunctionCompletions', () => {
  it('always includes common aggregates', () => {
    const labels = sqlFunctionCompletions().map((c) => c.label);
    expect(labels).toEqual(expect.arrayContaining(['COUNT', 'SUM', 'COALESCE']));
  });

  it('adds postgres-specific functions', () => {
    const labels = sqlFunctionCompletions('postgresql').map((c) => c.label);
    expect(labels).toContain('DATE_TRUNC');
    expect(labels).toContain('JSONB_BUILD_OBJECT');
  });

  it('adds mysql-specific functions', () => {
    const labels = sqlFunctionCompletions('mariadb').map((c) => c.label);
    expect(labels).toContain('GROUP_CONCAT');
    expect(labels).not.toContain('DATE_TRUNC');
  });

  it('treats cockroach as postgres and tidb as mysql', () => {
    expect(sqlFunctionCompletions('cockroach').map((c) => c.label)).toContain('DATE_TRUNC');
    expect(sqlFunctionCompletions('tidb').map((c) => c.label)).toContain('IFNULL');
  });

  it('adds sqlite helpers and falls back to common', () => {
    expect(sqlFunctionCompletions('sqlite').map((c) => c.label)).toContain('PRINTF');
    expect(sqlFunctionCompletions('oracle').map((c) => c.label)).toEqual(
      expect.arrayContaining(['COUNT', 'CASE']),
    );
    expect(sqlFunctionCompletions('postgres').map((c) => c.label)).toContain('STRING_AGG');
  });
});
