import type { DatabaseType } from '../types';
import { escapeIdent } from './databaseTypes';

export interface ParsedData {
  columns: string[];
  rows: Record<string, unknown>[];
}

function parseCSV(text: string): ParsedData {
  const lines = text.split('\n').filter((l) => l.trim().length > 0);
  if (lines.length === 0) return { columns: [], rows: [] };

  const parseLine = (line: string): string[] => {
    const result: string[] = [];
    let current = '';
    let inQuotes = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (inQuotes) {
        if (ch === '"') {
          if (i + 1 < line.length && line[i + 1] === '"') {
            current += '"';
            i++;
          } else {
            inQuotes = false;
          }
        } else {
          current += ch;
        }
      } else if (ch === '"') {
        inQuotes = true;
      } else if (ch === ',') {
        result.push(current);
        current = '';
      } else {
        current += ch;
      }
    }
    result.push(current);
    return result;
  };

  const columns = parseLine(lines[0]);
  const rows = lines.slice(1).map((line) => {
    const values = parseLine(line);
    const record: Record<string, unknown> = {};
    columns.forEach((col, i) => {
      const val = values[i] ?? '';
      record[col] = val === '' ? null : val;
    });
    return record;
  });

  return { columns, rows };
}

function parseJSON(text: string): ParsedData {
  const parsed = JSON.parse(text);
  const arr: Record<string, unknown>[] = Array.isArray(parsed) ? parsed : [parsed];
  if (arr.length === 0) return { columns: [], rows: [] };

  const columnSet = new Set<string>();
  for (const row of arr) {
    for (const key of Object.keys(row)) columnSet.add(key);
  }
  const columns = Array.from(columnSet);

  const rows = arr.map((item) => {
    const record: Record<string, unknown> = {};
    for (const col of columns) record[col] = item[col] ?? null;
    return record;
  });

  return { columns, rows };
}

export function parseImportData(content: string, format: 'csv' | 'json'): ParsedData {
  return format === 'csv' ? parseCSV(content) : parseJSON(content);
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

export function generateInsertSQL(tableName: string, data: ParsedData, databaseType?: string): string {
  if (data.rows.length === 0) return '';
  const colList = data.columns.map((c) => escapeSQLIdent(c, databaseType)).join(', ');
  return data.rows
    .map((row) => {
      const values = data.columns.map((col) => escapeSQLValue(row[col])).join(', ');
      return `INSERT INTO ${escapeSQLIdent(tableName, databaseType)} (${colList}) VALUES (${values});`;
    })
    .join('\n');
}
