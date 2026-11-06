//! Exact-text write proposals for agent-requested mutations (T24).
//!
//! A proposal is *not* an authorization to write. It binds one connection, one
//! database, the exact SQL text, its SHA-256 digest and a short expiry to an
//! unguessable proposal id. The id doubles as the one-time apply token: the
//! first successful apply consumes it, and every field must match verbatim.
//!
//! The module deliberately has no execution path. Hosts execute through the
//! existing single `DatabaseToolHost::execute_target` entry after [`WriteProposalStore::apply`]
//! has re-run the SQL guard and production/read-only checks.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use datazen_driver_api::ConnectionConfig;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use crate::execution::domain::sql_guard::{
    self, find_main_verb_index, sql_main_keyword, tokenize_sql,
};

/// Short-lived proposals: long enough to read the SQL, short enough that a
/// forgotten tab cannot be replayed later in the day.
pub const WRITE_PROPOSAL_TTL_SECS: i64 = 300;

/// Five-minute TTL used by [`WriteProposalStore::new`].
pub fn default_write_proposal_ttl() -> Duration {
    Duration::seconds(WRITE_PROPOSAL_TTL_SECS)
}

/// Defensive cap; a write proposal is human-reviewed SQL, not a data dump.
pub const MAX_WRITE_SQL_BYTES: usize = 256 * 1024;

/// Input for [`WriteProposalStore::propose`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProposalRequest {
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    /// Optional editor SQL used only to produce a review diff. Never executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_sql: Option<String>,
}

/// One statement summary inside a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteStatement {
    pub index: usize,
    pub verb: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub sql_hash: String,
}

/// Static impact preview. No statement is executed to build it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteImpact {
    pub statement_count: usize,
    pub tables: Vec<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub writes_data: bool,
    pub schema_changes: bool,
    pub summary: String,
}

/// A reviewable write proposal returned by `propose_write`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProposal {
    pub proposal_id: String,
    /// Alias of [`Self::proposal_id`], named for the one-time apply token.
    pub token: String,
    pub connection_id: String,
    pub database: String,
    /// Exact SQL bound to this proposal. The apply call must match byte-for-byte.
    pub sql: String,
    pub sql_hash: String,
    pub created_at: String,
    pub expires_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub statements: Vec<WriteStatement>,
    pub impact: WriteImpact,
    /// Production environments may be inspected but never applied.
    pub production_protected: bool,
    /// False for production or read-only connections. The proposal is still
    /// shown so the user understands why the write is blocked.
    pub apply_allowed: bool,
}

/// Input for [`WriteProposalStore::apply`]. All four binding fields are required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWriteRequest {
    pub token: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
}

/// A proposal that passed every binding and guard check and was consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedWrite {
    pub proposal: WriteProposal,
}

/// Typed refusal. Each variant maps to a stable `code` for hosts and tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum WriteProposalError {
    InvalidInput(String),
    NotFound,
    Expired,
    ConnectionMismatch,
    DatabaseMismatch,
    SqlMismatch,
    ProductionProtected,
    ReadOnly,
    NotWriteSql,
    GuardRejected(String),
}

impl WriteProposalError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::NotFound => "not_found",
            Self::Expired => "expired",
            Self::ConnectionMismatch => "connection_mismatch",
            Self::DatabaseMismatch => "database_mismatch",
            Self::SqlMismatch => "sql_mismatch",
            Self::ProductionProtected => "production_protected",
            Self::ReadOnly => "read_only",
            Self::NotWriteSql => "not_write_sql",
            Self::GuardRejected(_) => "guard_rejected",
        }
    }
}

impl std::fmt::Display for WriteProposalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid write proposal: {message}"),
            Self::NotFound => write!(f, "write proposal not found or already used"),
            Self::Expired => write!(f, "write proposal expired; request a new proposal"),
            Self::ConnectionMismatch => write!(f, "write proposal connection does not match"),
            Self::DatabaseMismatch => write!(f, "write proposal database does not match"),
            Self::SqlMismatch => write!(f, "write proposal SQL does not match exactly"),
            Self::ProductionProtected => {
                write!(f, "production connections never allow writes")
            }
            Self::ReadOnly => write!(f, "connection is read-only"),
            Self::NotWriteSql => write!(
                f,
                "not a write statement: only INSERT/UPDATE/DELETE/DDL or a write PRAGMA can be proposed; SELECT/SHOW/EXPLAIN and read-only PRAGMA are rejected"
            ),
            Self::GuardRejected(message) => write!(f, "SQL guard rejected the write: {message}"),
        }
    }
}

impl std::error::Error for WriteProposalError {}

#[derive(Debug)]
struct StoredProposal {
    proposal: WriteProposal,
}

/// In-memory, short-lived proposal registry. Hosts own one instance; the
/// store never persists SQL (only the proposal returned to the reviewing UI).
#[derive(Debug)]
pub struct WriteProposalStore {
    inner: Mutex<HashMap<String, StoredProposal>>,
    ttl: Duration,
    max_entries: usize,
}

impl Default for WriteProposalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteProposalStore {
    pub fn new() -> Self {
        Self::with_ttl(default_write_proposal_ttl())
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
            max_entries: 256,
        }
    }

    /// Create a proposal. This never executes SQL. Production/read-only
    /// connections still get a reviewable proposal, marked `apply_allowed =
    /// false`, so the UI can explain the refusal before the user clicks Allow.
    pub fn propose(
        &self,
        request: &WriteProposalRequest,
        config: &ConnectionConfig,
        _safe_mode: bool,
        now: DateTime<Utc>,
    ) -> Result<WriteProposal, WriteProposalError> {
        validate_request(request)?;
        let sql_hash = sql_hash(&request.sql);
        let statements = summarize_statements(&request.sql);
        let effects = collect_write_effects(&request.sql);
        let impact = build_impact(&statements, &effects);
        let production_protected = is_production_connection(config);
        let read_only = config.read_only;
        let apply_allowed = !production_protected && !read_only;
        let created_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let expires_at = (now + self.ttl).to_rfc3339_opts(SecondsFormat::Secs, true);
        let proposal_id = Uuid::new_v4().to_string();
        let diff = request
            .previous_sql
            .as_deref()
            .map(|previous| diff_sql(previous, &request.sql));

        let proposal = WriteProposal {
            proposal_id: proposal_id.clone(),
            token: proposal_id.clone(),
            connection_id: request.connection_id.trim().to_string(),
            database: request.database.trim().to_string(),
            sql: request.sql.clone(),
            sql_hash,
            created_at,
            expires_at,
            diff,
            statements,
            impact,
            production_protected,
            apply_allowed,
        };

        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        prune_expired(&mut inner, now);
        if inner.len() >= self.max_entries {
            if let Some(oldest) = inner
                .iter()
                .min_by_key(|(_, stored)| stored.proposal.created_at.as_str())
                .map(|(id, _)| id.clone())
            {
                inner.remove(&oldest);
            }
        }
        inner.insert(
            proposal_id,
            StoredProposal {
                proposal: proposal.clone(),
            },
        );
        Ok(proposal)
    }

    /// Validate all four bindings, re-run the safety guard, then atomically
    /// consume the token. A mismatch does not consume the proposal; a
    /// successful apply does, before the host executes anything.
    pub fn apply(
        &self,
        request: &ApplyWriteRequest,
        config: &ConnectionConfig,
        safe_mode: bool,
        now: DateTime<Utc>,
    ) -> Result<AppliedWrite, WriteProposalError> {
        let token = request.token.trim();
        if token.is_empty() {
            return Err(WriteProposalError::InvalidInput(
                "token is required".to_string(),
            ));
        }
        ensure_write_sql(&request.sql)?;
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(stored) = inner.get(token) else {
            return Err(WriteProposalError::NotFound);
        };
        if now >= parse_expiry(&stored.proposal.expires_at)? {
            inner.remove(token);
            return Err(WriteProposalError::Expired);
        }
        if stored.proposal.connection_id != request.connection_id.trim() {
            return Err(WriteProposalError::ConnectionMismatch);
        }
        if stored.proposal.database != request.database.trim() {
            return Err(WriteProposalError::DatabaseMismatch);
        }
        if stored.proposal.sql != request.sql || stored.proposal.sql_hash != sql_hash(&request.sql)
        {
            return Err(WriteProposalError::SqlMismatch);
        }
        if is_production_connection(config) || stored.proposal.production_protected {
            return Err(WriteProposalError::ProductionProtected);
        }
        if config.read_only {
            return Err(WriteProposalError::ReadOnly);
        }
        if let Err(message) =
            sql_guard::check_sql(&stored.proposal.sql, config.read_only, safe_mode)
        {
            if config.read_only {
                return Err(WriteProposalError::ReadOnly);
            }
            return Err(WriteProposalError::GuardRejected(message));
        }
        let applied = inner
            .remove(token)
            .ok_or(WriteProposalError::NotFound)?
            .proposal;
        Ok(AppliedWrite { proposal: applied })
    }

    /// Remove a proposal the user explicitly denied. Returns the removed
    /// proposal so the host can audit its target without keeping SQL.
    pub fn deny(
        &self,
        token: &str,
        _now: DateTime<Utc>,
    ) -> Result<Option<WriteProposal>, WriteProposalError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(WriteProposalError::InvalidInput(
                "token is required".to_string(),
            ));
        }
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(stored) = inner.remove(token) else {
            return Ok(None);
        };
        // Denying an expired proposal is still a valid cleanup: the UI may
        // race the wall clock and must be able to dismiss it. Expiry only
        // forbids apply, never deny.
        Ok(Some(stored.proposal))
    }

    /// Number of live proposals (test/inspection helper).
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn validate_request(request: &WriteProposalRequest) -> Result<(), WriteProposalError> {
    if request.connection_id.trim().is_empty() {
        return Err(WriteProposalError::InvalidInput(
            "connection_id is required".to_string(),
        ));
    }
    if request.sql.trim().is_empty() {
        return Err(WriteProposalError::InvalidInput(
            "sql is required".to_string(),
        ));
    }
    if request.sql.len() > MAX_WRITE_SQL_BYTES {
        return Err(WriteProposalError::InvalidInput(format!(
            "sql exceeds {MAX_WRITE_SQL_BYTES} bytes"
        )));
    }
    if request.sql.as_bytes().contains(&0) {
        return Err(WriteProposalError::InvalidInput(
            "sql contains a null byte".to_string(),
        ));
    }
    ensure_write_sql(&request.sql)?;
    Ok(())
}

fn parse_expiry(value: &str) -> Result<DateTime<Utc>, WriteProposalError> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| WriteProposalError::InvalidInput("invalid expiry".to_string()))
}

fn prune_expired(inner: &mut HashMap<String, StoredProposal>, now: DateTime<Utc>) {
    inner.retain(|_, stored| {
        parse_expiry(&stored.proposal.expires_at)
            .map(|expiry| now < expiry)
            .unwrap_or(false)
    });
}

/// Reject read-only SQL before a proposal is minted and again before apply.
/// This is intentionally stricter than `check_sql`: a writable connection with
/// Safe Mode off still must not turn the confirmation channel into a read path.
fn ensure_write_sql(sql: &str) -> Result<(), WriteProposalError> {
    if sql_guard::is_write_only_sql(sql) {
        Ok(())
    } else {
        Err(WriteProposalError::NotWriteSql)
    }
}

/// SHA-256 of the exact SQL text, lowercase hex.
pub fn sql_hash(sql: &str) -> String {
    format!("{:x}", Sha256::digest(sql.as_bytes()))
}

fn summarize_statements(sql: &str) -> Vec<WriteStatement> {
    let statement_texts = sql_guard::split_sql_statements(sql);
    let mut statements = Vec::with_capacity(statement_texts.len());
    for (index, statement) in statement_texts.into_iter().enumerate() {
        let tokens = tokenize_sql(&statement);
        let verb = sql_main_keyword(&statement).unwrap_or_else(|| "UNKNOWN".to_string());
        let table = statement_table(&verb, &tokens);
        let has_where = tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("WHERE"));
        let destructive =
            matches!(verb.as_str(), "DROP" | "TRUNCATE") || (verb == "DELETE" && !has_where);
        statements.push(WriteStatement {
            index,
            verb,
            table,
            has_where,
            destructive,
            sql_hash: sql_hash(statement.trim()),
        });
    }
    statements
}

fn statement_table(verb: &str, tokens: &[String]) -> Option<String> {
    let start = find_main_verb_index(tokens)?;
    let after = &tokens[start.saturating_add(1)..];
    let value = match verb {
        "INSERT" | "MERGE" | "UPSERT" | "REPLACE" => token_after(after, "INTO"),
        "UPDATE" => after.first().map(String::as_str),
        "DELETE" => token_after(after, "FROM"),
        "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "RENAME" | "COMMENT" => {
            token_after(after, "TABLE")
                .or_else(|| token_after(after, "VIEW"))
                .or_else(|| token_after(after, "INDEX"))
                .or_else(|| {
                    after
                        .iter()
                        .position(|token| token.eq_ignore_ascii_case("TABLE"))
                        .and_then(|pos| after.get(pos + 1).map(String::as_str))
                })
        }
        _ => None,
    }?;
    let cleaned = value
        .trim_matches('"')
        .trim_matches('`')
        .trim_matches('\'')
        .to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn token_after<'a>(tokens: &'a [String], keyword: &str) -> Option<&'a str> {
    tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case(keyword))
        .and_then(|position| tokens.get(position + 1).map(String::as_str))
}

/// One actual data-changing effect, including DML found inside a CTE. The
/// top-level statement list cannot represent this: a `WITH ... SELECT` still
/// deletes rows in its CTE.
#[derive(Debug, Clone)]
struct WriteEffect {
    verb: String,
    table: Option<String>,
    has_where: bool,
    destructive: bool,
}

fn collect_write_effects(sql: &str) -> Vec<WriteEffect> {
    let mut effects = Vec::new();
    for statement in sql_guard::split_sql_statements(sql) {
        let tokens = tokenize_sql(&statement);
        if let Some(effect) = write_effect(&tokens) {
            effects.push(effect);
        }
        for body in sql_guard::data_modifying_cte_bodies(&tokens) {
            if let Some(effect) = write_effect(body) {
                effects.push(effect);
            }
        }
    }
    effects
}

fn write_effect(tokens: &[String]) -> Option<WriteEffect> {
    let main = find_main_verb_index(tokens)?;
    let verb = tokens.get(main)?.to_uppercase();
    if !is_effect_verb(&verb) {
        return None;
    }
    let has_where = sql_guard::has_top_level_where(&tokens.join(" "));
    Some(WriteEffect {
        table: statement_table(&verb, tokens),
        destructive: matches!(verb.as_str(), "DROP" | "TRUNCATE")
            || (verb == "DELETE" && !has_where),
        verb,
        has_where,
    })
}

fn is_effect_verb(verb: &str) -> bool {
    matches!(
        verb,
        "INSERT"
            | "UPDATE"
            | "DELETE"
            | "MERGE"
            | "UPSERT"
            | "REPLACE"
            | "COPY"
            | "LOAD"
            | "CREATE"
            | "ALTER"
            | "DROP"
            | "TRUNCATE"
            | "RENAME"
            | "COMMENT"
            | "GRANT"
            | "REVOKE"
    )
}

fn build_impact(statements: &[WriteStatement], effects: &[WriteEffect]) -> WriteImpact {
    let mut tables = Vec::new();
    for effect in effects {
        if let Some(table) = &effect.table {
            if !tables.iter().any(|existing| existing == table) {
                tables.push(table.clone());
            }
        }
    }
    if effects.is_empty() {
        // Keep the preview useful for statements without a data effect (for
        // example a write PRAGMA).
        for statement in statements {
            if let Some(table) = &statement.table {
                if !tables.iter().any(|existing| existing == table) {
                    tables.push(table.clone());
                }
            }
        }
    }
    let has_where = effects.iter().all(|effect| match effect.verb.as_str() {
        "UPDATE" | "DELETE" => effect.has_where,
        _ => true,
    });
    let destructive = effects.iter().any(|effect| effect.destructive);
    let writes_data = effects.iter().any(|effect| {
        matches!(
            effect.verb.as_str(),
            "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "UPSERT" | "REPLACE" | "COPY" | "LOAD"
        )
    });
    let schema_changes = effects.iter().any(|effect| {
        matches!(
            effect.verb.as_str(),
            "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "RENAME" | "COMMENT" | "GRANT" | "REVOKE"
        )
    });
    let verbs: Vec<String> = if effects.is_empty() {
        statements
            .iter()
            .map(|statement| statement.verb.clone())
            .collect()
    } else {
        effects.iter().map(|effect| effect.verb.clone()).collect()
    };
    let summary = format!(
        "{} statement(s): {}{}",
        statements.len(),
        if verbs.is_empty() {
            "none".to_string()
        } else {
            verbs.join(", ")
        },
        if destructive {
            " (destructive)"
        } else if !has_where {
            " (no WHERE on a data change)"
        } else {
            ""
        }
    );
    WriteImpact {
        statement_count: statements.len(),
        tables,
        has_where,
        destructive,
        writes_data,
        schema_changes,
        summary,
    }
}

/// Coarse line diff for review only: unchanged lines are context, removed
/// lines are prefixed `-`, added lines `+`. It is deliberately not a merge
/// algorithm; the exact SQL is the contract, the diff is a reading aid.
pub fn diff_sql(previous: &str, proposed: &str) -> String {
    if previous == proposed {
        return String::new();
    }
    let previous_lines: Vec<&str> = previous.lines().collect();
    let proposed_lines: Vec<&str> = proposed.lines().collect();
    let mut out = Vec::new();
    let mut prefix = 0;
    while prefix < previous_lines.len()
        && prefix < proposed_lines.len()
        && previous_lines[prefix] == proposed_lines[prefix]
    {
        out.push(format!(" {}", previous_lines[prefix]));
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix + prefix < previous_lines.len()
        && suffix + prefix < proposed_lines.len()
        && previous_lines[previous_lines.len() - 1 - suffix]
            == proposed_lines[proposed_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    for line in &previous_lines[prefix..previous_lines.len() - suffix] {
        out.push(format!("-{line}"));
    }
    for line in &proposed_lines[prefix..proposed_lines.len() - suffix] {
        out.push(format!("+{line}"));
    }
    for line in &previous_lines[previous_lines.len() - suffix..] {
        out.push(format!(" {line}"));
    }
    out.join("\n")
}

/// Production is a safety classification, never a name heuristic. The
/// frontend preset group and explicit environment markers are the only
/// signals; a connection merely named "prod" is not treated as production.
pub fn is_production_connection(config: &ConnectionConfig) -> bool {
    if let Some(group) = config.group.as_deref() {
        let normalized = group.trim().to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "preset:production" | "preset:prod" | "production" | "prod"
        ) {
            return true;
        }
    }
    let Some(options) = config.options.as_ref() else {
        return false;
    };
    for key in [
        "environment",
        "env",
        "environmentType",
        "environment_type",
        "stage",
    ] {
        if let Some(value) = options.get(key).and_then(serde_json::Value::as_str) {
            if matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "production" | "prod" | "live"
            ) {
                return true;
            }
        }
    }
    ["isProduction", "is_production", "production"]
        .iter()
        .any(|key| options.get(*key).and_then(serde_json::Value::as_bool) == Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn config(group: Option<&str>, read_only: bool) -> ConnectionConfig {
        let mut value = json!({
            "id": "conn-1",
            "name": "Local",
            "databaseType": "postgres",
            "database": "app",
            "readOnly": read_only,
        });
        if let Some(group) = group {
            value["group"] = json!(group);
        }
        serde_json::from_value(value).expect("connection config")
    }

    fn request(sql: &str) -> WriteProposalRequest {
        WriteProposalRequest {
            connection_id: "conn-1".into(),
            database: "app".into(),
            sql: sql.into(),
            previous_sql: None,
        }
    }

    fn apply(proposal: &WriteProposal) -> ApplyWriteRequest {
        ApplyWriteRequest {
            token: proposal.token.clone(),
            connection_id: proposal.connection_id.clone(),
            database: proposal.database.clone(),
            sql: proposal.sql.clone(),
        }
    }

    #[test]
    fn proposal_binds_exact_sql_hash_and_short_expiry_without_executing() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let sql = "UPDATE users SET role = 'admin' WHERE id = 42";
        let proposal = store
            .propose(&request(sql), &config(None, false), false, now)
            .unwrap();
        assert_eq!(proposal.connection_id, "conn-1");
        assert_eq!(proposal.database, "app");
        assert_eq!(proposal.sql, sql);
        assert_eq!(proposal.sql_hash, sql_hash(sql));
        assert_eq!(proposal.sql_hash.len(), 64);
        assert_eq!(proposal.token, proposal.proposal_id);
        assert_eq!(proposal.created_at, "2026-01-02T03:04:05Z");
        assert_eq!(proposal.expires_at, "2026-01-02T03:09:05Z");
        assert!(proposal.apply_allowed);
        assert!(!proposal.production_protected);
        assert_eq!(proposal.statements[0].verb, "UPDATE");
        assert_eq!(proposal.statements[0].table.as_deref(), Some("users"));
        assert!(proposal.impact.writes_data);
        assert!(proposal.impact.has_where);
        assert!(proposal.impact.summary.contains("UPDATE"));
        assert_eq!(store.len(), 1, "propose stores a proposal, never executes");
    }

    #[test]
    fn exact_apply_consumes_token_once() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("DELETE FROM logs WHERE id = 1"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        let applied = store
            .apply(&apply(&proposal), &config(None, false), false, now)
            .unwrap();
        assert_eq!(applied.proposal.sql, proposal.sql);
        assert_eq!(store.len(), 0);
        assert_eq!(
            store.apply(&apply(&proposal), &config(None, false), false, now),
            Err(WriteProposalError::NotFound)
        );
    }

    #[test]
    fn tampered_sql_connection_or_database_is_rejected_and_token_survives() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("UPDATE t SET a = 1 WHERE id = 7"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();

        let mut tampered = apply(&proposal);
        tampered.sql = "UPDATE t SET a = 2 WHERE id = 7".into();
        assert_eq!(
            store.apply(&tampered, &config(None, false), false, now),
            Err(WriteProposalError::SqlMismatch)
        );

        let mut tampered = apply(&proposal);
        tampered.connection_id = "conn-2".into();
        assert_eq!(
            store.apply(&tampered, &config(None, false), false, now),
            Err(WriteProposalError::ConnectionMismatch)
        );

        let mut tampered = apply(&proposal);
        tampered.database = "other".into();
        assert_eq!(
            store.apply(&tampered, &config(None, false), false, now),
            Err(WriteProposalError::DatabaseMismatch)
        );

        assert_eq!(store.len(), 1);
        assert!(store
            .apply(&apply(&proposal), &config(None, false), false, now)
            .is_ok());
    }

    #[test]
    fn whitespace_and_case_changes_are_not_the_same_text() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("UPDATE t SET a = 1 WHERE id = 7"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        let mut changed = apply(&proposal);
        changed.sql = "UPDATE t SET a = 1  WHERE id = 7".into();
        assert_eq!(
            store.apply(&changed, &config(None, false), false, now),
            Err(WriteProposalError::SqlMismatch)
        );
    }

    #[test]
    fn expired_token_is_removed_and_rejected() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("UPDATE t SET a = 1 WHERE id = 7"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        let later = now + Duration::seconds(301);
        assert_eq!(
            store.apply(&apply(&proposal), &config(None, false), false, later),
            Err(WriteProposalError::Expired)
        );
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn production_group_is_protected_from_apply() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("DELETE FROM users WHERE id = 1"),
                &config(Some("preset:production"), false),
                false,
                now,
            )
            .unwrap();
        assert!(proposal.production_protected);
        assert!(!proposal.apply_allowed);
        assert_eq!(
            store.apply(
                &apply(&proposal),
                &config(Some("preset:production"), false),
                false,
                now
            ),
            Err(WriteProposalError::ProductionProtected)
        );
    }

    #[test]
    fn environment_markers_protect_without_name_heuristics() {
        let mut marked = config(None, false);
        marked.options = Some(
            json!({ "environment": "prod" })
                .as_object()
                .expect("object")
                .clone(),
        );
        assert!(is_production_connection(&marked));

        let mut named = config(None, false);
        named.name = "production-copy".into();
        assert!(!is_production_connection(&named));
    }

    #[test]
    fn read_only_connection_gets_a_proposal_but_cannot_apply() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("UPDATE t SET a = 1 WHERE id = 7"),
                &config(None, true),
                false,
                now,
            )
            .unwrap();
        assert!(!proposal.apply_allowed);
        assert_eq!(
            store.apply(&apply(&proposal), &config(None, true), false, now),
            Err(WriteProposalError::ReadOnly)
        );
    }

    #[test]
    fn safe_mode_runs_the_guard_again_at_apply_time() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("DROP TABLE users"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(proposal.impact.destructive);
        assert!(matches!(
            store.apply(&apply(&proposal), &config(None, false), true, now),
            Err(WriteProposalError::GuardRejected(_))
        ));
    }

    #[test]
    fn diff_is_review_metadata_only() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let mut req = request("UPDATE t SET a = 2 WHERE id = 7");
        req.previous_sql = Some("UPDATE t SET a = 1 WHERE id = 7".into());
        let proposal = store
            .propose(&req, &config(None, false), false, now)
            .unwrap();
        let diff = proposal.diff.expect("diff");
        assert!(diff.contains("-UPDATE t SET a = 1 WHERE id = 7"));
        assert!(diff.contains("+UPDATE t SET a = 2 WHERE id = 7"));
    }

    #[test]
    fn read_only_sql_never_gets_a_proposal() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        for sql in [
            "SELECT * FROM users",
            "WITH c AS (SELECT 1) SELECT * FROM c",
            "EXPLAIN SELECT * FROM users",
            "EXPLAIN ANALYZE UPDATE users SET role = 'admin' WHERE id = 1",
            "SHOW TABLES",
            "DESCRIBE users",
            "PRAGMA table_info(users)",
            "PRAGMA index_list(users)",
            "PRAGMA database_list",
            "PRAGMA journal_mode",
            "VALUES (1)",
            "SELECT * FROM users FOR UPDATE",
            "UPDATE users SET role = 'admin' WHERE id = 1; SELECT * FROM users",
            "UPDATE users SET role = 'admin' WHERE id = 1; PRAGMA table_info(users)",
            "SELECT * INTO archive FROM users",
            "CALL refresh_stats()",
            "VACUUM",
            "SET search_path = public",
            "-- just a comment",
        ] {
            let error = store
                .propose(&request(sql), &config(None, false), false, now)
                .expect_err(sql);
            assert!(
                matches!(error, WriteProposalError::NotWriteSql),
                "{sql}: {error}"
            );
            assert_eq!(error.code(), "not_write_sql");
            assert_eq!(store.len(), 0, "{sql} must not be stored");
        }
    }

    #[test]
    fn write_sql_and_data_modifying_ctes_are_allowed() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        for sql in [
            "INSERT INTO users (id) VALUES (1)",
            "UPDATE users SET role = 'admin' WHERE id = 1",
            "DELETE FROM users WHERE id = 1",
            "CREATE TABLE audit_log (id bigint)",
            "PRAGMA journal_mode = WAL",
            "PRAGMA foreign_keys=ON",
            "PRAGMA journal_mode(WAL)",
            "PRAGMA foreign_keys(ON)",
            "UPDATE users SET role = 'admin' WHERE id = 1; -- done",
            "WITH d AS (DELETE FROM users WHERE id = 1 RETURNING *) SELECT * FROM d",
            "UPDATE users SET role = 'admin' WHERE id = 1; DELETE FROM users WHERE id = 2",
        ] {
            store
                .propose(&request(sql), &config(None, false), false, now)
                .unwrap_or_else(|error| panic!("{sql} should be a write: {error}"));
        }
    }

    #[test]
    fn data_modifying_cte_impact_comes_from_nested_dml() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(
            proposal.impact.writes_data,
            "a data-modifying CTE is a write: {:?}",
            proposal.impact
        );
        assert!(proposal.impact.tables.iter().any(|table| table == "users"));
        assert!(proposal.impact.summary.contains("DELETE"));
        assert!(!proposal.impact.has_where);
        assert!(proposal.impact.destructive);
    }

    #[test]
    fn nested_and_multiple_ctes_contribute_all_write_effects() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let nested = store
            .propose(
                &request(
                    "WITH outer AS (WITH inner AS (DELETE FROM users WHERE id = 1 RETURNING *) SELECT * FROM inner) SELECT * FROM outer",
                ),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(nested.impact.writes_data);
        assert!(nested.impact.tables.iter().any(|table| table == "users"));
        assert!(nested.impact.summary.contains("DELETE"));
        assert_eq!(
            nested.impact.summary.matches("DELETE").count(),
            1,
            "nested CTE effects must not be collected twice"
        );

        let multiple = store
            .propose(
                &request(
                    "WITH d AS (DELETE FROM users WHERE id = 1 RETURNING *), u AS (UPDATE accounts SET role = 'admin' WHERE id = 2 RETURNING *) SELECT * FROM d, u",
                ),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(multiple.impact.writes_data);
        assert!(multiple.impact.tables.iter().any(|table| table == "users"));
        assert!(multiple
            .impact
            .tables
            .iter()
            .any(|table| table == "accounts"));
        assert!(multiple.impact.summary.contains("DELETE"));
        assert!(multiple.impact.summary.contains("UPDATE"));
        assert!(multiple.impact.has_where);
    }

    #[test]
    fn safe_mode_guards_nested_cte_updates_and_deletes() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let unsafely = store
            .propose(
                &request("WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(matches!(
            store.apply(&apply(&unsafely), &config(None, false), true, now),
            Err(WriteProposalError::GuardRejected(_))
        ));
        assert_eq!(store.len(), 1, "a guarded apply must not consume the token");

        let nested = store
            .propose(
                &request(
                    "WITH outer AS (WITH inner AS (UPDATE users SET role = 'admin' RETURNING *) SELECT * FROM inner) SELECT * FROM outer",
                ),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(matches!(
            store.apply(&apply(&nested), &config(None, false), true, now),
            Err(WriteProposalError::GuardRejected(_))
        ));

        let safely = store
            .propose(
                &request("WITH d AS (DELETE FROM users WHERE id = 1 RETURNING *) SELECT * FROM d"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(store
            .apply(&apply(&safely), &config(None, false), true, now)
            .is_ok());
    }

    #[test]
    fn apply_rejects_read_only_sql_even_for_a_stored_proposal() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let mut proposal = store
            .propose(
                &request("UPDATE users SET role = 'admin' WHERE id = 1"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        // Simulate a proposal written by an older build: apply must still
        // refuse to turn it into a read execution.
        proposal.sql = "SELECT * FROM users".into();
        proposal.sql_hash = sql_hash(&proposal.sql);
        store.inner.lock().unwrap().insert(
            proposal.proposal_id.clone(),
            StoredProposal {
                proposal: proposal.clone(),
            },
        );
        let error = store
            .apply(&apply(&proposal), &config(None, false), false, now)
            .expect_err("read-only apply");
        assert_eq!(error, WriteProposalError::NotWriteSql);
        assert_eq!(
            store.len(),
            1,
            "a rejected apply must not consume the token"
        );
    }

    #[test]
    fn empty_or_oversized_sql_is_rejected() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert!(matches!(
            store.propose(&request("   "), &config(None, false), false, now),
            Err(WriteProposalError::InvalidInput(_))
        ));
        let huge = "x".repeat(MAX_WRITE_SQL_BYTES + 1);
        assert!(matches!(
            store.propose(&request(&huge), &config(None, false), false, now),
            Err(WriteProposalError::InvalidInput(_))
        ));
    }

    #[test]
    fn deny_removes_proposal() {
        let store = WriteProposalStore::new();
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let proposal = store
            .propose(
                &request("DELETE FROM t WHERE id = 1"),
                &config(None, false),
                false,
                now,
            )
            .unwrap();
        assert!(store.deny(&proposal.token, now).unwrap().is_some());
        assert_eq!(store.len(), 0);
    }
}
