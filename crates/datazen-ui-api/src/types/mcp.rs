//! The MCP client/approval/audit shapes the aggregate declares — ported
//! from React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` carries two MCP blocks of its own:
//!
//! * the **client block**: `McpServerConfig` (with `isValidMcpServerId`),
//!   `McpClientStatus`, `McpToolInfo` — all three already owned by
//!   `ai_store_types` (T126's field-complete mirrors, whose docs name this
//!   port as their owner) and `platform_shared`'s `WebMcp*` vocabulary is a
//!   **different** family (the web MCP host's shapes, not these), so the
//!   aggregate re-exports the three from `ai_store_types` and this module
//!   ports only the function;
//! * the **approval + audit block (T19)** and the HTTP listener status:
//!   [`McpApprovalRequest`], [`McpApprovalDecision`], [`McpAuditRecord`],
//!   [`McpHttpStatus`] — no Rust home existed anywhere (the kernel's
//!   approval/audit domain is the backend's own layer), so this module is
//!   their mirror.
//!
//! Pure contracts — they cross the IPC boundary (approval prompts, audit
//! listings, the settings/status panel) — so the wire family's camelCase
//! serde law applies, and the module compiles for wasm32 exactly like its
//! `chart_types` sibling.

use serde::{Deserialize, Serialize};

use crate::connection_types::Value;

/// React `McpHttpStatus.transport`'s inline union `'http' | 'none'` — the
/// listener's live transport. React declares it inline with no exported
/// name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpHttpTransport {
    Http,
    None,
}

impl McpHttpTransport {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::None => "none",
        }
    }
}

/// React `McpHttpStatus` — the Desktop opt-in MCP Streamable HTTP
/// listener's status. `token` is present only in the enable/regenerate
/// response that generated it.
///
/// `url`, `port` and `tokenLast4` are `?: string | null` / `?: number |
/// null` — null on the wire when set to `None`, absent also accepted —
/// while `token` is plain `?:` (absent stays absent: the token never rides
/// a status poll, only the response that minted it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpStatus {
    pub running: bool,
    pub enabled: bool,
    pub transport: McpHttpTransport,
    /// Null on the wire when set to `None`; absent also accepted.
    pub url: Option<String>,
    /// Actual bound port (random when configuredPort is 0). Null on the
    /// wire when set to `None`; absent also accepted.
    pub port: Option<u16>,
    /// The port the settings asked for (0 = ask the OS for a random one).
    pub configured_port: u16,
    /// Null on the wire when set to `None`; absent also accepted.
    pub token_last4: Option<String>,
    pub token_configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// React `McpApprovalRequest` — what the approval prompt shows: the tool
/// call awaiting a decision, with its scope legs and expiry.
///
/// The scope legs (`connectionId`, `database`, `namespace`, `sql`) are
/// `?: string | null` — null on the wire when set to `None`, absent also
/// accepted — and `arguments` is `Record<string, unknown>` → opaque
/// [`Value`] map (the crate's `unknown` rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpApprovalRequest {
    pub request_id: String,
    pub tool: String,
    /// Null on the wire when set to `None`; absent also accepted.
    pub connection_id: Option<String>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub database: Option<String>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub namespace: Option<String>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub sql: Option<String>,
    /// The tool's arguments — opaque JSON.
    pub arguments: serde_json::Map<String, Value>,
    pub created_at: String,
    pub expires_at: String,
    /// How long the prompt stays up.
    pub timeout_ms: u64,
}

/// React `McpApprovalDecision = 'allow_once' | 'deny' | 'cancel' |
/// 'timeout'` — the four ways an approval request ends (the timeout arm is
/// what the host itself reports when nobody answered in time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApprovalDecision {
    AllowOnce,
    Deny,
    Cancel,
    Timeout,
}

impl McpApprovalDecision {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow_once",
            Self::Deny => "deny",
            Self::Cancel => "cancel",
            Self::Timeout => "timeout",
        }
    }
}

/// React `McpAuditRecord.decision`'s inline union `'allow' | 'deny' |
/// 'timeout' | 'cancel' | 'read'` — the audit log's outcome vocabulary
/// (note the extra `'read'` arm the approval decision does not have: a
/// read needs no approval but is still audited). React declares it inline
/// with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpAuditDecision {
    Allow,
    Deny,
    Timeout,
    Cancel,
    Read,
}

impl McpAuditDecision {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Timeout => "timeout",
            Self::Cancel => "cancel",
            Self::Read => "read",
        }
    }
}

/// React `McpAuditRecord` — one row of the MCP tool-call audit trail.
///
/// The scope legs (`connectionId`, `databaseName`, `namespace`,
/// `requestId`) and the outcome legs (`rowCount`, `reason`, `index`) are
/// `?: <T> | null` — null on the wire when set to `None`, absent also
/// accepted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditRecord {
    pub id: String,
    pub organization_id: String,
    pub principal_id: String,
    /// Null on the wire when set to `None`; absent also accepted.
    pub connection_id: Option<String>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub database_name: Option<String>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub namespace: Option<String>,
    pub tool: String,
    pub decision: McpAuditDecision,
    /// Rows a read returned, when the decision produced rows. Null on the
    /// wire when set to `None`; absent also accepted.
    pub row_count: Option<u64>,
    /// Why a deny happened, when recorded. Null on the wire when set to
    /// `None`; absent also accepted.
    pub reason: Option<String>,
    /// The listing's row index, when the host paged the trail. Null on
    /// the wire when set to `None`; absent also accepted.
    pub index: Option<u64>,
    /// Null on the wire when set to `None`; absent also accepted.
    pub request_id: Option<String>,
    pub created_at: String,
}

/// React `isValidMcpServerId` — the aggregate's one **value** export.
///
/// Mirrors Rust `validate_mcp_server_id`: `^[a-zA-Z0-9_-]+$` — at least
/// one character, every character alphanumeric, `-` or `_`. Hand-rolled
/// (`is_ascii_alphanumeric`) so the wasm path needs no regex dependency,
/// the same trade `platform_shared::mcp_client`'s twin validator records
/// (that one guards the **web** server ids; this one the stored
/// `McpServerConfig` ids).
pub fn is_valid_mcp_server_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn the_mcp_unions_keep_their_literals() {
        let transports = [
            (McpHttpTransport::Http, "http"),
            (McpHttpTransport::None, "none"),
        ];
        for (value, literal) in transports {
            assert_eq!(value.as_str(), literal);
            let parsed: McpHttpTransport = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let decisions = [
            (McpApprovalDecision::AllowOnce, "allow_once"),
            (McpApprovalDecision::Deny, "deny"),
            (McpApprovalDecision::Cancel, "cancel"),
            (McpApprovalDecision::Timeout, "timeout"),
        ];
        for (value, literal) in decisions {
            assert_eq!(value.as_str(), literal);
            let parsed: McpApprovalDecision = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let audit = [
            (McpAuditDecision::Allow, "allow"),
            (McpAuditDecision::Deny, "deny"),
            (McpAuditDecision::Timeout, "timeout"),
            (McpAuditDecision::Cancel, "cancel"),
            (McpAuditDecision::Read, "read"),
        ];
        for (value, literal) in audit {
            assert_eq!(value.as_str(), literal);
            let parsed: McpAuditDecision = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn the_http_status_keeps_the_null_and_absent_families_apart() {
        let status = McpHttpStatus {
            running: true,
            enabled: true,
            transport: McpHttpTransport::Http,
            url: None,
            port: None,
            configured_port: 0,
            token_last4: None,
            token_configured: true,
            token: None,
        };
        let wire = encode(&status);
        // `?: T | null` legs keep explicit nulls…
        assert_eq!(wire["url"], serde_json::Value::Null);
        assert_eq!(wire["port"], serde_json::Value::Null);
        assert_eq!(wire["tokenLast4"], serde_json::Value::Null);
        // …while the plain `?:` token leg stays absent (the token never
        // rides a status poll).
        assert!(wire.get("token").is_none());
        let back: McpHttpStatus = decode(wire);
        assert_eq!(back, status);

        // The minted-token response carries the token once, with the last-4.
        let minted = McpHttpStatus {
            url: Some("http://127.0.0.1:39187/mcp".into()),
            port: Some(39187),
            token_last4: Some("9f2c".into()),
            token: Some("dz_mcp_9f2c…".into()),
            ..status
        };
        let wire = encode(&minted);
        assert_eq!(wire["token"], "dz_mcp_9f2c…");
        assert_eq!(wire["tokenLast4"], "9f2c");
        // The stopped listener says `none`.
        let stopped: McpHttpStatus = decode(serde_json::json!({
            "running": false,
            "enabled": false,
            "transport": "none",
            "configuredPort": 0,
            "tokenConfigured": false
        }));
        assert_eq!(stopped.transport, McpHttpTransport::None);
    }

    #[test]
    fn an_approval_request_round_trips_its_scope_and_expiry() {
        let request = McpApprovalRequest {
            request_id: "req-1".into(),
            tool: "sql_query".into(),
            connection_id: Some("c1".into()),
            database: Some("app".into()),
            namespace: None,
            sql: Some("SELECT * FROM users".into()),
            arguments: {
                let mut map = serde_json::Map::new();
                map.insert("limit".into(), serde_json::json!(10));
                map
            },
            created_at: "2026-11-20T04:00:00Z".into(),
            expires_at: "2026-11-20T04:01:00Z".into(),
            timeout_ms: 60_000,
        };
        let wire = encode(&request);
        assert_eq!(wire["tool"], "sql_query");
        assert_eq!(wire["arguments"]["limit"], 10);
        assert_eq!(wire["timeoutMs"], 60_000);
        // The absent namespace keeps its explicit null.
        assert_eq!(wire["namespace"], serde_json::Value::Null);
        let back: McpApprovalRequest = decode(wire);
        assert_eq!(back, request);
    }

    #[test]
    fn an_audit_record_round_trips() {
        let record = McpAuditRecord {
            id: "audit-1".into(),
            organization_id: "org".into(),
            principal_id: "user".into(),
            connection_id: None,
            database_name: None,
            namespace: None,
            tool: "fs_read".into(),
            decision: McpAuditDecision::Read,
            row_count: None,
            reason: None,
            index: Some(7),
            request_id: None,
            created_at: "2026-11-20T04:00:00Z".into(),
        };
        let wire = encode(&record);
        assert_eq!(wire["decision"], "read");
        assert_eq!(wire["index"], 7);
        // The scope legs keep explicit nulls.
        assert_eq!(wire["connectionId"], serde_json::Value::Null);
        assert_eq!(wire["reason"], serde_json::Value::Null);
        let back: McpAuditRecord = decode(wire);
        assert_eq!(back, record);
    }

    #[test]
    fn server_id_validation_mirrors_the_rust_kernel_rule() {
        // `^[a-zA-Z0-9_-]+$` — one or more of alphanumerics, `-`, `_`.
        for valid in ["fs", "Postgres_92", "tool-1", "A_b-C9"] {
            assert!(is_valid_mcp_server_id(valid), "{valid:?} must be valid");
        }
        // Empty, whitespace, other punctuation and non-ASCII all fail.
        for invalid in ["", " ", "a b", "a/b", "a.b", "中文", "a!"] {
            assert!(!is_valid_mcp_server_id(invalid), "{invalid:?} must be invalid");
        }
    }
}
