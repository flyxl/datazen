import type { SqlDialectStrategy, SqlDialectProfile } from '@datazen/driver-sdk';
import { BaseTableSqlGenerator } from '@datazen/driver-sdk';

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
    getViewDdlQuery(viewName: string) {
      return {
        sql: `SHOW CREATE VIEW \`${viewName}\``,
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
      const quotedCols = opts.columns
        .map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`)
        .join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar}${usingKw} (${quotedCols})`;
    },
  },
  backupOptions: MYSQL_OPTIONS,
  tableSql: new BaseTableSqlGenerator('`'),
};

export const mysqlDialectProfile: SqlDialectProfile = {
  quoteStyle: 'backtick',
  foldCase: 'lower',
  projectionAliasVisibility: 'order-group',
  parameterPolicy: {
    atNamed: false,
    question: true,
    dollarPositional: false,
    template: false,
  },
};
