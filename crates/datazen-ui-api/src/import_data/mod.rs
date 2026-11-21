mod csv;
mod encoding;
mod json;
mod sql;
mod tests;
mod types;
mod xlsx;

pub use csv::{dedupe_columns, detect_csv_delimiter, parse_csv_import};
pub use encoding::{decode_import_base64, decode_import_text, ImportTextDecoder, Utf8TextDecoder};
pub use json::parse_json_import;
pub use sql::generate_insert_sql;
pub use types::{
    detect_import_format, is_supported_encoding, CsvParseOptions, ImportDiagnostic,
    ImportEmptyFieldPolicy, ImportFormat, ImportParseError, ImportParseErrorKind,
    ImportParseResult, JsonParseOptions, ParsedData, XlsxParseOptions, XlsxParseResult,
    IMPORT_ENCODINGS,
};

pub fn parse_import_data(
    content: &str,
    format: ImportFormat,
    csv_options: &CsvParseOptions,
) -> Result<ParsedData, ImportParseError> {
    let result = match format {
        ImportFormat::Csv => parse_csv_import(content, csv_options)?,
        ImportFormat::Json => parse_json_import(content)?,
        ImportFormat::Xlsx => {
            return Err(ImportParseError::new(
                ImportParseErrorKind::UnsupportedSheet,
                "use parse_xlsx_import for XLSX payloads",
            ));
        }
    };
    Ok(ParsedData {
        columns: result.columns,
        rows: result.rows,
    })
}
pub use xlsx::{parse_xlsx, parse_xlsx_import, XlsxBookReader, XlsxWorkbook};
