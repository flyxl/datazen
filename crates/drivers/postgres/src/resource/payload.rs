//! Turning a Driver Command payload into what the `ResultSink` channel can carry.
//!
//! This is the narrowest possible decoder, and the narrowness is the point. A
//! catalog listing or an admin command answers with free-form JSON that has no
//! truthful representation as statement rows and a `StatementSummary`.
//! Decoding it as "no rows, succeeded" would be the shell behaviour the contract forbids — the caller
//! would believe a command ran and saw nothing, when in fact it saw something
//! this channel cannot express. So an unrecognised payload is an explicit
//! [`ResourceError::OperationNotSupported`], and the caller is told which
//! channel to use instead.
//!
//! Error messages name the payload's *shape* and never its contents: a catalog
//! result carries object names and row data, which do not belong in an error
//! string.

use datazen_driver_api::resource::{ResourceError, ResultChunk, StatementSummary};
use datazen_driver_api::{MultiQueryResult, StatementResult};

use super::capabilities::POSTGRES_PROVIDER_ID;

/// The `execute` acknowledgement produced by `execute_standard_sql_command`.
///
/// Read by hand rather than through a `Deserialize` impl, so the driver crate
/// does not take a `serde` dependency for one field, and so the shape stays
/// exact: a payload is an acknowledgement only when `rowsAffected` is its one
/// key. Any other key means it is somebody else's JSON and it is refused.
fn execute_rows_affected(data: &serde_json::Value) -> Option<u64> {
    let map = data.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get("rowsAffected")?.as_u64()
}

/// Split a Driver Command payload into the statement results and the chunks the
/// sink receives. Empty results are represented only by a typed summary.
///
/// Exactly two shapes are accepted, both produced by
/// `execute_standard_sql_command`: `query` → `MultiQueryResult`, and `execute` →
/// `{"rowsAffected": n}`. Anything else is refused, not emptied.
pub(super) fn decode_command_result(
    data: &serde_json::Value,
    execute_sql: &str,
) -> Result<(Vec<StatementResult>, Vec<StatementOutput>), ResourceError> {
    if let Ok(multi) = serde_json::from_value::<MultiQueryResult>(data.clone()) {
        let mut output = Vec::with_capacity(multi.results.len() * 2);
        for (statement_index, result) in multi.results.iter().enumerate() {
            if !result.rows.is_empty() {
                output.push(StatementOutput::Rows(ResultChunk {
                    statement_index,
                    sql: result.sql.clone(),
                    rows: result.rows.clone(),
                    rows_affected: None,
                }));
            }
            output.push(StatementOutput::Summary(StatementSummary {
                statement_index,
                sql: result.sql.clone(),
                rows_affected: result.rows_affected,
            }));
        }
        return Ok((multi.results, output));
    }

    if let Some(rows_affected) = execute_rows_affected(data) {
        return Ok((
            Vec::new(),
            vec![StatementOutput::Summary(StatementSummary {
                statement_index: 0,
                sql: execute_sql.to_owned(),
                rows_affected: Some(rows_affected),
            })],
        ));
    }

    Err(ResourceError::OperationNotSupported {
        driver: POSTGRES_PROVIDER_ID.to_string(),
        operation: "execute_on_resource".to_string(),
        reason: format!(
            "the command returned {}, which the statement-result decoder cannot represent. \
             Reporting it as an empty success would hide the result; run this \
             command through the driver command API on a connection handle instead.",
            describe_shape(data)
        ),
    })
}

/// Preserve row batches and completion summaries in their statement order.
#[derive(Debug)]
pub(super) enum StatementOutput {
    Rows(ResultChunk),
    Summary(StatementSummary),
}

/// Name the payload's shape for an error message without echoing its contents.
fn describe_shape(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("an object with keys [{}]", keys.join(", "))
        }
        serde_json::Value::Array(items) => format!("an array of {} item(s)", items.len()),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::String(_) => "a bare string".to_string(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => "a bare scalar".to_string(),
    }
}
