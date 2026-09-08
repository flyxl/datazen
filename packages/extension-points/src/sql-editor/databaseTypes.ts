/**
 * Database types and driver metadata registry for SQL editor.
 */
import type { FunctionEntry } from './functionTypes';
import type { SqlDialectQuoteStyle, SqlProjectionAliasVisibility } from './semanticTypes';

export type DatabaseType = string;

export interface ColumnSchema {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string | null;
  isPrimaryKey?: boolean;
  isAutoIncrement?: boolean;
  comment?: string | null;
  extra?: string | null;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  unique?: boolean;
  primary?: boolean;
  indexType?: string;
}

export interface ForeignKeyInfo {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate?: string;
  onDelete?: string;
}

export interface TableSchema {
  tableName: string;
  columns: ColumnSchema[];
  primaryKeys: string[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
}

export interface SqlDialectProfile {
  quoteStyle: SqlDialectQuoteStyle;
  foldCase: 'lower' | 'upper' | 'preserve';
  projectionAliasVisibility: SqlProjectionAliasVisibility;
  parameterPolicy: {
    atNamed?: boolean;
    question?: boolean;
    dollarPositional?: boolean;
    template?: boolean;
  };
  reservedKeywords?: readonly string[];
}

export interface DatabaseTypeMetaLite {
  id?: string;
  name?: string;
  sqlDialect?: string;
  sqlFunctions?: readonly FunctionEntry[];
  sqlDialectProfile?: SqlDialectProfile;
  [key: string]: unknown;
}

export const DB_REGISTRY: Record<string, DatabaseTypeMetaLite> = {};

export function registerDriverMeta(id: string, meta: DatabaseTypeMetaLite): void {
  DB_REGISTRY[id] = meta;
}

export function registerDriverMetas(metas: Record<string, DatabaseTypeMetaLite>): void {
  Object.assign(DB_REGISTRY, metas);
}
