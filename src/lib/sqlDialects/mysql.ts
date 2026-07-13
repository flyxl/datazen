import type { SqlDialectStrategy } from './types';

const MYSQL_OPTIONS = [
  { id: 'no-data', label: '--no-data' },
  { id: 'add-drop-table', label: '--add-drop-table' },
  { id: 'single-transaction', label: '--single-transaction' },
  { id: 'routines', label: '--routines' },
  { id: 'triggers', label: '--triggers' },
  { id: 'no-create-info', label: '--no-create-info' },
];

export const mysqlDialect: SqlDialectStrategy = {
  family: 'mysql',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `SHOW CREATE TABLE \`${tableName}\``,
        extractColumnIndex: 1,
      };
    },
  },
  index: {
    supportedIndexMethods: ['btree', 'hash'],
    getDropIndexSql(indexName, tableName, quoteChar) {
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar} ON ${quoteChar}${tableName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const usingKw = opts.method === 'hash' ? ' USING hash' : '';
      const quotedCols = opts.columns.map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`).join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar}${usingKw} (${quotedCols})`;
    },
  },
  backupOptions: MYSQL_OPTIONS,
};
