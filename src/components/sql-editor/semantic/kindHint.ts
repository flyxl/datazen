import type { SqlStatementKind } from './types';

function isWhitespace(code: number): boolean {
  return code === 32 || code === 9 || code === 10 || code === 13 || code === 12;
}

function isIdentStart(code: number): boolean {
  return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || code === 95;
}

function readWord(source: string, from: number, to: number): string {
  let i = from;
  while (i < to && isWhitespace(source.charCodeAt(i))) i += 1;
  const start = i;
  while (i < to) {
    const code = source.charCodeAt(i);
    if (!isIdentStart(code) && !(code >= 48 && code <= 57)) break;
    i += 1;
  }
  return i > start ? source.slice(start, i).toUpperCase() : '';
}

/** Detect statement kind from leading keyword without allocating statement text. */
export function detectKindHint(
  source: string,
  from: number,
  to: number,
): SqlStatementKind | undefined {
  let i = from;
  while (i < to && isWhitespace(source.charCodeAt(i))) i += 1;
  if (i >= to) return undefined;

  const word = readWord(source, i, to);
  if (!word) return undefined;

  if (word === 'WITH' || word === 'SELECT') return 'select';
  if (word === 'INSERT') return 'insert';
  if (word === 'UPDATE') return 'update';
  if (word === 'DELETE') return 'delete';
  if (word === 'MERGE') return 'merge';
  if (
    word === 'CREATE' ||
    word === 'ALTER' ||
    word === 'DROP' ||
    word === 'TRUNCATE' ||
    word === 'RENAME' ||
    word === 'GRANT' ||
    word === 'REVOKE'
  ) {
    return 'ddl';
  }
  if (word === 'COMMENT') {
    const rest = source.slice(i, Math.min(to, i + 16)).toUpperCase();
    if (rest.startsWith('COMMENT ON')) return 'ddl';
  }
  if (
    word === 'BEGIN' ||
    word === 'COMMIT' ||
    word === 'ROLLBACK' ||
    word === 'SAVEPOINT' ||
    word === 'END'
  ) {
    return 'transaction';
  }
  if (word === 'START') {
    const rest = source.slice(i, Math.min(to, i + 24)).toUpperCase();
    if (rest.startsWith('START TRANSACTION')) return 'transaction';
  }
  return 'other';
}
