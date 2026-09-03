import { describe, expect, it } from 'vitest';
import { isDangerousWriteStatement, sqlContainsDangerousWrite } from '../dangerousSql';

describe('dangerousSql', () => {
  it('detects DROP and TRUNCATE statements', () => {
    expect(isDangerousWriteStatement('DROP TABLE t')).toBe(true);
    expect(isDangerousWriteStatement('DROP VIEW v')).toBe(true);
    expect(isDangerousWriteStatement('TRUNCATE TABLE t')).toBe(true);
    expect(isDangerousWriteStatement('TRUNCATE t')).toBe(true);
  });

  it('ignores safe statements and comments', () => {
    expect(isDangerousWriteStatement('SELECT 1')).toBe(false);
    expect(isDangerousWriteStatement('UPDATE t SET x = 1 WHERE id = 1')).toBe(false);
    expect(isDangerousWriteStatement('-- DROP TABLE t\nSELECT 1')).toBe(false);
    expect(isDangerousWriteStatement('/* TRUNCATE t */ SELECT 1')).toBe(false);
    expect(isDangerousWriteStatement("SELECT 'DROP TABLE t'")).toBe(false);
  });

  it('checks any statement in a script', () => {
    expect(sqlContainsDangerousWrite('SELECT 1; DROP TABLE t')).toBe(true);
    expect(sqlContainsDangerousWrite('SELECT 1; UPDATE t SET x = 1 WHERE id = 1')).toBe(false);
    expect(sqlContainsDangerousWrite("SELECT 'a; DROP TABLE t'; SELECT 1")).toBe(false);
  });
});
