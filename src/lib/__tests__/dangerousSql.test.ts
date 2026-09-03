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

  describe('[tester] edge cases', () => {
    it('returns false for empty or whitespace-only input', () => {
      expect(isDangerousWriteStatement('')).toBe(false);
      expect(isDangerousWriteStatement('   ')).toBe(false);
      expect(isDangerousWriteStatement('\n\t  \n')).toBe(false);
      expect(sqlContainsDangerousWrite('')).toBe(false);
      expect(sqlContainsDangerousWrite('  ;  ;  ')).toBe(false);
    });

    it('detects DROP/TRUNCATE after leading line and block comments', () => {
      expect(isDangerousWriteStatement('-- hint\nDROP TABLE t')).toBe(true);
      expect(isDangerousWriteStatement('/* note */\nTRUNCATE TABLE t')).toBe(true);
      expect(isDangerousWriteStatement('-- DROP\n-- more\nDROP VIEW v')).toBe(true);
    });

    it('ignores DROP/TRUNCATE inside leading comments only', () => {
      expect(isDangerousWriteStatement('/* DROP TABLE t */ SELECT 1')).toBe(false);
      expect(isDangerousWriteStatement('-- TRUNCATE t\nSELECT 1')).toBe(false);
    });

    it('handles very long SQL with dangerous statement at end', () => {
      const prefix = 'SELECT 1 FROM t WHERE id IN (' + '1,'.repeat(5000) + '9999)';
      expect(sqlContainsDangerousWrite(`${prefix}; DROP TABLE t`)).toBe(true);
      expect(sqlContainsDangerousWrite(prefix)).toBe(false);
    });

    it('detects inline comment between DROP and TABLE (frontend; backend safe mode may miss)', () => {
      // dangerousSql regex matches DROP at statement head; sql_guard tokenize_sql strips
      // inline block comments so the verb can become TABLE — documented heuristic gap.
      expect(isDangerousWriteStatement('DROP/**/TABLE t')).toBe(true);
    });

    it('does not treat keyword split across comment as dangerous (heuristic gap)', () => {
      expect(isDangerousWriteStatement('/* DROP */ TABLE t')).toBe(false);
    });

    it('detects WITH … DROP pattern', () => {
      expect(isDangerousWriteStatement('WITH c AS (SELECT 1) DROP TABLE t')).toBe(true);
    });
  });
});
