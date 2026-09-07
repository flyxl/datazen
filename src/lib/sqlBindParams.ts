export type SqlParamKind = 'named' | 'positional';

export type SqlParamSyntax = 'colon' | 'at' | 'dollar-positional' | 'question' | 'template';

export type SqlParamStableId = `named:${string}` | `dollar:${number}` | `question:${number}`;

export interface SqlParamDialectPolicy {
  /** Recognize `@name` placeholders (default false). */
  enableAt?: boolean;
  /** Recognize `?` placeholders (default false). */
  enableQuestion?: boolean;
  /** Recognize `${name}` template placeholders (default true). */
  enableTemplate?: boolean;
  /** Exclude `DECLARE @x` / `SET @x =` variables from bind descriptors (default true when enableAt). */
  excludeDeclaredAtVars?: boolean;
}

export const DEFAULT_DIALECT_POLICY: Required<SqlParamDialectPolicy> = {
  enableAt: true,
  enableQuestion: true,
  enableTemplate: true,
  excludeDeclaredAtVars: true,
};

export interface SqlParam {
  name: string;
  kind: SqlParamKind;
  syntax: SqlParamSyntax;
  stableId: SqlParamStableId;
  /** 1-based ordinal for `?` placeholders. */
  ordinal?: number;
}

export interface SqlParamOccurrence {
  from: number;
  to: number;
  id: SqlParamStableId;
  token: string;
  syntax: SqlParamSyntax;
  name: string;
  ordinal?: number;
}

export interface SqlParamOccurrenceV2 {
  from: number;
  to: number;
  id: SqlParamStableId;
  token: string;
}

export interface SqlBindPayloadV2 {
  version: 2;
  values: Record<string, string | number | boolean | null>;
  occurrences: SqlParamOccurrenceV2[];
}

const IDENT = /[A-Za-z_][A-Za-z0-9_]*/y;
const DIGITS = /[0-9]+/y;

function resolvePolicy(policy?: SqlParamDialectPolicy): Required<SqlParamDialectPolicy> {
  return { ...DEFAULT_DIALECT_POLICY, ...policy };
}

function namedId(name: string): SqlParamStableId {
  return `named:${name}`;
}

function dollarId(n: number): SqlParamStableId {
  return `dollar:${n}`;
}

function questionId(n: number): SqlParamStableId {
  return `question:${n}`;
}

/** Extract bind placeholders with occurrence spans, skipping quoted strings and comments. */
export function parseSqlParamOccurrences(
  sql: string,
  policy?: SqlParamDialectPolicy,
): SqlParamOccurrence[] {
  const resolved = resolvePolicy(policy);
  const declaredAt =
    resolved.enableAt && resolved.excludeDeclaredAtVars
      ? findDeclaredAtVars(sql)
      : new Set<string>();
  const occurrences: SqlParamOccurrence[] = [];
  let questionOrdinal = 0;
  let i = 0;

  while (i < sql.length) {
    const skipped = skipNonCode(sql, i);
    if (skipped !== i) {
      i = skipped;
      continue;
    }

    const ch = sql[i];

    if (ch === ':') {
      if (sql[i + 1] === ':') {
        i += 2;
        continue;
      }
      IDENT.lastIndex = i + 1;
      const m = IDENT.exec(sql);
      if (m) {
        const name = m[0];
        const from = i;
        const to = IDENT.lastIndex;
        pushOccurrence(occurrences, {
          from,
          to,
          id: namedId(name),
          token: sql.slice(from, to),
          syntax: 'colon',
          name,
        });
        i = to;
        continue;
      }
    }

    if (ch === '@' && resolved.enableAt) {
      if (sql[i + 1] === '@') {
        i += 2;
        continue;
      }
      IDENT.lastIndex = i + 1;
      const m = IDENT.exec(sql);
      if (m) {
        const name = m[0];
        if (!declaredAt.has(name.toLowerCase())) {
          const from = i;
          const to = IDENT.lastIndex;
          pushOccurrence(occurrences, {
            from,
            to,
            id: namedId(name),
            token: sql.slice(from, to),
            syntax: 'at',
            name,
          });
        }
        i = IDENT.lastIndex;
        continue;
      }
    }

    if (ch === '$') {
      if (resolved.enableTemplate && sql[i + 1] === '{') {
        IDENT.lastIndex = i + 2;
        const m = IDENT.exec(sql);
        if (m && sql[IDENT.lastIndex] === '}') {
          const name = m[0];
          const from = i;
          const to = IDENT.lastIndex + 1;
          pushOccurrence(occurrences, {
            from,
            to,
            id: namedId(name),
            token: sql.slice(from, to),
            syntax: 'template',
            name,
          });
          i = to;
          continue;
        }
      }
      DIGITS.lastIndex = i + 1;
      const m = DIGITS.exec(sql);
      if (m) {
        const num = Number(m[0]);
        const from = i;
        const to = DIGITS.lastIndex;
        pushOccurrence(occurrences, {
          from,
          to,
          id: dollarId(num),
          token: sql.slice(from, to),
          syntax: 'dollar-positional',
          name: m[0],
        });
        i = to;
        continue;
      }
      const dq = skipDollarQuote(sql, i);
      if (dq !== i) {
        i = dq;
        continue;
      }
    }

    if (ch === '?' && resolved.enableQuestion && isQuestionBindPlaceholder(sql, i)) {
      questionOrdinal += 1;
      pushOccurrence(occurrences, {
        from: i,
        to: i + 1,
        id: questionId(questionOrdinal),
        token: '?',
        syntax: 'question',
        name: String(questionOrdinal),
        ordinal: questionOrdinal,
      });
      i += 1;
      continue;
    }

    i += 1;
  }

  return occurrences;
}

/** Deduped bind descriptors derived from occurrences (stable order of first appearance). */
export function parseSqlParams(sql: string, policy?: SqlParamDialectPolicy): SqlParam[] {
  const seen = new Set<string>();
  const params: SqlParam[] = [];
  for (const occ of parseSqlParamOccurrences(sql, policy)) {
    if (seen.has(occ.id)) continue;
    seen.add(occ.id);
    params.push({
      name: occ.name,
      kind:
        occ.syntax === 'dollar-positional' || occ.syntax === 'question' ? 'positional' : 'named',
      syntax: occ.syntax,
      stableId: occ.id,
      ordinal: occ.ordinal,
    });
  }
  return params;
}

export function buildBindPayloadV2(
  sql: string,
  rawValues: Record<string, string>,
  policy?: SqlParamDialectPolicy,
): SqlBindPayloadV2 {
  const occurrences = parseSqlParamOccurrences(sql, policy);
  const values: Record<string, string | number | boolean | null> = {};
  const payloadOccurrences: SqlParamOccurrenceV2[] = [];

  for (const occ of occurrences) {
    const raw = rawValues[occ.id] ?? rawValues[occ.name] ?? '';
    values[occ.id] = coerceParamValue(raw);
    payloadOccurrences.push({
      from: occ.from,
      to: occ.to,
      id: occ.id,
      token: occ.token,
    });
  }

  return { version: 2, values, occurrences: payloadOccurrences };
}

/** Legacy payload builder keyed by bare param name. */
export function paramsToPayload(
  params: SqlParam[],
  values: Record<string, unknown>,
): Record<string, string | number | boolean | null> {
  const out: Record<string, string | number | boolean | null> = {};
  for (const p of params) {
    const raw = values[p.stableId] ?? values[p.name] ?? '';
    out[p.name] = coerceParamValue(raw);
  }
  return out;
}

/**
 * Safely substitute parameter values into the SQL text prior to backend execution.
 * Correctly escapes strings, formats numbers and booleans, handles NULLs,
 * and eliminates database dialect syntax errors for @, ?, ${}, etc.
 */
export function substituteSqlParams(
  sql: string,
  rawValues: Record<string, unknown>,
  policy?: SqlParamDialectPolicy,
): string {
  const occurrences = parseSqlParamOccurrences(sql, policy);
  if (occurrences.length === 0) return sql;

  // Sort occurrences by from descending so earlier string offsets remain valid
  const sorted = [...occurrences].sort((a, b) => b.from - a.from);
  let result = sql;

  for (const occ of sorted) {
    const rawVal =
      rawValues[occ.id] ??
      rawValues[occ.name] ??
      (occ.ordinal !== undefined ? rawValues[String(occ.ordinal)] : undefined) ??
      (occ.name.startsWith('$') ? rawValues[occ.name.slice(1)] : undefined) ??
      (occ.name.startsWith('@') ? rawValues[occ.name.slice(1)] : undefined) ??
      (occ.name.startsWith(':') ? rawValues[occ.name.slice(1)] : undefined);

    let formatted: string;
    if (rawVal === null || rawVal === undefined) {
      formatted = 'NULL';
    } else if (typeof rawVal === 'boolean') {
      formatted = rawVal ? 'TRUE' : 'FALSE';
    } else if (typeof rawVal === 'number') {
      formatted = String(rawVal);
    } else {
      const str = String(rawVal);
      const trimmed = str.trim();
      if (trimmed === '' || trimmed.toLowerCase() === 'null') {
        formatted = 'NULL';
      } else if (trimmed.toLowerCase() === 'true' || trimmed.toLowerCase() === 'false') {
        formatted = trimmed.toUpperCase();
      } else if (/^-?\d+(\.\d+)?$/.test(trimmed)) {
        formatted = trimmed;
      } else {
        formatted = `'${str.replace(/'/g, "''")}'`;
      }
    }

    result = result.slice(0, occ.from) + formatted + result.slice(occ.to);
  }

  return result;
}

/** Return the original-syntax label for a param (e.g. `:name`, `@name`, `$1`, `?`, `${name}`). */
export function getParamLabel(param: SqlParam): string {
  switch (param.syntax) {
    case 'colon':
      return `:${param.name}`;
    case 'at':
      return `@${param.name}`;
    case 'dollar-positional':
      return `$${param.name}`;
    case 'question':
      return '?';
    case 'template':
      return `\${${param.name}}`;
  }
}

/**
 * Deterministic fingerprint of param stable IDs for S5-C consumption.
 * Order matters: reflects first-appearance order from parseSqlParams.
 */
export function paramFingerprint(params: SqlParam[]): string {
  if (params.length === 0) return '';
  return params.map((p) => p.stableId).join(',');
}

/** Sensitive param names that must NOT be recorded in history. */
export const SENSITIVE_PARAM_NAMES: ReadonlySet<string> = new Set([
  'password',
  'passwd',
  'token',
  'secret',
  'key',
  'credential',
  'api_key',
  'apikey',
  'access_token',
  'auth',
]);

export function coerceParamValue(raw: unknown): string | number | boolean | null {
  if (raw === null || raw === undefined) return null;
  if (typeof raw === 'number' || typeof raw === 'boolean') return raw;
  const str = String(raw);
  const trimmed = str.trim();
  if (trimmed === '' || trimmed.toLowerCase() === 'null') return null;
  if (trimmed.toLowerCase() === 'true') return true;
  if (trimmed.toLowerCase() === 'false') return false;
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  return str;
}

function pushOccurrence(list: SqlParamOccurrence[], occ: SqlParamOccurrence) {
  list.push(occ);
}

function isQuestionBindPlaceholder(sql: string, i: number): boolean {
  const next = sql[i + 1];
  if (next === '|' || next === '&' || next === '-' || next === '#') {
    return false;
  }
  let j = i + 1;
  while (j < sql.length && sql[j] === ' ') {
    j += 1;
  }
  // PostgreSQL json key existence operator: `? 'key'`
  if (sql[j] === "'") {
    return false;
  }
  return true;
}

function skipNonCode(sql: string, i: number): number {
  const ch = sql[i];
  if (ch === "'" || ch === '"' || ch === '`') {
    return skipQuote(sql, i);
  }
  if (ch === '-' && sql[i + 1] === '-') {
    const nl = sql.indexOf('\n', i);
    return nl === -1 ? sql.length : nl + 1;
  }
  if (ch === '/' && sql[i + 1] === '*') {
    const end = sql.indexOf('*/', i + 2);
    return end === -1 ? sql.length : end + 2;
  }
  if (ch === '$') {
    return skipDollarQuote(sql, i);
  }
  return i;
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

function skipDollarQuote(sql: string, i: number): number {
  if (sql[i] !== '$') return i;
  let j = i + 1;
  while (j < sql.length && (isIdentChar(sql[j]) || sql[j] === '_')) {
    j += 1;
  }
  if (j >= sql.length || sql[j] !== '$') return i;
  const tag = sql.slice(i, j + 1);
  const close = sql.indexOf(tag, j + 1);
  return close === -1 ? sql.length : close + tag.length;
}

function isIdentChar(ch: string): boolean {
  const code = ch.charCodeAt(0);
  return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || (code >= 48 && code <= 57);
}

/** Collect `@name` variables introduced by top-level DECLARE / SET in each statement. */
export function findDeclaredAtVars(sql: string): Set<string> {
  const declared = new Set<string>();
  for (const stmt of splitStatements(sql)) {
    collectDeclaredAtVarsInStatement(stmt, declared);
  }
  return declared;
}

function collectDeclaredAtVarsInStatement(stmt: string, declared: Set<string>) {
  const trimmed = stmt.trimStart();
  if (!trimmed) return;

  const upper = trimmed.toUpperCase();
  if (upper.startsWith('DECLARE')) {
    const body = trimmed.slice('DECLARE'.length);
    for (const m of body.matchAll(/@([A-Za-z_][A-Za-z0-9_]*)/g)) {
      declared.add(m[1].toLowerCase());
    }
    return;
  }

  if (upper.startsWith('SET')) {
    const m = /^SET\s+@([A-Za-z_][A-Za-z0-9_]*)\s*=/i.exec(trimmed);
    if (m) {
      declared.add(m[1].toLowerCase());
    }
  }
}

function splitStatements(sql: string): string[] {
  const stmts: string[] = [];
  let start = 0;
  let i = 0;
  while (i < sql.length) {
    const skipped = skipNonCode(sql, i);
    if (skipped !== i) {
      i = skipped;
      continue;
    }
    if (sql[i] === ';') {
      stmts.push(sql.slice(start, i));
      start = i + 1;
    }
    i += 1;
  }
  stmts.push(sql.slice(start));
  return stmts;
}
