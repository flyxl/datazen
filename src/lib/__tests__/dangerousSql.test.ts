import { describe, expect, it } from 'vitest';
import {
  classifyRisk,
  isDangerousWriteStatement,
  sqlContainsDangerousWrite,
} from '../dangerousSql';
import fixture from './fixtures/sqlRiskCases.json';

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

describe('classifyRisk', () => {
  it('classifies SELECT as read', () => {
    const a = classifyRisk('SELECT * FROM t');
    expect(a.classification).toBe('read');
    expect(a.hasHighRisk).toBe(false);
    expect(a.findings).toEqual([]);
  });

  it('classifies UPDATE with top-level WHERE as mutation (no finding)', () => {
    const a = classifyRisk('UPDATE t SET x = 1 WHERE id = 1');
    expect(a.classification).toBe('mutation');
    expect(a.statements[0].hasTopLevelWhere).toBe(true);
    expect(a.findings).toEqual([]);
  });

  it('flags UPDATE/DELETE without a top-level WHERE as no-where', () => {
    const update = classifyRisk('UPDATE t SET x = 1');
    expect(update.findings.map((f) => f.type)).toEqual(['no-where']);
    expect(update.hasHighRisk).toBe(true);
    const del = classifyRisk('DELETE FROM t');
    expect(del.findings.map((f) => f.type)).toEqual(['no-where']);
  });

  it('excludes WHERE inside a subquery', () => {
    const a = classifyRisk('UPDATE t SET x = (SELECT y FROM u WHERE id = 1)');
    expect(a.statements[0].hasTopLevelWhere).toBe(false);
    expect(a.findings.map((f) => f.type)).toEqual(['no-where']);
  });

  it('excludes WHERE inside a CTE while still classifying the real verb', () => {
    const a = classifyRisk('WITH c AS (SELECT 1 WHERE x = 2) UPDATE t SET y = 3');
    expect(a.statements[0].verb).toBe('UPDATE');
    expect(a.statements[0].hasTopLevelWhere).toBe(false);
    expect(a.findings.map((f) => f.type)).toEqual(['no-where']);
  });

  it('excludes WHERE inside a string literal', () => {
    const read = classifyRisk("SELECT 'WHERE' FROM dual");
    expect(read.classification).toBe('read');
    expect(read.findings).toEqual([]);
    const upd = classifyRisk("UPDATE t SET x = 'WHERE'");
    expect(upd.findings.map((f) => f.type)).toEqual(['no-where']);
  });

  it('excludes WHERE inside a block comment (frontend stricter than Host)', () => {
    const a = classifyRisk('UPDATE t SET x = 1 /* WHERE g */');
    expect(a.statements[0].hasTopLevelWhere).toBe(false);
    expect(a.findings.map((f) => f.type)).toEqual(['no-where']);
  });

  it('classifies DROP / TRUNCATE as destructive findings', () => {
    expect(classifyRisk('DROP TABLE t').findings[0].type).toBe('drop');
    expect(classifyRisk('TRUNCATE TABLE t').findings[0].type).toBe('truncate');
  });

  it('does not flag regular mutation with a WHERE as high-risk', () => {
    expect(classifyRisk('UPDATE t SET x = 1 WHERE id = 2').hasHighRisk).toBe(false);
  });

  it('aggregates findings and highest risk across statements', () => {
    const a = classifyRisk('SELECT 1; DROP TABLE t');
    expect(a.classification).toBe('mutation');
    expect(a.findings.map((f) => f.type)).toEqual(['drop']);
    expect(a.statements).toHaveLength(2);
    const clean = classifyRisk('SELECT 1; SELECT 2');
    expect(clean.classification).toBe('read');
  });

  it('reports unknown for unrecognized verbs', () => {
    const a = classifyRisk('GIBBERISH foo');
    expect(a.classification).toBe('unknown');
  });

  it('preserves the original range of a finding', () => {
    const sql = 'SELECT 1; DROP TABLE t';
    const finding = classifyRisk(sql).findings[0];
    expect(sql.slice(finding.from, finding.to).toUpperCase()).toBe('DROP');
    expect(finding.statementIndex).toBe(1);
  });

  it('handles a WITH CTE that resolves to DROP', () => {
    const a = classifyRisk('WITH c AS (SELECT 1) DROP TABLE t');
    expect(a.classification).toBe('mutation');
    expect(a.findings.map((f) => f.type)).toEqual(['drop']);
  });

  it('normalizes fullwidth DROP to a drop finding', () => {
    const a = classifyRisk('ＤＲＯＰ TABLE t');
    expect(a.classification).toBe('mutation');
    expect(a.findings.map((f) => f.type)).toEqual(['drop']);
  });

  it('detects comment-hides-write-verb for unknown verbs', () => {
    const a = classifyRisk('/* DROP */ TABLE t');
    expect(a.statements[0].commentHidesWriteVerb).toBe(true);
    expect(a.statements[0].classification).toBe('unknown');
  });

  it('classifies SELECT INTO / EXPLAIN ANALYZE as may-write but not strict-write', () => {
    const into = classifyRisk('SELECT a INTO b FROM c');
    expect(into.classification).toBe('mutation');
    expect(into.statements[0].strictWrite).toBe(false);
    expect(into.hasHighRisk).toBe(false);
    const ea = classifyRisk('EXPLAIN ANALYZE DELETE FROM t');
    expect(ea.classification).toBe('mutation');
    expect(ea.statements[0].strictWrite).toBe(false);
  });

  it('returns empty assessment for empty / whitespace-only input', () => {
    expect(classifyRisk('').statements).toHaveLength(0);
    expect(classifyRisk('   ').statements).toHaveLength(0);
    expect(classifyRisk('  ;  ').statements).toHaveLength(0);
  });
});

describe('shared fixture sqlRiskCases (frontend classification)', () => {
  it.each(fixture.cases)('$id', (tc) => {
    const a = classifyRisk(tc.sql);
    expect(a.classification).toBe(tc.frontendClassification);
    expect(a.findings.map((f) => f.type)).toEqual(tc.frontendFindings);
    expect(a.hasHighRisk).toBe(tc.frontendHighRisk);
  });
});
