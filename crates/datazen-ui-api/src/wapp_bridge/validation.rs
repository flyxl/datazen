use serde_json::{Map, Value};

use super::protocol::BridgeErrorCode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeApiError {
    pub code: BridgeErrorCode,
    pub message: String,
}

impl BridgeApiError {
    pub fn new(code: BridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::BadRequest, message)
    }

    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::Permission, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::NotFound, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::Internal, message)
    }

    pub fn rate_limit(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::RateLimit, message)
    }
}

pub fn as_nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandInvokeInput {
    pub connection_id: String,
    pub command: String,
    pub args: Value,
}

pub fn parse_command_invoke(payload: Option<&Value>) -> Result<CommandInvokeInput, BridgeApiError> {
    const MESSAGE: &str = "command.invoke requires {connectionId, command, args?}";
    let object = payload.and_then(Value::as_object);
    let connection_id = object
        .and_then(|value| value.get("connectionId"))
        .and_then(|value| as_nonempty_string(Some(value)));
    let command = object
        .and_then(|value| value.get("command"))
        .and_then(|value| as_nonempty_string(Some(value)));
    let args = match object.and_then(|value| value.get("args")) {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(Value::Object(_)) => object
            .and_then(|value| value.get("args"))
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        Some(_) => return Err(BridgeApiError::bad_request(MESSAGE)),
    };
    match (connection_id, command) {
        (Some(connection_id), Some(command)) => Ok(CommandInvokeInput {
            connection_id,
            command,
            args,
        }),
        _ => Err(BridgeApiError::bad_request(MESSAGE)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorageInput {
    pub key: String,
    pub value: Value,
}

pub fn parse_storage(kind: &str, payload: Option<&Value>) -> Result<StorageInput, BridgeApiError> {
    let object = payload.and_then(Value::as_object);
    let key = object
        .and_then(|value| value.get("key"))
        .and_then(|value| as_nonempty_string(Some(value)));
    let key = match key {
        Some(key) => key,
        None => {
            return Err(BridgeApiError::bad_request(format!(
                "{kind} requires a non-empty string key"
            )))
        }
    };
    let value = object
        .and_then(|value| value.get("value"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(StorageInput { key, value })
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotifyInput {
    pub title: String,
    pub body: Option<String>,
}

pub fn parse_notify(payload: Option<&Value>) -> Result<NotifyInput, BridgeApiError> {
    const MESSAGE: &str = "ui.notify requires {title, body?}";
    let object = payload.and_then(Value::as_object);
    let title = object
        .and_then(|value| value.get("title"))
        .and_then(|value| as_nonempty_string(Some(value)));
    let body = match object.and_then(|value| value.get("body")) {
        None => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => return Err(BridgeApiError::bad_request(MESSAGE)),
    };
    match title {
        Some(title) => Ok(NotifyInput { title, body }),
        None => Err(BridgeApiError::bad_request(MESSAGE)),
    }
}

pub fn parse_i18n_key(payload: Option<&Value>) -> Result<String, BridgeApiError> {
    let key = payload
        .and_then(Value::as_object)
        .and_then(|value| value.get("key"))
        .and_then(|value| as_nonempty_string(Some(value)));
    key.ok_or_else(|| BridgeApiError::bad_request("i18n.getString requires {key}"))
}

pub fn looks_like_not_found(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("not found") || lowered.contains("no such") || lowered.contains("unknown")
}
