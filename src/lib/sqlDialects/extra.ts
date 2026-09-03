import type { TableSchema } from '../../types';
import type { SqlDialectStrategy } from './types';
import { BaseTableSqlGenerator } from './baseTableSql';

class ClickHouseTableSqlGenerator extends BaseTableSqlGenerator {
  constructor() {
    super('`');
  }

  override generateUpdate(tableRef: string, schema: TableSchema): string {
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

class ElasticsearchTableSqlGenerator extends BaseTableSqlGenerator {
  constructor() {
    super('"');
  }

  override generateInsert(): string {
    throw new Error('Elasticsearch SQL does not support INSERT statements');
  }

  override generateUpdate(): string {
    throw new Error('Elasticsearch SQL does not support UPDATE statements');
  }

  override generateDelete(): string {
    throw new Error('Elasticsearch SQL does not support DELETE statements');
  }
}

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
  tableSql: new BaseTableSqlGenerator('"'),
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
  backupOptions: [
    { id: 'schema-only', label: '--schema-only' },
    { id: 'data-only', label: '--data-only' },
    { id: 'clean', label: '--clean' },
    { id: 'no-owner', label: '--no-owner' },
  ],
  tableSql: new ClickHouseTableSqlGenerator(),
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
  backupOptions: [
    { id: 'schema-only', label: '--schema-only' },
    { id: 'data-only', label: '--data-only' },
    { id: 'clean', label: '--clean' },
    { id: 'no-owner', label: '--no-owner' },
    { id: 'single-transaction', label: '--single-transaction' },
  ],
  tableSql: new BaseTableSqlGenerator('"'),
};

export const elasticsearchDialect: SqlDialectStrategy = {
  family: 'elasticsearch',
  ddl: noDdl,
  index: standardIndex('bare'),
  backupOptions: [],
  tableSql: new ElasticsearchTableSqlGenerator(),
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
  tableSql: new BaseTableSqlGenerator('"'),
};
