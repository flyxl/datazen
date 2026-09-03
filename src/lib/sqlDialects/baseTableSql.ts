import type { TableSchema } from '../../types';
import type { GeneratedSqlType, TableSqlDialect } from './types';

/**
 * Base ANSI SQL generator for table template statements.
 * Dialects can inherit and override individual statement generators.
 */
export class BaseTableSqlGenerator implements TableSqlDialect {
  constructor(protected quoteChar: string = '"') {}

  protected quote(ident: string): string {
    if (this.quoteChar === '`') return `\`${ident.replaceAll('`', '``')}\``;
    if (this.quoteChar === '"') return `"${ident.replaceAll('"', '""')}"`;
    if (this.quoteChar === '[') return `[${ident.replaceAll(']', ']]')}]`;
    return ident;
  }

  formatTableRef(tableName: string, schemaPrefix?: string): string {
    if (schemaPrefix && schemaPrefix.trim()) {
      return `${this.quote(schemaPrefix.trim())}.${this.quote(tableName)}`;
    }
    return this.quote(tableName);
  }

  generateSelect(tableRef: string, schema: TableSchema): string {
    const cols = schema.columns.map((c) => this.quote(c.name)).join(', ');
    return `SELECT ${cols}\nFROM ${tableRef};`;
  }

  generateInsert(tableRef: string, schema: TableSchema): string {
    const insertableCols = schema.columns.filter((c) => !c.isAutoIncrement);
    const colList = insertableCols.map((c) => `  ${this.quote(c.name)}`).join(',\n');
    const valList = insertableCols
      .map((c) => {
        if (c.defaultValue) return `  ${c.defaultValue}`;
        if (
          c.dataType.toLowerCase().includes('int') ||
          c.dataType.toLowerCase().includes('numeric')
        ) {
          return '  0';
        }
        return "  ''";
      })
      .join(',\n');
    return `INSERT INTO ${tableRef} (\n${colList}\n) VALUES (\n${valList}\n);`;
  }

  generateUpdate(tableRef: string, schema: TableSchema): string {
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
    return `UPDATE ${tableRef}\nSET\n${setClauses}\nWHERE ${whereClause};`;
  }

  generateDelete(tableRef: string, schema: TableSchema): string {
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
    return `DELETE FROM ${tableRef}\nWHERE ${whereClause};`;
  }

  generateSql(type: GeneratedSqlType, tableRef: string, schema: TableSchema): string {
    switch (type) {
      case 'select':
        return this.generateSelect(tableRef, schema);
      case 'insert':
        return this.generateInsert(tableRef, schema);
      case 'update':
        return this.generateUpdate(tableRef, schema);
      case 'delete':
        return this.generateDelete(tableRef, schema);
    }
  }
}
