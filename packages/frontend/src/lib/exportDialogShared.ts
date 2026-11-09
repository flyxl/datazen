import { fileCommands } from '../commands/file';
import type { ExportFormat, ExportResult, ExportScope } from './exportData';
import { getDefaultFilename } from './exportData';

/**
 * The one format list both export dialogs render (DBR-06: 网格与表级导出的
 * 格式显示统一——此前网格缺 SQL UPDATE、表级缺 TSV，同一张表从两个入口
 * 导出能选的格式却不同)。 Order matters: it is the display order of both
 * dialogs' format comboboxes.
 */
export const EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'tsv', label: 'TSV' },
  { value: 'json', label: 'JSON' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'xlsx', label: 'Excel (XLSX)' },
  { value: 'sql_insert', label: 'SQL INSERT' },
  { value: 'sql_update', label: 'SQL UPDATE' },
];

export interface ExportFormatOption {
  value: ExportFormat;
  label: string;
  /** Offered but greyed out — the format stays visible with its reason. */
  disabled?: boolean;
  /** i18n key of the explanation shown while this format is disabled. */
  disabledReasonKey?: string;
}

/**
 * Format options for the current export range (DBR-06: 格式不支持当前范围时
 * 禁用并解释原因——不隐藏，被问“为什么没有 XLSX”时才有答案可给)。
 *
 * - XLSX needs every row in memory, so the streaming scopes (entire table,
 *   filter results) keep it disabled with that reason — an explicit, honest
 *   limitation rather than a fake streaming writer.
 * - SQL UPDATE needs a key column for its WHERE; without one it is disabled
 *   rather than silently writing `WHERE <first column> = …`.
 */
export function buildExportFormatOptions(opts: {
  streamingScope: boolean;
  hasKeyColumn: boolean;
}): ExportFormatOption[] {
  return EXPORT_FORMAT_OPTIONS.map((option) => {
    if (option.value === 'xlsx' && opts.streamingScope) {
      return { ...option, disabled: true, disabledReasonKey: 'export.formatDisabledStream' };
    }
    if (option.value === 'sql_update' && !opts.hasKeyColumn) {
      return { ...option, disabled: true, disabledReasonKey: 'export.formatDisabledNoKey' };
    }
    return { ...option };
  });
}

export function buildExportScopeOptions(
  t: (
    key:
      | 'export.currentPage'
      | 'export.selectedRows'
      | 'export.entireTable'
      | 'export.filteredTable'
      | 'common.rows',
    params?: Record<string, string | number>,
  ) => string,
  rowCount: number,
  selectedCount: number,
  entireTable?: { totalRows?: number } | false,
  /**
   * Offer "the current filter results, streamed" (DB-15). Passed only when a
   * builder for the filtered statement exists — the scope must never appear
   * for a table whose filtered statement cannot be built.
   */
  filteredTable?: boolean,
): { value: ExportScope; label: string; disabled?: boolean }[] {
  const options: { value: ExportScope; label: string; disabled?: boolean }[] = [
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
  if (filteredTable) {
    options.push({
      value: 'filtered_table',
      label: t('export.filteredTable'),
    });
  }
  if (entireTable) {
    const total = entireTable.totalRows;
    options.push({
      value: 'entire_table',
      label:
        total != null && total >= 0
          ? `${t('export.entireTable')} (${total} ${t('common.rows')})`
          : t('export.entireTable'),
    });
  }
  return options;
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
