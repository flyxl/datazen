import type { SqlDialectStrategy } from './types';

function standardIndex(dropPattern: 'table' | 'bare'): SqlDialectStrategy['index'] {
  return {
    supportedIndexMethods: ['btree', 'hash'],
    getDropIndexSql(indexName, tableName, quoteChar) {
      if (dropPattern === 'bare') {
        return `DROP INDEX ${quoteChar}${indexName}${quoteChar}`;
      }
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar} ON ${quoteChar}${tableName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const quotedCols = opts.columns
        .map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`)
        .join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar} (${quotedCols})`;
    },
  };
}

/** No-op DDL: returns no rows, keeps the DDL tab harmless for non-SQL engines. */
const noDdl = {
  getTableDdlQuery() {
    return { sql: 'SELECT NULL AS sql WHERE 0', extractColumnIndex: 0 };
  },
};

export const sqlserverDialect: SqlDialectStrategy = {
  family: 'sqlserver',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `SELECT OBJECT_DEFINITION(OBJECT_ID('${tableName.replace(/'/g, "''")}'))`,
        extractColumnIndex: 0,
      };
    },
  },
  index: standardIndex('table'),
  backupOptions: [
    { id: 'schema-only', label: '--schema-only' },
    { id: 'data-only', label: '--data-only' },
    { id: 'clean', label: '--clean' },
    { id: 'no-owner', label: '--no-owner' },
    { id: 'single-transaction', label: '--single-transaction' },
  ],
};

export const clickhouseDialect: SqlDialectStrategy = {
  family: 'clickhouse',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `SELECT create_table_query FROM system.tables WHERE database = currentDatabase() AND name = '${tableName.replace(/'/g, "''")}'`,
        extractColumnIndex: 0,
      };
    },
  },
  index: standardIndex('table'),
  backupOptions: [],
};

export const duckdbDialect: SqlDialectStrategy = {
  family: 'duckdb',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `SELECT sql FROM duckdb_tables() WHERE table_name = '${tableName.replace(/'/g, "''")}'`,
        extractColumnIndex: 0,
      };
    },
    getViewDdlQuery(viewName: string) {
      return {
        sql: `SELECT sql FROM duckdb_views() WHERE view_name = '${viewName.replace(/'/g, "''")}'`,
        extractColumnIndex: 0,
      };
    },
  },
  index: standardIndex('bare'),
  backupOptions: [],
};

export const elasticsearchDialect: SqlDialectStrategy = {
  family: 'elasticsearch',
  ddl: noDdl,
  index: standardIndex('bare'),
  backupOptions: [],
};

export const mongodbDialect: SqlDialectStrategy = {
  family: 'mongodb',
  ddl: noDdl,
  index: standardIndex('bare'),
  backupOptions: [],
};

export const genericDialect: SqlDialectStrategy = {
  family: 'generic',
  ddl: noDdl,
  index: standardIndex('bare'),
  backupOptions: [],
};
