//! Save one complete, previously published native result. This path never executes SQL.
use datazen::shared::id::{ArtifactId, Counter};
use datazen_driver_api::resource::ResultChunk;
use datazen_ui_api::{
    apply_csv_formula_guard, serialize_cells, NativeQueryCsvSource, SerializeFormat,
};
use std::path::Path;

use super::RuntimeConnectionUseCases;

const UNAVAILABLE: &str = "The retained query result is no longer available.";
const TOO_LARGE: &str = "This retained query result is too large to save as CSV.";
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) async fn save(
    runtime: &RuntimeConnectionUseCases,
    source: NativeQueryCsvSource,
    destination: &Path,
) -> Result<(), String> {
    if !destination.is_absolute()
        || !destination
            .extension()
            .and_then(|suffix| suffix.to_str())
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case("csv"))
    {
        return Err("Choose a new .csv file.".into());
    }
    let context = crate::host_request_context();
    let artifact_id = ArtifactId::new(source.artifact_id);
    let first = runtime
        .read_artifact_chunk(context, artifact_id.clone(), Counter::ZERO)
        .await
        .map_err(|_| UNAVAILABLE)?;
    if !first.complete || first.published_chunk_count.get() == 0 {
        return Err(UNAVAILABLE.into());
    }
    if first.published_byte_length.get() > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    let mut has_columns = false;
    let mut rows = Vec::new();
    for index in 0..first.published_chunk_count.get() {
        let chunk = if index == 0 {
            first.clone()
        } else {
            runtime
                .read_artifact_chunk(context, artifact_id.clone(), Counter::new(index))
                .await
                .map_err(|_| UNAVAILABLE)?
        };
        if !chunk.complete || chunk.source.execution_id.as_str() != source.execution_id {
            return Err(UNAVAILABLE.into());
        }
        if chunk.source.statement_index != source.statement_index {
            continue;
        }
        let Some(output) = chunk.output else {
            continue;
        };
        if output["kind"] == "statementMetadata" {
            let columns = output["columns"].as_array().ok_or(UNAVAILABLE)?;
            if columns.len() != source.columns.len()
                || !columns
                    .iter()
                    .zip(&source.columns)
                    .all(|(column, name)| column["name"].as_str() == Some(name.as_str()))
            {
                return Err(UNAVAILABLE.into());
            }
            has_columns = true;
        } else if let Ok(chunk) = serde_json::from_value::<ResultChunk>(output) {
            if chunk.statement_index != source.statement_index as usize {
                return Err(UNAVAILABLE.into());
            }
            for row in chunk.rows {
                if row.len() != source.columns.len() {
                    return Err(UNAVAILABLE.into());
                }
                rows.push(
                    row.into_iter()
                        .map(|cell| {
                            cell.map(serde_json::to_value)
                                .transpose()
                                .map(|cell| cell.unwrap_or_default())
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| UNAVAILABLE)?,
                );
                if rows.len() > source.row_count as usize {
                    return Err(UNAVAILABLE.into());
                }
            }
        }
    }
    if !has_columns || rows.len() != source.row_count as usize {
        return Err(UNAVAILABLE.into());
    }
    let report = retained_csv(&source.columns, rows);
    if report.len() as u64 > MAX_BYTES {
        return Err(TOO_LARGE.into());
    }
    crate::jobs::save_new_file(destination, &report)
        .map_err(|_| "Could not save the result. Choose a new writable .csv file.".into())
}

fn retained_csv(columns: &[String], rows: Vec<Vec<serde_json::Value>>) -> String {
    let mut result = String::new();
    let header = columns
        .iter()
        .cloned()
        .map(serde_json::Value::String)
        .collect();
    for row in std::iter::once(header).chain(rows) {
        for (index, cell) in row.into_iter().enumerate() {
            if index > 0 {
                result.push(',');
            }
            // CSV's physical representation preserves SQL NULL versus an empty
            // string. Most CSV readers return both as a zero-length field.
            match cell {
                serde_json::Value::Null => {}
                serde_json::Value::String(ref s) if s.is_empty() => result.push_str("\"\""),
                cell => result.push_str(&apply_csv_formula_guard(&serialize_cells(
                    &[vec![cell]],
                    SerializeFormat::Csv,
                    &[],
                    false,
                ))),
            }
        }
        result.push('\n');
    }
    result
}
