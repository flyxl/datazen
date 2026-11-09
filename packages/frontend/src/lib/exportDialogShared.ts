import type { ExportFormat, ExportScope } from './exportData';

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

/** DataTable export (array rows). Item 3d: SQL UPDATE is offered — both
 * dialogs pass the primary key when known, and the Rust exporter falls
 * back to the first column otherwise (the old frontend rule). */
export const ARRAY_EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'tsv', label: 'TSV' },
  { value: 'json', label: 'JSON' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'xlsx', label: 'Excel (XLSX)' },
  { value: 'sql_insert', label: 'SQL INSERT' },
  { value: 'sql_update', label: 'SQL UPDATE' },
];

/**
 * Formats a scope may offer on this form (plan §2.8, task 6).
 *
 * Desktop offers every format for every scope — the Rust exporter handles
 * the SQL scopes and `ProvidedDataFetcher` the loaded ones. Web has no SQL
 * path: `entire_table` / `filtered_table` go through the web exporter,
 * which only assembles CSV (and has no zip writer), so those scopes offer
 * CSV only; `current_page` / `selected` format in JS
 * (`exportFormat.ts`), so every format is available on both forms.
 * `isDesktop` is the caller's `IS_DESKTOP_BUILD`, passed in so tests can
 * flip it (it is a compile-time constant in production).
 */
export function formatOptionsForScope(
  allOptions: { value: ExportFormat; label: string }[],
  scope: ExportScope,
  isDesktop: boolean,
): { value: ExportFormat; label: string }[] {
  if (isDesktop || scope === 'current_page' || scope === 'selected') {
    return allOptions;
  }
  return allOptions.filter((option) => option.value === 'csv');
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
