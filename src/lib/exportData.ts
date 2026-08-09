import * as XLSX from 'xlsx';
import type { ColumnSchema, DatabaseType } from '../types';
import { escapeIdent } from './databaseTypes';

export type ExportFormat = 'csv' | 'tsv' | 'json' | 'markdown' | 'xlsx' | 'sql_insert' | 'sql_update';
export type ExportScope = 'current_page' | 'selected';

export type ExportResult =
  | { kind: 'text'; content: string; extension: string; mimeType: string }
  | { kind: 'binary'; dataBase64: string; extension: string; mimeType: string };

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

function escapeMarkdownCell(value: unknown): string {
  if (value === null || value === undefined) return '';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return str.replaceAll('|', '\\|').replaceAll('\n', ' ').replaceAll('\r', '');
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

export function generateExport(options: ExportOptions): ExportResult {
  const { tableName, rows, selectedRows, scope, selectedColumns, format, databaseType } = options;
  const dataRows = getRows(rows, selectedRows, scope);
  const cols = selectedColumns;

  switch (format) {
    case 'csv': {
      const header = cols.map(escapeCSV).join(',');
      const body = dataRows.map((row) =>
        cols.map((col) => escapeCSV(row[col])).join(','),
      );
      return { kind: 'text', content: [header, ...body].join('\n'), extension: 'csv', mimeType: 'text/csv' };
    }

    case 'tsv': {
      const escapeTSV = (v: unknown) => {
        if (v === null || v === undefined) return '';
        const s = typeof v === 'object' ? JSON.stringify(v) : String(v);
        return s.replaceAll('\t', ' ').replaceAll('\n', ' ').replaceAll('\r', '');
      };
      const header = cols.map(escapeTSV).join('\t');
      const body = dataRows.map((row) =>
        cols.map((col) => escapeTSV(row[col])).join('\t'),
      );
      return { kind: 'text', content: [header, ...body].join('\n'), extension: 'tsv', mimeType: 'text/tab-separated-values' };
    }

    case 'json': {
      const data = dataRows.map((row) => {
        const obj: Record<string, unknown> = {};
        for (const col of cols) obj[col] = row[col] ?? null;
        return obj;
      });
      return { kind: 'text', content: JSON.stringify(data, null, 2), extension: 'json', mimeType: 'application/json' };
    }

    case 'markdown': {
      const header = `| ${cols.join(' | ')} |`;
      const separator = `| ${cols.map(() => '---').join(' | ')} |`;
      const body = dataRows.map((row) =>
        `| ${cols.map((col) => escapeMarkdownCell(row[col])).join(' | ')} |`,
      );
      return {
        kind: 'text',
        content: [header, separator, ...body].join('\n'),
        extension: 'md',
        mimeType: 'text/markdown',
      };
    }

    case 'xlsx': {
      const sheetRows = dataRows.map((row) => {
        const obj: Record<string, unknown> = {};
        for (const col of cols) obj[col] = row[col] ?? null;
        return obj;
      });
      const ws = XLSX.utils.json_to_sheet(sheetRows, { header: cols });
      const wb = XLSX.utils.book_new();
      XLSX.utils.book_append_sheet(wb, ws, 'Sheet1');
      const dataBase64 = XLSX.write(wb, { type: 'base64', bookType: 'xlsx' });
      return {
        kind: 'binary',
        dataBase64,
        extension: 'xlsx',
        mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      };
    }

    case 'sql_insert': {
      const colList = cols.map((c) => escapeSQLIdent(c, databaseType)).join(', ');
      const statements = dataRows.map((row) => {
        const values = cols.map((col) => escapeSQLValue(row[col])).join(', ');
        return `INSERT INTO ${escapeSQLIdent(tableName, databaseType)} (${colList}) VALUES (${values});`;
      });
      return { kind: 'text', content: statements.join('\n'), extension: 'sql', mimeType: 'text/sql' };
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
      return { kind: 'text', content: statements.join('\n'), extension: 'sql', mimeType: 'text/sql' };
    }
  }
}

export function getDefaultFilename(tableName: string, format: ExportFormat): string {
  const ts = new Date().toISOString().slice(0, 19).replace(/[T:]/g, '-');
  const extMap: Record<ExportFormat, string> = {
    csv: 'csv',
    tsv: 'tsv',
    json: 'json',
    markdown: 'md',
    xlsx: 'xlsx',
    sql_insert: 'sql',
    sql_update: 'sql',
  };
  const ext = extMap[format];
  return `${tableName}_${ts}.${ext}`;
}

/**
 * Generate export from array-based data (used by DataTable).
 * Converts column names + 2D row arrays into record format and delegates to generateExport.
 */
export function generateExportFromArrays(options: {
  columnNames: string[];
  rows: unknown[][];
  selectedRows?: Set<number>;
  scope?: ExportScope;
  format: ExportFormat;
  tableName?: string;
  databaseType?: string;
}): ExportResult {
  const {
    columnNames,
    rows,
    selectedRows = new Set<number>(),
    scope = 'current_page',
    format,
    tableName = 'data',
    databaseType,
  } = options;

  const recordRows: Record<string, unknown>[] = rows.map((row) => {
    const obj: Record<string, unknown> = {};
    for (let i = 0; i < columnNames.length; i++) {
      obj[columnNames[i]] = row[i] ?? null;
    }
    return obj;
  });

  const columns: ColumnSchema[] = columnNames.map((name) => ({
    name,
    dataType: 'text',
    nullable: true,
    isPrimaryKey: false,
    isAutoIncrement: false,
    defaultValue: undefined,
    comment: undefined,
  }));

  return generateExport({
    tableName,
    columns,
    rows: recordRows,
    selectedRows,
    scope,
    selectedColumns: columnNames,
    format,
    databaseType,
  });
}
