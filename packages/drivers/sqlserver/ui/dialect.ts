import type { SqlDialectStrategy, SqlDialectProfile } from '@datazen/driver-sdk';
import { BaseTableSqlGenerator } from '@datazen/driver-sdk';

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
  index: {
    supportedIndexMethods: ['btree', 'hash'],
    getDropIndexSql(indexName, tableName, quoteChar) {
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar} ON ${quoteChar}${tableName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const quotedCols = opts.columns
        .map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`)
        .join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar} (${quotedCols})`;
    },
  },
  backupOptions: [
    { id: 'schema-only', label: '--schema-only' },
    { id: 'data-only', label: '--data-only' },
    { id: 'clean', label: '--clean' },
    { id: 'no-owner', label: '--no-owner' },
    { id: 'single-transaction', label: '--single-transaction' },
  ],
  tableSql: new BaseTableSqlGenerator('"'),
};

export const sqlserverDialectProfile: SqlDialectProfile = {
  quoteStyle: 'bracket',
  foldCase: 'preserve',
  projectionAliasVisibility: 'broad',
  parameterPolicy: {
    atNamed: true,
    question: false,
    dollarPositional: false,
    template: false,
  },
};
