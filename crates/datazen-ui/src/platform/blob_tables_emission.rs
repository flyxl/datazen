//! T043-S2: the emission stage of the web-shaped tables exporter.
//!
//! The second half of [`super::blob_tables_exporter`]: `emit_data_plan`'s
//! two branches, the single/multi plan merge, `deliver`, and the `Saved`
//! return. Split out of `blob_tables_exporter.rs` so neither file exceeds
//! the 800-line recommendation — the `impl BlobTablesExporter` block lives
//! here and reaches the exporter's injected reader / deliver sink through
//! its `pub(crate)` accessors, and the pure helpers through
//! [`super::blob_tables_exporter`]'s `pub(crate)` surface.

#![allow(dead_code)]
// The emission stage is driven by tests today and by T131's port trait
// tomorrow; the lib itself has no caller yet. Same allowance as
// `blob_tables_exporter.rs` (S1).

use serde_json::Value;

use super::blob_tables_exporter::{
    base64_to_bytes, csv_cell, download_blob, BlobTablesExportError, BlobTablesExporter,
    DeliveredContent, EmittedDataPlan, ExportTablesOutcome, PlannedExport,
    TablePageRequestContext,
};
use super::export_format::{format_rows_as_text, format_rows_as_xlsx};
use super::export_plan::{
    ExportDataFormat, ExportFilePlan, ExportTableInput, ExportTableSource, ProvidedColumn,
    TablesExportRequest,
};

// ── S2: emission stage ──────────────────────────────────────────────────

impl BlobTablesExporter {
    /// Emit one planned data file — React's `emitDataPlan`, in React
    /// order. `plan` must be the `Data` variant (React enforces this
    /// with `Extract<ExportFilePlan, { kind: 'data' }>`; a DDL plan
    /// here is a caller bug and fails with [`BlobTablesExportError::NotDataPlan`]).
    ///
    /// * `provided` source — the caller's own rows, formatted in memory
    ///   instead of re-reading the table. CSV goes through the
    ///   [`csv_cell`] emission (header first, one `\n`-terminated line
    ///   per row; no DDL prefix — `build_file_plans` only ever prefixes
    ///   DDL to a SQL-format data plan, so a CSV plan never carries
    ///   one); XLSX through [`format_rows_as_xlsx`] + [`base64_to_bytes`]
    ///   (a DDL prefix on a workbook is a planner bug and fails);
    ///   Parquet is refused; every other format through
    ///   [`format_rows_as_text`], with a DDL prefix joined as
    ///   `{prefix}\n\n{content}` when the plan carries one.
    /// * table source — page until the reader says the table is
    ///   exhausted; the first page contributes the header line, every
    ///   page contributes its rows as [`csv_cell`] lines, so peak memory
    ///   is one page. Each page call overrides the request context's
    ///   schema with the table's own. A `ddlPrefix` rides first as
    ///   `{prefix}\n\n`.
    pub(crate) async fn emit_data_plan(
        &self,
        planned: &PlannedExport,
        plan: &ExportFilePlan,
    ) -> Result<EmittedDataPlan, BlobTablesExportError> {
        let ExportFilePlan::Data {
            table, ddl_prefix, ..
        } = plan
        else {
            return Err(BlobTablesExportError::NotDataPlan);
        };
        if let ExportTableSource::Provided { columns, rows } = &table.source {
            return Self::emit_provided_data(table, columns, rows, ddl_prefix.as_deref(), planned);
        }
        self.emit_table_data(table, ddl_prefix.as_deref(), planned)
            .await
    }

    /// The `provided` branch of [`BlobTablesExporter::emit_data_plan`].
    /// Pure except for the XLSX workbook build (which allocates the
    /// workbook in memory).
    fn emit_provided_data(
        table: &ExportTableInput,
        columns: &[ProvidedColumn],
        rows: &[Vec<Value>],
        ddl_prefix: Option<&str>,
        planned: &PlannedExport,
    ) -> Result<EmittedDataPlan, BlobTablesExportError> {
        let data_format = planned.request.data_format;
        match data_format {
            ExportDataFormat::Csv => {
                // No DDL prefix here: `build_file_plans` only ever
                // prefixes DDL to a SQL-format data plan, so a CSV plan
                // never carries one.
                let mut chunks = String::new();
                let header: Vec<String> = columns
                    .iter()
                    .map(|column| csv_cell(&Value::String(column.name.clone())))
                    .collect();
                chunks.push_str(&header.join(","));
                chunks.push('\n');
                for row in rows {
                    let cells: Vec<String> = columns
                        .iter()
                        .enumerate()
                        .map(|(i, _)| csv_cell(row.get(i).unwrap_or(&Value::Null)))
                        .collect();
                    chunks.push_str(&cells.join(","));
                    chunks.push('\n');
                }
                Ok(EmittedDataPlan {
                    content: DeliveredContent::Text(chunks),
                    mime_type: "text/csv".to_owned(),
                })
            }
            ExportDataFormat::Xlsx => {
                // A DDL prefix cannot ride a workbook; it only ever
                // accompanies a SQL data format, and a SQL data plan is
                // text — reaching here with one would be a planner bug,
                // not a user error.
                if ddl_prefix.is_some() {
                    return Err(BlobTablesExportError::DdlPrefixOnXlsx);
                }
                let xlsx = format_rows_as_xlsx(columns, rows)?;
                Ok(EmittedDataPlan {
                    content: DeliveredContent::Bytes(base64_to_bytes(&xlsx.data_base64)?),
                    mime_type: xlsx.mime_type.to_owned(),
                })
            }
            ExportDataFormat::Parquet => {
                // Provided-data Parquet is a PRD non-goal (the dialogs
                // grey it out for these scopes); a smuggled request
                // fails loudly instead of silently rendering a
                // wrong-format file.
                Err(BlobTablesExportError::ProvidedParquetUnsupported)
            }
            _ => {
                let formatted = format_rows_as_text(
                    &table.table_name,
                    columns,
                    rows,
                    data_format,
                    planned.request.database_type.as_deref(),
                    table.primary_key.as_deref(),
                )?;
                let content = match ddl_prefix {
                    Some(prefix) => format!("{prefix}\n\n{}", formatted.content),
                    None => formatted.content,
                };
                Ok(EmittedDataPlan {
                    content: DeliveredContent::Text(content),
                    mime_type: formatted.mime_type.to_owned(),
                })
            }
        }
    }

    /// The table branch of [`BlobTablesExporter::emit_data_plan`]: page
    /// until the reader says the table is exhausted. `Some(Value::Null)`
    /// counts as exhausted, exactly as React's `cursor === null` break
    /// (see [`super::blob_tables_exporter::TablePage::next_cursor`]).
    async fn emit_table_data(
        &self,
        table: &ExportTableInput,
        ddl_prefix: Option<&str>,
        planned: &PlannedExport,
    ) -> Result<EmittedDataPlan, BlobTablesExportError> {
        let mut chunks = String::new();
        if let Some(prefix) = ddl_prefix {
            chunks.push_str(prefix);
            chunks.push_str("\n\n");
        }
        let mut cursor: Option<Value> = None;
        let mut first = true;
        // Page until the reader says the table is exhausted; each page
        // is appended so peak memory is one page, mirroring the
        // streaming form.
        loop {
            let page = self
                .reader()
                .read_page(
                    &table.table_name,
                    cursor.as_ref(),
                    &TablePageRequestContext {
                        schema: table
                            .schema
                            .clone()
                            .or_else(|| planned.request_context.schema.clone()),
                        ..planned.request_context.clone()
                    },
                )
                .await?;
            if first {
                let header: Vec<String> =
                    page.columns.iter().map(|col| csv_cell(&Value::String(col.clone()))).collect();
                chunks.push_str(&header.join(","));
                chunks.push('\n');
                first = false;
            }
            for row in &page.rows {
                let cells: Vec<String> =
                    row.iter().map(csv_cell).collect();
                chunks.push_str(&cells.join(","));
                chunks.push('\n');
            }
            cursor = page.next_cursor;
            if matches!(cursor, None | Some(Value::Null)) {
                break;
            }
        }
        Ok(EmittedDataPlan {
            content: DeliveredContent::Text(chunks),
            mime_type: "text/csv".to_owned(),
        })
    }

    /// Hand one emitted download to the injected sink, or fail loudly
    /// when no sink was wired (there is no Rune default: React's
    /// default *is* the real Blob download, which cannot run here).
    fn deliver(
        &self,
        content: DeliveredContent,
        file_name: &str,
        mime_type: &str,
    ) -> Result<(), BlobTablesExportError> {
        let Some(deliver) = self.deliver_sink() else {
            return Err(BlobTablesExportError::NoDownloadSink);
        };
        deliver(&download_blob(content, file_name, mime_type));
        Ok(())
    }

    /// Emit a planned export and deliver the single download — the tail
    /// of React's `exportTables`, split out so tests can drive the merge
    /// with hand-built plans (see the module docs).
    ///
    /// A single data plan delivers as-is (binary for XLSX, text for the
    /// rest) with its own MIME type. Several plans only reach here when
    /// they are all `.sql` — anything else needed a zip and was refused
    /// in [`BlobTablesExporter::plan_export`] — so the merge below is
    /// text-only by construction, and a binary plan in it is refused.
    pub(crate) async fn emit_planned(
        &self,
        planned: &PlannedExport,
    ) -> Result<ExportTablesOutcome, BlobTablesExportError> {
        if planned.plans.len() == 1 {
            if let Some(ExportFilePlan::Data { .. }) = planned.plans.first() {
                let emitted = self.emit_data_plan(planned, &planned.plans[0]).await?;
                self.deliver(emitted.content, &planned.file_name, &emitted.mime_type)?;
                return Ok(ExportTablesOutcome::Saved(planned.request.tables.len()));
            }
        }
        let multi = planned.plans.len() > 1;
        let mut chunks = String::new();
        // The desktop single-file merge: one plan is written directly,
        // several plans are concatenated with
        // `-- ===== {filename} =====` separators. (Several plans only
        // reach here when they are all `.sql` — anything else needed a
        // zip and was refused above.)
        for plan in &planned.plans {
            if multi {
                chunks.push_str(&format!("-- ===== {} =====\n\n", plan.filename()));
            }
            match plan {
                ExportFilePlan::Ddl { text, .. } => chunks.push_str(text),
                ExportFilePlan::Data { .. } => {
                    let emitted = self.emit_data_plan(planned, plan).await?;
                    let DeliveredContent::Text(text) = emitted.content else {
                        return Err(BlobTablesExportError::BinaryCannotMerge);
                    };
                    chunks.push_str(&text);
                }
            }
            if multi {
                chunks.push_str("\n\n");
            }
        }
        // Pinned (S1 tester finding (c)): React's merge path calls
        // `deliver(chunks, fileName)` with no mime, so `downloadBlob`'s
        // default `text/csv` applies; `download_blob` has no default,
        // so the merge passes `"text/csv"` explicitly here.
        self.deliver(
            DeliveredContent::Text(chunks),
            &planned.file_name,
            "text/csv",
        )?;
        Ok(ExportTablesOutcome::Saved(planned.request.tables.len()))
    }

    /// The full `exportTables(request)`: capability gates + DDL
    /// resolution + shared planning ([`BlobTablesExporter::plan_export`])
    /// and then the emission stage ([`BlobTablesExporter::emit_planned`]).
    pub async fn export_tables(
        &self,
        request: TablesExportRequest,
    ) -> Result<ExportTablesOutcome, BlobTablesExportError> {
        let planned = self.plan_export(request).await?;
        self.emit_planned(&planned).await
    }
}
