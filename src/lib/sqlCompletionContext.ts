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

/** Last SQL keyword before the cursor, ignoring strings and comments. */
export function lastBareSqlKeyword(sql: string): string | null {
  let last: string | null = null;
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
      if (TRACKED_KWS.has(word)) last = word;
      i = j;
      continue;
    }
    i += 1;
  }
  return last;
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
  const kw = lastBareSqlKeyword(stmt);
  if (!kw) return 'any';
  if (TABLE_KWS.has(kw)) return 'table';
  if (COLUMN_KWS.has(kw)) return 'column';
  return 'any';
}

export function filterCompletionsByKind(
  options: readonly Completion[],
  kind: SqlCompletionKind,
): Completion[] {
  if (kind === 'column') return options.filter((o) => o.type === 'property');
  if (kind === 'table') return options.filter((o) => o.type === 'type');
  return [...options];
}

function isThenable<T>(value: T | PromiseLike<T>): value is PromiseLike<T> {
  return typeof value === 'object' && value !== null && 'then' in value;
}

/** Schema completions that hide tables/schemas while typing columns (and vice versa). */
export function contextualSchemaCompletion(config: SQLConfig): CompletionSource {
  const inner = schemaCompletionSource(config);
  return (context: CompletionContext) => {
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
