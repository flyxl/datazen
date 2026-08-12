export type SqlParamKind = 'named' | 'positional';

export interface SqlParam {
  name: string;
  kind: SqlParamKind;
}

const IDENT = /[A-Za-z_][A-Za-z0-9_]*/y;
const DIGITS = /[0-9]+/y;

/** Extract `:name` / `$1` placeholders, skipping quoted strings and comments. */
export function parseSqlParams(sql: string): SqlParam[] {
  const found: SqlParam[] = [];
  const seen = new Set<string>();
  let i = 0;
  while (i < sql.length) {
    const ch = sql[i];
    if (ch === "'" || ch == '"' || ch === '`') {
      i = skipQuote(sql, i);
      continue;
    }
    if (ch === '-' && sql[i + 1] === '-') {
      const nl = sql.indexOf('\n', i);
      i = nl === -1 ? sql.length : nl + 1;
      continue;
    }
    if (ch === '/' && sql[i + 1] === '*') {
      const end = sql.indexOf('*/', i + 2);
      i = end === -1 ? sql.length : end + 2;
      continue;
    }
    if (ch === ':') {
      // Postgres casts (`::int`) are not bind placeholders.
      if (sql[i + 1] === ':') {
        i += 2;
        continue;
      }
      IDENT.lastIndex = i + 1;
      const m = IDENT.exec(sql);
      if (m) {
        push(found, seen, { name: m[0], kind: 'named' });
        i = IDENT.lastIndex;
        continue;
      }
    }
    if (ch === '$') {
      DIGITS.lastIndex = i + 1;
      const m = DIGITS.exec(sql);
      if (m) {
        push(found, seen, { name: m[0], kind: 'positional' });
        i = DIGITS.lastIndex;
        continue;
      }
    }
    i += 1;
  }
  return found;
}

function push(found: SqlParam[], seen: Set<string>, param: SqlParam) {
  const key = `${param.kind}:${param.name}`;
  if (seen.has(key)) return;
  seen.add(key);
  found.push(param);
}

function skipQuote(sql: string, i: number): number {
  const quote = sql[i];
  let j = i + 1;
  while (j < sql.length) {
    if (sql[j] === quote) {
      if (quote === "'" && sql[j + 1] === "'") {
        j += 2;
        continue;
      }
      return j + 1;
    }
    j += 1;
  }
  return sql.length;
}

export function paramsToPayload(params: SqlParam[], values: Record<string, string>): Record<string, string | number | boolean | null> {
  const out: Record<string, string | number | boolean | null> = {};
  for (const p of params) {
    const raw = values[p.name] ?? '';
    out[p.name] = coerceParamValue(raw);
  }
  return out;
}

export function coerceParamValue(raw: string): string | number | boolean | null {
  const trimmed = raw.trim();
  if (trimmed === '' || trimmed.toLowerCase() === 'null') return null;
  if (trimmed.toLowerCase() === 'true') return true;
  if (trimmed.toLowerCase() === 'false') return false;
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  return raw;
}
