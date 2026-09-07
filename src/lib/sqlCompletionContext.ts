import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';
import { schemaCompletionSource, type SQLConfig } from '@codemirror/lang-sql';

export type SqlCompletionKind = 'column' | 'table' | 'any';

const TABLE_KWS = new Set(['from', 'join', 'into', 'update', 'table', 'using', 'truncate']);

const COLUMN_KWS = new Set([
  'select',
  'where',
  'having',
  'set',
  'on',
  'and',
  'or',
  'not',
  'between',
  'like',
  'ilike',
  'in',
  'when',
  'returning',
  'group',
  'order',
  'by',
  'distinct',
  'values',
  'limit',
  'offset',
]);

const TRACKED_KWS = new Set([...TABLE_KWS, ...COLUMN_KWS]);

/** Last SQL keyword before the cursor, with its end offset, ignoring strings and comments. */
export function lastBareSqlKeywordWithOffset(sql: string): { kw: string; end: number } | null {
  let lastKw: string | null = null;
  let lastEnd = 0;
  let i = 0;
  while (i < sql.length) {
    const ch = sql[i]!;
    const next = sql[i + 1];
    if (ch === "'") {
      i += 1;
      while (i < sql.length) {
        if (sql[i] === "'" && sql[i + 1] === "'") {
          i += 2;
          continue;
        }
        if (sql[i] === "'") {
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }
    if (ch === '-' && next === '-') {
      while (i < sql.length && sql[i] !== '\n') i += 1;
      continue;
    }
    if (ch === '/' && next === '*') {
      i += 2;
      while (i < sql.length && !(sql[i] === '*' && sql[i + 1] === '/')) i += 1;
      i += 2;
      continue;
    }
    if (/[A-Za-z_]/.test(ch)) {
      let j = i + 1;
      while (j < sql.length && /[\w$]/.test(sql[j]!)) j += 1;
      const word = sql.slice(i, j).toLowerCase();
      if (TRACKED_KWS.has(word)) {
        lastKw = word;
        lastEnd = j;
      }
      i = j;
      continue;
    }
    i += 1;
  }
  return lastKw ? { kw: lastKw, end: lastEnd } : null;
}

/** Last SQL keyword before the cursor, ignoring strings and comments. */
export function lastBareSqlKeyword(sql: string): string | null {
  const res = lastBareSqlKeywordWithOffset(sql);
  return res ? res.kw : null;
}

/**
 * Detect whether the text tail following a table-clause keyword (FROM, JOIN, INTO, UPDATE)
 * is still in the process of typing a table name, vs. already completed the table name.
 */
export function isTableContext(tail: string): boolean {
  // If there is a comma, consider after the last comma (for multi-table FROM clause)
  const commaIdx = tail.lastIndexOf(',');
  const segment = commaIdx !== -1 ? tail.slice(commaIdx + 1) : tail;

  const trimmedLeading = segment.trimStart();
  if (trimmedLeading === '') {
    return true; // e.g. "FROM " or "FROM table, "
  }

  // Tokenize the segment into words / identifiers.
  // A qualified identifier like public.users or "public"."users" counts as one table reference.
  const re =
    /(?:["`\[][^"`\]]+["`\]]|\b[A-Za-z0-9_$]+\b)(?:\s*\.\s*(?:["`\[][^"`\]]+["`\]]|\b[A-Za-z0-9_$]+\b))*/g;
  const matches: Array<{ text: string; from: number; to: number }> = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(segment)) !== null) {
    matches.push({ text: m[0], from: m.index, to: m.index + m[0].length });
  }

  if (matches.length === 0) return true;
  if (matches.length === 1) {
    const firstMatch = matches[0]!;
    const afterFirst = segment.slice(firstMatch.to);
    // If there is whitespace or trailing characters after the first table identifier, table is completed!
    if (/^\s+/.test(afterFirst)) {
      return false; // Table complete, user is typing alias or WHERE / JOIN
    }
    return true; // Still typing first table name prefix (e.g. "FROM ec")
  }

  // 2 or more tokens (e.g. "er_customers WHE") -> definitely not table context
  return false;
}

/** `INSERT INTO t (` column list — last keyword is still INTO. */
function inInsertColumnList(stmt: string): boolean {
  return /\binto\b[\s\S]*\((?:[^)'"`]|'[^']*'|"[^"]*"|`[^`]*`)*$/i.test(stmt);
}

/**
 * `FROM` / `JOIN` → tables; `WHERE` / `SELECT` / `HAVING` → columns.
 * After `ident.` the schema path already picked the level — do not filter.
 */
export function inferSqlCompletionKind(textBeforeCursor: string): SqlCompletionKind {
  if (/\.\s*(?:["`][\w$]*|[\w$]*)$/.test(textBeforeCursor)) return 'any';
  const stmtStart = textBeforeCursor.lastIndexOf(';') + 1;
  const stmt = textBeforeCursor.slice(stmtStart);
  if (inInsertColumnList(stmt)) return 'column';
  const last = lastBareSqlKeywordWithOffset(stmt);
  if (!last) return 'any';
  if (TABLE_KWS.has(last.kw)) {
    const tail = stmt.slice(last.end);
    if (isTableContext(tail)) {
      return 'table';
    }
    // Table is already specified; user is typing alias or subsequent clause keywords (WHERE, JOIN, etc.)
    return 'any';
  }
  if (COLUMN_KWS.has(last.kw)) return 'column';
  return 'any';
}

/**
 * Extract qualifier parts from text before cursor when the user is typing
 * a dot-qualified name (e.g. `alias.` or `schema.table.`).
 *
 * Returns an array of segment names (unquoted), or an empty array if no
 * qualifier is present.
 *
 * Examples:
 *   "SELECT o." → ["o"]
 *   "SELECT o." → ["o"]
 *   "SELECT public.users." → ["public", "users"]
 */
/**
 * Extract qualifier parts from text before cursor when the user is typing
 * a dot-qualified name (e.g. `alias.` or `schema.table.`).
 *
 * Returns an array of segment names (unquoted), or an empty array if no
 * qualifier is present.
 */
export function extractQualifierParts(textBeforeCursor: string): string[] {
  const text = textBeforeCursor;
  if (!text) return [];

  let i = text.length - 1;

  // 1. Skip the identifier currently being typed at the cursor (if any)
  if (i >= 0 && (text[i] === '"' || text[i] === '`' || text[i] === ']')) {
    const closeQuote = text[i]!;
    const openQuote = closeQuote === ']' ? '[' : closeQuote;
    i -= 1;
    while (i >= 0 && text[i] !== openQuote) i -= 1;
    if (i >= 0) i -= 1; // skip the open quote
  } else {
    while (i >= 0 && /[A-Za-z0-9_$]/.test(text[i]!)) i -= 1;
  }

  // 2. Skip any whitespace between the dot and the identifier being typed
  while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;

  // 3. The separator MUST be a '.' directly preceding the current identifier
  if (i < 0 || text[i] !== '.') {
    return [];
  }

  // 4. We confirmed this is a dot context. Now collect all qualifier segments before this dot.
  const segments: string[] = [];

  while (i >= 0 && text[i] === '.') {
    i -= 1; // skip the '.'
    // skip whitespace before dot
    while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;
    if (i < 0) break;

    if (text[i] === '"') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '"') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end).replace(/""/g, '"'));
        i -= 1;
      } else {
        break;
      }
    } else if (text[i] === '`') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '`') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end));
        i -= 1;
      } else {
        break;
      }
    } else if (text[i] === ']') {
      const end = i;
      i -= 1;
      while (i >= 0 && text[i] !== '[') i -= 1;
      if (i >= 0) {
        segments.unshift(text.slice(i + 1, end).replace(/]]/g, ']'));
        i -= 1;
      } else {
        break;
      }
    } else if (/[A-Za-z0-9_$]/.test(text[i]!)) {
      const end = i + 1;
      while (i >= 0 && /[A-Za-z0-9_$]/.test(text[i]!)) i -= 1;
      segments.unshift(text.slice(i + 1, end));
    } else {
      break;
    }

    // Skip whitespace before this segment to check if there is an outer dot
    while (i >= 0 && (text[i] === ' ' || text[i] === '\t')) i -= 1;
  }

  return segments;
}

/**
 * Detect whether the cursor is right after `alias.` and the qualifier
 * maps to exactly one scope binding (returns `'unique'`), is ambiguous
 * (`'ambiguous'`), or is unknown/not a qualifier (`'none'`).
 */
export function detectAliasDotContext(textBeforeCursor: string): {
  kind: 'unique' | 'ambiguous' | 'none';
  qualifier: string;
} {
  const parts = extractQualifierParts(textBeforeCursor);
  if (parts.length === 0) return { kind: 'none', qualifier: '' };
  if (parts.length === 1) {
    // Single qualifier — this could be an alias or a table name.
    // We return 'unique' here; the schemaCompletion module will do the
    // actual scope resolution to determine ambiguity.
    return { kind: 'unique', qualifier: parts[0]! };
  }
  // Multiple qualifier parts — treat as a schema.table path, not an alias.
  return { kind: 'none', qualifier: parts.join('.') };
}

export function filterCompletionsByKind(
  options: readonly Completion[],
  kind: SqlCompletionKind,
): Completion[] {
  if (kind === 'column') {
    return options
      .filter((o) => o.type === 'property')
      .map((o) => (o.boost !== undefined ? o : { ...o, boost: 5 }));
  }
  if (kind === 'table') {
    return options
      .filter((o) => o.type === 'type')
      .map((o) => (o.boost !== undefined ? o : { ...o, boost: 10 }));
  }
  return [...options];
}

/** Keywords allowed in table contexts (e.g. after FROM, JOIN, INTO, UPDATE) */
export const TABLE_CONTEXT_ALLOWED_KEYWORDS = new Set([
  'select',
  'values',
  'lateral',
  'unnest',
  'only',
  'final',
  'json_table',
  'xmltable',
]);

/**
 * Keywords allowed in column / expression / WHERE contexts.
 * Suppresses irrelevant DDL/admin/transaction keywords (like CREATE, ALTER, DROP, VACUUM, GRANT),
 * retaining only valid expression operators, predicates, and terminating clauses.
 */
export const COLUMN_CONTEXT_ALLOWED_KEYWORDS = new Set([
  'and',
  'or',
  'not',
  'is',
  'null',
  'true',
  'false',
  'in',
  'between',
  'like',
  'ilike',
  'similar',
  'escape',
  'exists',
  'case',
  'when',
  'then',
  'else',
  'end',
  'distinct',
  'any',
  'all',
  'some',
  'select',
  'from',
  'as',
  'into',
  'where',
  'join',
  'left',
  'right',
  'inner',
  'full',
  'cross',
  'on',
  'order',
  'group',
  'having',
  'limit',
  'offset',
  'fetch',
  'union',
  'intersect',
  'except',
  'returning',
  'window',
  'for',
  'interval',
  'array',
  'collate',
]);

/**
 * Filter keyword completions based on the SQL context.
 * In table context, suppress extraneous keywords (like PG_EXCEPTION_*, OCCURRENCES_REGEX, etc.),
 * only retaining valid table-position keywords with reduced boost.
 * In column/WHERE context, suppress DDL/admin commands, keeping only expression operators and clauses.
 */
export function filterKeywordsByKind(
  options: readonly Completion[],
  kind: SqlCompletionKind,
): Completion[] {
  if (kind === 'table') {
    return options
      .filter((o) => TABLE_CONTEXT_ALLOWED_KEYWORDS.has(o.label.toLowerCase()))
      .map((o) => ({ ...o, boost: (o.boost ?? 0) - 2 }));
  }
  if (kind === 'column') {
    return options
      .filter((o) => COLUMN_CONTEXT_ALLOWED_KEYWORDS.has(o.label.toLowerCase()))
      .map((o) => ({ ...o, boost: (o.boost ?? 0) - 2 }));
  }
  return [...options];
}

function isThenable<T>(value: T | PromiseLike<T>): value is PromiseLike<T> {
  return typeof value === 'object' && value !== null && 'then' in value;
}

/**
 * Keyword completion source that filters out irrelevant keywords in table context.
 */
export function contextualKeywordCompletion(source: CompletionSource): CompletionSource {
  return (context: CompletionContext) => {
    const textBefore = context.state.sliceDoc(0, context.pos);
    if (detectAliasDotContext(textBefore).kind !== 'none') {
      // After an alias dot (e.g. eo. or schema.table.), do not suggest SQL keywords
      return null;
    }
    const kind = inferSqlCompletionKind(textBefore);
    const raw = source(context);
    if (!raw) return null;
    const apply = (result: CompletionResult): CompletionResult => {
      const filtered = filterKeywordsByKind(result.options, kind);
      return { ...result, options: filtered };
    };
    if (isThenable(raw)) {
      return Promise.resolve(raw).then((result) => (result ? apply(result) : result));
    }
    return apply(raw);
  };
}

/** Schema completions that hide tables/schemas while typing columns (and vice versa). */
export function contextualSchemaCompletion(config: SQLConfig): CompletionSource {
  const inner = schemaCompletionSource(config);
  return (context: CompletionContext) => {
    const textBefore = context.state.sliceDoc(0, context.pos);
    if (detectAliasDotContext(textBefore).kind !== 'none') {
      // An alias dot or qualified path is being typed (e.g. `eo.` or `schema.table.`).
      // Let the semantic-aware schemaAwareCompletionSource handle it exclusively to prevent duplicate columns!
      return null;
    }
    const raw = inner(context);
    if (!raw) return null;
    const apply = (result: CompletionResult): CompletionResult => {
      const kind = inferSqlCompletionKind(context.state.sliceDoc(0, context.pos));
      return { ...result, options: filterCompletionsByKind(result.options, kind) };
    };
    if (isThenable(raw)) {
      return Promise.resolve(raw).then((result) => (result ? apply(result) : result));
    }
    return apply(raw);
  };
}
