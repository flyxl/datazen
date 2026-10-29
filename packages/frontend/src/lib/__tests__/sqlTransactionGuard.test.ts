import { describe, expect, it } from 'vitest';
import {
  analyzeTransactionSql,
  isAbortedTransactionError,
  splitSqlStatements,
  stripSqlNoise,
} from '../sqlTransactionGuard';

describe('sqlTransactionGuard', () => {
  describe('masking / splitting', () => {
    it('masks -- comment BEGIN so analysis ignores it', () => {
      const masked = stripSqlNoise('SELECT 1; -- BEGIN\nSELECT 2');
      expect(masked).toContain('SELECT 2');
      expect(analyzeTransactionSql('SELECT 1; -- BEGIN\nSELECT 2').hasUnclosedBegin).toBe(false);
    });

    it('masks # line comments containing semicolons', () => {
      const stmts = splitSqlStatements('SELECT 1; # foo; bar\nSELECT 2');
      // Comment `;` is masked so we do not get a spurious empty split mid-comment.
      expect(stmts).toEqual(['SELECT 1', '# foo  bar\nSELECT 2']);
    });

    it('masks block comments containing semicolons', () => {
      const stmts = splitSqlStatements('SELECT 1; /* a; b */ SELECT 2');
      expect(stmts).toEqual(['SELECT 1', '/* a  b */ SELECT 2']);
    });

    it('tolerates unclosed block comments', () => {
      expect(splitSqlStatements('SELECT 1; /* never closed')).toEqual([
        'SELECT 1',
        '/* never closed',
      ]);
    });

    it('splits on semicolons outside single quotes', () => {
      const stmts = splitSqlStatements("SELECT 'a;b'; SELECT 2");
      expect(stmts).toHaveLength(2);
      expect(stmts[0]!.startsWith('SELECT')).toBe(true);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('handles escaped single quotes', () => {
      const stmts = splitSqlStatements("SELECT 'it''s;ok'; SELECT 2");
      expect(stmts).toHaveLength(2);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('handles double-quoted identifiers with semicolons', () => {
      const stmts = splitSqlStatements('SELECT "a;b"; SELECT 2');
      expect(stmts).toHaveLength(2);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('handles escaped double quotes', () => {
      const stmts = splitSqlStatements('SELECT "a""b;c"; SELECT 2');
      expect(stmts).toHaveLength(2);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('masks dollar-quoted bodies with semicolons', () => {
      const stmts = splitSqlStatements('SELECT $$foo;bar$$; SELECT 2');
      expect(stmts).toHaveLength(2);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('masks tagged dollar-quoted bodies', () => {
      const stmts = splitSqlStatements('SELECT $tag$foo;bar$tag$; SELECT 2');
      expect(stmts).toHaveLength(2);
      expect(stmts[1]).toBe('SELECT 2');
    });

    it('falls through when dollar-quote tag has no closer', () => {
      // Unclosed $tag$ — semicolon still splits (heuristic, not a full parser)
      expect(splitSqlStatements('SELECT $tag$foo; SELECT 2')).toEqual([
        'SELECT $tag$foo',
        'SELECT 2',
      ]);
    });

    it('leaves lone $ without dollar-quote tag alone', () => {
      expect(splitSqlStatements('SELECT $1; SELECT 2')).toEqual(['SELECT $1', 'SELECT 2']);
    });

    it('filters empty statements', () => {
      expect(splitSqlStatements('SELECT 1;;;')).toEqual(['SELECT 1']);
    });
  });

  describe('analyzeTransactionSql', () => {
    it('detects unclosed BEGIN / START TRANSACTION', () => {
      expect(analyzeTransactionSql('BEGIN; INSERT INTO t VALUES (1);').hasUnclosedBegin).toBe(true);
      expect(analyzeTransactionSql('START TRANSACTION; UPDATE t SET a=1;').hasUnclosedBegin).toBe(
        true,
      );
      expect(analyzeTransactionSql('BEGIN WORK; UPDATE t SET a=1;').hasUnclosedBegin).toBe(true);
    });

    it('treats COMMIT / END / ROLLBACK as closing', () => {
      expect(analyzeTransactionSql('BEGIN; UPDATE t SET a=1; COMMIT;').hasUnclosedBegin).toBe(
        false,
      );
      expect(analyzeTransactionSql('BEGIN; UPDATE t SET a=1; COMMIT WORK;').hasUnclosedBegin).toBe(
        false,
      );
      expect(analyzeTransactionSql('BEGIN; UPDATE t SET a=1; END;').hasUnclosedBegin).toBe(false);
      expect(
        analyzeTransactionSql('BEGIN; UPDATE t SET a=1; END TRANSACTION;').hasUnclosedBegin,
      ).toBe(false);
      expect(analyzeTransactionSql('BEGIN; UPDATE t SET a=1; ROLLBACK;').hasUnclosedBegin).toBe(
        false,
      );
      expect(
        analyzeTransactionSql('BEGIN; UPDATE t SET a=1; ROLLBACK WORK;').hasUnclosedBegin,
      ).toBe(false);
    });

    it('does not treat ROLLBACK TO SAVEPOINT as closing the outer TX', () => {
      expect(
        analyzeTransactionSql('BEGIN; SAVEPOINT s1; ROLLBACK TO s1; INSERT INTO t VALUES (1);')
          .hasUnclosedBegin,
      ).toBe(true);
    });

    it('ignores BEGIN inside comments', () => {
      expect(analyzeTransactionSql('-- BEGIN\nSELECT 1;').hasUnclosedBegin).toBe(false);
      expect(analyzeTransactionSql('/* BEGIN */\nSELECT 1;').hasUnclosedBegin).toBe(false);
    });

    it('tracks nested begin/commit depth and counts', () => {
      const nested = analyzeTransactionSql('BEGIN; BEGIN; COMMIT;');
      expect(nested.hasUnclosedBegin).toBe(true);
      expect(nested.beginCount).toBe(2);
      expect(nested.endCount).toBe(1);
      expect(nested.depth).toBe(1);

      const balanced = analyzeTransactionSql('BEGIN; BEGIN; COMMIT; COMMIT;');
      expect(balanced.hasUnclosedBegin).toBe(false);
      expect(balanced.depth).toBe(0);
    });

    it('does not go negative when COMMIT has no BEGIN', () => {
      const a = analyzeTransactionSql('COMMIT;');
      expect(a.depth).toBe(0);
      expect(a.hasUnclosedBegin).toBe(false);
      expect(a.endCount).toBe(1);
    });
  });

  describe('isAbortedTransactionError', () => {
    it('detects postgres aborted messages', () => {
      expect(
        isAbortedTransactionError(
          'current transaction is aborted, commands ignored until end of transaction block',
        ),
      ).toBe(true);
      expect(isAbortedTransactionError('ERROR: 25P02')).toBe(true);
      expect(isAbortedTransactionError('current transaction is in error')).toBe(true);
      expect(isAbortedTransactionError('当前事务已中止，命令被忽略直到事务块结束')).toBe(true);
      expect(isAbortedTransactionError('事务中止')).toBe(true);
    });

    it('rejects unrelated / empty errors', () => {
      expect(isAbortedTransactionError('syntax error at or near "FOO"')).toBe(false);
      expect(isAbortedTransactionError(null)).toBe(false);
      expect(isAbortedTransactionError(undefined)).toBe(false);
      expect(isAbortedTransactionError('')).toBe(false);
    });
  });
});
