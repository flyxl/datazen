use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::filename::{format_extension, text_mime_type};
use super::types::*;
use crate::data_table_context_menu::SerializeFormat;
use crate::serialize_cells;
use crate::ExportDataFormat;

const BACKTICK_DIALECTS: &[&str] = &[
    "mysql",
    "mariadb",
    "doris",
    "starrocks",
    "manticore",
    "ob_oracle",
    "kiwi",
];

const NO_QUOTE_DIALECTS: &[&str] = &[
    "elasticsearch",
    "hbase",
    "influxdb",
    "mongodb",
    "redis",
    "vector",
    "victoriametrics",
];

const MYSQL_ESCAPE_DIALECTS: &[&str] = &[
    "mysql",
    "mariadb",
    "doris",
    "starrocks",
    "manticore",
    "ob_oracle",
];

fn require_columns(columns: &[ExportColumn]) -> Result<Vec<String>, ExportFormatError> {
    if columns.is_empty() {
        return Err(ExportFormatError::NoColumns);
    }
    Ok(columns.iter().map(|column| column.name.clone()).collect())
}

fn cell_at(row: &[Value], index: usize) -> Value {
    row.get(index).cloned().unwrap_or(Value::Null)
}

pub fn sql_quote_char(database_type: Option<&str>) -> &'static str {
    match database_type {
        None => "\"",
        Some(db) if BACKTICK_DIALECTS.contains(&db) => "`",
        Some("sqlserver") => "[",
        Some(db) if NO_QUOTE_DIALECTS.contains(&db) => "",
        Some(_) => "\"",
    }
}

pub fn escape_sql_ident(name: &str, database_type: Option<&str>) -> String {
    match sql_quote_char(database_type) {
        "`" => format!("`{}`", name.replace('`', "``")),
        "\"" => format!("\"{}\"", name.replace('"', "\"\"")),
        "[" => format!("[{}]", name.replace(']', "]]")),
        _ => name.to_owned(),
    }
}

fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn quoted_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub fn default_escape_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.to_owned(),
        Value::String(text) => quoted_literal(text),
        Value::Array(_) | Value::Object(_) => quoted_literal(&json_text(value)),
    }
}

pub fn mysql_escape_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.to_owned(),
        Value::String(text) => format!(
            "'{}'",
            text.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\'', "''")
        ),
        Value::Array(_) | Value::Object(_) => format!(
            "'{}'",
            json_text(value)
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\'', "''")
        ),
    }
}

pub fn escape_sql_literal(value: &Value, database_type: Option<&str>) -> String {
    if database_type.is_some_and(|db| MYSQL_ESCAPE_DIALECTS.contains(&db)) {
        mysql_escape_sql_literal(value)
    } else {
        default_escape_sql_literal(value)
    }
}

fn sql_begin_transaction(database_type: Option<&str>) -> &'static str {
    if database_type == Some("sqlserver") {
        "BEGIN TRANSACTION;"
    } else {
        "BEGIN;"
    }
}

fn sql_commit_transaction() -> &'static str {
    "COMMIT;"
}

fn format_sql_insert_header(
    table_name: &str,
    cols: &[String],
    database_type: Option<&str>,
) -> String {
    let col_list = cols
        .iter()
        .map(|col| escape_sql_ident(col, database_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({}) VALUES",
        escape_sql_ident(table_name, database_type),
        col_list
    )
}

fn format_sql_insert_tuple(row: &[Value], cols: &[String], database_type: Option<&str>) -> String {
    let values = cols
        .iter()
        .enumerate()
        .map(|(index, _)| escape_sql_literal(&cell_at(row, index), database_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({values})")
}

pub fn format_sql_insert_script(
    table_name: &str,
    cols: &[String],
    rows: &[Vec<Value>],
    database_type: Option<&str>,
    batch_size: usize,
) -> String {
    let mut lines = vec![sql_begin_transaction(database_type).to_owned()];
    if rows.is_empty() {
        lines.push(sql_commit_transaction().to_owned());
        return lines.join("\n");
    }
    let header = format_sql_insert_header(table_name, cols, database_type);
    let size = batch_size.max(1);
    for chunk in rows.chunks(size) {
        let tuples = chunk
            .iter()
            .map(|row| format!("  {}", format_sql_insert_tuple(row, cols, database_type)))
            .collect::<Vec<_>>()
            .join(",\n");
        lines.push(format!("{header}\n{tuples};"));
    }
    lines.push(sql_commit_transaction().to_owned());
    lines.join("\n")
}

pub fn update_key_column(
    cols: &[String],
    columns: &[ExportColumn],
    primary_key: Option<&str>,
) -> Option<String> {
    if let Some(key) = primary_key {
        if cols.iter().any(|col| col == key) {
            return Some(key.to_owned());
        }
    }
    columns
        .iter()
        .find(|column| column.is_primary_key)
        .map(|column| column.name.clone())
        .or_else(|| cols.first().cloned())
}

pub fn format_sql_update_script(
    table_name: &str,
    columns: &[ExportColumn],
    rows: &[Vec<Value>],
    database_type: Option<&str>,
    primary_key: Option<&str>,
) -> Result<String, ExportFormatError> {
    let cols = require_columns(columns)?;
    let pk_name =
        update_key_column(&cols, columns, primary_key).ok_or(ExportFormatError::NoColumns)?;
    let pk_index = cols.iter().position(|col| col == &pk_name).unwrap_or(0);
    let set_indexes: Vec<usize> = (0..cols.len()).filter(|index| *index != pk_index).collect();
    if set_indexes.is_empty() {
        return Err(ExportFormatError::SqlUpdateNeedsNonKeyColumn);
    }
    let mut lines = vec![sql_begin_transaction(database_type).to_owned()];
    for row in rows {
        let set_clauses = set_indexes
            .iter()
            .map(|index| {
                format!(
                    "{} = {}",
                    escape_sql_ident(&cols[*index], database_type),
                    escape_sql_literal(&cell_at(row, *index), database_type)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let where_clause = format!(
            "{} = {}",
            escape_sql_ident(&pk_name, database_type),
            escape_sql_literal(&cell_at(row, pk_index), database_type)
        );
        lines.push(format!(
            "UPDATE {} SET {} WHERE {};",
            escape_sql_ident(table_name, database_type),
            set_clauses,
            where_clause
        ));
    }
    lines.push(sql_commit_transaction().to_owned());
    Ok(lines.join("\n"))
}

fn to_json_value(value: &Value) -> Value {
    value.clone()
}

fn rows_as_records(cols: &[String], rows: &[Vec<Value>]) -> Vec<Map<String, Value>> {
    rows.iter()
        .map(|row| {
            let mut record = Map::new();
            for (index, col) in cols.iter().enumerate() {
                record.insert(col.clone(), to_json_value(&cell_at(row, index)));
            }
            record
        })
        .collect()
}

fn escape_tsv_field(value: &Value) -> String {
    if matches!(value, Value::Null) {
        return String::new();
    }
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => json_text(value),
        other => other.to_string(),
    };
    text.replace('\t', " ").replace('\n', " ").replace('\r', "")
}

pub fn apply_csv_formula_guard(escaped: &str) -> String {
    if escaped.starts_with(|ch| matches!(ch, '=' | '+' | '-' | '@')) && !escaped.starts_with('"') {
        format!("\"{escaped}\"")
    } else {
        escaped.to_owned()
    }
}

fn csv_field(value: &Value) -> String {
    serialize_cells(&[vec![value.clone()]], SerializeFormat::Csv, &[], false)
}

pub fn format_rows_as_text(
    options: &FormatRowsOptions,
    format: ExportDataFormat,
) -> Result<TextExportResult, ExportFormatError> {
    let cols = require_columns(&options.columns)?;
    let extension = format_extension(format).to_owned();
    let mime_type = text_mime_type(format)
        .ok_or(ExportFormatError::BinaryFormatNotText(format))?
        .to_owned();
    let content = match format {
        ExportDataFormat::Csv => {
            let header = cols
                .iter()
                .map(|col| apply_csv_formula_guard(&csv_field(&Value::String(col.clone()))))
                .collect::<Vec<_>>()
                .join(",");
            let body = options
                .rows
                .iter()
                .map(|row| {
                    cols.iter()
                        .enumerate()
                        .map(|(index, _)| apply_csv_formula_guard(&csv_field(&cell_at(row, index))))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect::<Vec<_>>();
            [header, body.join("\n")].join("\n")
        }
        ExportDataFormat::Tsv => {
            let header = cols
                .iter()
                .map(|col| escape_tsv_field(&Value::String(col.clone())))
                .collect::<Vec<_>>()
                .join("\t");
            let body = options
                .rows
                .iter()
                .map(|row| {
                    (0..cols.len())
                        .map(|index| escape_tsv_field(&cell_at(row, index)))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>();
            [header, body.join("\n")].join("\n")
        }
        ExportDataFormat::Json => {
            let payload = rows_as_records(&cols, &options.rows);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_owned())
        }
        ExportDataFormat::Markdown => {
            serialize_cells(&options.rows, SerializeFormat::Markdown, &cols, false)
        }
        ExportDataFormat::SqlInsert => format_sql_insert_script(
            &options.table_name,
            &cols,
            &options.rows,
            options.database_type.as_deref(),
            SQL_INSERT_BATCH_SIZE,
        ),
        ExportDataFormat::SqlUpdate => format_sql_update_script(
            &options.table_name,
            &options.columns,
            &options.rows,
            options.database_type.as_deref(),
            options.primary_key.as_deref(),
        )?,
        ExportDataFormat::Xlsx | ExportDataFormat::Parquet => {
            return Err(ExportFormatError::BinaryFormatNotText(format));
        }
    };
    Ok(TextExportResult {
        content,
        extension,
        mime_type,
    })
}

pub fn to_xlsx_cell(value: &Value) -> XlsxCellValue {
    match value {
        Value::Null => XlsxCellValue::Blank,
        Value::Bool(flag) => XlsxCellValue::Bool(*flag),
        Value::Number(number) => XlsxCellValue::Number(number.to_string()),
        Value::String(text) => XlsxCellValue::Text(text.clone()),
        Value::Array(_) | Value::Object(_) => XlsxCellValue::ObjectJson(json_text(value)),
    }
}

pub fn bytes_to_xlsx_cell(bytes: &[u8]) -> XlsxCellValue {
    XlsxCellValue::BytesHex(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn xlsx_sheet_rows(
    options: &FormatRowsOptions,
) -> Result<Vec<BTreeMap<String, XlsxCellValue>>, ExportFormatError> {
    let cols = require_columns(&options.columns)?;
    Ok(options
        .rows
        .iter()
        .map(|row| {
            let mut record = BTreeMap::new();
            for (index, col) in cols.iter().enumerate() {
                record.insert(col.clone(), to_xlsx_cell(&cell_at(row, index)));
            }
            record
        })
        .collect())
}

pub fn format_rows_as_xlsx(
    options: &FormatRowsOptions,
    format: Option<ExportDataFormat>,
) -> Result<BinaryExportResult, ExportFormatError> {
    if let Some(format) = format {
        if format != ExportDataFormat::Xlsx {
            return Err(ExportFormatError::XlsxFormatRejected(format));
        }
    }
    let _ = xlsx_sheet_rows(options)?;
    Err(ExportFormatError::XlsxWriterUnavailable)
}
