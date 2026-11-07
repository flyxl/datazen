/**
 * Identifier quoting and folding.
 *
 * Pure functions over a {@link ResolvedSqlDialectProfile}. They live here, below
 * both the SQL editor and the Visual Query Builder, so that anything deriving a
 * relation key folds and quotes identifiers identically — the cache is keyed by
 * the folded identity, so a second implementation would silently split entries.
 */

import type { ResolvedSqlDialectProfile, SqlDialectQuoteStyle } from '@datazen/extension-points';

/**
 * Reserved words that force quoting. Dialects may add their own through
 * {@link SqlDialectProfile.reservedKeywords}.
 */
export const SQL_RESERVED_IDENTIFIER_KEYWORDS = new Set([
  'all',
  'analyse',
  'analyze',
  'and',
  'any',
  'array',
  'as',
  'asc',
  'asymmetric',
  'both',
  'case',
  'cast',
  'check',
  'collate',
  'collation',
  'column',
  'constraint',
  'create',
  'cross',
  'current_catalog',
  'current_date',
  'current_role',
  'current_schema',
  'current_time',
  'current_timestamp',
  'current_user',
  'default',
  'deferrable',
  'delete',
  'desc',
  'describe',
  'distinct',
  'do',
  'drop',
  'else',
  'end',
  'except',
  'false',
  'fetch',
  'for',
  'foreign',
  'from',
  'full',
  'grant',
  'group',
  'having',
  'ilike',
  'in',
  'index',
  'initially',
  'inner',
  'insert',
  'intersect',
  'into',
  'is',
  'isnull',
  'join',
  'key',
  'keys',
  'lateral',
  'leading',
  'left',
  'like',
  'limit',
  'localtime',
  'localtimestamp',
  'lock',
  'natural',
  'not',
  'notnull',
  'null',
  'offset',
  'on',
  'only',
  'or',
  'order',
  'outer',
  'overlaps',
  'placing',
  'primary',
  'references',
  'returning',
  'right',
  'select',
  'session_user',
  'set',
  'similar',
  'some',
  'symmetric',
  'system_user',
  'table',
  'tablesample',
  'then',
  'to',
  'trailing',
  'true',
  'union',
  'unique',
  'update',
  'user',
  'using',
  'values',
  'variadic',
  'verbose',
  'when',
  'where',
  'window',
  'with',
]);

export function foldIdentifier(
  value: string,
  foldCase: ResolvedSqlDialectProfile['foldCase'],
): string {
  if (foldCase === 'lower') return value.toLowerCase();
  if (foldCase === 'upper') return value.toUpperCase();
  return value;
}

export function needsQuoting(
  value: string,
  quoteStyle: SqlDialectQuoteStyle,
  foldCase: ResolvedSqlDialectProfile['foldCase'],
  extraReservedKeywords?: ReadonlySet<string>,
): boolean {
  if (!value) return true;
  if (quoteStyle === 'none') return false;
  if (!/^[A-Za-z_][A-Za-z0-9_$]*$/.test(value)) return true;
  const lower = value.toLowerCase();
  if (SQL_RESERVED_IDENTIFIER_KEYWORDS.has(lower)) return true;
  if (extraReservedKeywords?.has(lower)) return true;
  if (foldCase === 'lower' && /[A-Z]/.test(value)) return true;
  if (foldCase === 'upper' && /[a-z]/.test(value)) return true;
  return false;
}

function escapeForQuote(value: string, style: SqlDialectQuoteStyle): string {
  switch (style) {
    case 'double':
      return value.replace(/"/g, '""');
    case 'backtick':
      return value;
    case 'bracket':
      return value.replace(/]/g, ']]');
    default:
      return value;
  }
}

export function wrapQuoted(value: string, style: SqlDialectQuoteStyle): string {
  const escaped = escapeForQuote(value, style);
  switch (style) {
    case 'double':
      return `"${escaped}"`;
    case 'backtick':
      return `\`${escaped}\``;
    case 'bracket':
      return `[${escaped}]`;
    default:
      return value;
  }
}

/** Quote an identifier for a dialect, honouring `quoteStyle: 'none'`. */
export function quoteIdentifierForDialect(
  value: string,
  profile: ResolvedSqlDialectProfile,
): string {
  if (profile.quoteStyle === 'none') return value;
  return wrapQuoted(value, profile.quoteStyle);
}
