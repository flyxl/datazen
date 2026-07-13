import type { SqlDialectStrategy } from './types';

const PG_OPTIONS = [
  { id: 'data-only', label: '--data-only' },
  { id: 'clean', label: '--clean' },
  { id: 'create', label: '--create' },
  { id: 'no-owner', label: '--no-owner' },
  { id: 'schema-only', label: '--schema-only' },
  { id: 'format-custom', label: '--format=custom' },
];

export const postgresqlDialect: SqlDialectStrategy = {
  family: 'postgresql',
  ddl: {
    getTableDdlQuery(tableName: string) {
      return {
        sql: `
      SELECT
        'CREATE TABLE ' || quote_ident(schemaname) || '.' || quote_ident(tablename) || E' (\\n' ||
        string_agg(
          '  ' || quote_ident(attname) || ' ' || format_type(atttypid, atttypmod) ||
          CASE WHEN NOT attnotnull THEN '' ELSE ' NOT NULL' END ||
          CASE WHEN pg_get_expr(adbin, adrelid) IS NOT NULL
               THEN ' DEFAULT ' || pg_get_expr(adbin, adrelid)
               ELSE '' END,
          E',\\n' ORDER BY attnum
        ) || E'\\n);' AS ddl
      FROM pg_tables t
      JOIN pg_attribute a ON a.attrelid = (quote_ident(schemaname) || '.' || quote_ident(tablename))::regclass
      LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
      WHERE a.attnum > 0 AND NOT a.attisdropped
        AND tablename = '${tableName}'
      GROUP BY schemaname, tablename;
    `,
        extractColumnIndex: 0,
      };
    },
  },
  index: {
    supportedIndexMethods: ['btree', 'hash', 'gin', 'gist'],
    getDropIndexSql(indexName, _tableName, quoteChar) {
      return `DROP INDEX ${quoteChar}${indexName}${quoteChar}`;
    },
    getCreateIndexSql(opts) {
      const uniqueKw = opts.unique ? 'UNIQUE ' : '';
      const usingKw = opts.method && opts.method !== 'btree' ? ` USING ${opts.method}` : '';
      const quotedCols = opts.columns.map((c) => `${opts.quoteChar}${c}${opts.quoteChar}`).join(', ');
      return `CREATE ${uniqueKw}INDEX ${opts.quoteChar}${opts.indexName}${opts.quoteChar} ON ${opts.quoteChar}${opts.tableName}${opts.quoteChar}${usingKw} (${quotedCols})`;
    },
  },
  backupOptions: PG_OPTIONS,
};
