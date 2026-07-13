
export type SqlDialectFamily = 'postgresql' | 'mysql' | 'sqlite' | 'trino';

export interface DdlDialect {
  /** SQL to fetch DDL; returns how to extract DDL string from first result row */
  getTableDdlQuery(tableName: string, schema?: string): { sql: string; extractColumnIndex: number };
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
}
