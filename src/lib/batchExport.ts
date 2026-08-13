import type { ColumnSchema } from '../types';
import { generateExport } from './exportData';

export type BatchExportMode = 'data_only' | 'structure_only' | 'data_and_structure';
export type BatchExportDataFormat = 'csv' | 'json' | 'sql_insert';

export interface BatchExportTableInput {
  tableName: string;
  /** DDL text; required when mode includes structure */
  ddl?: string | null;
  columns: { name: string }[];
  rows: Record<string, unknown>[];
  /** Pre-formatted data file body from a streaming export (skips generateExport). */
  streamedData?: string;
}

export interface BuildBatchExportOptions {
  tables: BatchExportTableInput[];
  mode: BatchExportMode;
  dataFormat: BatchExportDataFormat;
  databaseType?: string;
}

/** One output file per table (or combined sql). */
export interface BatchExportFile {
  filename: string;
  content: string;
}

const DATA_EXT: Record<BatchExportDataFormat, string> = {
  csv: 'csv',
  json: 'json',
  sql_insert: 'sql',
};

function resolveDdl(table: BatchExportTableInput): string {
  if (table.ddl != null && table.ddl.trim() !== '') {
    return table.ddl;
  }
  return `-- DDL unavailable for ${table.tableName}`;
}

function toColumnSchemas(columns: { name: string }[]): ColumnSchema[] {
  return columns.map((col) => {
    const full = col as ColumnSchema;
    return {
      name: col.name,
      dataType: full.dataType ?? 'text',
      nullable: full.nullable ?? true,
      isPrimaryKey: full.isPrimaryKey ?? false,
      isAutoIncrement: full.isAutoIncrement ?? false,
      defaultValue: full.defaultValue,
      comment: full.comment,
    };
  });
}

function exportTableData(
  table: BatchExportTableInput,
  dataFormat: BatchExportDataFormat,
  databaseType?: string,
): string {
  if (table.streamedData != null) {
    return table.streamedData;
  }
  const selectedColumns = table.columns.map((c) => c.name);
  const result = generateExport({
    tableName: table.tableName,
    columns: toColumnSchemas(table.columns),
    rows: table.rows,
    selectedRows: new Set(),
    scope: 'current_page',
    selectedColumns,
    format: dataFormat,
    databaseType,
  });
  if (result.kind !== 'text') {
    throw new Error(`Batch export format ${dataFormat} must produce text`);
  }
  return result.content;
}

/**
 * Select table names for batch export.
 * `'all'` → every name in `allTableNames`; otherwise dedupe and keep only names present in `all`.
 */
export function selectTablesForExport(
  allTableNames: string[],
  selected: string[] | 'all',
): string[] {
  if (selected === 'all') {
    return [...allTableNames];
  }
  const allowed = new Set(allTableNames);
  const seen = new Set<string>();
  const result: string[] = [];
  for (const name of selected) {
    if (!allowed.has(name) || seen.has(name)) continue;
    seen.add(name);
    result.push(name);
  }
  return result;
}

/** Build one or more output files for the given tables and export mode. */
export function buildBatchExportFiles(options: BuildBatchExportOptions): BatchExportFile[] {
  const { tables, mode, dataFormat, databaseType } = options;
  const files: BatchExportFile[] = [];

  for (const table of tables) {
    const base = table.tableName;

    if (mode === 'structure_only') {
      files.push({
        filename: `${base}.sql`,
        content: resolveDdl(table),
      });
      continue;
    }

    if (mode === 'data_only') {
      files.push({
        filename: `${base}.${DATA_EXT[dataFormat]}`,
        content: exportTableData(table, dataFormat, databaseType),
      });
      continue;
    }

    // data_and_structure
    if (dataFormat === 'sql_insert') {
      const ddl = resolveDdl(table);
      const inserts = exportTableData(table, 'sql_insert', databaseType);
      files.push({
        filename: `${base}.sql`,
        content: `${ddl}\n\n${inserts}`,
      });
      continue;
    }

    // csv / json → separate DDL + data files; still emit data when ddl missing
    files.push({
      filename: `${base}.sql`,
      content: resolveDdl(table),
    });
    files.push({
      filename: `${base}.${DATA_EXT[dataFormat]}`,
      content: exportTableData(table, dataFormat, databaseType),
    });
  }

  return files;
}

/**
 * Join multiple export files into one text blob (for “merge into a single file”).
 * Default separator banners each section with `-- ===== filename =====`.
 * Only valid when every file is SQL (or a caller-supplied separator is used).
 */
export function combineBatchExportFiles(files: BatchExportFile[], separator?: string): string {
  if (files.length === 0) return '';

  if (separator !== undefined) {
    return files.map((f) => f.content).join(separator);
  }

  return files.map((f) => `-- ===== ${f.filename} =====\n\n${f.content}`).join('\n\n');
}

function fileExtension(filename: string): string {
  const dot = filename.lastIndexOf('.');
  return dot >= 0 ? filename.slice(dot + 1).toLowerCase() : '';
}

/** CSV/JSON (and mixed SQL+data) cannot be concatenated into one valid file. */
export function batchExportNeedsZip(
  files: BatchExportFile[],
  outputMode: 'single' | 'zip',
): boolean {
  if (outputMode === 'zip') return true;
  if (files.length <= 1) return false;
  const exts = new Set(files.map((f) => fileExtension(f.filename)));
  if (exts.size > 1) return true;
  const ext = [...exts][0] ?? '';
  return ext === 'csv' || ext === 'json' || ext === 'tsv';
}

/**
 * Default download name for a batch export.
 * Extension follows the data format unless the result is a zip or combined SQL.
 */
export function getBatchExportDefaultFilename(
  mode: BatchExportMode,
  zipOrMulti: boolean,
  dataFormat: BatchExportDataFormat = 'sql_insert',
): string {
  const ts = new Date().toISOString().slice(0, 19).replace(/[T:]/g, '-');
  const prefix =
    mode === 'structure_only'
      ? 'export_structure'
      : mode === 'data_only'
        ? 'export_data'
        : 'export_full';
  if (zipOrMulti) {
    return `${prefix}_${ts}.zip`;
  }
  if (mode === 'structure_only' || dataFormat === 'sql_insert') {
    return `${prefix}_${ts}.sql`;
  }
  return `${prefix}_${ts}.${DATA_EXT[dataFormat]}`;
}
