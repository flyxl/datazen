import { StateField, type EditorState, type Transaction } from '@codemirror/state';
import { detectKindHint } from './kindHint';
import { buildLineIndex, isWhitespaceCode, lineAtOffset, type LineIndex } from './lineIndex';
import { scanSql } from './scanner';
import { isCommentKind, SqlTokenKind } from './tokens';
import type { SqlToken } from './types';
import type {
  SqlStatementRange,
  SqlTextRange,
  StatementIndexCacheKey,
  StatementIndexSnapshot,
} from './types';

const DEGRADED_PATTERN =
  /\bDELIMITER\b|\bCREATE\s+(?:OR\s+REPLACE\s+)?(?:PROCEDURE|FUNCTION|TRIGGER)\b/i;

const BOUNDARY_SCAN = { includeOther: false, includeText: false } as const;

function trimRangeBounds(source: string, from: number, to: number): SqlTextRange {
  let contentFrom = from;
  let contentTo = to;
  while (contentFrom < contentTo && isWhitespaceCode(source.charCodeAt(contentFrom))) {
    contentFrom += 1;
  }
  while (contentTo > contentFrom && isWhitespaceCode(source.charCodeAt(contentTo - 1))) {
    contentTo -= 1;
  }
  if (contentTo > contentFrom && source.charCodeAt(contentTo - 1) === 59) {
    contentTo -= 1;
    while (contentTo > contentFrom && isWhitespaceCode(source.charCodeAt(contentTo - 1))) {
      contentTo -= 1;
    }
  }
  return { from: contentFrom, to: contentTo };
}

function isBlankRange(source: string, from: number, to: number): boolean {
  for (let i = from; i < to; i += 1) {
    if (!isWhitespaceCode(source.charCodeAt(i))) return false;
  }
  return true;
}

export const SQL_STATEMENT_START_KEYWORDS = new Set([
  'select',
  'with',
  'insert',
  'update',
  'delete',
  'merge',
  'upsert',
  'replace',
  'create',
  'alter',
  'drop',
  'truncate',
  'rename',
  'grant',
  'revoke',
  'begin',
  'start',
  'commit',
  'rollback',
  'savepoint',
  'release',
  'set',
  'show',
  'reset',
  'use',
  'describe',
  'desc',
  'explain',
  'call',
  'exec',
  'execute',
  'prepare',
  'deallocate',
  'copy',
  'lock',
  'vacuum',
  'analyze',
  'reindex',
  'cluster',
  'checkpoint',
  'discard',
  'listen',
  'notify',
  'load',
  'import',
  'export',
  'pragma',
  'declare',
  'fetch',
  'move',
  'close',
  'from',
  'do',
  'refresh',
  'comment',
  'optimize',
]);

function isExecutableSqlStart(source: string, offset: number, to: number): boolean {
  const tail = source.slice(offset, Math.min(to, offset + 64));
  const match = /^\(?\s*([a-zA-Z_][a-zA-Z0-9_$]*)/.exec(tail);
  if (!match) return false;
  return SQL_STATEMENT_START_KEYWORDS.has(match[1]!.toLowerCase());
}

function computeFirstExecutableLine(
  source: string,
  from: number,
  to: number,
  lineStarts: LineIndex,
): number {
  let i = from;
  while (i < to) {
    const code = source.charCodeAt(i);
    const next = i + 1 < to ? source.charCodeAt(i + 1) : 0;
    if (isWhitespaceCode(code)) {
      i += 1;
      continue;
    }
    if (code === 45 && next === 45) {
      while (i < to && source.charCodeAt(i) !== 10) i += 1;
      continue;
    }
    if (code === 35) {
      while (i < to && source.charCodeAt(i) !== 10) i += 1;
      continue;
    }
    if (code === 47 && next === 42) {
      i += 2;
      while (i + 1 < to && !(source.charCodeAt(i) === 42 && source.charCodeAt(i + 1) === 47))
        i += 1;
      i = Math.min(to, i + 2);
      continue;
    }
    if (code === 39 || code === 34 || code === 96 || code === 91 || code === 36) {
      i += 1;
      continue;
    }
    if (!isExecutableSqlStart(source, i, to)) {
      return 0;
    }
    return lineAtOffset(lineStarts, i);
  }
  return 0;
}

function hasExecutableCode(source: string, from: number, to: number): boolean {
  let i = from;
  while (i < to) {
    const code = source.charCodeAt(i);
    const next = i + 1 < to ? source.charCodeAt(i + 1) : 0;
    if (isWhitespaceCode(code)) {
      i += 1;
      continue;
    }
    if (code === 45 && next === 45) {
      while (i < to && source.charCodeAt(i) !== 10) i += 1;
      continue;
    }
    if (code === 35) {
      while (i < to && source.charCodeAt(i) !== 10) i += 1;
      continue;
    }
    if (code === 47 && next === 42) {
      i += 2;
      while (i + 1 < to && !(source.charCodeAt(i) === 42 && source.charCodeAt(i + 1) === 47))
        i += 1;
      i = Math.min(to, i + 2);
      continue;
    }
    if (code === 39 || code === 34 || code === 96 || code === 91) {
      i += 1;
      while (i < to) {
        const c = source.charCodeAt(i);
        if (c === code) {
          if ((code === 39 || code === 34) && i + 1 < to && source.charCodeAt(i + 1) === code) {
            i += 2;
            continue;
          }
          if (code === 91 && i + 1 < to && source.charCodeAt(i + 1) === 93) {
            i += 2;
            continue;
          }
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }
    return true;
  }
  return false;
}

function statementBoundaries(source: string, tokens: readonly SqlToken[]): number[] {
  const ends: number[] = [];
  for (const token of tokens) {
    if (token.kind === SqlTokenKind.Semicolon && token.parenDepth === 0) {
      ends.push(token.to);
    }
  }
  if (ends.length === 0 || ends[ends.length - 1] !== source.length) {
    ends.push(source.length);
  }
  return ends;
}

function buildSingleRange(
  source: string,
  from: number,
  to: number,
  index: number,
  confidence: 'exact' | 'degraded',
  delimiterFrom: number | null,
  delimiterTo: number | null,
  lineStarts: LineIndex,
): SqlStatementRange {
  const content = trimRangeBounds(source, from, to);
  return {
    from,
    to,
    index,
    contentFrom: content.from,
    contentTo: content.to,
    delimiterFrom,
    delimiterTo,
    firstExecutableLine: computeFirstExecutableLine(source, from, to, lineStarts),
    kindHint: detectKindHint(source, content.from, content.to),
    confidence,
  };
}

function findDelimiter(
  source: string,
  from: number,
  to: number,
): { from: number; to: number } | null {
  let end = to;
  while (end > from && isWhitespaceCode(source.charCodeAt(end - 1))) {
    end -= 1;
  }
  if (end > from && source.charCodeAt(end - 1) === 59) {
    return { from: end - 1, to: end };
  }
  return null;
}

/** Build statement ranges using a precomputed token stream (avoids rescanning). */
export function buildStatementRangesFromTokens(
  source: string,
  tokens: readonly SqlToken[],
): SqlStatementRange[] {
  if (!source) return [];

  const lineStarts = buildLineIndex(source);

  if (DEGRADED_PATTERN.test(source)) {
    const delimiter = findDelimiter(source, 0, source.length);
    return [
      buildSingleRange(
        source,
        0,
        source.length,
        0,
        'degraded',
        delimiter?.from ?? null,
        delimiter?.to ?? null,
        lineStarts,
      ),
    ];
  }

  const boundaries = statementBoundaries(source, tokens);
  const ranges: SqlStatementRange[] = [];
  let from = 0;
  let index = 0;

  for (const to of boundaries) {
    if (from >= to) continue;
    if (isBlankRange(source, from, to)) {
      from = to;
      continue;
    }

    const delimiter = findDelimiter(source, from, to);
    ranges.push(
      buildSingleRange(
        source,
        from,
        to,
        index,
        'exact',
        delimiter?.from ?? null,
        delimiter?.to ?? null,
        lineStarts,
      ),
    );
    index += 1;
    from = to;
  }

  return ranges;
}

/** Build statement ranges for the full document (single lexical scan). */
export function buildStatementRanges(source: string): SqlStatementRange[] {
  if (!source) return [];
  const { tokens } = scanSql(source, BOUNDARY_SCAN);
  return buildStatementRangesFromTokens(source, tokens);
}

export function getStatementContent(source: string, range: SqlStatementRange): string {
  return source.slice(range.contentFrom, range.contentTo);
}

export function getValidStatementRanges(ranges: readonly SqlStatementRange[]): SqlStatementRange[] {
  return ranges.filter((r) => r.contentTo > r.contentFrom);
}

function offsetInRange(offset: number, range: SqlTextRange): boolean {
  return offset >= range.from && offset <= range.to;
}

function isPureCommentOffset(tokens: readonly SqlToken[], offset: number): boolean {
  for (const token of tokens) {
    if (offset >= token.from && offset < token.to) {
      return isCommentKind(token.kind);
    }
    if (token.from > offset) break;
  }
  return false;
}

/**
 * Locate the statement range for a cursor offset.
 * Returns null when the cursor sits in a pure comment region with no executable statement.
 */
export function findStatementAtCursor(
  source: string,
  cursorOffset: number,
  ranges?: readonly SqlStatementRange[],
  tokens?: readonly SqlToken[],
): SqlStatementRange | null {
  const allRanges = ranges ?? buildStatementRanges(source);
  const valid = getValidStatementRanges(allRanges);
  if (valid.length === 0) return null;

  const lexTokens = tokens ?? scanSql(source, BOUNDARY_SCAN).tokens;

  for (const range of valid) {
    if (offsetInRange(cursorOffset, range)) {
      if (cursorOffset < range.contentFrom) {
        const prev = valid.find((r) => r.index === range.index - 1);
        return prev ?? range;
      }
      if (
        isPureCommentOffset(lexTokens, cursorOffset) &&
        !hasExecutableCode(source, range.from, range.to)
      ) {
        return null;
      }
      return range;
    }
  }

  if (cursorOffset < valid[0]!.from) {
    return valid[0]!;
  }
  if (cursorOffset > valid[valid.length - 1]!.to) {
    return valid[valid.length - 1]!;
  }

  let left: SqlStatementRange | null = null;
  for (const range of valid) {
    if (range.to <= cursorOffset) {
      left = range;
    } else {
      break;
    }
  }
  return left ?? valid[0]!;
}

/** Legacy-compatible helper: trimmed physical statement text at cursor. */
export function getStatementTextAtCursor(source: string, cursorOffset: number): string {
  if (!source.trim()) return '';

  const range = findStatementAtCursor(source, cursorOffset);
  if (range) {
    const text = source.slice(range.from, range.to).trim();
    if (text) return text;
  }

  const valid = getValidStatementRanges(buildStatementRanges(source));
  if (valid.length === 0) return source.trim();
  return source.slice(valid[0]!.from, valid[0]!.to).trim();
}

// ── Incremental index + cache ─────────────────────────────────────────────

const indexCache = new Map<string, StatementIndexSnapshot>();
const CACHE_LIMIT = 32;

function makeCacheKey(key: StatementIndexCacheKey): string {
  return `${key.docIdentity}:${key.dialectId}:${key.revision}`;
}

export function getCachedStatementIndex(
  key: StatementIndexCacheKey,
): StatementIndexSnapshot | undefined {
  return indexCache.get(makeCacheKey(key));
}

export function setCachedStatementIndex(
  key: StatementIndexCacheKey,
  snapshot: StatementIndexSnapshot,
): void {
  const cacheKey = makeCacheKey(key);
  if (indexCache.size >= CACHE_LIMIT) {
    const first = indexCache.keys().next().value;
    if (first) indexCache.delete(first);
  }
  indexCache.set(cacheKey, snapshot);
}

export function clearStatementIndexCache(): void {
  indexCache.clear();
}

/** Rebuild statement ranges on document change. */
export function updateStatementRangesIncremental(
  source: string,
  _previous: readonly SqlStatementRange[],
  _changeFrom: number,
): SqlStatementRange[] {
  return buildStatementRanges(source);
}

export type StatementIndexState = StatementIndexSnapshot & {
  docIdentity: string;
  dialectId: string;
};

export function createStatementIndexState(
  source: string,
  revision: number,
  docIdentity = 'default',
  dialectId = 'standard',
): StatementIndexState {
  const ranges = buildStatementRanges(source);
  const snapshot: StatementIndexState = {
    revision,
    ranges,
    sourceLength: source.length,
    docIdentity,
    dialectId,
  };
  setCachedStatementIndex({ docIdentity, dialectId, revision }, snapshot);
  return snapshot;
}

export function updateStatementIndexState(
  state: StatementIndexState,
  source: string,
  tr: Transaction,
): StatementIndexState {
  if (!tr.docChanged) return state;

  let changeFrom = 0;
  tr.changes.iterChanges((from) => {
    changeFrom = from;
  });
  const ranges = updateStatementRangesIncremental(source, state.ranges, changeFrom);
  const next: StatementIndexState = {
    revision: state.revision + 1,
    ranges,
    sourceLength: source.length,
    docIdentity: state.docIdentity,
    dialectId: state.dialectId,
  };
  setCachedStatementIndex(
    { docIdentity: state.docIdentity, dialectId: state.dialectId, revision: next.revision },
    next,
  );
  return next;
}

const statementIndexFieldCache = new Map<string, StateField<StatementIndexState>>();

export function statementIndexField(docIdentity = 'default', dialectId = 'standard') {
  const key = `${docIdentity}::${dialectId}`;
  let field = statementIndexFieldCache.get(key);
  if (!field) {
    field = StateField.define<StatementIndexState>({
      create(state: EditorState) {
        return createStatementIndexState(state.doc.toString(), 0, docIdentity, dialectId);
      },
      update(value, tr) {
        if (!tr.docChanged) return value;
        return updateStatementIndexState(value, tr.state.doc.toString(), tr);
      },
    });
    statementIndexFieldCache.set(key, field);
  }
  return field;
}
