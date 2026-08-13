import { strToU8, zipSync } from 'fflate';
import { fileCommands } from '../commands/file';
import {
  batchExportNeedsZip,
  buildBatchExportFiles,
  combineBatchExportFiles,
  getBatchExportDefaultFilename,
  type BatchExportDataFormat,
  type BatchExportFile,
  type BatchExportMode,
  type BatchExportTableInput,
} from './batchExport';
import { streamTableExportText, type StreamQueryFn } from './exportStream';

export type BatchExportOutputMode = 'single' | 'zip';

export interface BatchExportProgress {
  current: number;
  total: number;
  tableName: string;
}

export interface RunBatchExportJobOptions {
  tableNames: string[];
  mode: BatchExportMode;
  dataFormat: BatchExportDataFormat;
  outputMode: BatchExportOutputMode;
  databaseType?: string;
  connectionId?: string;
  loadTableExportData: (tableName: string) => Promise<BatchExportTableInput>;
  /** When set, table data is streamed via query_stream instead of using in-memory rows. */
  streamQuery?: StreamQueryFn;
  onProgress?: (progress: BatchExportProgress) => void;
  /** Injectable for tests; defaults to fileCommands.saveTextWithDialog */
  saveText?: (
    contents: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) => Promise<boolean>;
  /** Injectable for tests; defaults to fileCommands.saveBase64WithDialog */
  saveBase64?: (
    dataBase64: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) => Promise<boolean>;
}

export type RunBatchExportJobResult = { status: 'saved' } | { status: 'cancelled' };

/** Convert zip bytes to base64 for saveBase64WithDialog. */
export function uint8ToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]!);
  }
  return btoa(binary);
}

/** Pack batch export files into a zip archive (base64). */
export function zipBatchExportFiles(files: BatchExportFile[]): string {
  const archive: Record<string, Uint8Array> = {};
  for (const file of files) {
    archive[file.filename] = strToU8(file.content);
  }
  return uint8ToBase64(zipSync(archive));
}

function defaultZipFilename(mode: BatchExportMode, dataFormat: BatchExportDataFormat): string {
  return getBatchExportDefaultFilename(mode, true, dataFormat);
}

async function resolveTableInput(
  tableName: string,
  mode: BatchExportMode,
  dataFormat: BatchExportDataFormat,
  options: RunBatchExportJobOptions,
): Promise<BatchExportTableInput> {
  const meta = await options.loadTableExportData(tableName);
  const needsData = mode !== 'structure_only';
  if (!needsData) {
    return { ...meta, rows: [] };
  }
  if (
    options.streamQuery &&
    options.connectionId &&
    meta.rows.length === 0 &&
    meta.streamedData == null
  ) {
    const columns = meta.columns.map((c) => c.name);
    const content = await streamTableExportText({
      connectionId: options.connectionId,
      tableName,
      columns,
      format: dataFormat,
      databaseType: options.databaseType,
      streamQuery: options.streamQuery,
    });
    return { ...meta, rows: [], streamedData: content };
  }
  return meta;
}

/**
 * Load selected tables, build export files, then save as a single merged file or ZIP.
 * Returns `cancelled` when the user dismisses the native save dialog.
 *
 * Data is streamed via `query_stream` when `streamQuery` + `connectionId` are provided;
 * otherwise in-memory `rows` from `loadTableExportData` are used (tests / fallback).
 */
export async function runBatchExportJob(
  options: RunBatchExportJobOptions,
): Promise<RunBatchExportJobResult> {
  const {
    tableNames,
    mode,
    dataFormat,
    outputMode,
    databaseType,
    onProgress,
    saveText = fileCommands.saveTextWithDialog,
    saveBase64 = fileCommands.saveBase64WithDialog,
  } = options;

  if (tableNames.length === 0) {
    throw new Error('no_tables_selected');
  }

  const tables: BatchExportTableInput[] = [];
  const total = tableNames.length;
  for (let i = 0; i < tableNames.length; i += 1) {
    const tableName = tableNames[i]!;
    onProgress?.({ current: i + 1, total, tableName });
    tables.push(await resolveTableInput(tableName, mode, dataFormat, options));
  }

  const files = buildBatchExportFiles({
    tables,
    mode,
    dataFormat,
    databaseType,
  });

  const useZip = batchExportNeedsZip(files, outputMode);
  if (useZip) {
    const dataBase64 = zipBatchExportFiles(files);
    const zipName = defaultZipFilename(mode, dataFormat);
    const saved = await saveBase64(dataBase64, zipName, 'ZIP', ['zip']);
    return saved ? { status: 'saved' } : { status: 'cancelled' };
  }

  const content = files.length === 1 ? (files[0]?.content ?? '') : combineBatchExportFiles(files);
  const filename =
    files.length === 1
      ? (files[0]?.filename ?? getBatchExportDefaultFilename(mode, false, dataFormat))
      : getBatchExportDefaultFilename(mode, false, dataFormat);
  const ext = filename.split('.').pop() ?? 'txt';
  const saved = await saveText(content, filename, ext.toUpperCase(), [ext, 'txt']);
  return saved ? { status: 'saved' } : { status: 'cancelled' };
}
