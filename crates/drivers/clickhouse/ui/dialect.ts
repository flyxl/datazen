import type { SqlDialectStrategy, SqlDialectProfile, TableSchema } from '@datazen/driver-sdk';
import { BaseTableSqlGenerator } from '@datazen/driver-sdk';

class ClickHouseTableSqlGenerator extends BaseTableSqlGenerator {
  constructor() {
    super('`');
  }

  override generateUpdate(tableRef: string, schema: TableSchema): string {
    if (schema.columns.length === 0) {
      return super.generateUpdate(tableRef, schema);
    }
    const nonPkCols = schema.columns.filter(
      (c) => !schema.primaryKeys.includes(c.name) && !c.isPrimaryKey,
    );
    const targetCols = nonPkCols.length > 0 ? nonPkCols : schema.columns;
    const setClauses = targetCols.map((c) => `  ${this.quote(c.name)} = ''`).join(',\n');

    const pks =
      schema.primaryKeys.length > 0
        ? schema.primaryKeys
        : schema.columns.filter((c) => c.isPrimaryKey).map((c) => c.name);

    let whereClause: string;
    if (pks.length > 0) {
      whereClause = pks.map((pk) => `${this.quote(pk)} = `).join(' AND ');
    } else {
      whereClause = '/* WARNING: Primary Key not found. Specify condition */';
    }
    return `ALTER TABLE ${tableRef}\nUPDATE\n${setClauses}\nWHERE ${whereClause};`;
  }

  override generateDelete(tableRef: string, schema: TableSchema): string {
    if (schema.columns.length === 0) {
      return super.generateDelete(tableRef, schema);
    }
    const pks =
      schema.primaryKeys.length > 0
        ? schema.primaryKeys
        : schema.columns.filter((c) => c.isPrimaryKey).map((c) => c.name);

    let whereClause: string;
    if (pks.length > 0) {
      whereClause = pks.map((pk) => `${this.quote(pk)} = `).join(' AND ');
    } else {
      whereClause = '/* WARNING: Primary Key not found. Specify condition */';
    }
    return `ALTER TABLE ${tableRef}\nDELETE WHERE ${whereClause};`;
  }
}

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
  ],
  tableSql: new ClickHouseTableSqlGenerator(),
};

export const clickhouseDialectProfile: SqlDialectProfile = {
  quoteStyle: 'backtick',
  foldCase: 'lower',
  projectionAliasVisibility: 'broad',
  parameterPolicy: {
    atNamed: false,
    question: false,
    dollarPositional: false,
    template: false,
  },
};
