/**
 * Export format/scope vocabulary shared by the export dialogs.
 *
 * Item 3c retired the in-memory export implementation that used to live
 * here (`generateExport` / `generateExportFromArrays` and the SQL/CSV
 * helpers). The frontend-DataFetcher plan (task 3) retired the Rust
 * `provided` source next: already-loaded rows now go through
 * `ProvidedDataFetcher` (`lib/dataFetcher.ts`, formatted in memory by
 * `exportFormat.ts`), while entire/filtered rows go through
 * `SqlDataFetcher` / `exportSingleTableViaRust` (`lib/batchExportJob.ts`,
 * `export_tables_stream`). The only frontend XLSX writer left is the
 * import side (`importData.parseXLSX`); the SheetJS export branch is gone.
 *
 * The escaping kernels themselves were never here: `gridClipboard` owns
 * the TSV/CSV/Markdown serializers (clipboard and export shared them),
 * and the SQL-literal dialect lives on the Rust side.
 */

export type ExportFormat =
  | 'csv'
  | 'tsv'
  | 'json'
  | 'markdown'
  | 'xlsx'
  | 'sql_insert'
  | 'sql_update';
export type ExportScope = 'current_page' | 'selected' | 'entire_table' | 'filtered_table';
