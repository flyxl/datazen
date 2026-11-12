import type { ExportFormat, ExportScope } from './exportData';

/** Formats shared by both connection ExportDialog and DataTable DataExportDialog. */
export const SHARED_EXPORT_FORMAT_OPTIONS: { value: ExportFormat; label: string }[] = [
  { value: 'csv', label: 'CSV' },
  { value: 'json', label: 'JSON' },
  { value: 'markdown', label: 'Markdown' },
  { value: 'xlsx', label: 'Excel (XLSX)' },
  { value: 'parquet', label: 'Parquet' },
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
  { value: 'parquet', label: 'Parquet' },
  { value: 'sql_insert', label: 'SQL INSERT' },
  { value: 'sql_update', label: 'SQL UPDATE' },
];

/**
 * One format entry of both export dialogs, with its availability.
 *
 * An unavailable format stays listed and greyed out, with its reason on
 * screen — never hidden, never silently reset (DBR-06, restored for the
 * new architecture).
 */
export interface ExportFormatOption {
  value: ExportFormat;
  label: string;
  /** Offered but greyed out — the format stays visible with its reason. */
  disabled?: boolean;
  /** i18n key of the explanation shown while this format is disabled. */
  disabledReasonKey?:
    | 'export.formatDisabledNoKey'
    | 'export.formatDisabledWebSqlScope'
    | 'export.formatDisabledParquetRustOnly';
}

/**
 * Format options for the current export range, both dialogs (DBR-06 rules
 * re-derived for the frontend-DataFetcher architecture).
 *
 * - XLSX is never disabled: both roads serve it now — SheetJS in the
 *   frontend for the provided scopes, the Rust BinaryFormatter for the
 *   SQL scopes. The old streaming-scope XLSX ban is obsolete and stays
 *   dropped.
 * - Parquet is Rust-only (PRD §4 F8): the desktop streaming scopes reach
 *   the Rust batch exporter, but the provided scopes have no frontend
 *   writer and web has no Rust path — offered but greyed out with its
 *   reason there, never hidden.
 * - SQL UPDATE needs a key column for its WHERE; without one it is
 *   disabled rather than silently writing `WHERE <first column> = …`.
 * - Web has no SQL path: the streaming scopes (entire table / filter
 *   results) go through the web exporter, which only assembles CSV, so
 *   every non-CSV format is disabled there with that reason.
 *
 * `isDesktop` is the caller's `IS_DESKTOP_BUILD`, passed in so tests can
 * flip it (it is a compile-time constant in production).
 */
export function buildExportFormatOptions(opts: {
  /** Full format list of the calling dialog (ARRAY_ or OBJECT_ options). */
  options: { value: ExportFormat; label: string }[];
  scope: ExportScope;
  isDesktop: boolean;
  hasKeyColumn: boolean;
}): ExportFormatOption[] {
  const isStreamingScope = opts.scope === 'entire_table' || opts.scope === 'filtered_table';
  return opts.options.map((option) => {
    // The key-column rule is the more specific one: on a web streaming
    // scope without a key, SQL UPDATE carries the no-key reason while the
    // other non-CSV formats carry the web reason, so both explanations
    // stay on screen (DBR-06 showed both too).
    if (option.value === 'sql_update' && !opts.hasKeyColumn) {
      return { ...option, disabled: true, disabledReasonKey: 'export.formatDisabledNoKey' };
    }
    // Parquet runs through the Rust batch exporter only (PRD §4 F8): it
    // stays available on the desktop streaming scopes and is greyed out
    // everywhere else — the provided scopes (no frontend writer, provided
    // Parquet is a PRD non-goal) and all of web (no Rust path).
    if (option.value === 'parquet' && (!opts.isDesktop || !isStreamingScope)) {
      return {
        ...option,
        disabled: true,
        disabledReasonKey: 'export.formatDisabledParquetRustOnly',
      };
    }
    if (!opts.isDesktop && isStreamingScope && option.value !== 'csv') {
      return {
        ...option,
        disabled: true,
        disabledReasonKey: 'export.formatDisabledWebSqlScope',
      };
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
