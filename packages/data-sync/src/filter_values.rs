//! Lossless conversion between the JSON filter contract and driver values.

use crate::DataSyncError;
use datazen_driver_api::Value;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

pub(super) fn scalar_value(value: &serde_json::Value) -> Result<Value, DataSyncError> {
    if value.is_array() {
        return Err(DataSyncError::validation(
            "sync filter comparison requires one scalar value",
        ));
    }
    json_to_value(value)
}

pub(super) fn in_values(value: &serde_json::Value) -> Result<Vec<Value>, DataSyncError> {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>(),
        serde_json::Value::String(value) => Ok(value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect()),
        _ => Err(DataSyncError::validation(
            "sync filter IN requires an array or comma-separated string",
        )),
    }
}

pub(super) fn value_to_json(value: &Value) -> Result<serde_json::Value, DataSyncError> {
    Ok(match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Integer(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| DataSyncError::validation("sync filter float is not finite"))?,
        Value::String(value) | Value::Timestamp(value) => serde_json::Value::String(value.clone()),
        Value::Bytes(value) => serde_json::json!({
            "$datazenType": "bytes",
            "encoding": "base64",
            "value": BASE64.encode(value),
        }),
        Value::Json(value) => value.clone(),
    })
}

pub(super) fn json_to_value(value: &serde_json::Value) -> Result<Value, DataSyncError> {
    Ok(match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(*value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Float))
            .ok_or_else(|| DataSyncError::validation("sync filter number is out of range"))?,
        serde_json::Value::String(value) => Value::String(value.clone()),
        serde_json::Value::Object(value)
            if value.get("$datazenType") == Some(&serde_json::Value::String("bytes".into()))
                && value.get("encoding") == Some(&serde_json::Value::String("base64".into())) =>
        {
            let encoded = value
                .get("value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| DataSyncError::validation("sync filter bytes value is required"))?;
            Value::Bytes(BASE64.decode(encoded).map_err(|_| {
                DataSyncError::validation("sync filter bytes value is invalid base64")
            })?)
        }
        _ => {
            return Err(DataSyncError::validation(
                "sync filter IN values must be scalar",
            ))
        }
    })
}
