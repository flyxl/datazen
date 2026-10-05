//! One source statement spooled to a private file, avoiding unstable OFFSET pages.
//! Native streaming drivers keep memory bounded. Legacy materializing drivers
//! retain their documented query_stream memory limitation.
use super::error::TransferError;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value};
use datazen_driver_api::QueryStreamEvent;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// The public IPC Value is untagged; disk rows must preserve its exact variant.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum SpoolValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(u64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(serde_json::Value),
}
impl From<Value> for SpoolValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::Bool(v),
            Value::Integer(v) => Self::Integer(v),
            Value::Float(v) => Self::Float(v.to_bits()),
            Value::String(v) => Self::String(v),
            Value::Bytes(v) => Self::Bytes(v),
            Value::Timestamp(v) => Self::Timestamp(v),
            Value::Json(v) => Self::Json(v),
        }
    }
}
impl From<SpoolValue> for Value {
    fn from(value: SpoolValue) -> Self {
        match value {
            SpoolValue::Null => Self::Null,
            SpoolValue::Bool(v) => Self::Bool(v),
            SpoolValue::Integer(v) => Self::Integer(v),
            SpoolValue::Float(v) => Self::Float(f64::from_bits(v)),
            SpoolValue::String(v) => Self::String(v),
            SpoolValue::Bytes(v) => Self::Bytes(v),
            SpoolValue::Timestamp(v) => Self::Timestamp(v),
            SpoolValue::Json(v) => Self::Json(v),
        }
    }
}

pub struct ScanRows {
    reader: Option<BufReader<std::fs::File>>,
    path: std::path::PathBuf,
}
impl Drop for ScanRows {
    fn drop(&mut self) {
        drop(self.reader.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
impl ScanRows {
    pub fn next_batch(&mut self, limit: usize) -> Result<Vec<Vec<Option<Value>>>, TransferError> {
        let mut rows = Vec::new();
        for _ in 0..limit {
            let mut line = String::new();
            if self
                .reader
                .as_mut()
                .ok_or_else(|| TransferError::validation("source spool is closed"))?
                .read_line(&mut line)
                .map_err(|e| TransferError::validation(e.to_string()))?
                == 0
            {
                break;
            }
            let row: Vec<Option<SpoolValue>> = serde_json::from_str(&line)
                .map_err(|e| TransferError::validation(e.to_string()))?;
            rows.push(row.into_iter().map(|v| v.map(Value::from)).collect());
        }
        Ok(rows)
    }
}

pub async fn scan_rows(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    sql: &str,
    expected_columns: Vec<String>,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<ScanRows, TransferError> {
    scan_rows_with_params(driver, handle, sql, &[], expected_columns, cancelled).await
}

pub async fn scan_rows_with_params(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    sql: &str,
    params: &[Value],
    expected_columns: Vec<String>,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<ScanRows, TransferError> {
    let path = std::env::temp_dir().join(format!("datazen-transfer-{}.rows", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|e| TransferError::validation(e.to_string()))?;
    let mut result = ScanRows {
        reader: Some(BufReader::new(file)),
        path,
    };
    let file = result
        .reader
        .as_ref()
        .ok_or_else(|| TransferError::validation("source spool is closed"))?
        .get_ref()
        .try_clone()
        .map_err(|e| TransferError::validation(e.to_string()))?;
    let state = Arc::new(Mutex::new((
        std::io::BufWriter::new(file),
        None::<String>,
        0u8,
    )));
    let sink = Arc::clone(&state);
    let callback = Arc::new(move |event: QueryStreamEvent| {
        let Ok(mut state) = sink.lock() else {
            return;
        };
        if state.1.is_some() {
            return;
        }
        if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
            state.1 = Some("transfer cancelled during source scan".into());
            return;
        }
        match event {
            QueryStreamEvent::StatementStart { index, columns, .. } => {
                let names: Vec<_> = columns.iter().map(|c| c.name.clone()).collect();
                if state.2 != 0 || index != 0 || names != expected_columns {
                    state.1 = Some("source stream projection or statement count changed".into());
                } else {
                    state.2 = 1;
                }
            }
            QueryStreamEvent::Rows { index, rows } => {
                if state.2 != 1 || index != 0 {
                    state.1 = Some("rows outside source statement".into());
                    return;
                }
                for row in rows {
                    if row.len() != expected_columns.len() {
                        state.1 = Some("source row width differs from projection".into());
                        break;
                    }
                    let row: Vec<Option<SpoolValue>> =
                        row.into_iter().map(|v| v.map(SpoolValue::from)).collect();
                    let write = serde_json::to_writer(&mut state.0, &row)
                        .map_err(|e| e.to_string())
                        .and_then(|_| state.0.write_all(b"\n").map_err(|e| e.to_string()));
                    if let Err(error) = write {
                        state.1 = Some(error);
                        break;
                    }
                }
            }
            QueryStreamEvent::StatementEnd {
                index, truncated, ..
            } => {
                if truncated || index != 0 || state.2 != 1 {
                    state.1 = Some("source stream truncated or invalid statement end".into());
                } else {
                    state.2 = 2;
                }
            }
            _ => {}
        }
    });
    driver
        .query_stream_with_params(handle, sql, params, None, callback)
        .await
        .map_err(|e| TransferError::validation(e.to_string()))?;
    {
        let mut state = state
            .lock()
            .map_err(|_| TransferError::validation("source spool lock failed"))?;
        if state.2 != 2 && state.1.is_none() {
            state.1 = Some("source statement did not complete".into());
        }
        if let Some(error) = state.1.take() {
            return Err(TransferError::validation(error));
        }
        state
            .0
            .flush()
            .map_err(|e| TransferError::validation(e.to_string()))?;
    }
    result
        .reader
        .as_mut()
        .ok_or_else(|| TransferError::validation("source spool is closed"))?
        .seek(SeekFrom::Start(0))
        .map_err(|e| TransferError::validation(e.to_string()))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spool_preserves_variants_and_float_bits() {
        let values = vec![
            None,
            Some(Value::Null),
            Some(Value::Json(serde_json::json!([1, 2]))),
            Some(Value::Json(serde_json::json!("foo"))),
            Some(Value::Json(serde_json::json!(null))),
            Some(Value::Json(serde_json::json!(true))),
            Some(Value::Json(serde_json::json!(123))),
            Some(Value::Bytes(vec![0, 255])),
            Some(Value::Timestamp("2026-09-14".into())),
            Some(Value::String(
                "12345678901234567890.12345678901234567890".into(),
            )),
            Some(Value::Float(f64::NAN)),
            Some(Value::Float(-0.0)),
            Some(Value::Float(f64::INFINITY)),
        ];
        for original in values {
            let encoded = serde_json::to_string(&original.clone().map(SpoolValue::from)).unwrap();
            let decoded: Option<SpoolValue> = serde_json::from_str(&encoded).unwrap();
            let actual = decoded.map(Value::from);
            match (original, actual) {
                (Some(Value::Float(a)), Some(Value::Float(b))) => {
                    assert_eq!(a.to_bits(), b.to_bits())
                }
                (a, b) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
            }
        }
    }
}
