import { fileCommands } from '../commands/file';
import type { ExportFormat, ExportResult, ExportScope } from './exportData';
import { getDefaultFilename } from './exportData';

/** Formats shared by both connection ExportDialog and DataTable DataExportDialog. */
export const SHARED_EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'json', label: 'JSON' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'xlsx', label: 'Excel (XLSX)' },
  { value: 'sql_insert', label: 'SQL INSERT' },
];

/** Connection table export (object rows) — includes SQL UPDATE. */
export const OBJECT_EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  ...SHARED_EXPORT_FORMAT_OPTIONS,
  { value: 'sql_update', label: 'SQL UPDATE' },
];

/** DataTable export (array rows) — includes TSV, no SQL UPDATE. */
export const ARRAY_EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'tsv', label: 'TSV' },
  { value: 'json', label: 'JSON' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'xlsx', label: 'Excel (XLSX)' },
  { value: 'sql_insert', label: 'SQL INSERT' },
];

export function buildExportScopeOptions(
  t: (key: 'export.currentPage' | 'export.selectedRows' | 'common.rows') => string,
  rowCount: number,
  selectedCount: number,
): { value: ExportScope; label: string; disabled?: boolean }[] {
  return [
    {
      value: 'current_page',
      label: `${t('export.currentPage')} (${rowCount} ${t('common.rows')})`,
    },
    {
      value: 'selected',
      label: `${t('export.selectedRows')} (${selectedCount} ${t('common.rows')})`,
      disabled: selectedCount === 0,
    },
  ];
}

/** Save an export result via the native file dialog. Returns false if the user cancelled. */
export async function saveExportResultWithDialog(
  result: ExportResult,
  tableName: string,
  format: ExportFormat,
): Promise<boolean> {
  const defaultName = getDefaultFilename(tableName, format);
  if (result.kind === 'binary') {
    return fileCommands.saveBase64WithDialog(
      result.dataBase64,
      defaultName,
      result.extension.toUpperCase(),
      [result.extension],
    );
  }
  return fileCommands.saveTextWithDialog(
    result.content,
    defaultName,
    result.extension.toUpperCase(),
    [result.extension],
  );
}
