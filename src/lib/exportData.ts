import type { ColumnSchema, DatabaseType } from '../types';
import { escapeIdent } from './databaseTypes';

export type ExportFormat = 'csv' | 'json' | 'sql_insert' | 'sql_update';
export type ExportScope = 'current_page' | 'selected';

interface ExportOptions {
  tableName: string;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  selectedRows: Set<number>;
  scope: ExportScope;
  selectedColumns: string[];
  format: ExportFormat;
  databaseType?: string;
}

function escapeCSV(value: unknown): string {
  if (value === null || value === undefined) return '';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  if (str.includes(',') || str.includes('"') || str.includes('\n')) {
    return `"${str.replaceAll('"', '""')}"`;
  }
  return str;
}

function escapeSQLValue(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return `'${str.replaceAll("'", "''")}'`;
}

function escapeSQLIdent(name: string, dbType?: string): string {
  return escapeIdent(name, dbType as DatabaseType | undefined);
}

function getRows(rows: Record<string, unknown>[], selectedRows: Set<number>, scope: ExportScope): Record<string, unknown>[] {
  if (scope === 'selected' && selectedRows.size > 0) {
    return rows.filter((_, i) => selectedRows.has(i));
  }
  return rows;
}

export function generateExport(options: ExportOptions): { content: string; extension: string; mimeType: string } {
  const { tableName, rows, selectedRows, scope, selectedColumns, format, databaseType } = options;
  const dataRows = getRows(rows, selectedRows, scope);
  const cols = selectedColumns;

  switch (format) {
    case 'csv': {
      const header = cols.map(escapeCSV).join(',');
      const body = dataRows.map((row) =>
        cols.map((col) => escapeCSV(row[col])).join(','),
      );
      return { content: [header, ...body].join('\n'), extension: 'csv', mimeType: 'text/csv' };
    }

    case 'json': {
      const data = dataRows.map((row) => {
        const obj: Record<string, unknown> = {};
        for (const col of cols) obj[col] = row[col] ?? null;
        return obj;
      });
      return { content: JSON.stringify(data, null, 2), extension: 'json', mimeType: 'application/json' };
    }

    case 'sql_insert': {
      const colList = cols.map((c) => escapeSQLIdent(c, databaseType)).join(', ');
      const statements = dataRows.map((row) => {
        const values = cols.map((col) => escapeSQLValue(row[col])).join(', ');
        return `INSERT INTO ${escapeSQLIdent(tableName, databaseType)} (${colList}) VALUES (${values});`;
      });
      return { content: statements.join('\n'), extension: 'sql', mimeType: 'text/sql' };
    }

    case 'sql_update': {
      const pk = options.columns.find((c) => c.isPrimaryKey);
      const pkName = pk?.name ?? cols[0];
      const statements = dataRows.map((row) => {
        const setClauses = cols
          .filter((c) => c !== pkName)
          .map((col) => `${escapeSQLIdent(col, databaseType)} = ${escapeSQLValue(row[col])}`)
          .join(', ');
        const where = `${escapeSQLIdent(pkName, databaseType)} = ${escapeSQLValue(row[pkName])}`;
        return `UPDATE ${escapeSQLIdent(tableName, databaseType)} SET ${setClauses} WHERE ${where};`;
      });
      return { content: statements.join('\n'), extension: 'sql', mimeType: 'text/sql' };
    }
  }
}

export function getDefaultFilename(tableName: string, format: ExportFormat): string {
  const ts = new Date().toISOString().slice(0, 19).replace(/[T:]/g, '-');
  const extMap: Record<ExportFormat, string> = { csv: 'csv', json: 'json', sql_insert: 'sql', sql_update: 'sql' };
  const ext = extMap[format];
  return `${tableName}_${ts}.${ext}`;
}
