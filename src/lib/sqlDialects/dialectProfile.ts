/**
 * Dialect profile resolution.
 *
 * A dialect profile decides how identifiers are folded and quoted, which
 * placeholder syntax is accepted, and how visible projection aliases are. It is
 * dialect metadata, not editor state, so it lives here rather than inside the
 * SQL editor — the editor and the Visual Query Builder both resolve relation
 * keys through it, and a second copy would let their keys drift apart.
 *
 * Resolution order: the driver's own `sqlDialectProfile` declaration wins, then
 * the dialect family's built-in default, then the standard profile. A driver
 * that declares only some fields keeps its family's defaults for the rest.
 */

import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import type { ResolvedSqlDialectProfile, SqlDialectProfile } from './types';

export const STANDARD_DIALECT_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'double',
  foldCase: 'lower',
  projectionAliasVisibility: 'select-only',
  parameterPolicy: {
    atNamed: false,
    question: false,
    dollarPositional: true,
    dollarNamed: false,
    template: false,
  },
};

const SQLITE_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'double',
  foldCase: 'lower',
  projectionAliasVisibility: 'select-only',
  parameterPolicy: {
    atNamed: false,
    question: true,
    dollarPositional: false,
    dollarNamed: false,
    template: false,
  },
};

const MYSQL_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'backtick',
  foldCase: 'lower',
  projectionAliasVisibility: 'order-group',
  parameterPolicy: {
    atNamed: false,
    question: true,
    dollarPositional: false,
    dollarNamed: false,
    template: false,
  },
};

const SQLSERVER_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'bracket',
  foldCase: 'preserve',
  projectionAliasVisibility: 'broad',
  parameterPolicy: {
    atNamed: true,
    question: false,
    dollarPositional: false,
    dollarNamed: false,
    template: false,
  },
};

const DUCKDB_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'double',
  foldCase: 'lower',
  projectionAliasVisibility: 'select-only',
  parameterPolicy: {
    atNamed: false,
    question: true,
    dollarPositional: true,
    dollarNamed: false,
    template: false,
  },
};

const CLICKHOUSE_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'backtick',
  foldCase: 'lower',
  projectionAliasVisibility: 'broad',
  parameterPolicy: {
    atNamed: false,
    question: false,
    dollarPositional: false,
    dollarNamed: false,
    template: false,
  },
};

const ORACLE_PROFILE: ResolvedSqlDialectProfile = {
  quoteStyle: 'double',
  foldCase: 'upper',
  projectionAliasVisibility: 'broad',
  parameterPolicy: {
    atNamed: false,
    question: false,
    dollarPositional: false,
    dollarNamed: false,
    template: false,
  },
};

/**
 * Built-in defaults keyed by dialect id *or* family alias.
 *
 * Looked up by the resolved dialect id (so `mariadb` and `mssql` can differ from
 * their families) — deliberately *not* by the driver's `sqlDialect` family, so a
 * driver that declares nothing keeps exactly the default it had before this
 * table moved here.
 */
const DIALECT_PROFILES: Record<string, ResolvedSqlDialectProfile> = {
  standard: STANDARD_DIALECT_PROFILE,
  generic: STANDARD_DIALECT_PROFILE,
  postgresql: STANDARD_DIALECT_PROFILE,
  postgres: STANDARD_DIALECT_PROFILE,
  pg: STANDARD_DIALECT_PROFILE,
  sqlite: SQLITE_PROFILE,
  mysql: MYSQL_PROFILE,
  mariadb: MYSQL_PROFILE,
  sqlserver: SQLSERVER_PROFILE,
  mssql: SQLSERVER_PROFILE,
  duckdb: DUCKDB_PROFILE,
  clickhouse: CLICKHOUSE_PROFILE,
  oracle: ORACLE_PROFILE,
};

/**
 * Normalize a driver's declaration.
 *
 * The declaration replaces the family default wholesale; only the placeholder
 * flags are defaulted, because a driver that omits one means "not accepted".
 */
function normalizeDeclaredProfile(declared: SqlDialectProfile): ResolvedSqlDialectProfile {
  return {
    quoteStyle: declared.quoteStyle,
    foldCase: declared.foldCase,
    projectionAliasVisibility: declared.projectionAliasVisibility,
    parameterPolicy: {
      atNamed: declared.parameterPolicy.atNamed ?? false,
      question: declared.parameterPolicy.question ?? false,
      dollarPositional: declared.parameterPolicy.dollarPositional ?? false,
      dollarNamed: declared.parameterPolicy.dollarNamed ?? false,
      template: declared.parameterPolicy.template ?? false,
    },
    reservedKeywords: declared.reservedKeywords,
  };
}

const profileCache = new Map<string, ResolvedSqlDialectProfile>();

/**
 * Resolve the dialect profile for a dialect id (or a `sqlDialect` family name).
 *
 * A driver's own `sqlDialectProfile` declaration is authoritative and replaces
 * its family default wholesale — matching how the adapter has always resolved
 * it, so keys computed here stay identical to keys computed before.
 */
export function resolveSqlDialectProfile(dialectId: string): ResolvedSqlDialectProfile {
  const key = dialectId.trim().toLowerCase() || 'standard';
  const cached = profileCache.get(key);
  if (cached) return cached;

  const meta =
    DB_REGISTRY[key as DatabaseType] ??
    Object.values(DB_REGISTRY).find((m) => m.sqlDialect === key);
  const declared = meta?.sqlDialectProfile;

  const profile = declared
    ? normalizeDeclaredProfile(declared)
    : (DIALECT_PROFILES[key] ?? STANDARD_DIALECT_PROFILE);
  profileCache.set(key, profile);
  return profile;
}

/** Every dialect id or family alias that has a built-in default profile. */
export function listBuiltinDialectProfileIds(): readonly string[] {
  return Object.keys(DIALECT_PROFILES);
}

/** Fold an unquoted identifier the way the dialect does. */
export function foldIdentifierForDialect(value: string, dialectId: string): string {
  const { foldCase } = resolveSqlDialectProfile(dialectId);
  if (foldCase === 'lower') return value.toLowerCase();
  if (foldCase === 'upper') return value.toUpperCase();
  return value;
}
