use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ThemeSnapshot;

pub const BRIDGE_CHANNEL: &str = "datazen-wapp";
pub const REQUEST_TIMEOUT_MS: u64 = 30_000;
pub const MAX_INFLIGHT_REQUESTS: u32 = 20;
pub const NOTIFY_MIN_INTERVAL_MS: i64 = 5_000;
pub const MAX_REQUEST_ID_CHARS: usize = 128;
pub const SESSION_REQUEST_LIMIT: usize = 1024;
pub const ERROR_MESSAGE_LIMIT_CHARS: usize = 500;

pub const BRIDGE_ERROR_CODES: [&str; 8] = [
    "E_PERMISSION",
    "E_NOT_FOUND",
    "E_TIMEOUT",
    "E_RATE_LIMIT",
    "E_WAPP_DISABLED",
    "E_BAD_REQUEST",
    "E_NOT_IMPLEMENTED",
    "E_INTERNAL",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BridgeErrorCode {
    #[serde(rename = "E_PERMISSION")]
    Permission,
    #[serde(rename = "E_NOT_FOUND")]
    NotFound,
    #[serde(rename = "E_TIMEOUT")]
    Timeout,
    #[serde(rename = "E_RATE_LIMIT")]
    RateLimit,
    #[serde(rename = "E_WAPP_DISABLED")]
    WappDisabled,
    #[serde(rename = "E_BAD_REQUEST")]
    BadRequest,
    #[serde(rename = "E_NOT_IMPLEMENTED")]
    NotImplemented,
    #[serde(rename = "E_INTERNAL")]
    Internal,
}

impl BridgeErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "E_PERMISSION",
            Self::NotFound => "E_NOT_FOUND",
            Self::Timeout => "E_TIMEOUT",
            Self::RateLimit => "E_RATE_LIMIT",
            Self::WappDisabled => "E_WAPP_DISABLED",
            Self::BadRequest => "E_BAD_REQUEST",
            Self::NotImplemented => "E_NOT_IMPLEMENTED",
            Self::Internal => "E_INTERNAL",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "E_PERMISSION" => Some(Self::Permission),
            "E_NOT_FOUND" => Some(Self::NotFound),
            "E_TIMEOUT" => Some(Self::Timeout),
            "E_RATE_LIMIT" => Some(Self::RateLimit),
            "E_WAPP_DISABLED" => Some(Self::WappDisabled),
            "E_BAD_REQUEST" => Some(Self::BadRequest),
            "E_NOT_IMPLEMENTED" => Some(Self::NotImplemented),
            "E_INTERNAL" => Some(Self::Internal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BridgeTarget {
    Host,
}

impl BridgeTarget {
    pub fn as_str(self) -> &'static str {
        "host"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappRequestEnvelope {
    pub ch: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_nonce: Option<String>,
    pub target: BridgeTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappOkEnvelope {
    pub ch: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<String>,
    pub target: BridgeTarget,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeErrorPayload {
    pub code: BridgeErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappErrEnvelope {
    pub ch: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_id: Option<String>,
    pub target: BridgeTarget,
    pub ok: bool,
    pub payload: BridgeErrorPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WappResponseEnvelope {
    Ok(WappOkEnvelope),
    Err(WappErrEnvelope),
}

impl WappResponseEnvelope {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn kind(&self) -> &str {
        match self {
            Self::Ok(envelope) => envelope.kind.as_str(),
            Self::Err(envelope) => envelope.kind.as_str(),
        }
    }

    pub fn req_id(&self) -> Option<&str> {
        match self {
            Self::Ok(envelope) => envelope.req_id.as_deref(),
            Self::Err(envelope) => envelope.req_id.as_deref(),
        }
    }

    pub fn error_code(&self) -> Option<BridgeErrorCode> {
        match self {
            Self::Ok(_) => None,
            Self::Err(envelope) => Some(envelope.payload.code),
        }
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Ok(_) => None,
            Self::Err(envelope) => Some(envelope.payload.message.as_str()),
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionConfigLike {
    pub id: String,
    pub name: String,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeConnectionSummary {
    pub id: String,
    pub name: String,
    pub db_type: String,
}

pub fn to_public_connection(config: &ConnectionConfigLike) -> BridgeConnectionSummary {
    BridgeConnectionSummary {
        id: config.id.clone(),
        name: config.name.clone(),
        db_type: config.database_type.clone(),
    }
}

pub fn ok_envelope(ch: &str, req_id: Option<&str>, payload: Option<Value>) -> WappOkEnvelope {
    WappOkEnvelope {
        ch: ch.to_owned(),
        kind: String::new(),
        req_id: req_id.map(str::to_owned),
        target: BridgeTarget::Host,
        ok: true,
        payload,
    }
}

pub fn err_envelope(
    ch: &str,
    req_id: Option<&str>,
    code: BridgeErrorCode,
    message: impl Into<String>,
) -> WappErrEnvelope {
    WappErrEnvelope {
        ch: ch.to_owned(),
        kind: String::new(),
        req_id: req_id.map(str::to_owned),
        target: BridgeTarget::Host,
        ok: false,
        payload: BridgeErrorPayload {
            code,
            message: message.into(),
        },
    }
}

pub fn response_type_of(request_kind: &str, ok: bool) -> String {
    if ok {
        format!("{request_kind}.ok")
    } else {
        format!("{request_kind}.err")
    }
}

pub fn is_request_envelope(value: &Value) -> bool {
    let object = match value.as_object() {
        Some(object) => object,
        None => return false,
    };
    object.get("ch").and_then(Value::as_str) == Some(BRIDGE_CHANNEL)
        && object.get("target").and_then(Value::as_str) == Some("host")
        && object.get("type").and_then(Value::as_str).is_some()
}

pub fn parse_request(value: &Value) -> Option<WappRequestEnvelope> {
    if !is_request_envelope(value) {
        return None;
    }
    serde_json::from_value(value.clone()).ok()
}

pub fn theme_snapshot_value(snapshot: &ThemeSnapshot) -> Value {
    let mut tokens = Map::new();
    for (name, value) in &snapshot.tokens {
        tokens.insert(name.clone(), Value::String(value.clone()));
    }
    serde_json::json!({
        "v": snapshot.v,
        "dark": snapshot.dark,
        "tokens": Value::Object(tokens),
    })
}

pub fn host_ready_envelope(
    ch: &str,
    api_version: u32,
    instance_nonce: &str,
    locale: &str,
    snapshot: &ThemeSnapshot,
) -> Value {
    serde_json::json!({
        "ch": ch,
        "type": "host.ready",
        "target": "host",
        "payload": {
            "apiVersion": api_version,
            "instanceNonce": instance_nonce,
            "locale": locale,
            "dark": snapshot.dark,
            "tokens": theme_snapshot_value(snapshot)["tokens"].clone(),
        },
    })
}

pub fn theme_apply_envelope(snapshot: &ThemeSnapshot) -> Value {
    serde_json::json!({
        "ch": BRIDGE_CHANNEL,
        "type": "theme.apply",
        "target": "host",
        "payload": theme_snapshot_value(snapshot),
    })
}
