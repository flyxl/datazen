use crate::ExportDataFormat;

pub fn format_extension(format: ExportDataFormat) -> &'static str {
    match format {
        ExportDataFormat::Csv => "csv",
        ExportDataFormat::Tsv => "tsv",
        ExportDataFormat::Json => "json",
        ExportDataFormat::Markdown => "md",
        ExportDataFormat::SqlInsert | ExportDataFormat::SqlUpdate => "sql",
        ExportDataFormat::Xlsx => "xlsx",
        ExportDataFormat::Parquet => "parquet",
    }
}

pub fn build_provided_export_filename(table_name: &str, format: ExportDataFormat) -> String {
    format!("{}.{}", table_name, format_extension(format))
}

pub fn text_mime_type(format: ExportDataFormat) -> Option<&'static str> {
    match format {
        ExportDataFormat::Csv => Some("text/csv"),
        ExportDataFormat::Tsv => Some("text/tab-separated-values"),
        ExportDataFormat::Json => Some("application/json"),
        ExportDataFormat::Markdown => Some("text/markdown"),
        ExportDataFormat::SqlInsert | ExportDataFormat::SqlUpdate => Some("text/sql"),
        ExportDataFormat::Xlsx | ExportDataFormat::Parquet => None,
    }
}
