use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDriverCommandStreamRequest {
    #[serde(default)]
    pub db_session_id: Option<String>,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
    /// F1: optional explicit database pin — the session is switched to this
    /// logical database before the command runs (same mechanism as the
    /// query-family commands; `None`/blank keeps the current active database).
    #[serde(default)]
    pub database: Option<String>,
    /// F7: optional target schema (PG-family engines). Rewrite-capable
    /// drivers inline it as a qualified name; others ignore it.
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub apply_result_limit: Option<bool>,
    #[serde(default)]
    pub record_history: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct ExecuteDriverCommandStreamOpts {
    pub apply_result_limit: bool,
    pub record_history: bool,
}

impl Default for ExecuteDriverCommandStreamOpts {
    fn default() -> Self {
        Self {
            apply_result_limit: true,
            record_history: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDriverCommandRequest {
    #[serde(default)]
    pub db_session_id: Option<String>,
    #[serde(default)]
    pub driver_type: Option<String>,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
    /// F1: optional explicit database pin (session-bound commands only —
    /// ignored for unbound `driverType` requests). See
    /// `ensure_session_database` for the switching semantics.
    #[serde(default)]
    pub database: Option<String>,
    /// F7: optional target schema (PG-family engines). Passed through into
    /// the command input for SQL commands so rewrite-capable drivers can
    /// inline it; ignored otherwise.
    #[serde(default)]
    pub schema: Option<String>,
}
