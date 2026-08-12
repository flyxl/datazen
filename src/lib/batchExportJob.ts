import { strToU8, zipSync } from 'fflate';
import { fileCommands } from '../commands/file';
import {
  buildBatchExportFiles,
  combineBatchExportFiles,
  getBatchExportDefaultFilename,
  type BatchExportDataFormat,
  type BatchExportFile,
  type BatchExportMode,
  type BatchExportTableInput,
} from './batchExport';

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
  loadTableExportData: (tableName: string) => Promise<BatchExportTableInput>;
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

function defaultZipFilename(mode: BatchExportMode): string {
  return getBatchExportDefaultFilename(mode, true).replace(/\.sql$/i, '.zip');
}

/**
 * Load selected tables, build export files, then save as a single merged file or ZIP.
 * Returns `cancelled` when the user dismisses the native save dialog.
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
    loadTableExportData,
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
    tables.push(await loadTableExportData(tableName));
  }

  const files = buildBatchExportFiles({
    tables,
    mode,
    dataFormat,
    databaseType,
  });

  if (outputMode === 'single') {
    const content = combineBatchExportFiles(files);
    const filename = getBatchExportDefaultFilename(mode, false);
    const saved = await saveText(content, filename, 'SQL', ['sql', 'txt']);
    return saved ? { status: 'saved' } : { status: 'cancelled' };
  }

  const dataBase64 = zipBatchExportFiles(files);
  const zipName = defaultZipFilename(mode);
  const saved = await saveBase64(dataBase64, zipName, 'ZIP', ['zip']);
  return saved ? { status: 'saved' } : { status: 'cancelled' };
}
