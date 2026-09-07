import {
  parseQualifiedNameText,
  unquoteBacktick,
  unquoteBracket,
  unquoteDouble,
} from './quoteHelper';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseType } from '../../../types';
import type {
  QualifiedRelationId,
  SqlDialectAdapter,
  SqlDialectQuoteStyle,
  SqlParameterPolicy,
  SqlProjectionAliasVisibility,
} from './types';

export type DialectProfile = {
  quoteStyle: SqlDialectQuoteStyle;
  foldCase: 'lower' | 'upper' | 'preserve';
  projectionAliasVisibility: SqlProjectionAliasVisibility;
  parameterPolicy: SqlParameterPolicy;
  reservedKeywords?: readonly string[];
};

const STANDARD_PROFILE: DialectProfile = {
  quoteStyle: 'double',
  foldCase: 'lower',
  projectionAliasVisibility: 'select-only',
  parameterPolicy: {
    atNamed: false,
    question: false,
    dollarPositional: true,
    template: false,
  },
};

const PROFILES: Record<string, DialectProfile> = {
  standard: STANDARD_PROFILE,
  generic: STANDARD_PROFILE,
  postgresql: STANDARD_PROFILE,
  postgres: STANDARD_PROFILE,
  pg: STANDARD_PROFILE,
  sqlite: {
    quoteStyle: 'double',
    foldCase: 'lower',
    projectionAliasVisibility: 'select-only',
    parameterPolicy: { atNamed: false, question: true, dollarPositional: false, template: false },
  },
  mysql: {
    quoteStyle: 'backtick',
    foldCase: 'lower',
    projectionAliasVisibility: 'order-group',
    parameterPolicy: { atNamed: false, question: true, dollarPositional: false, template: false },
  },
  mariadb: {
    quoteStyle: 'backtick',
    foldCase: 'lower',
    projectionAliasVisibility: 'order-group',
    parameterPolicy: { atNamed: false, question: true, dollarPositional: false, template: false },
  },
  sqlserver: {
    quoteStyle: 'bracket',
    foldCase: 'preserve',
    projectionAliasVisibility: 'broad',
    parameterPolicy: { atNamed: true, question: false, dollarPositional: false, template: false },
  },
  mssql: {
    quoteStyle: 'bracket',
    foldCase: 'preserve',
    projectionAliasVisibility: 'broad',
    parameterPolicy: { atNamed: true, question: false, dollarPositional: false, template: false },
  },
  duckdb: {
    quoteStyle: 'double',
    foldCase: 'lower',
    projectionAliasVisibility: 'select-only',
    parameterPolicy: { atNamed: false, question: true, dollarPositional: true, template: false },
  },
  clickhouse: {
    quoteStyle: 'backtick',
    foldCase: 'lower',
    projectionAliasVisibility: 'broad',
    parameterPolicy: { atNamed: false, question: false, dollarPositional: false, template: false },
  },
  oracle: {
    quoteStyle: 'double',
    foldCase: 'upper',
    projectionAliasVisibility: 'broad',
    parameterPolicy: { atNamed: false, question: false, dollarPositional: false, template: false },
  },
};

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

function foldIdentifier(value: string, foldCase: DialectProfile['foldCase']): string {
  if (foldCase === 'lower') return value.toLowerCase();
  if (foldCase === 'upper') return value.toUpperCase();
  return value;
}

function needsQuoting(
  value: string,
  quoteStyle?: SqlDialectQuoteStyle,
  foldCase?: DialectProfile['foldCase'],
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

function wrapQuoted(value: string, style: SqlDialectQuoteStyle): string {
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

function createAdapter(dialectId: string, profile: DialectProfile): SqlDialectAdapter {
  const { quoteStyle, foldCase, projectionAliasVisibility, parameterPolicy, reservedKeywords } =
    profile;
  const extraReserved = reservedKeywords
    ? new Set(reservedKeywords.map((k) => k.toLowerCase()))
    : undefined;

  return {
    dialectId,
    quoteStyle,
    foldUnquotedIdentifier: (value) => foldIdentifier(value, foldCase),
    shouldQuoteIdentifier: (value) => needsQuoting(value, quoteStyle, foldCase, extraReserved),
    quoteIdentifier: (value) => {
      if (quoteStyle === 'none') return value;
      return wrapQuoted(value, quoteStyle);
    },
    unquoteIdentifier: (value) => {
      const d = unquoteDouble(value);
      if (d !== null) return d;
      const b = unquoteBacktick(value);
      if (b !== null) return b;
      const br = unquoteBracket(value);
      if (br !== null) return br;
      if (/^[A-Za-z_][A-Za-z0-9_$]*$/.test(value)) return value;
      return null;
    },
    parseQualifiedName: (text: string): QualifiedRelationId | null => parseQualifiedNameText(text),
    compareIdentifiers: (a, b) => foldIdentifier(a, foldCase) === foldIdentifier(b, foldCase),
    projectionAliasVisibility,
    parameterPolicy,
  };
}

const adapterCache = new Map<string, SqlDialectAdapter>();

/** Resolve a dialect adapter by dialect id or sqlDialect family string. */
export function getDialectAdapter(dialectId: string): SqlDialectAdapter {
  const key = dialectId.trim().toLowerCase() || 'standard';
  const cached = adapterCache.get(key);
  if (cached) return cached;

  const meta =
    DB_REGISTRY[key as DatabaseType] ??
    Object.values(DB_REGISTRY).find((m) => m.sqlDialect === key);
  const driverProfile = meta?.sqlDialectProfile;

  const profile: DialectProfile = driverProfile
    ? {
        quoteStyle: driverProfile.quoteStyle,
        foldCase: driverProfile.foldCase,
        projectionAliasVisibility: driverProfile.projectionAliasVisibility,
        parameterPolicy: {
          atNamed: driverProfile.parameterPolicy.atNamed ?? false,
          question: driverProfile.parameterPolicy.question ?? false,
          dollarPositional: driverProfile.parameterPolicy.dollarPositional ?? false,
          template: driverProfile.parameterPolicy.template ?? false,
        },
        reservedKeywords: driverProfile.reservedKeywords,
      }
    : (PROFILES[key] ?? STANDARD_PROFILE);

  const adapter = createAdapter(key, profile);
  adapterCache.set(key, adapter);
  return adapter;
}

export function listSupportedDialectIds(): readonly string[] {
  const ids = new Set<string>([...Object.keys(PROFILES), ...Object.keys(DB_REGISTRY)]);
  return Array.from(ids);
}
