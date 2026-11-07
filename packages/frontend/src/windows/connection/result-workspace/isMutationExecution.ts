import type { StatementResult } from '../../../types';

export const QUERY_VERBS = new Set([
  'SELECT',
  'SHOW',
  'EXPLAIN',
  'DESC',
  'DESCRIBE',
  'PRAGMA',
  'VALUES',
]);

export const MUTATION_VERBS = new Set([
  'INSERT',
  'UPDATE',
  'DELETE',
  'CREATE',
  'ALTER',
  'DROP',
  'TRUNCATE',
  'RENAME',
  'GRANT',
  'REVOKE',
  'SET',
  'BEGIN',
  'COMMIT',
  'ROLLBACK',
  'USE',
  'MERGE',
  'UPSERT',
  'REPLACE',
]);

/** Strips leading SQL comments (--..., /*...*\/, #...) and whitespace. */
export function stripLeadingSqlComments(sql: string): string {
  let s = sql.trim();
  while (true) {
    if (s.startsWith('--') || s.startsWith('#')) {
      const newlineIdx = s.indexOf('\n');
      if (newlineIdx === -1) return '';
      s = s.slice(newlineIdx + 1).trim();
    } else if (s.startsWith('/*')) {
      const closeIdx = s.indexOf('*/');
      if (closeIdx === -1) return '';
      s = s.slice(closeIdx + 2).trim();
    } else {
      break;
    }
  }
  return s;
}

/**
 * Extracts the primary top-level SQL verb from a statement.
 * Handles leading comments and CTE queries (WITH ... SELECT / WITH ... INSERT).
 */
export function getTopLevelSqlVerb(sql: string): string {
  const clean = stripLeadingSqlComments(sql);
  if (!clean) return '';

  const firstWordMatch = clean.match(/^([A-Za-z]+)\b/);
  if (!firstWordMatch) return '';
  const firstVerb = firstWordMatch[1]!.toUpperCase();

  if (firstVerb !== 'WITH') {
    return firstVerb;
  }

  // If starts with WITH, scan past CTE definitions (tracking parenthesis depth) to find main statement verb
  let depth = 0;
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let i = 4; // skip 'WITH'
  const len = clean.length;

  while (i < len) {
    const ch = clean[i];
    const next = i + 1 < len ? clean[i + 1] : '';

    if (inSingleQuote) {
      if (ch === "'" && next === "'") {
        i += 2;
        continue;
      }
      if (ch === "'") inSingleQuote = false;
      i++;
      continue;
    }
    if (inDoubleQuote) {
      if (ch === '"' && next === '"') {
        i += 2;
        continue;
      }
      if (ch === '"') inDoubleQuote = false;
      i++;
      continue;
    }

    if (ch === "'") {
      inSingleQuote = true;
      i++;
      continue;
    }
    if (ch === '"') {
      inDoubleQuote = true;
      i++;
      continue;
    }

    // Skip comments
    if (ch === '-' && next === '-') {
      const nl = clean.indexOf('\n', i + 2);
      i = nl === -1 ? len : nl + 1;
      continue;
    }
    if (ch === '/' && next === '*') {
      const end = clean.indexOf('*/', i + 2);
      i = end === -1 ? len : end + 2;
      continue;
    }

    if (ch === '(') {
      depth++;
      i++;
      continue;
    }
    if (ch === ')') {
      depth--;
      i++;
      continue;
    }

    // When outside parenthesis (depth === 0), look for statement-initiating keywords
    if (depth === 0 && /[A-Za-z]/.test(ch)) {
      const sub = clean.slice(i);
      const m = sub.match(/^([A-Za-z]+)\b/);
      if (m) {
        const word = m[1]!.toUpperCase();
        if (word === 'RECURSIVE' || word === 'AS') {
          i += m[0].length;
          continue;
        }
        if (QUERY_VERBS.has(word) || MUTATION_VERBS.has(word)) {
          return word;
        }
        i += m[0].length;
        continue;
      }
    }

    i++;
  }

  return 'SELECT'; // Default WITH CTEs to SELECT
}

/**
 * Returns true if the statement result represents a DML / DDL statement execution
 * that does not produce a tabular recordset (even if rowsAffected is 0 or positive).
 */
export function isMutationExecution(result: StatementResult): boolean {
  // If there are columns returned or tabular rows returned, it's a tabular result (e.g. SELECT or RETURNING)
  if (result.columns.length > 0 || result.rows.length > 0) {
    return false;
  }

  // If SQL is present, determine the top-level verb
  if (result.sql) {
    const verb = getTopLevelSqlVerb(result.sql);
    if (QUERY_VERBS.has(verb)) {
      return false; // SELECT / SHOW / EXPLAIN etc. with 0 rows should display default empty result set
    }
    if (MUTATION_VERBS.has(verb)) {
      return true; // INSERT / UPDATE / DELETE / DDL without tabular output is a mutation
    }
  }

  // Fallback: If no SQL or unrecognized verb, do not treat as mutation.
  // In PostgreSQL/MySQL, empty SELECT statements also return rowsAffected = 0 with columns = [].
  // Treating them as mutation when verb is unknown would cause false positives.
  return false;
}
