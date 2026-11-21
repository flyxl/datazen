use serde_json::Value;

use crate::types::ColumnSchema;

use super::types::{ColumnValues, RowIdentity};

pub fn stable_serialize(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_owned()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) if number.as_f64().is_some_and(|value| value.is_finite()) => {
            Some(number.to_string())
        }
        Value::Number(_) => None,
        Value::String(text) => serde_json::to_string(text).ok(),
        Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(stable_serialize(item)?);
            }
            Some(format!("[{}]", parts.join(",")))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut parts = Vec::with_capacity(keys.len());
            for key in keys {
                let serialized = stable_serialize(map.get(key)?)?;
                let quoted = serde_json::to_string(key).ok()?;
                parts.push(format!("{quoted}:{serialized}"));
            }
            Some(format!("{{{}}}", parts.join(",")))
        }
    }
}

fn is_stable_identity_value(value: &Value) -> bool {
    *value != Value::Null && stable_serialize(value).is_some()
}

fn value_for_column(row: &ColumnValues, column: &str) -> Value {
    row.get(column).cloned().unwrap_or(Value::Null)
}

pub fn build_row_identity(
    row: &ColumnValues,
    primary_key_columns: &[ColumnSchema],
) -> Option<RowIdentity> {
    if primary_key_columns.is_empty() {
        return None;
    }
    let mut identity = RowIdentity::new();
    for column in primary_key_columns {
        let value = value_for_column(row, &column.name);
        if !is_stable_identity_value(&value) {
            return None;
        }
        identity.insert(column.name.clone(), value);
    }
    Some(identity)
}

pub fn row_identity_key(identity: &RowIdentity) -> Result<String, String> {
    if identity.is_empty() {
        return Err("Row identity requires primary-key values".to_owned());
    }
    let mut keys: Vec<&String> = identity.keys().collect();
    keys.sort();
    let mut parts = Vec::with_capacity(keys.len());
    for column in keys {
        let value = match identity.get(column) {
            Some(value) => value,
            None => return Err("Row identity contains a NULL or unstable value".to_owned()),
        };
        let serialized = match stable_serialize(value) {
            Some(serialized) if is_stable_identity_value(value) => serialized,
            _ => return Err("Row identity contains a NULL or unstable value".to_owned()),
        };
        let quoted = serde_json::to_string(column)
            .map_err(|_| "Row identity contains a NULL or unstable value".to_owned())?;
        parts.push(format!("{quoted}:{serialized}"));
    }
    Ok(format!("[{}]", parts.join(",")))
}

pub fn duplicate_row_identity_keys(
    rows: &[ColumnValues],
    primary_key_columns: &[ColumnSchema],
) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    for row in rows {
        let identity = match build_row_identity(row, primary_key_columns) {
            Some(identity) => identity,
            None => continue,
        };
        if let Ok(key) = row_identity_key(&identity) {
            *counts.entry(key).or_insert(0usize) += 1;
        }
    }
    let mut duplicates: Vec<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(key, _)| key)
        .collect();
    duplicates.sort();
    duplicates
}

pub fn values_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    let left = left.unwrap_or(&Value::Null);
    let right = right.unwrap_or(&Value::Null);
    serde_json::to_string(left).ok() == serde_json::to_string(right).ok()
}

pub fn normalize_edit_value(value: Option<&Value>) -> Option<Option<Value>> {
    let value = value?;
    match value {
        Value::Number(number) if number.as_f64().is_some_and(|value| !value.is_finite()) => None,
        other => Some(Some(other.clone())),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn stable_serialize_uses_sorted_object_keys_and_json_quotes() {
        assert_eq!(stable_serialize(&Value::Null).as_deref(), Some("null"));
        assert_eq!(stable_serialize(&json!(true)).as_deref(), Some("true"));
        assert_eq!(
            stable_serialize(&json!("a\"b")).as_deref(),
            Some("\"a\\\"b\"")
        );
        assert_eq!(stable_serialize(&json!(42)).as_deref(), Some("42"));
        let object = json!({"b": 2, "a": 1});
        assert_eq!(
            stable_serialize(&object).as_deref(),
            Some("{\"a\":1,\"b\":2}")
        );
        assert_eq!(
            stable_serialize(&json!([1, "x"])).as_deref(),
            Some("[1,\"x\"]")
        );
    }

    #[test]
    fn row_identity_requires_all_primary_keys_to_be_stable() {
        let pk = [ColumnSchema {
            name: "id".to_owned(),
            data_type: "int".to_owned(),
            nullable: false,
            default_value: None,
            is_primary_key: Some(true),
            is_auto_increment: None,
            comment: None,
        }];
        assert_eq!(build_row_identity(&ColumnValues::new(), &[]), None);
        let identity = build_row_identity(
            &{
                let mut row = ColumnValues::new();
                row.insert("id".to_owned(), json!(7));
                row
            },
            &pk,
        );
        assert_eq!(
            row_identity_key(&identity.expect("identity")).as_deref(),
            Ok("[\"id\":7]")
        );
        let missing = build_row_identity(&ColumnValues::new(), &pk);
        assert_eq!(missing, None);
    }

    #[test]
    fn row_identity_key_rejects_empty_and_null_values() {
        assert_eq!(
            row_identity_key(&RowIdentity::new()),
            Err("Row identity requires primary-key values".to_owned())
        );
        let mut identity = RowIdentity::new();
        identity.insert("id".to_owned(), Value::Null);
        assert_eq!(
            row_identity_key(&identity),
            Err("Row identity contains a NULL or unstable value".to_owned())
        );
    }

    #[test]
    fn identity_key_is_independent_of_column_insertion_order() {
        let mut first = RowIdentity::new();
        first.insert("a".to_owned(), json!(1));
        first.insert("b".to_owned(), json!("x"));
        let mut second = RowIdentity::new();
        second.insert("b".to_owned(), json!("x"));
        second.insert("a".to_owned(), json!(1));
        assert_eq!(row_identity_key(&first), row_identity_key(&second));
    }

    #[test]
    fn duplicate_identities_are_sorted_and_unbuildable_rows_skipped() {
        let pk = [ColumnSchema {
            name: "id".to_owned(),
            data_type: "int".to_owned(),
            nullable: false,
            default_value: None,
            is_primary_key: Some(true),
            is_auto_increment: None,
            comment: None,
        }];
        let row = |value: Value| {
            let mut values = ColumnValues::new();
            values.insert("id".to_owned(), value);
            values
        };
        let rows = [
            row(json!(2)),
            row(json!(1)),
            row(json!(2)),
            row(Value::Null),
        ];
        assert_eq!(duplicate_row_identity_keys(&rows, &pk), ["[\"id\":2]"]);
    }

    #[test]
    fn values_equal_folds_missing_into_null() {
        assert!(values_equal(None, Some(&Value::Null)));
        assert!(values_equal(Some(&json!(1)), Some(&json!(1))));
        assert!(!values_equal(Some(&json!(1)), Some(&json!(2))));
    }

    #[test]
    fn normalize_edit_value_preserves_the_three_write_intents() {
        assert_eq!(normalize_edit_value(None), None);
        assert_eq!(
            normalize_edit_value(Some(&Value::Null)),
            Some(Some(Value::Null))
        );
        assert_eq!(
            normalize_edit_value(Some(&json!("x"))),
            Some(Some(json!("x")))
        );
        assert_eq!(
            normalize_edit_value(Some(&json!([1, 2]))),
            Some(Some(json!([1, 2])))
        );
    }
}
