import { describe, it, expect } from 'vitest';
import { resolveExecutionTarget, offsetToLine } from '../resolveExecutionTarget';

describe('resolveExecutionTarget', () => {
  const multiSql = `SELECT 1 AS one;

SELECT id, name, email
FROM users
WHERE active = true;

SELECT 3 AS three;`;

  it('calculates line numbers correctly', () => {
    expect(offsetToLine('abc\ndef\nghi', 0)).toBe(1);
    expect(offsetToLine('abc\ndef\nghi', 3)).toBe(1);
    expect(offsetToLine('abc\ndef\nghi', 4)).toBe(2);
    expect(offsetToLine('abc\ndef\nghi', 8)).toBe(3);
  });

  describe('selection precedence', () => {
    it('always executes selection regardless of strategy', () => {
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: 0,
        selection: 'SELECT custom',
        strategy: 'entire_script',
      });
      expect(res.strategyUsed).toBe('selection');
      expect(res.sql).toBe('SELECT custom');
      expect(res.needsAsk).toBe(false);
    });
  });

  describe('current_statement strategy', () => {
    it('executes statement containing cursor', () => {
      // Offset inside "SELECT id, name, email..."
      const offset = multiSql.indexOf('FROM users');
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: offset,
        strategy: 'current_statement',
      });
      expect(res.strategyUsed).toBe('current_statement');
      expect(res.sql).toContain('FROM users');
      expect(res.needsAsk).toBe(false);
      expect(res.currentStatement?.fromLine).toBe(3);
    });
  });

  describe('entire_script strategy', () => {
    it('executes entire doc even when cursor is in single statement', () => {
      const offset = multiSql.indexOf('FROM users');
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: offset,
        strategy: 'entire_script',
      });
      expect(res.strategyUsed).toBe('entire_script');
      expect(res.sql).toBe(multiSql.trim());
      expect(res.statementCount).toBe(3);
      expect(res.needsAsk).toBe(false);
    });
  });

  describe('largest_statement strategy', () => {
    it('picks statement directly under cursor if cursor inside statement', () => {
      const offset = multiSql.indexOf('1 AS one');
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: offset,
        strategy: 'largest_statement',
      });
      expect(res.sql).toBe('SELECT 1 AS one;');
    });

    it('picks largest statement when cursor is in whitespace/transition', () => {
      // Offset in blank line between statement 1 and 2
      const offset = multiSql.indexOf('\n\nSELECT id');
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: offset + 1,
        strategy: 'largest_statement',
      });
      expect(res.sql).toContain('FROM users');
    });
  });

  describe('ask strategy', () => {
    it('requests user ask when multiple statements exist', () => {
      const offset = multiSql.indexOf('FROM users');
      const res = resolveExecutionTarget({
        doc: multiSql,
        cursorOffset: offset,
        strategy: 'ask',
      });
      expect(res.needsAsk).toBe(true);
      expect(res.statementCount).toBe(3);
      expect(res.currentStatement?.sql).toContain('FROM users');
    });

    it('does not request ask when only single statement exists', () => {
      const singleSql = 'SELECT 42;';
      const res = resolveExecutionTarget({
        doc: singleSql,
        cursorOffset: 3,
        strategy: 'ask',
      });
      expect(res.needsAsk).toBe(false);
      expect(res.sql).toBe('SELECT 42;');
    });
  });

  describe('empty document', () => {
    it('handles empty document gracefully', () => {
      const res = resolveExecutionTarget({
        doc: '',
        cursorOffset: 0,
        strategy: 'current_statement',
      });
      expect(res.sql).toBe('');
      expect(res.statementCount).toBe(0);
      expect(res.needsAsk).toBe(false);
    });
  });
});
