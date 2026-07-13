import type { SqlDialectStrategy } from './types';

export const trinoDialect: SqlDialectStrategy = {
  family: 'trino',
  ddl: {
    getTableDdlQuery(tableName: string, schema?: string) {
      const qualified = schema ? `"${schema}"."${tableName}"` : `"${tableName}"`;
      return {
        sql: `SHOW CREATE TABLE ${qualified}`,
        extractColumnIndex: 0,
      };
    },
  },
  index: {
    supportedIndexMethods: ['btree'],
    getDropIndexSql(indexName, tableName, quoteChar) {
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar} ON ${quoteChar}${tableName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const quotedCols = opts.columns.map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`).join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar} (${quotedCols})`;
    },
  },
  backupOptions: [],
};
