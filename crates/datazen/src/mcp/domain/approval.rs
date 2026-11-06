//! MCP per-call approval vocabulary and port.
//!
//! The host owns presentation and persistence; this module owns the decision
//! vocabulary and the credential-safe preview. A denied, timed-out, or
//! cancelled call never reaches a tool executor.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::types::{AiDataEgressLevel, AiSafetyGateConfig};

/// One non-read-only/high-risk MCP call waiting for a human decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpApprovalRequest {
    pub request_id: String,
    pub tool: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub namespace: Option<String>,
    /// Exact SQL after credential redaction, never an audit copy.
    pub sql: Option<String>,
    /// Exact JSON arguments after credential redaction.
    pub arguments: Value,
    pub created_at: String,
    pub expires_at: String,
    pub timeout_ms: u64,
}

/// The only decisions the host may return. `AllowOnce` is deliberately not a
/// persistent grant; every later call asks again.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpApprovalDecision {
    AllowOnce,
    Deny,
    Timeout,
    Cancel,
}

impl McpApprovalDecision {
    pub fn audit_decision(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow",
            Self::Deny => "deny",
            Self::Timeout => "timeout",
            Self::Cancel => "cancel",
        }
    }

    pub fn is_allowed(self) -> bool {
        matches!(self, Self::AllowOnce)
    }
}

/// Presentation port. Desktop emits a frontend event and waits for the answer;
/// tests can inject a deterministic implementation.
#[async_trait]
pub trait McpApprovalPort: Send + Sync {
    async fn decide(&self, request: &McpApprovalRequest) -> McpApprovalDecision;
}

/// Fail-closed default used by headless hosts and tests without a presenter.
pub struct DenyAllApproval;

#[async_trait]
impl McpApprovalPort for DenyAllApproval {
    async fn decide(&self, _request: &McpApprovalRequest) -> McpApprovalDecision {
        McpApprovalDecision::Deny
    }
}

/// Build the payload shown to the user. SQL is kept exact for the approver
/// while credential-bearing values are replaced; audit never persists this.
pub fn approval_preview(
    request_id: &str,
    tool: &str,
    connection_id: Option<&str>,
    database: Option<&str>,
    namespace: Option<&str>,
    sql: Option<&str>,
    arguments: Option<&Map<String, Value>>,
    created_at: &str,
    expires_at: &str,
    timeout_ms: u64,
) -> McpApprovalRequest {
    let gate = preview_gate();
    McpApprovalRequest {
        request_id: request_id.to_string(),
        tool: tool.to_string(),
        connection_id: connection_id.map(str::to_owned),
        database: database.map(str::to_owned),
        namespace: namespace.map(str::to_owned),
        sql: sql.map(|sql| redact_for_gate(sql, &gate)),
        arguments: arguments
            .map(|args| redact_json(&Value::Object(args.clone()), &gate))
            .unwrap_or_else(|| Value::Object(Map::new())),
        created_at: created_at.to_string(),
        expires_at: expires_at.to_string(),
        timeout_ms,
    }
}

fn preview_gate() -> AiSafetyGateConfig {
    AiSafetyGateConfig {
        redact_credentials: true,
        data_egress_level: AiDataEgressLevel::Unrestricted,
        max_context_bytes: 1_000_000,
        ..AiSafetyGateConfig::default()
    }
}

fn redact_json(value: &Value, gate: &AiSafetyGateConfig) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    if sensitive_key(key) {
                        (key.clone(), Value::String("[REDACTED]".into()))
                    } else {
                        (key.clone(), redact_json(value, gate))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(|v| redact_json(v, gate)).collect()),
        Value::String(text) => Value::String(redact_for_gate(text, gate)),
        other => other.clone(),
    }
}

fn sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "credential",
        "authorization",
        "bearer",
        "api_key",
        "apikey",
        "private_key",
        "privatekey",
        "passphrase",
    ]
    .iter()
    .any(|word| lower == *word || lower.ends_with(&format!("_{word}")) || lower.ends_with(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_redacts_credentials_but_keeps_sql_and_shape() {
        let args = serde_json::json!({
            "connection_id": "c1",
            "sql": "INSERT INTO users(name, password) VALUES ('a', 'pw')",
            "password": "pw",
            "nested": {"access_token": "abc", "keep": "visible"}
        });
        let request = approval_preview(
            "r1",
            "query",
            Some("c1"),
            Some("db"),
            None,
            Some("INSERT INTO users(name, password) VALUES ('a', 'pw')"),
            args.as_object(),
            "now",
            "later",
            300_000,
        );
        assert!(request
            .sql
            .as_deref()
            .unwrap()
            .contains("INSERT INTO users"));
        assert_eq!(request.arguments["password"], "[REDACTED]");
        assert_eq!(request.arguments["nested"]["access_token"], "[REDACTED]");
        assert_eq!(request.arguments["nested"]["keep"], "visible");
        assert!(!request.arguments.to_string().contains("abc"));
    }

    #[test]
    fn decisions_map_to_audit_vocabulary() {
        assert_eq!(McpApprovalDecision::AllowOnce.audit_decision(), "allow");
        assert_eq!(McpApprovalDecision::Deny.audit_decision(), "deny");
        assert_eq!(McpApprovalDecision::Timeout.audit_decision(), "timeout");
        assert_eq!(McpApprovalDecision::Cancel.audit_decision(), "cancel");
        assert!(McpApprovalDecision::AllowOnce.is_allowed());
        assert!(!McpApprovalDecision::Deny.is_allowed());
    }
}
