import type { TableSchema } from '../../types';

export type SqlDialectFamily = string;

export type GeneratedSqlType = 'select' | 'insert' | 'update' | 'delete';

export interface TableSqlDialect {
  formatTableRef(tableName: string, schemaPrefix?: string): string;
  generateSelect(tableRef: string, schema: TableSchema): string;
  generateInsert(tableRef: string, schema: TableSchema): string;
  generateUpdate(tableRef: string, schema: TableSchema): string;
  generateDelete(tableRef: string, schema: TableSchema): string;
  generateSql(type: GeneratedSqlType, tableRef: string, schema: TableSchema): string;
}

export interface DdlDialect {
  /** SQL to fetch DDL for a table; returns how to extract DDL string from first result row */
  getTableDdlQuery(tableName: string, schema?: string): { sql: string; extractColumnIndex: number };
  /** SQL to fetch DDL for a view; falls back to getTableDdlQuery if not provided */
  getViewDdlQuery?(viewName: string, schema?: string): { sql: string; extractColumnIndex: number };
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

export interface SqlDialectStrategy {
  family: SqlDialectFamily;
  ddl: DdlDialect;
  index: IndexDialect;
  backupOptions: BackupOption[];
  /** SQL to empty a table; defaults to `TRUNCATE TABLE <quoted>`. */
  getTruncateTableSql?: (quotedName: string) => string;
  /** Table template SQL generator for this dialect (SELECT, INSERT, UPDATE, DELETE). */
  tableSql?: TableSqlDialect;
}
