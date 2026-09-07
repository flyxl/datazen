/**
 * Lightweight SQL transaction heuristics for Query Panel UX.
 *
 * Not a full SQL parser: strips comments/strings, splits on `;`, then looks at
 * statement-leading transaction keywords.
 *
 * Statement splitting delegates to the unified semantic scanner.
 */
import { maskSemicolonsInLiterals } from '../components/sql-editor/semantic/scanner';

/** Mask `;` inside comments and quoted literals so a naive split is safe. */
export function stripSqlNoise(sql: string): string {
  return maskSemicolonsInLiterals(sql);
}

export function splitSqlStatements(sql: string): string[] {
  return maskSemicolonsInLiterals(sql)
    .split(';')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function normalizeStmtHead(stmt: string): string {
  return stmt.trim().replace(/\s+/g, ' ').toUpperCase();
}

function isBeginStmt(head: string): boolean {
  return head === 'BEGIN' || head.startsWith('BEGIN ') || head.startsWith('START TRANSACTION');
}

function isCommitStmt(head: string): boolean {
  return (
    head === 'COMMIT' || head.startsWith('COMMIT ') || head === 'END' || head.startsWith('END ')
  );
}

/** Outer ROLLBACK that ends a transaction (not ROLLBACK TO SAVEPOINT). */
function isRollbackStmt(head: string): boolean {
  if (!head.startsWith('ROLLBACK')) return false;
  return !/^ROLLBACK\s+TO\b/.test(head);
}

export interface TransactionSqlAnalysis {
  /** BEGIN/START TRANSACTION count exceeds COMMIT/ROLLBACK closers. */
  hasUnclosedBegin: boolean;
  beginCount: number;
  endCount: number;
  depth: number;
}

/** Detect scripts that open a transaction without closing it. */
export function analyzeTransactionSql(sql: string): TransactionSqlAnalysis {
  const stmts = splitSqlStatements(sql);
  let depth = 0;
  let beginCount = 0;
  let endCount = 0;

  for (const stmt of stmts) {
    const head = normalizeStmtHead(stmt);
    if (isBeginStmt(head)) {
      beginCount += 1;
      depth += 1;
    } else if (isCommitStmt(head) || isRollbackStmt(head)) {
      endCount += 1;
      depth = Math.max(0, depth - 1);
    }
  }

  return {
    hasUnclosedBegin: depth > 0,
    beginCount,
    endCount,
    depth,
  };
}

/** Driver / PG error text for an aborted open transaction. */
export function isAbortedTransactionError(message: string | null | undefined): boolean {
  if (!message) return false;
  return (
    /current transaction is (aborted|in error)/i.test(message) ||
    /commands ignored until end of transaction block/i.test(message) ||
    /25P02/i.test(message) ||
    /事务.*(已)?中止|当前事务已中止|直到事务块结束/i.test(message)
  );
}
