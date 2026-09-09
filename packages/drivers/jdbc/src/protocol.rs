//! JSON-RPC 2.0 contract shared with `datazen-jdbc-agent`.
//!
//! Wire format: one JSON object per line on stdio.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire protocol version negotiated in `agent.hello`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Agent semver reported by the companion jar (keep in sync with Java).
pub const AGENT_VERSION: &str = "0.1.0";

pub mod methods {
    pub const AGENT_HELLO: &str = "agent.hello";
    pub const AGENT_SHUTDOWN: &str = "agent.shutdown";
    pub const SESSION_OPEN: &str = "session.open";
    pub const SESSION_CLOSE: &str = "session.close";
    pub const META_DATABASES: &str = "meta.databases";
    pub const META_TABLES: &str = "meta.tables";
    pub const META_COLUMNS: &str = "meta.columns";
    pub const QUERY_EXECUTE: &str = "query.execute";
    pub const QUERY_FETCH: &str = "query.fetch";
    pub const QUERY_CLOSE: &str = "query.close";
    pub const QUERY_CANCEL: &str = "query.cancel";
    pub const EXEC_UPDATE: &str = "exec.update";
    pub const TX_BEGIN: &str = "tx.begin";
    pub const TX_COMMIT: &str = "tx.commit";
    pub const TX_ROLLBACK: &str = "tx.rollback";
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }

    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcErrorBody>,
}

impl JsonRpcResponse {
    pub fn from_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }

    /// Extract numeric request id when present.
    pub fn id_u64(&self) -> Option<u64> {
        match &self.id {
            Some(Value::Number(n)) => n.as_u64(),
            Some(Value::String(s)) => s.parse().ok(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RpcError {
    #[error("JSON-RPC error {code}: {message}")]
    Remote { code: i64, message: String, data: Option<Value> },
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("serde: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for RpcError {
    fn from(e: serde_json::Error) -> Self {
        RpcError::Serde(e.to_string())
    }
}

/// Build the `agent.hello` result payload shape expected by Host.
pub fn agent_version_info() -> Value {
    serde_json::json!({
        "agentVersion": AGENT_VERSION,
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": ["jdbc", "session", "query.stream", "tx"]
    })
}

/// Validate `agent.hello` result; returns agent version string.
pub fn validate_hello_result(result: &Value) -> Result<String, RpcError> {
    let proto = result
        .get("protocolVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| RpcError::Protocol("hello missing protocolVersion".into()))?;
    if proto != PROTOCOL_VERSION as u64 {
        return Err(RpcError::Protocol(format!(
            "incompatible protocolVersion {proto}, host expects {PROTOCOL_VERSION}"
        )));
    }
    let ver = result
        .get("agentVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(ver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn hello_payload_has_required_keys() {
        let v = agent_version_info();
        assert_eq!(v["protocolVersion"], 1);
        assert!(v["agentVersion"].as_str().is_some());
        assert!(v["capabilities"].as_array().is_some());
    }

    #[test]
    fn request_roundtrip_line() {
        let req = JsonRpcRequest::new(7, methods::AGENT_HELLO, Some(serde_json::json!({
            "hostVersion": "dev",
            "protocolVersion": 1
        })));
        let line = req.to_line().unwrap();
        assert!(line.contains("\"method\":\"agent.hello\""));
        assert!(line.contains("\"id\":7"));
    }

    #[test]
    fn parse_success_response() {
        let line = r#{"jsonrpc":"2.0","id":1,"result":{"agentVersion":"0.1.0","protocolVersion":1}}#;
        let resp = JsonRpcResponse::from_line(line).unwrap();
        assert_eq!(resp.id_u64(), Some(1));
        assert!(resp.error.is_none());
        let ver = validate_hello_result(resp.result.as_ref().unwrap()).unwrap();
        assert_eq!(ver, "0.1.0");
    }

    #[test]
    fn parse_error_response() {
        let line = r#{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found","data":{"category":"internal"}}}#;
        let resp = JsonRpcResponse::from_line(line).unwrap();
        assert_eq!(resp.id_u64(), Some(2));
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn reject_wrong_protocol_version() {
        let v = serde_json::json!({"agentVersion":"9.0.0","protocolVersion":99});
        assert!(validate_hello_result(&v).is_err());
    }
}
