//! JSON-RPC 2.0 contract constants shared with `datazen-jdbc-agent`.
//!
//! Full method schemas live in `protocol.md`. This module holds version and
//! method name constants used by the Rust client.

/// Wire protocol version negotiated in `agent.hello`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Agent semver reported by the bundled / companion jar (keep in sync with Java).
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

/// Build the `agent.hello` result payload shape expected by Host.
pub fn agent_version_info() -> serde_json::Value {
    serde_json::json!({
        "agentVersion": AGENT_VERSION,
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": ["jdbc", "session", "query.stream", "tx"]
    })
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
}
