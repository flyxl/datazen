//! Turning a Driver Command payload into what the `ResultSink` channel can carry.
//!
//! This is the narrowest possible decoder, and the narrowness is the point.
//! `HBaseDriver::execute_command` dispatches three shapes in all: `query` →
//! `MultiQueryResult`, `execute` → `{"rowsAffected": n}`, and the Stargate
//! catalog commands (`list_databases` / `list_tables` / `get_table_schema`) →
//! an object carrying a *catalog*, not rows.
//!
//! Only the first two have a truthful representation as a [`ResultChunk`].
//! Decoding a catalog object as "no rows, succeeded" would be the shell
//! behaviour the contract forbids — the caller would believe a command ran and
//! saw nothing, when in fact it saw something this channel cannot express. So
//! the catalog shape is refused with an explicit
//! [`ResourceError::OperationNotSupported`], and the refusal names which of the
//! driver's own commands produced it.
//!
//! Error messages name the payload's *shape* and never its contents: a catalog
//! result carries table names and row data, which do not belong in an error
//! string.

use datazen_driver_api::resource::{ResourceError, ResultChunk};
use datazen_driver_api::{MultiQueryResult, StatementResult};

use super::capabilities::HBASE_PROVIDER_ID;

/// The `execute` acknowledgement produced by `execute_standard_sql_command`.
///
/// Read by hand rather than through a `Deserialize` impl, so the shape stays
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
/// sink receives.
///
/// Exactly two shapes are accepted, both produced by
/// `execute_standard_sql_command`. Anything else — including this driver's own
/// catalog commands — is refused, not emptied.
pub(super) fn decode_command_result(
    data: &serde_json::Value,
) -> Result<(Vec<StatementResult>, Vec<ResultChunk>), ResourceError> {
    if let Ok(multi) = serde_json::from_value::<MultiQueryResult>(data.clone()) {
        let chunks = multi
            .results
            .iter()
            .enumerate()
            .map(|(statement_index, result)| ResultChunk {
                statement_index,
                sql: result.sql.clone(),
                rows: result.rows.clone(),
                rows_affected: result.rows_affected,
            })
            .collect();
        return Ok((multi.results, chunks));
    }

    if let Some(rows_affected) = execute_rows_affected(data) {
        return Ok((
            Vec::new(),
            vec![ResultChunk {
                statement_index: 0,
                // The `execute` path reports no statement text; an empty string
                // is the honest placeholder for "not carried by this command".
                sql: String::new(),
                rows: Vec::new(),
                rows_affected: Some(rows_affected),
            }],
        ));
    }

    Err(ResourceError::OperationNotSupported {
        driver: HBASE_PROVIDER_ID.to_string(),
        operation: "execute_on_resource".to_string(),
        reason: format!(
            "the command returned {}, which the ResultSink channel cannot carry as a ResultChunk \
             — a Stargate catalog listing is an object, not a row set. Reporting it as an empty \
             success would hide the result; run this command through the driver command API on a \
             connection handle instead.",
            describe_shape(data)
        ),
    })
}

/// Name the payload's shape for an error message without echoing its contents.
fn describe_shape(data: &serde_json::Value) -> String {
    match data {
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