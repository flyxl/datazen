use super::{DataFormat, SQL_INSERT_BATCH_SIZE};
use datazen_driver_api::{DatabaseDriver, LegacySqlDialect, SqlDialect, Value};
use std::sync::Arc;
// ---------------------------------------------------------------------------
// Pure value/identifier escaping (mirrors src/lib/exportData.ts)
// ---------------------------------------------------------------------------

pub fn fmt_ident(name: &str, db_type: Option<&str>) -> String {
    let quote = match db_type.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("mysql") | Some("mariadb") => '`',
        _ => '"',
    };
    format!(
        "{quote}{}{quote}",
        name.replace(quote, &format!("{quote}{quote}"))
    )
}

const CSV_BYTES_PREFIX: &str = "datazen:bytes:hex:";
const CSV_TEXT_PREFIX: &str = "datazen:text:";

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn escape_reserved_csv_text(value: &str) -> String {
    if value.starts_with(CSV_BYTES_PREFIX) || value.starts_with(CSV_TEXT_PREFIX) {
        format!("{CSV_TEXT_PREFIX}{value}")
    } else {
        value.to_string()
    }
}

pub fn value_as_string(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(true)) => "true".into(),
        Some(Value::Bool(false)) => "false".into(),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Float(n)) => n.to_string(),
        Some(Value::String(s)) => escape_reserved_csv_text(s),
        Some(Value::Bytes(b)) => format!("{CSV_BYTES_PREFIX}{}", bytes_to_hex(b)),
        Some(Value::Timestamp(s)) => escape_reserved_csv_text(s),
        Some(Value::Json(j)) => j.to_string(),
    }
}

pub fn escape_csv_field(value: &Option<Value>) -> String {
    let s = value_as_string(value);
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        return format!("\"{}\"", s.replace('"', "\"\""));
    }
    s
}

pub fn is_bytes_json_marker(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("$datazenType"))
        .and_then(serde_json::Value::as_str)
        == Some("bytes")
}

pub fn json_value(value: Option<Value>) -> serde_json::Value {
    match value {
        None | Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(b)) => serde_json::Value::Bool(b),
        Some(Value::Integer(n)) => serde_json::Value::from(n),
        Some(Value::Float(n)) => serde_json::Value::from(n),
        Some(Value::String(s)) => serde_json::Value::String(s),
        Some(Value::Bytes(b)) => serde_json::json!({
            "$datazenType": "bytes",
            "encoding": "hex",
            "value": bytes_to_hex(&b),
        }),
        Some(Value::Timestamp(s)) => serde_json::Value::String(s),
        Some(Value::Json(j)) if is_bytes_json_marker(&j) => serde_json::json!({
            "$datazenType": "json",
            "value": j,
        }),
        Some(Value::Json(j)) => j,
    }
}

/// Build the SELECT SQL for a table export.
pub fn build_select_sql(table: &str, columns: &[String], db_type: Option<&str>) -> String {
    let qtable = fmt_ident(table, db_type);
    if columns.is_empty() {
        return format!("SELECT * FROM {qtable}");
    }
    let cols = columns
        .iter()
        .map(|c| fmt_ident(c, db_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("SELECT {cols} FROM {qtable}")
}

// ---------------------------------------------------------------------------
// Streamable text formatter
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum FormatterState {
    Csv,
    Json { started: bool },
    SqlInsert { pending: Vec<Vec<Option<Value>>> },
}

/// Formats row batches to text, preserving streaming state across calls.
pub struct StreamFormatter {
    format: DataFormat,
    table: String,
    database_type: Option<String>,
    driver: Arc<dyn DatabaseDriver>,
    state: FormatterState,
}

impl StreamFormatter {
    pub fn new(
        format: DataFormat,
        table: String,
        database_type: Option<String>,
        driver: Arc<dyn DatabaseDriver>,
    ) -> Self {
        let state = match format {
            DataFormat::Csv => FormatterState::Csv,
            DataFormat::Json => FormatterState::Json { started: false },
            DataFormat::SqlInsert => FormatterState::SqlInsert {
                pending: Vec::new(),
            },
        };
        StreamFormatter {
            format,
            table,
            database_type,
            driver,
            state,
        }
    }

    /// Text to write at the very start of a table (JSON `[`, SQL `BEGIN`).
    pub fn header(&mut self, columns: &[String]) -> String {
        match self.format {
            DataFormat::Json => "[".into(),
            DataFormat::SqlInsert => {
                let re = if matches!(self.database_type.as_deref(), Some("sqlserver")) {
                    "BEGIN TRANSACTION;"
                } else {
                    "BEGIN;"
                };
                format!("{re}\n")
            }
            DataFormat::Csv => {
                let mut out = String::new();
                for (i, c) in columns.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&escape_csv_field(&Some(Value::String(c.clone()))));
                }
                out.push('\n');
                out
            }
        }
    }

    /// Format a batch of rows into text.
    pub fn rows(
        &mut self,
        rows: &[Vec<Option<Value>>],
        columns: &[String],
    ) -> Result<String, datazen_driver_api::DriverError> {
        match &mut self.state {
            FormatterState::Csv => {
                let mut out = String::new();
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str(&escape_csv_field(cell));
                    }
                    out.push('\n');
                }
                Ok(out)
            }
            FormatterState::Json { started } => {
                let mut out = String::new();
                for row in rows {
                    let mut map = serde_json::Map::new();
                    for (i, col) in columns.iter().enumerate() {
                        map.insert(col.clone(), json_value(row.get(i).and_then(|v| v.clone())));
                    }
                    let obj = serde_json::to_string(&serde_json::Value::Object(map))
                        .unwrap_or_else(|_| "null".into());
                    if *started {
                        out.push(',');
                    }
                    *started = true;
                    out.push('\n');
                    out.push_str(&obj);
                }
                Ok(out)
            }
            FormatterState::SqlInsert { pending } => {
                pending.extend(rows.iter().cloned());
                self.flush_sql(columns)
            }
        }
    }

    pub fn flush_sql(
        &mut self,
        columns: &[String],
    ) -> Result<String, datazen_driver_api::DriverError> {
        let FormatterState::SqlInsert { pending } = &mut self.state else {
            return Ok(String::new());
        };
        if pending.is_empty() {
            return Ok(String::new());
        }
        let mut out = String::new();
        // 字面量渲染来自 SqlDialect 契约，旧驱动 API 逐条转发。
        let driver: &dyn DatabaseDriver = self.driver.as_ref();
        let dialect = LegacySqlDialect::new(driver);
        let mut batch: Vec<Vec<Option<Value>>> = Vec::new();
        for row in pending.drain(..) {
            batch.push(row);
            if batch.len() >= SQL_INSERT_BATCH_SIZE {
                out.push_str(&emit_insert_batch(
                    columns,
                    &self.table,
                    &self.database_type,
                    &dialect,
                    &batch,
                )?);
                batch.clear();
            }
        }
        if !batch.is_empty() {
            out.push_str(&emit_insert_batch(
                columns,
                &self.table,
                &self.database_type,
                &dialect,
                &batch,
            )?);
        }
        Ok(out)
    }

    /// Text to write at the very end of a table dump (JSON `]`, SQL `COMMIT`).
    pub fn tail(&mut self, columns: &[String]) -> Result<String, datazen_driver_api::DriverError> {
        match self.format {
            DataFormat::Json => Ok({
                let FormatterState::Json { started } = &mut self.state else {
                    return Ok(String::new());
                };
                if *started {
                    "\n]".into()
                } else {
                    "]".into()
                }
            }),
            DataFormat::SqlInsert => {
                let batched = self.flush_sql(columns);
                Ok(format!("{}COMMIT;\n", batched?))
            }
            DataFormat::Csv => Ok(String::new()),
        }
    }
}

pub fn emit_insert_batch(
    columns: &[String],
    table: &str,
    database_type: &Option<String>,
    dialect: &dyn SqlDialect,
    rows: &[Vec<Option<Value>>],
) -> Result<String, datazen_driver_api::DriverError> {
    let db_type = database_type.as_deref();
    let col_list = columns
        .iter()
        .map(|c| fmt_ident(c, db_type))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!(
        "INSERT INTO {} ({col_list}) VALUES\n",
        fmt_ident(table, db_type)
    );
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n  ");
        } else {
            out.push_str("  ");
        }
        let values = row
            .iter()
            .map(|value| dialect.try_format_sql_literal(value))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        out.push_str(&format!("({values})"));
    }
    out.push_str(";\n");
    Ok(out)
}
