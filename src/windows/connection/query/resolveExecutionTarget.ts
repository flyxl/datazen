import {
  buildStatementRanges,
  getValidStatementRanges,
  findStatementAtCursor,
  type SqlStatementRange,
} from '@datazen/extension-points';
import type { SqlExecutionStrategy } from '../../../types';

export interface StatementTargetInfo {
  sql: string;
  from: number;
  to: number;
  fromLine: number;
  toLine: number;
  index: number;
}

export interface ResolveExecutionTargetResult {
  sql: string;
  strategyUsed: SqlExecutionStrategy | 'selection';
  needsAsk: boolean;
  statementCount: number;
  currentStatement?: StatementTargetInfo;
  allStatements: StatementTargetInfo[];
  entireScript: string;
}

export function offsetToLine(source: string, offset: number): number {
  if (offset <= 0) return 1;
  const bounded = Math.min(offset, source.length);
  let line = 1;
  for (let i = 0; i < bounded; i++) {
    if (source.charCodeAt(i) === 10) line++;
  }
  return line;
}

function toStatementTargetInfo(source: string, r: SqlStatementRange): StatementTargetInfo {
  const sql = source.slice(r.from, r.to).trim();
  return {
    sql,
    from: r.from,
    to: r.to,
    fromLine: offsetToLine(source, r.contentFrom),
    toLine: offsetToLine(source, r.contentTo),
    index: r.index,
  };
}

export function resolveExecutionTarget({
  doc,
  cursorOffset = 0,
  selection = '',
  strategy = 'current_statement',
}: {
  doc: string;
  cursorOffset?: number;
  selection?: string;
  strategy?: SqlExecutionStrategy;
}): ResolveExecutionTargetResult {
  const trimmedSelection = selection.trim();
  const entireScript = doc.trim();
  const allRanges = buildStatementRanges(doc);
  const validRanges = getValidStatementRanges(allRanges);
  const allStatements = validRanges.map((r) => toStatementTargetInfo(doc, r));

  // 1. Selection takes absolute precedence (§5.1)
  if (trimmedSelection.length > 0) {
    return {
      sql: trimmedSelection,
      strategyUsed: 'selection',
      needsAsk: false,
      statementCount: 1,
      allStatements,
      entireScript,
    };
  }

  if (validRanges.length === 0) {
    return {
      sql: entireScript,
      strategyUsed: strategy,
      needsAsk: false,
      statementCount: 0,
      allStatements,
      entireScript,
    };
  }

  const currentRange = findStatementAtCursor(doc, cursorOffset, validRanges) ?? validRanges[0];
  const currentStatement = currentRange ? toStatementTargetInfo(doc, currentRange) : undefined;

  // 2. Entire script strategy
  if (strategy === 'entire_script') {
    return {
      sql: entireScript,
      strategyUsed: 'entire_script',
      needsAsk: false,
      statementCount: validRanges.length,
      currentStatement,
      allStatements,
      entireScript,
    };
  }

  // 3. Ask strategy (§5.1)
  if (strategy === 'ask') {
    if (validRanges.length > 1) {
      return {
        sql: currentStatement ? currentStatement.sql : entireScript,
        strategyUsed: 'ask',
        needsAsk: true,
        statementCount: validRanges.length,
        currentStatement,
        allStatements,
        entireScript,
      };
    }
    return {
      sql: currentStatement ? currentStatement.sql : entireScript,
      strategyUsed: 'ask',
      needsAsk: false,
      statementCount: validRanges.length,
      currentStatement,
      allStatements,
      entireScript,
    };
  }

  // 4. Largest statement strategy
  if (strategy === 'largest_statement') {
    const isDirectlyInside = cursorOffset >= currentRange.from && cursorOffset <= currentRange.to;

    if (isDirectlyInside) {
      return {
        sql: currentStatement?.sql || entireScript,
        strategyUsed: 'largest_statement',
        needsAsk: false,
        statementCount: validRanges.length,
        currentStatement,
        allStatements,
        entireScript,
      };
    }

    // Cursor in whitespace / transition: pick statement with largest code length
    let largest = currentRange;
    for (const r of validRanges) {
      const len = r.contentTo - r.contentFrom;
      const curLen = largest.contentTo - largest.contentFrom;
      if (len > curLen) {
        largest = r;
      }
    }
    const largestStatement = toStatementTargetInfo(doc, largest);
    return {
      sql: largestStatement.sql,
      strategyUsed: 'largest_statement',
      needsAsk: false,
      statementCount: validRanges.length,
      currentStatement: largestStatement,
      allStatements,
      entireScript,
    };
  }

  // 5. Default: current_statement
  return {
    sql: currentStatement ? currentStatement.sql : entireScript,
    strategyUsed: 'current_statement',
    needsAsk: false,
    statementCount: validRanges.length,
    currentStatement,
    allStatements,
    entireScript,
  };
}
