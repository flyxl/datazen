use serde_json::Value;

use crate::{ColumnInfo, QueryResult};

pub fn default_process_columns() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "pid".to_owned(),
            data_type: "integer".to_owned(),
            nullable: false,
        },
        ColumnInfo {
            name: "user".to_owned(),
            data_type: "string".to_owned(),
            nullable: true,
        },
        ColumnInfo {
            name: "database".to_owned(),
            data_type: "string".to_owned(),
            nullable: true,
        },
        ColumnInfo {
            name: "client".to_owned(),
            data_type: "string".to_owned(),
            nullable: true,
        },
        ColumnInfo {
            name: "state".to_owned(),
            data_type: "string".to_owned(),
            nullable: true,
        },
        ColumnInfo {
            name: "query".to_owned(),
            data_type: "string".to_owned(),
            nullable: true,
        },
        ColumnInfo {
            name: "durationMs".to_owned(),
            data_type: "integer".to_owned(),
            nullable: true,
        },
    ]
}

pub fn process_column_label_key(name: &str) -> Option<&'static str> {
    match name {
        "pid" => Some("processList.colPid"),
        "user" => Some("processList.colUser"),
        "database" => Some("processList.colDatabase"),
        "client" => Some("processList.colClient"),
        "state" => Some("processList.colState"),
        "query" => Some("processList.colQuery"),
        "durationMs" => Some("processList.colDuration"),
        _ => None,
    }
}

fn is_query_result_shape(data: Option<&Value>) -> bool {
    matches!(data, Some(Value::Object(map)) if map.contains_key("columns") && map.contains_key("rows"))
        && match data {
            Some(Value::Object(map)) => {
                matches!(map.get("columns"), Some(Value::Array(_)))
                    && matches!(map.get("rows"), Some(Value::Array(_)))
            }
            _ => false,
        }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Array(items) => items.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

pub fn process_record_to_row(record: &Value) -> Vec<Option<Value>> {
    default_process_columns()
        .iter()
        .map(|column| match record.get(&column.name) {
            None | Some(Value::Null) => None,
            Some(value @ (Value::String(_) | Value::Number(_) | Value::Bool(_))) => {
                Some(value.clone())
            }
            Some(value) => Some(Value::String(js_string(value))),
        })
        .collect()
}

pub fn command_result_rows(data: Option<&Value>) -> QueryResult {
    if is_query_result_shape(data) {
        if let Some(value) = data {
            if let Ok(result) = serde_json::from_value::<QueryResult>(value.clone()) {
                return result;
            }
        }
    }
    if let Some(Value::Object(map)) = data {
        if let Some(Value::Array(processes)) = map.get("processes") {
            let rows = processes
                .iter()
                .filter(|item| matches!(item, Value::Object(_) | Value::Array(_)))
                .map(process_record_to_row)
                .collect();
            return QueryResult {
                columns: default_process_columns(),
                rows,
                rows_affected: None,
                execution_time_ms: 0,
            };
        }
    }
    QueryResult {
        columns: default_process_columns(),
        rows: Vec::new(),
        rows_affected: None,
        execution_time_ms: 0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessColumnDef {
    pub id: String,
    pub name: String,
    pub column_type: String,
}

pub fn command_result_columns(
    fallback: Option<&[ColumnInfo]>,
    translate: Option<&dyn Fn(&str) -> String>,
) -> Vec<ProcessColumnDef> {
    let default = default_process_columns();
    let columns = fallback.filter(|list| !list.is_empty()).unwrap_or(&default);
    columns
        .iter()
        .map(|column| ProcessColumnDef {
            id: column.name.clone(),
            name: match process_column_label_key(&column.name) {
                Some(key) => translate.map_or_else(|| column.name.clone(), |t| t(key)),
                None => column.name.clone(),
            },
            column_type: column.data_type.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn default_columns_and_labels_are_pinned() {
        let columns = default_process_columns();
        assert_eq!(columns.len(), 7);
        assert_eq!(columns[0].name, "pid");
        assert!(!columns[0].nullable);
        assert!(columns[1].nullable);
        assert_eq!(columns[6].name, "durationMs");
        assert_eq!(
            process_column_label_key("durationMs"),
            Some("processList.colDuration")
        );
        assert_eq!(process_column_label_key("unknown"), None);
    }

    #[test]
    fn query_result_shape_is_passed_through() {
        let data = json!({
            "columns": [{"name":"a","dataType":"string","nullable":true}],
            "rows": [["x"]],
            "executionTimeMs": 12,
            "rowsAffected": 3
        });
        let result = command_result_rows(Some(&data));
        assert_eq!(result.execution_time_ms, 12);
        assert_eq!(result.rows_affected, Some(3));
        assert_eq!(result.columns.len(), 1);
    }

    #[test]
    fn processes_payload_is_normalized_to_default_columns() {
        let data = json!({
            "processes": [
                {"pid": 42, "user": "alice", "state": "active", "extra": "ignored"},
                null,
                "not an object",
                {"pid": null, "database": {"nested": 1}, "durationMs": true}
            ]
        });
        let result = command_result_rows(Some(&data));
        assert_eq!(result.columns.len(), 7);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][0], Some(json!(42)));
        assert_eq!(result.rows[0][5], None);
        assert_eq!(result.rows[1][0], None);
        assert_eq!(result.rows[1][2], Some(json!("[object Object]")));
        assert_eq!(result.rows[1][6], Some(json!(true)));
        assert_eq!(result.execution_time_ms, 0);
    }

    #[test]
    fn non_process_payloads_yield_the_empty_default_result() {
        for data in [
            None,
            Some(&json!(null)),
            Some(&json!([])),
            Some(&json!({"other": 1})),
        ] {
            let result = command_result_rows(data);
            assert_eq!(result.columns.len(), 7);
            assert!(result.rows.is_empty());
            assert_eq!(result.execution_time_ms, 0);
        }
    }

    #[test]
    fn column_defs_use_localized_labels_with_a_fallback() {
        let defs = command_result_columns(None, Some(&|key: &str| format!("T:{key}")));
        assert_eq!(defs.len(), 7);
        assert_eq!(defs[0].id, "pid");
        assert_eq!(defs[0].name, "T:processList.colPid");
        assert_eq!(defs[0].column_type, "integer");

        let plain = command_result_columns(None, None);
        assert_eq!(plain[0].name, "pid");
        let custom = vec![ColumnInfo {
            name: "custom".to_owned(),
            data_type: "json".to_owned(),
            nullable: true,
        }];
        let defs = command_result_columns(Some(&custom), Some(&|_| "unused".to_owned()));
        assert_eq!(defs[0].id, "custom");
        assert_eq!(defs[0].name, "custom");
        let empty: Vec<ColumnInfo> = Vec::new();
        assert_eq!(command_result_columns(Some(&empty), None).len(), 7);
    }
}
