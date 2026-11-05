//! Query safety and the injectable execution port.
//!
//! The CLI owns policy (flags, production refusal); drivers own dialect and
//! execution. A fake executor in tests exercises the same path as a live
//! driver, so flag handling is never verified against a mock database.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use datazen::execution::domain::sql_guard::{
    check_sql, find_main_verb_index, normalize_fullwidth, reject_null_bytes,
    strip_comments_for_tokenize, tokenize_sql,
};
use datazen_driver_api::{create_driver, ConnectionConfig, QueryResult, Value};
use serde::Serialize;

use crate::error::{
    CliError, E_DANGEROUS_SQL, E_DRIVER, E_PRODUCTION_WRITE, E_READ_ONLY, E_TIMEOUT, E_UNSUPPORTED,
    E_WRITE_FLAG,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Safety {
    pub allow_writes: bool,
    pub allow_dangerous_sql: bool,
    /// Explicit operator assertion. Used in addition to group/name/option
    /// detection, never as a way to *disable* protection.
    pub production: bool,
}

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub connection_id: String,
    pub sql: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutput {
    pub connection: String,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
    pub truncated: bool,
}

impl QueryOutput {
    fn from_result(connection: &str, mut result: QueryResult, limit: usize) -> Self {
        let truncated = result.rows.len() > limit;
        result.rows.truncate(limit);
        let columns = result
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let column_types = result
            .columns
            .iter()
            .map(|column| column.data_type.clone())
            .collect();
        let rows = result
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(value_to_json).collect())
            .collect();
        Self {
            connection: connection.to_string(),
            columns,
            column_types,
            rows,
            rows_affected: result.rows_affected,
            execution_time_ms: result.execution_time_ms,
            truncated,
        }
    }

    pub fn table(&self) -> crate::output::Table {
        let mut table = crate::output::Table::new(self.columns.clone());
        for row in &self.rows {
            table.push(row.iter().map(json_cell).collect::<Vec<_>>());
        }
        table
    }
}

fn value_to_json(value: Option<Value>) -> serde_json::Value {
    match value {
        None | Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(value)) => serde_json::Value::Bool(value),
        Some(Value::Integer(value)) => serde_json::Value::Number(value.into()),
        Some(Value::Float(value)) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Value::String(value)) | Some(Value::Timestamp(value)) => {
            serde_json::Value::String(value)
        }
        Some(Value::Bytes(value)) => serde_json::Value::Array(
            value
                .into_iter()
                .map(|byte| serde_json::Value::Number(byte.into()))
                .collect(),
        ),
        Some(Value::Json(value)) => value,
    }
}

fn json_cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[async_trait]
pub trait QueryExecutor: Send + Sync {
    async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError>;
}

/// Driver-backed executor used by the command layer. The config is decrypted
/// before it is placed here and is never serialized into command output.
pub struct DriverExecutor {
    pub config: ConnectionConfig,
}

#[async_trait]
impl QueryExecutor for DriverExecutor {
    async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError> {
        ensure_supported_host_wiring(&self.config)?;
        let sql = limited_sql(&request.sql, request.limit, &self.config.database_type);
        execute_driver_query(&self.config, &sql).await
    }
}

/// Fail closed when a saved profile depends on host-side tunnel wiring the CLI
/// does not assemble. Silently connecting to the origin host would bypass the
/// user's network policy, so an explicit unsupported error is mandatory.
pub fn ensure_supported_host_wiring(config: &ConnectionConfig) -> Result<(), CliError> {
    let ssh_configured = config
        .ssh_tunnel
        .as_ref()
        .is_some_and(|tunnel| tunnel.enabled);
    let other_tunnel = config.tunnel_id.is_some()
        || config.tunnel_kind.is_some()
        || config.http_proxy_tunnel.is_some()
        || config.websocket_tunnel.is_some();
    if ssh_configured || other_tunnel {
        return Err(CliError::new(
            E_UNSUPPORTED,
            format!(
                "connection '{}' uses a tunnel; the CLI does not assemble tunnel transports and refuses to connect directly",
                config.name
            ),
        ));
    }
    Ok(())
}

/// The driver API's SQL limit is a row cap on the server, not a display
/// truncation. Leading/trivia comments are skipped for classification, the cap
/// is inserted before trailing comments, `WITH` only accepts a final SELECT,
/// and multi-statement payloads are left untouched.
fn limited_sql(sql: &str, limit: usize, database_type: &str) -> String {
    let cap = limit.min(u32::MAX as usize - 1) as u32;
    // MySQL treats `#` as a line comment. PostgreSQL uses `#`, `#>`, `#>>` as
    // operators and SQL Server uses `#name` for temp tables, so only enable
    // hash comments for MySQL-family dialects.
    let hash_comments = matches!(
        database_type.trim().to_ascii_lowercase().as_str(),
        "mysql" | "mariadb" | "doris" | "starrocks"
    );
    // A multi-statement payload cannot take one trailing LIMIT. Split on
    // top-level semicolons and cap every statement independently; non-SELECT
    // statements (including CTE DML) pass through untouched.
    let mut out = String::with_capacity(sql.len());
    let mut start = 0;
    let mut index = 0;
    while index < sql.len() {
        let rest = &sql[index..];
        let ch = rest.chars().next().unwrap_or_default();
        if let Some(end) = dollar_quote_end(sql, index) {
            index = end;
        } else if rest.starts_with("--") {
            index = skip_line_comment(sql, index, 2);
        } else if hash_comments && rest.starts_with('#') {
            index = skip_line_comment(sql, index, 1);
        } else if rest.starts_with("/*") {
            index = skip_block_comment(sql, index);
        } else if matches!(ch, '\'' | '"' | '`') {
            index = skip_quoted(sql, index, ch);
        } else {
            if ch == ';' {
                out.push_str(&limited_statement(&sql[start..=index], cap, hash_comments));
                start = index + 1;
            }
            index += ch.len_utf8();
        }
    }
    out.push_str(&limited_statement(&sql[start..], cap, hash_comments));
    out
}

fn limited_statement(sql: &str, cap: u32, hash_comments: bool) -> String {
    let (Some(first), Some(last)) = (
        first_code_index(sql, hash_comments),
        last_code_end(sql, hash_comments),
    ) else {
        return sql.to_string();
    };
    let leading = &sql[..first];
    let core = &sql[first..last];
    let suffix = &sql[last..];
    // Mask strings/identifiers, dollar-quoted bodies and comments before
    // tokenizing. Their content can contain `LIMIT`, `;`, `#` or `--` and must
    // never change where the real cap is inserted.
    let masked = mask_sql(core, hash_comments);
    let tokens = tokenize_sql(&masked);
    let Some(main) = find_main_verb_index(&tokens) else {
        return sql.to_string();
    };
    let main_verb = tokens[main].to_ascii_uppercase();
    let appendable = if main_verb == "SELECT" {
        true
    } else if main_verb == "WITH" {
        final_with_verb(&tokens).as_deref() == Some("SELECT")
    } else {
        false
    };
    if !appendable {
        return sql.to_string();
    }

    let has_limit = tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("LIMIT"));
    if has_limit {
        // The statement already has a LIMIT. Keep it, but wrap the statement
        // with a second cap so a larger user-authored LIMIT cannot make
        // `--limit` a display-only truncation.
        return format!(
            "{leading}SELECT * FROM ({core}) AS datazen_limit LIMIT {}{suffix}",
            cap + 1
        );
    }
    format!("{leading}{core} LIMIT {}{suffix}", cap + 1)
}

/// Skip whitespace and leading comments (`--`, `#`, `/* */`).
fn first_code_index(sql: &str, hash_comments: bool) -> Option<usize> {
    let mut index = 0;
    while index < sql.len() {
        let rest = &sql[index..];
        let ch = rest.chars().next()?;
        if ch.is_whitespace() {
            index += ch.len_utf8();
        } else if rest.starts_with("--") {
            index = skip_line_comment(sql, index, 2);
        } else if hash_comments && rest.starts_with('#') {
            index = skip_line_comment(sql, index, 1);
        } else if rest.starts_with("/*") {
            index = skip_block_comment(sql, index);
        } else {
            return Some(index);
        }
    }
    None
}

/// End index (exclusive) of the last code character, before trailing
/// whitespace/comments and the statement-ending semicolon.
fn last_code_end(sql: &str, hash_comments: bool) -> Option<usize> {
    let mut index = 0;
    let mut last = None;
    while index < sql.len() {
        let rest = &sql[index..];
        let ch = rest.chars().next()?;
        if let Some(end) = dollar_quote_end(sql, index) {
            index = end;
            last = Some(end);
            continue;
        }
        if rest.starts_with("--") {
            index = skip_line_comment(sql, index, 2);
            continue;
        }
        if hash_comments && rest.starts_with('#') {
            index = skip_line_comment(sql, index, 1);
            continue;
        }
        if rest.starts_with("/*") {
            index = skip_block_comment(sql, index);
            continue;
        }
        if ch.is_whitespace() || ch == ';' {
            index += ch.len_utf8();
            continue;
        }
        if matches!(ch, '\'' | '"' | '`') {
            index = skip_quoted(sql, index, ch);
            last = Some(index);
            continue;
        }
        index += ch.len_utf8();
        last = Some(index);
    }
    last
}

/// Replace comments and quoted/dollar-quoted bodies with spaces so keyword
/// scanning cannot see tokens that are actually data.
fn mask_sql(sql: &str, hash_comments: bool) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut index = 0;
    while index < sql.len() {
        let rest = &sql[index..];
        let ch = rest.chars().next().unwrap_or_default();
        let end = if let Some(end) = dollar_quote_end(sql, index) {
            Some(end)
        } else if rest.starts_with("--") {
            Some(skip_line_comment(sql, index, 2))
        } else if hash_comments && rest.starts_with('#') {
            Some(skip_line_comment(sql, index, 1))
        } else if rest.starts_with("/*") {
            Some(skip_block_comment(sql, index))
        } else if matches!(ch, '\'' | '"' | '`') {
            Some(skip_quoted(sql, index, ch))
        } else {
            None
        };
        if let Some(end) = end {
            out.push(' ');
            index = end;
        } else {
            out.push(ch);
            index += ch.len_utf8();
        }
    }
    out
}

fn skip_line_comment(sql: &str, mut index: usize, marker_len: usize) -> usize {
    index += marker_len;
    while index < sql.len() {
        let ch = sql[index..].chars().next().unwrap_or('\n');
        index += ch.len_utf8();
        if ch == '\n' {
            break;
        }
    }
    index
}

fn skip_block_comment(sql: &str, mut index: usize) -> usize {
    index += 2;
    while index < sql.len() {
        if sql[index..].starts_with("*/") {
            return index + 2;
        }
        index += sql[index..].chars().next().unwrap_or(' ').len_utf8();
    }
    index
}

fn skip_quoted(sql: &str, mut index: usize, quote: char) -> usize {
    index += quote.len_utf8();
    while index < sql.len() {
        let ch = sql[index..].chars().next().unwrap_or(quote);
        index += ch.len_utf8();
        if ch == quote {
            if sql[index..].starts_with(quote) {
                // SQL escapes a quote by doubling it.
                index += quote.len_utf8();
                continue;
            }
            break;
        }
    }
    index
}

/// If `index` starts a PostgreSQL dollar-quoted string, return the index just
/// past its closing delimiter (or end-of-input when unterminated).
///
/// Tags are the PostgreSQL `$tag$` shape; positional parameters such as `$1`
/// are not delimiters because they do not have a second `$`.
fn dollar_quote_end(sql: &str, index: usize) -> Option<usize> {
    let rest = sql.get(index..)?;
    if !rest.starts_with('$') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut tag_end = 1;
    while tag_end < bytes.len() {
        let byte = bytes[tag_end];
        if byte == b'$' {
            break;
        }
        if !byte.is_ascii_alphanumeric() && byte != b'_' {
            return None;
        }
        tag_end += 1;
    }
    if tag_end >= bytes.len() {
        return None;
    }
    let delimiter = &rest[..=tag_end];
    let after = index + delimiter.len();
    Some(
        sql[after..]
            .find(delimiter)
            .map_or(sql.len(), |offset| after + offset + delimiter.len()),
    )
}

/// For `WITH ...`, the final top-level statement verb. CTE bodies live inside
/// parentheses, so only depth-zero verbs are candidates.
fn final_with_verb(tokens: &[String]) -> Option<String> {
    let main = find_main_verb_index(tokens)?;
    if !tokens[main].eq_ignore_ascii_case("WITH") {
        return None;
    }
    let mut depth = 0usize;
    let mut final_verb = None;
    for token in tokens.iter().skip(main + 1) {
        match token.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                let upper = token.to_ascii_uppercase();
                if matches!(
                    upper.as_str(),
                    "SELECT"
                        | "INSERT"
                        | "UPDATE"
                        | "DELETE"
                        | "MERGE"
                        | "REPLACE"
                        | "VALUES"
                        | "TABLE"
                ) {
                    final_verb = Some(upper);
                }
            }
            _ => {}
        }
    }
    final_verb
}

async fn execute_driver_query(
    config: &ConnectionConfig,
    sql: &str,
) -> Result<QueryResult, CliError> {
    let driver = create_driver(&config.database_type).ok_or_else(|| {
        CliError::new(
            E_DRIVER,
            format!(
                "driver '{}' is not compiled into this CLI build",
                config.database_type
            ),
        )
    })?;
    let handle = driver
        .connect(config)
        .await
        .map_err(|error| CliError::new(E_DRIVER, format!("connection failed: {error}")))?;
    let result = driver
        .query(&handle, sql)
        .await
        .map_err(|error| CliError::new(E_DRIVER, format!("query failed: {error}")));
    let _ = driver.disconnect(handle).await;
    result
}

pub fn is_production(config: &ConnectionConfig, explicit: bool) -> bool {
    if explicit {
        return true;
    }
    let option_production = config
        .options
        .as_ref()
        .and_then(|options| options.get("environment"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("production"));
    if option_production {
        return true;
    }
    [config.name.as_str(), config.group.as_deref().unwrap_or("")]
        .iter()
        .any(|label| {
            label
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|part| {
                    part.eq_ignore_ascii_case("prod") || part.eq_ignore_ascii_case("production")
                })
        })
}

/// Enforce the default read-only policy and the two independent write flags.
/// The production check comes before flag handling: no flag combination may
/// turn a production write into a success.
pub fn guard_sql(sql: &str, config: &ConnectionConfig, safety: Safety) -> Result<(), CliError> {
    reject_null_bytes(sql).map_err(|message| CliError::new(E_READ_ONLY, message))?;
    let normalized = normalize_fullwidth(sql);
    let tokens = tokenize_sql(&strip_comments_for_tokenize(&normalized));
    let write = check_sql(&normalized, true, false).is_err();
    let mutating_pragma = has_mutating_pragma(&tokens);
    let read_only_pragma = is_read_only_pragma(&tokens);
    let dangerous = tokens.iter().any(|token| is_dangerous_keyword(token));
    let admin_state_change = tokens.iter().any(|token| is_state_changing_verb(token));
    // Production protection is deliberately broader than `check_sql`: admin
    // verbs such as ATTACH/DETACH/VACUUM, session/transaction control, and
    // mutating PRAGMA assignments can change state without appearing in the DML
    // verb table. A read-only PRAGMA introspection query is the only PRAGMA form
    // allowed to pass untouched.
    let state_changing =
        write || admin_state_change || mutating_pragma || (dangerous && !read_only_pragma);

    if state_changing && is_production(config, safety.production) {
        return Err(CliError::new(
            E_PRODUCTION_WRITE,
            "state-changing SQL is refused on production connections",
        ));
    }
    if config.read_only && state_changing {
        return Err(CliError::new(
            E_READ_ONLY,
            format!(
                "connection '{}' is read-only; state-changing SQL is not allowed",
                config.name
            ),
        ));
    }

    let dangerous = dangerous && !read_only_pragma;
    if dangerous {
        if !safety.allow_dangerous_sql {
            if !safety.allow_writes {
                return Err(CliError::new(
                    E_READ_ONLY,
                    "writes are disabled; pass --allow-writes to enable them",
                ));
            }
            return Err(CliError::new(
                E_DANGEROUS_SQL,
                "this statement needs --allow-dangerous-sql in addition to --allow-writes",
            ));
        }
        if !safety.allow_writes {
            return Err(CliError::new(
                E_WRITE_FLAG,
                "--allow-dangerous-sql does not imply --allow-writes",
            ));
        }
    } else if write && !safety.allow_writes {
        return Err(CliError::new(
            E_READ_ONLY,
            "writes are disabled; pass --allow-writes to enable them",
        ));
    }

    Ok(())
}

fn is_state_changing_verb(verb: &str) -> bool {
    matches!(
        verb.to_ascii_uppercase().as_str(),
        "ANALYZE"
            | "BEGIN"
            | "CHECKPOINT"
            | "COMMIT"
            | "DISCARD"
            | "END"
            | "LOCK"
            | "RELEASE"
            | "ROLLBACK"
            | "SAVEPOINT"
            | "SET"
            | "START"
            | "UNLOCK"
            | "USE"
    )
}

/// Return the pragma name and whether it has a parenthesized argument.
fn pragma_name(tokens: &[String], main: usize) -> Option<(String, bool)> {
    let mut index = main + 1;
    if tokens.get(index + 1).is_some_and(|token| token == ".") {
        index += 2;
    }
    let raw = tokens.get(index)?;
    // The tokenizer keeps `schema.name` as one token (the dot is not a
    // separator), so normalise `PRAGMA main.table_info` to its pragma name.
    let name = raw.rsplit('.').next().unwrap_or(raw).to_ascii_lowercase();
    let has_arg = tokens.get(index + 1).is_some_and(|token| token == "(");
    Some((name, has_arg))
}

fn is_read_only_pragma_at(tokens: &[String], main: usize) -> bool {
    let Some((name, has_arg)) = pragma_name(tokens, main) else {
        return false;
    };
    if tokens.iter().any(|token| token == "=") {
        return false;
    }
    if has_arg && !PRAGMA_ARG_READS.contains(&name.as_str()) {
        return false;
    }
    PRAGMA_READS.contains(&name.as_str())
}

fn is_read_only_pragma(tokens: &[String]) -> bool {
    find_main_verb_index(tokens).is_some_and(|main| {
        tokens
            .get(main)
            .is_some_and(|verb| verb.eq_ignore_ascii_case("PRAGMA"))
            && is_read_only_pragma_at(tokens, main)
    })
}

/// A multi-statement payload can hide a mutating PRAGMA after a read-only one;
/// inspect every PRAGMA token, not only the first statement.
fn has_mutating_pragma(tokens: &[String]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token.eq_ignore_ascii_case("PRAGMA") && !is_read_only_pragma_at(tokens, index)
    })
}

const PRAGMA_READS: &[&str] = &[
    "application_id",
    "auto_vacuum",
    "automatic_index",
    "busy_timeout",
    "cache_size",
    "cache_spill",
    "case_sensitive_like",
    "cell_size_check",
    "checkpoint_fullfsync",
    "collation_list",
    "compile_options",
    "count_changes",
    "data_version",
    "database_list",
    "defer_foreign_keys",
    "empty_result_callbacks",
    "encoding",
    "foreign_key_check",
    "foreign_key_list",
    "foreign_keys",
    "freelist_count",
    "full_column_names",
    "fullfsync",
    "function_list",
    "ignore_check_constraints",
    "index_info",
    "index_list",
    "index_xinfo",
    "integrity_check",
    "journal_mode",
    "journal_size_limit",
    "legacy_alter_table",
    "locking_mode",
    "max_page_count",
    "mmap_size",
    "module_list",
    "page_count",
    "page_size",
    "pragma_list",
    "query_only",
    "quick_check",
    "read_uncommitted",
    "recursive_triggers",
    "reverse_unordered_selects",
    "schema_version",
    "secure_delete",
    "short_column_names",
    "synchronous",
    "table_info",
    "table_list",
    "table_xinfo",
    "temp_store",
    "threads",
    "trusted_schema",
    "user_version",
    "writable_schema",
];

const PRAGMA_ARG_READS: &[&str] = &[
    "foreign_key_check",
    "foreign_key_list",
    "index_info",
    "index_list",
    "index_xinfo",
    "integrity_check",
    "quick_check",
    "table_info",
    "table_xinfo",
    "table_list",
];

fn is_dangerous_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "CREATE"
            | "ALTER"
            | "DROP"
            | "TRUNCATE"
            | "GRANT"
            | "REVOKE"
            | "VACUUM"
            | "ATTACH"
            | "DETACH"
            | "PRAGMA"
            | "COPY"
            | "LOAD"
            | "CALL"
            | "EXEC"
            | "EXECUTE"
            | "DO"
            | "REINDEX"
            | "CLUSTER"
            | "COMMENT"
            | "RENAME"
    )
}

/// Execute with a hard wall-clock timeout around the injected executor.
pub async fn run_with_executor(
    executor: Arc<dyn QueryExecutor>,
    request: QueryRequest,
    safety: Safety,
    config: &ConnectionConfig,
    timeout: Duration,
) -> Result<QueryOutput, CliError> {
    guard_sql(&request.sql, config, safety)?;
    let connection = request.connection_id.clone();
    let limit = request.limit;
    let result = tokio::time::timeout(timeout, executor.execute(&request))
        .await
        .map_err(|_| {
            CliError::new(
                E_TIMEOUT,
                format!(
                    "query exceeded the {} second timeout",
                    timeout.as_secs_f64()
                ),
            )
        })??;
    Ok(QueryOutput::from_result(&connection, result, limit))
}

#[cfg(test)]
mod tests {
    fn limited_pg(sql: &str, limit: usize) -> String {
        limited_sql(sql, limit, "postgresql")
    }

    fn limited_mysql(sql: &str, limit: usize) -> String {
        limited_sql(sql, limit, "mysql")
    }
    use super::*;
    use datazen_driver_api::{ColumnInfo, ConnectionConfig};
    use std::sync::Mutex;

    fn config(name: &str) -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "c1",
            "name": name,
            "databaseType": "sqlite",
            "host": null,
            "port": null,
            "database": "file:test",
            "schema": null,
            "username": null,
            "password": null,
            "sshTunnel": null,
            "httpProxyTunnel": null,
            "websocketTunnel": null,
            "readOnly": false
        }))
        .unwrap()
    }

    fn result(rows: usize) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: false,
            }],
            rows: (0..rows)
                .map(|index| vec![Some(Value::Integer(index as i64))])
                .collect(),
            rows_affected: None,
            execution_time_ms: 2,
        }
    }

    struct FakeExecutor {
        result: QueryResult,
        seen: Mutex<Vec<QueryRequest>>,
    }

    #[async_trait]
    impl QueryExecutor for FakeExecutor {
        async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError> {
            self.seen.lock().unwrap().push(request.clone());
            Ok(self.result.clone())
        }
    }

    #[test]
    fn default_blocks_writes_and_allow_writes_permits_dml() {
        let config = config("local");
        assert_eq!(
            guard_sql("UPDATE t SET x=1", &config, Safety::default())
                .unwrap_err()
                .code,
            E_READ_ONLY
        );
        assert!(guard_sql(
            "UPDATE t SET x=1",
            &config,
            Safety {
                allow_writes: true,
                ..Safety::default()
            }
        )
        .is_ok());
    }

    #[test]
    fn dangerous_sql_needs_its_own_flag() {
        let config = config("local");
        let write_only = Safety {
            allow_writes: true,
            ..Safety::default()
        };
        assert_eq!(
            guard_sql("DROP TABLE t", &config, write_only)
                .unwrap_err()
                .code,
            E_DANGEROUS_SQL
        );
        assert!(guard_sql(
            "DROP TABLE t",
            &config,
            Safety {
                allow_writes: true,
                allow_dangerous_sql: true,
                ..Safety::default()
            }
        )
        .is_ok());
    }

    #[test]
    fn dangerous_flag_alone_does_not_enable_writes() {
        let config = config("local");
        assert_eq!(
            guard_sql(
                "DROP TABLE t",
                &config,
                Safety {
                    allow_dangerous_sql: true,
                    ..Safety::default()
                }
            )
            .unwrap_err()
            .code,
            E_WRITE_FLAG
        );
    }

    #[test]
    fn read_verb_that_is_dangerous_still_needs_the_dangerous_flag() {
        let config = config("local");
        assert_eq!(
            guard_sql("PRAGMA journal_mode=DELETE", &config, Safety::default())
                .unwrap_err()
                .code,
            E_READ_ONLY
        );
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        assert!(guard_sql("PRAGMA journal_mode=DELETE", &config, both).is_ok());
    }

    #[test]
    fn production_refuses_mutating_pragma_with_both_flags() {
        let config = config("prod-db");
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        assert_eq!(
            guard_sql("PRAGMA journal_mode=WAL", &config, both)
                .unwrap_err()
                .code,
            E_PRODUCTION_WRITE
        );
        assert_eq!(
            guard_sql("PRAGMA user_version=7", &config, both)
                .unwrap_err()
                .code,
            E_PRODUCTION_WRITE
        );
        assert_eq!(
            guard_sql(
                "PRAGMA table_info(items); PRAGMA user_version=7",
                &config,
                both
            )
            .unwrap_err()
            .code,
            E_PRODUCTION_WRITE
        );
    }

    #[test]
    fn production_allows_read_only_pragma_introspection() {
        let config = config("prod-db");
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        assert!(guard_sql("PRAGMA journal_mode", &config, both).is_ok());
        assert!(guard_sql("PRAGMA table_info(items)", &config, both).is_ok());
        assert!(guard_sql("PRAGMA user_version", &config, both).is_ok());
        assert!(guard_sql("PRAGMA main.table_info(items)", &config, both).is_ok());
        assert_eq!(
            guard_sql("PRAGMA main.user_version=7", &config, both)
                .unwrap_err()
                .code,
            E_PRODUCTION_WRITE
        );
        assert_eq!(
            guard_sql("PRAGMA main.journal_mode=WAL", &config, both)
                .unwrap_err()
                .code,
            E_PRODUCTION_WRITE
        );
    }

    #[test]
    fn production_refuses_admin_state_changes_even_with_both_flags() {
        let config = config("prod-db");
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        for sql in [
            "ANALYZE",
            "SET search_path = public",
            "BEGIN",
            "USE other_database",
            "REINDEX",
        ] {
            assert_eq!(
                guard_sql(sql, &config, both).unwrap_err().code,
                E_PRODUCTION_WRITE,
                "{sql}"
            );
        }
    }

    #[test]
    fn production_refuses_admin_verbs_even_with_both_flags() {
        let config = config("prod-db");
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        assert_eq!(
            guard_sql("ATTACH DATABASE 'x' AS aux", &config, both)
                .unwrap_err()
                .code,
            E_PRODUCTION_WRITE
        );
    }

    #[test]
    fn limit_is_pushed_into_select_but_not_dml() {
        assert_eq!(
            limited_pg("SELECT * FROM t", 10),
            "SELECT * FROM t LIMIT 11"
        );
        assert_eq!(
            limited_pg("WITH x AS (SELECT 1) SELECT * FROM x;", 5),
            "WITH x AS (SELECT 1) SELECT * FROM x LIMIT 6;"
        );
        assert_eq!(limited_pg("UPDATE t SET x = 1", 10), "UPDATE t SET x = 1");
        assert_eq!(
            limited_pg("SELECT * FROM t LIMIT 100", 1),
            "SELECT * FROM (SELECT * FROM t LIMIT 100) AS datazen_limit LIMIT 2"
        );
    }

    #[test]
    fn limit_respects_leading_and_trailing_comments() {
        assert_eq!(
            limited_pg("-- lead\nSELECT * FROM t -- tail", 1),
            "-- lead\nSELECT * FROM t LIMIT 2 -- tail"
        );
        assert_eq!(
            limited_pg("/* lead */ SELECT 1; -- tail", 1),
            "/* lead */ SELECT 1 LIMIT 2; -- tail"
        );
        assert_eq!(
            limited_pg("SELECT 1 /* tail */", 1),
            "SELECT 1 LIMIT 2 /* tail */"
        );
    }

    #[test]
    fn limit_ignores_dollar_quotes() {
        assert_eq!(
            limited_pg("SELECT $$a;b$$ AS s -- limit", 1),
            "SELECT $$a;b$$ AS s LIMIT 2 -- limit"
        );
        assert_eq!(
            limited_pg("SELECT $tag$; -- LIMIT 99$tag$ AS s", 1),
            "SELECT $tag$; -- LIMIT 99$tag$ AS s LIMIT 2"
        );
        assert_eq!(
            limited_pg("SELECT 1; SELECT $$x;y$$", 1),
            "SELECT 1 LIMIT 2; SELECT $$x;y$$ LIMIT 2"
        );
    }

    #[test]
    fn hash_is_a_line_comment_only_for_mysql_dialects() {
        // PostgreSQL operators: `#` and `#>` are code, not comments.
        assert_eq!(limited_pg("SELECT 5 # 3", 5), "SELECT 5 # 3 LIMIT 6");
        assert_eq!(
            limited_pg("SELECT '{\"a\":1}'::jsonb #> '{a}'", 5),
            "SELECT '{\"a\":1}'::jsonb #> '{a}' LIMIT 6"
        );
        assert_eq!(
            limited_pg("SELECT '{\"a\":1}'::jsonb #>> '{a}'", 5),
            "SELECT '{\"a\":1}'::jsonb #>> '{a}' LIMIT 6"
        );
        // SQL Server temp-table identifiers start with `#`; do not cut them.
        assert_eq!(
            limited_sql("SELECT * FROM #temp", 5, "sqlserver"),
            "SELECT * FROM #temp LIMIT 6"
        );
        // MySQL keeps the comment after the injected LIMIT.
        assert_eq!(
            limited_mysql("SELECT SLEEP(0.2) # tail", 1),
            "SELECT SLEEP(0.2) LIMIT 2 # tail"
        );
        assert_eq!(
            limited_mysql("# lead\nSELECT 1 # tail", 1),
            "# lead\nSELECT 1 LIMIT 2 # tail"
        );
        assert_eq!(
            limited_mysql("SELECT 1; SELECT $$x;y$$ # tail", 1),
            "SELECT 1 LIMIT 2; SELECT $$x;y$$ LIMIT 2 # tail"
        );
    }

    #[test]
    fn limit_respects_with_final_statement_and_multiple_statements() {
        assert_eq!(
            limited_pg("WITH x AS (SELECT 1) SELECT * FROM x -- tail", 1),
            "WITH x AS (SELECT 1) SELECT * FROM x LIMIT 2 -- tail"
        );
        assert_eq!(
            limited_pg("WITH x AS (SELECT 1) UPDATE t SET x = 1 RETURNING x", 1),
            "WITH x AS (SELECT 1) UPDATE t SET x = 1 RETURNING x"
        );
        assert_eq!(
            limited_pg("WITH x AS (DELETE FROM t RETURNING 1) SELECT * FROM x", 1),
            "WITH x AS (DELETE FROM t RETURNING 1) SELECT * FROM x LIMIT 2"
        );
        assert_eq!(
            limited_pg(
                "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x RETURNING id",
                1
            ),
            "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x RETURNING id"
        );
        assert_eq!(
            limited_pg("WITH x AS (SELECT 1) DELETE FROM t RETURNING id", 1),
            "WITH x AS (SELECT 1) DELETE FROM t RETURNING id"
        );
        assert_eq!(
            limited_pg("SELECT 1; SELECT 2", 1),
            "SELECT 1 LIMIT 2; SELECT 2 LIMIT 2"
        );
        assert_eq!(
            limited_pg("SELECT 1; -- tail", 1),
            "SELECT 1 LIMIT 2; -- tail"
        );
    }

    #[test]
    fn tunnel_configuration_fails_closed() {
        let mut config = config("local");
        config.ssh_tunnel = Some(
            serde_json::from_value(serde_json::json!({
                "enabled": true,
                "host": "127.0.0.1",
                "port": 1,
                "username": "nobody"
            }))
            .unwrap(),
        );
        assert_eq!(
            ensure_supported_host_wiring(&config).unwrap_err().code,
            E_UNSUPPORTED
        );
    }

    #[test]
    fn production_refuses_writes_even_with_both_flags() {
        let mut config = config("prod-db");
        config.read_only = false;
        assert_eq!(
            guard_sql(
                "DELETE FROM t",
                &config,
                Safety {
                    allow_writes: true,
                    allow_dangerous_sql: true,
                    production: false,
                }
            )
            .unwrap_err()
            .code,
            E_PRODUCTION_WRITE
        );
    }

    #[test]
    fn stored_read_only_connection_refuses_writes() {
        let mut config = config("local");
        config.read_only = true;
        assert_eq!(
            guard_sql(
                "INSERT INTO t VALUES (1)",
                &config,
                Safety {
                    allow_writes: true,
                    ..Safety::default()
                }
            )
            .unwrap_err()
            .code,
            E_READ_ONLY
        );
    }

    #[tokio::test]
    async fn injected_executor_sees_limit_and_rows_are_truncated() {
        let executor = Arc::new(FakeExecutor {
            result: result(3),
            seen: Mutex::new(Vec::new()),
        });
        let output = run_with_executor(
            executor.clone(),
            QueryRequest {
                connection_id: "c1".into(),
                sql: "SELECT 1".into(),
                limit: 2,
            },
            Safety::default(),
            &config("local"),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(output.rows.len(), 2);
        assert!(output.truncated);
        assert_eq!(executor.seen.lock().unwrap()[0].limit, 2);
    }

    #[tokio::test]
    async fn slow_executor_returns_stable_timeout_code() {
        struct Slow;
        #[async_trait]
        impl QueryExecutor for Slow {
            async fn execute(&self, _request: &QueryRequest) -> Result<QueryResult, CliError> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(result(1))
            }
        }
        let error = run_with_executor(
            Arc::new(Slow),
            QueryRequest {
                connection_id: "c1".into(),
                sql: "SELECT 1".into(),
                limit: 10,
            },
            Safety::default(),
            &config("local"),
            Duration::from_millis(10),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, E_TIMEOUT);
    }
}
