import { describe, expect, it } from 'vitest';
import {
  isMutationExecution,
  getTopLevelSqlVerb,
  stripLeadingSqlComments,
} from '../isMutationExecution';
import type { StatementResult } from '../../../../types';

describe('stripLeadingSqlComments and getTopLevelSqlVerb', () => {
  it('strips line comments and block comments', () => {
    expect(stripLeadingSqlComments('-- hello\nSELECT 1')).toBe('SELECT 1');
    expect(stripLeadingSqlComments('/* hello */\nSELECT 1')).toBe('SELECT 1');
    expect(stripLeadingSqlComments('/* multiline\ncomment */ SELECT 1')).toBe('SELECT 1');
  });

  it('detects top-level verb for standard statements', () => {
    expect(getTopLevelSqlVerb('SELECT * FROM users')).toBe('SELECT');
    expect(getTopLevelSqlVerb('INSERT INTO users VALUES (1)')).toBe('INSERT');
    expect(getTopLevelSqlVerb('UPDATE users SET name = 1')).toBe('UPDATE');
    expect(getTopLevelSqlVerb('DELETE FROM users')).toBe('DELETE');
    expect(getTopLevelSqlVerb('CREATE TABLE t (id INT)')).toBe('CREATE');
  });

  it('detects top-level verb for CTE statements', () => {
    expect(getTopLevelSqlVerb('WITH cte AS (SELECT 1) SELECT * FROM cte')).toBe('SELECT');
    expect(
      getTopLevelSqlVerb('WITH RECURSIVE cte AS (SELECT 1) INSERT INTO t SELECT * FROM cte'),
    ).toBe('INSERT');
  });
});

describe('isMutationExecution', () => {
  it('returns true for PG/MySQL INSERT with rowsAffected and no columns', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 1,
      executionTimeMs: 15,
      sql: "INSERT INTO users (name) VALUES ('Alice')",
    };
    expect(isMutationExecution(result)).toBe(true);
  });

  it('returns true for UPDATE / DELETE with rowsAffected = 0', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 0,
      executionTimeMs: 20,
      sql: 'UPDATE users SET status = 1 WHERE id = 999',
    };
    expect(isMutationExecution(result)).toBe(true);
  });

  it('returns true for DDL statements (CREATE TABLE, DROP TABLE, ALTER TABLE)', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: undefined,
      executionTimeMs: 35,
      sql: 'CREATE TABLE test_table (id INT PRIMARY KEY)',
    };
    expect(isMutationExecution(result)).toBe(true);
  });

  it('returns false for SELECT queries with column definitions and 0 rows', () => {
    const result: StatementResult = {
      columns: ['id', 'name'],
      rows: [],
      rowsAffected: undefined,
      executionTimeMs: 10,
      sql: 'SELECT id, name FROM users WHERE 1=0',
    };
    expect(isMutationExecution(result)).toBe(false);
  });

  it('returns false for PostgreSQL empty SELECT returning 0 columns and rowsAffected = 0', () => {
    // This matches the exact user bug report where PG returns 0 columns, 0 rows, rowsAffected: 0
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 0,
      executionTimeMs: 2,
      sql: "SELECT * FROM er_customers WHERE er_customers.name = '张军' AND er_customers.city = '上海'",
    };
    expect(isMutationExecution(result)).toBe(false);
  });

  it('returns false for SELECT queries with leading comments', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 0,
      executionTimeMs: 3,
      sql: '-- check non-existent user\nSELECT * FROM users WHERE id = -1',
    };
    expect(isMutationExecution(result)).toBe(false);

    const blockCommentResult: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 0,
      executionTimeMs: 3,
      sql: '/* query test */ SELECT * FROM users WHERE id = -1',
    };
    expect(isMutationExecution(blockCommentResult)).toBe(false);
  });

  it('returns false for CTE queries (WITH ... SELECT)', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 0,
      executionTimeMs: 5,
      sql: 'WITH cte AS (SELECT * FROM users WHERE id = 0) SELECT * FROM cte',
    };
    expect(isMutationExecution(result)).toBe(false);
  });

  it('returns true for CTE mutating queries (WITH ... INSERT)', () => {
    const result: StatementResult = {
      columns: [],
      rows: [],
      rowsAffected: 1,
      executionTimeMs: 5,
      sql: 'WITH cte AS (SELECT 1) INSERT INTO target_table SELECT * FROM cte',
    };
    expect(isMutationExecution(result)).toBe(true);
  });

  it('returns false for SHOW, EXPLAIN, and DESCRIBE queries with 0 rows', () => {
    expect(
      isMutationExecution({
        columns: [],
        rows: [],
        rowsAffected: 0,
        executionTimeMs: 1,
        sql: "SHOW TABLES LIKE 'non_existent%'",
      }),
    ).toBe(false);

    expect(
      isMutationExecution({
        columns: [],
        rows: [],
        rowsAffected: 0,
        executionTimeMs: 1,
        sql: 'EXPLAIN SELECT 1',
      }),
    ).toBe(false);
  });

  it('returns false for INSERT ... RETURNING with returned rows/columns', () => {
    const result: StatementResult = {
      columns: ['id'],
      rows: [{ id: 1 }],
      rowsAffected: 1,
      executionTimeMs: 12,
      sql: "INSERT INTO users (name) VALUES ('Bob') RETURNING id",
    };
    expect(isMutationExecution(result)).toBe(false);
  });
});
