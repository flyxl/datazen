//! Decoding what a command actually returned.
//!
//! The rule this file exists to enforce: **an unrecognised payload is an
//! explicit error, never an empty success.** Decoding it as "no rows,
//! succeeded" is the shell behaviour the contract forbids — a caller would see a
//! completed read that returned nothing and could not tell the difference from a
//! real empty result set.
//!
//! Two shapes are recognised, both produced by
//! `execute_standard_sql_command` (`driver-api/src/traits.rs:1120`):
//!
//! * a `MultiQueryResult` — one `ResultChunk` per statement result;
//! * a `{"rowsAffected": n}` object — one chunk, no rows, produced by the
//!   `execute` command.
//!
//! Anything else is refused by *shape*, never by content: the error names how
//! many keys the object had or how many items the array held, which says what
//! arrived without echoing a single value out of it.

use datazen_driver_api::resource::{ResourceError, ResultChunk};
use datazen_driver_api::{MultiQueryResult, StatementResult};

use super::capabilities::VECTOR_PROVIDER_ID;

/// Decode one command's payload into statements and row chunks.
///
/// Both vectors are returned because the caller needs the statements for the
/// completion record and the chunks for the sink, and both come from one
/// decode — decoding twice would be a chance for the two views to disagree.
pub fn decode_command_result(
    value: &serde_json::Value,
) -> Result<(Vec<StatementResult>, Vec<ResultChunk>), ResourceError> {
    if let Ok(multi) = serde_json::from_value::<MultiQueryResult>(value.clone()) {
        let chunks = multi
            .results
            .iter()
            .enumerate()
            .map(|(index, result)| ResultChunk {
                statement_index: index,
                sql: result.sql.clone(),
                rows: result.rows.clone(),
                rows_affected: result.rows_affected,
            })
            .collect();
        return Ok((multi.results, chunks));
    }

    if let Some(affected) = single_rows_affected(value) {
        return Ok((
            Vec::new(),
            vec![ResultChunk {
                statement_index: 0,
                sql: String::new(),
                rows: Vec::new(),
                rows_affected: Some(affected),
            }],
        ));
    }

    Err(ResourceError::OperationNotSupported {
        driver: VECTOR_PROVIDER_ID.to_string(),
        operation: "execute_on_resource".to_string(),
        reason: format!(
            "the command returned a payload this provider cannot decode: {}. Reporting it as an \
             empty success would be indistinguishable from a read that really returned no rows",
            describe_shape(value)
        ),
    })
}

/// `Some(n)` only for the exact `{"rowsAffected": n}` shape, so a payload that
/// merely *mentions* the key cannot be mistaken for it.
fn single_rows_affected(value: &serde_json::Value) -> Option<u64> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object.get("rowsAffected").and_then(|v| v.as_u64())
}

/// What the payload looked like, without saying what was in it.
fn describe_shape(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("an object with {} key(s) [{}]", keys.len(), keys.join(", "))
        }
        serde_json::Value::Array(items) => format!("an array of {} item(s)", items.len()),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::String(_) => "a bare string".to_string(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => "a bare scalar".to_string(),
    }
}
