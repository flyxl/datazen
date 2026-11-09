/**
 * Export format/scope vocabulary shared by the export dialogs.
 *
 * Item 3c retired the in-memory export implementation that used to live
 * here (`generateExport` / `generateExportFromArrays` and the SQL/CSV
 * helpers): every scope — current page, selected rows, entire table,
 * filtered table — now goes through the Rust batch exporter
 * (`export_tables_stream`) via `batchExportJob` (`exportSingleTableViaRust`
 * for table/SQL sources, `exportProvidedRowsViaRust` for already-loaded
 * rows). The only frontend XLSX writer left is the import side
 * (`importData.parseXLSX`); the SheetJS export branch is gone.
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
