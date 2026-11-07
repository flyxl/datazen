import type { TableSchema } from './databaseTypes';

export type SqlDialectFamily = string;

export type GeneratedSqlType = 'select' | 'insert' | 'update' | 'delete';

export interface TableSqlDialect {
  formatTableRef(tableName: string, schemaPrefix?: string | null): string;
  generateSelect(tableRef: string, schema: TableSchema): string;
  generateInsert(tableRef: string, schema: TableSchema): string;
  generateUpdate(tableRef: string, schema: TableSchema): string;
  generateDelete(tableRef: string, schema: TableSchema): string;
  generateSql(type: GeneratedSqlType, tableRef: string, schema: TableSchema): string;
}

export interface DdlDialect {
  /** SQL to fetch DDL for a table; returns how to extract DDL string from first result row */
  getTableDdlQuery(
    tableName: string,
    schema?: string | null,
  ): { sql: string; extractColumnIndex: number };
  /** SQL to fetch DDL for a view; falls back to getTableDdlQuery if not provided */
  getViewDdlQuery?(
    viewName: string,
    schema?: string | null,
  ): { sql: string; extractColumnIndex: number };
}

export interface IndexDialect {
  supportedIndexMethods: Array<'btree' | 'hash' | 'gin' | 'gist'>;
  getDropIndexSql(indexName: string, tableName: string, quoteChar: string): string;
  getCreateIndexSql(opts: {
    indexName: string;
    tableName: string;
    columns: string[];
    unique?: boolean;
    method?: 'btree' | 'hash' | 'gin' | 'gist';
    quoteChar: string;
  }): string;
}

export interface BackupOption {
  id: string;
  label: string;
}

export type SqlDialectQuoteStyle = 'double' | 'backtick' | 'bracket' | 'none';

export type SqlDialectFoldCase = 'lower' | 'upper' | 'preserve';

export type SqlProjectionAliasVisibility = 'select-only' | 'order-group' | 'broad';

/** Placeholder policy a dialect accepts. Resolved form requires every flag. */
export type SqlParameterPolicy = {
  atNamed?: boolean;
  question?: boolean;
  dollarPositional?: boolean;
  dollarNamed?: boolean;
  template?: boolean;
};

/** A candidate bind token found by the shared SQL scanner. */
export interface SqlParameterOccurrence {
  from: number;
  to: number;
  id: string;
  token: string;
  syntax: 'colon' | 'at' | 'dollar-positional' | 'dollar-named' | 'question' | 'template';
  name: string;
  ordinal?: number;
}

/** Optional driver-owned filtering for syntax that is ambiguous in its dialect. */
export interface SqlParameterStrategy {
  filterOccurrences?: (
    sql: string,
    occurrences: readonly SqlParameterOccurrence[],
  ) => readonly SqlParameterOccurrence[];
}

/** Placeholder policy with every flag decided. */
export type ResolvedSqlParameterPolicy = Required<SqlParameterPolicy>;

/**
 * A dialect profile a **driver** declares through
 * {@link DatabaseTypeMeta.sqlDialectProfile}. Every field is optional beyond the
 * shape, because a driver may override only what differs from its family.
 */
export interface SqlDialectProfile {
  quoteStyle: SqlDialectQuoteStyle;
  foldCase: SqlDialectFoldCase;
  projectionAliasVisibility: SqlProjectionAliasVisibility;
  parameterPolicy: SqlParameterPolicy;
  reservedKeywords?: readonly string[];
}

/**
 * A dialect profile with every field decided: the driver's declaration layered
 * over its dialect family's defaults, or the standard profile.
 */
export interface ResolvedSqlDialectProfile {
  quoteStyle: SqlDialectQuoteStyle;
  foldCase: SqlDialectFoldCase;
  projectionAliasVisibility: SqlProjectionAliasVisibility;
  parameterPolicy: ResolvedSqlParameterPolicy;
  reservedKeywords?: readonly string[];
}

export interface SqlDialectStrategy {
  family: SqlDialectFamily;
  ddl: DdlDialect;
  index: IndexDialect;
  backupOptions: BackupOption[];
  /** SQL to empty a table; defaults to `TRUNCATE TABLE <quoted>`. */
  getTruncateTableSql?: (quotedName: string) => string;
  /** Table template SQL generator for this dialect (SELECT, INSERT, UPDATE, DELETE). */
  tableSql?: TableSqlDialect;
  /**
   * How this dialect caps the rows a statement returns.
   *
   * Absent means the standard `LIMIT n` at the end of the statement — the
   * shape most dialects use. A dialect that spells the cap differently
   * declares it here, so the host never has to know a driver name.
   */
  rowLimit?: RowLimitShape;
}

/**
 * How a dialect caps the number of rows a statement returns.
 *
 * The cap is not one shape. Most dialects end the statement with `LIMIT n`;
 * SQL Server puts `TOP n` into the select list; Oracle 12c+ ends the
 * statement with `FETCH FIRST n ROWS ONLY`. `LIMIT` on SQL Server or Oracle
 * is a syntax error, so the cap belongs to the dialect, never to the caller.
 */
export interface RowLimitShape {
  /**
   * Where the cap goes.
   *
   * - `trailing`: the cap ends the statement (`… FETCH FIRST 10 ROWS ONLY`);
   * - `select-list`: the cap sits right behind `SELECT [DISTINCT]`
   *   (`SELECT DISTINCT TOP 10 …`).
   */
  readonly placement: 'trailing' | 'select-list';
  /** The cap for the given number of rows. */
  readonly clause: (limit: number) => string;
}
