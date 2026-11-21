use serde_json::{Map, Value};

use super::csv::dedupe_columns;
use super::types::{ImportParseError, ImportParseErrorKind, ImportParseResult};

pub fn parse_json_import(text: &str) -> Result<ImportParseResult, ImportParseError> {
    let parsed: Value = serde_json::from_str(text).map_err(|error| {
        ImportParseError::new(ImportParseErrorKind::InvalidJson, error.to_string())
    })?;
    let items: Vec<Map<String, Value>> = match parsed {
        Value::Array(values) => {
            for value in &values {
                if !matches!(value, Value::Object(_)) {
                    return Err(ImportParseError::new(
                        ImportParseErrorKind::JsonShape,
                        "JSON array items must all be objects",
                    ));
                }
            }
            values
                .into_iter()
                .filter_map(|value| match value {
                    Value::Object(map) => Some(map),
                    _ => None,
                })
                .collect()
        }
        Value::Object(map) => vec![map],
        _ => {
            return Err(ImportParseError::new(
                ImportParseErrorKind::JsonShape,
                "JSON payload must be an object or an array",
            ));
        }
    };
    if items.is_empty() {
        return Ok(ImportParseResult::default());
    }

    let mut column_set = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in &items {
        for key in item.keys() {
            if seen.insert(key.clone()) {
                column_set.push(key.clone());
            }
        }
    }
    let mut diagnostics = Vec::new();
    let columns = dedupe_columns(column_set, &mut diagnostics);
    let rows: Vec<Map<String, Value>> = items
        .iter()
        .map(|item| {
            let mut record = Map::new();
            for column in &columns {
                record.insert(
                    column.clone(),
                    item.get(column).cloned().unwrap_or(Value::Null),
                );
            }
            record
        })
        .collect();
    let row_origins = rows.iter().map(|_| 0).collect();
    Ok(ImportParseResult {
        columns,
        rows,
        row_origins,
        diagnostics,
    })
}
