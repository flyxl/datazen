import type { SqlDialectStrategy, SqlDialectProfile } from '@datazen/driver-sdk';
import { BaseTableSqlGenerator } from '@datazen/driver-sdk';

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
  backupOptions: [
    { id: 'schema-only', label: '--schema-only' },
    { id: 'data-only', label: '--data-only' },
    { id: 'clean', label: '--clean' },
    { id: 'no-owner', label: '--no-owner' },
    { id: 'single-transaction', label: '--single-transaction' },
  ],
  tableSql: new BaseTableSqlGenerator('"'),
};

export const duckdbDialectProfile: SqlDialectProfile = {
  quoteStyle: 'double',
  foldCase: 'lower',
  projectionAliasVisibility: 'select-only',
  parameterPolicy: {
    atNamed: false,
    question: true,
    dollarPositional: true,
    template: false,
  },
};
