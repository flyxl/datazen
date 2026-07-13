import type { SqlDialectStrategy } from './types';

export const sqliteDialect: SqlDialectStrategy = {
  family: 'sqlite',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `SELECT sql FROM sqlite_master WHERE type='table' AND name='${tableName}'`,
        extractColumnIndex: 0,
      };
    },
  },
  index: {
    supportedIndexMethods: ['btree'],
    getDropIndexSql(indexName, _tableName, quoteChar) {
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const quotedCols = opts.columns.map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`).join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar} (${quotedCols})`;
    },
  },
  backupOptions: [],
};
