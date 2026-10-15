//! MySQL / MariaDB driver backed by sqlx MySqlPool.

#[path = "catalog.rs"]
mod catalog;
#[path = "connection.rs"]
mod connection;
#[path = "execution.rs"]
mod execution;
#[path = "resource_stream.rs"]
mod resource_stream;
#[path = "schema_table.rs"]
mod schema_table;
#[path = "sql.rs"]
mod sql;
#[path = "type_decode.rs"]
mod type_decode;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

fn supports_consistent_snapshot_engine(engine: Option<&str>) -> bool {
    engine.is_some_and(|value| value.eq_ignore_ascii_case("InnoDB"))
}

const MYSQL_TABLE_OPTIONS_QUERY: &str =
    "SELECT t.ENGINE, collation_map.CHARACTER_SET_NAME AS TABLE_CHARSET, \
            t.TABLE_COLLATION AS TABLE_COLLATION, NULLIF(t.TABLE_COMMENT, '') AS TABLE_COMMENT \
     FROM information_schema.TABLES t \
     LEFT JOIN information_schema.COLLATION_CHARACTER_SET_APPLICABILITY collation_map \
       ON collation_map.COLLATION_NAME = t.TABLE_COLLATION \
     WHERE t.TABLE_SCHEMA = ? AND t.TABLE_NAME = ?";

use crate::resource_capabilities::CONTROL_POOL_CONNECTIONS;
use crate::structure;
use async_trait::async_trait;
use catalog::{
    dump_mysql_routines, dump_mysql_triggers, extract_named_create_column,
    extract_show_create_table,
};
use connection::{build_mysql_options, DEFAULT_HOST, DEFAULT_PORT};
use datazen_driver_api::*;
use execution::{build_kill_query_sql, MysqlQueryExecution};
use sql::{apply_mysql_select_limit, split_mysql_statements};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::pool::PoolConnection;
use sqlx::{MySql, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

pub(crate) use type_decode::{decode_mysql_text, decode_mysql_text_idx, decode_mysql_text_opt};

#[derive(Debug, PartialEq, Eq)]
enum ClusterScopeDecision {
    Cluster { scope: &'static str, id: String },
    NodeFallback,
    Unknown,
}

fn mysql_group_replication_scope(
    group_name: Option<&str>,
    member_state: Result<Option<&str>, ()>,
) -> ClusterScopeDecision {
    let Some(group_name) = group_name.map(str::trim).filter(|name| !name.is_empty()) else {
        return if group_name.is_some() {
            ClusterScopeDecision::NodeFallback
        } else {
            ClusterScopeDecision::Unknown
        };
    };

    match member_state {
        Err(()) => ClusterScopeDecision::Unknown,
        Ok(None) => ClusterScopeDecision::NodeFallback,
        Ok(Some(state)) if state.eq_ignore_ascii_case("ONLINE") => ClusterScopeDecision::Cluster {
            scope: "mysql-group-replication",
            id: group_name.to_string(),
        },
        Ok(Some(_)) => ClusterScopeDecision::Unknown,
    }
}

fn mysql_group_replication_plugin_active(
    plugin_statuses: Option<&HashMap<String, String>>,
) -> Option<bool> {
    let plugin_statuses = plugin_statuses?;
    match plugin_statuses
        .get("group_replication")
        .map(String::as_str)
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        None | Some("INACTIVE") | Some("DISABLED") => Some(false),
        Some("ACTIVE") => Some(true),
        Some(_) => None,
    }
}

fn mariadb_wsrep_scope(statuses: Option<&HashMap<String, String>>) -> ClusterScopeDecision {
    let Some(statuses) = statuses else {
        return ClusterScopeDecision::Unknown;
    };
    if statuses.is_empty() {
        return ClusterScopeDecision::NodeFallback;
    }

    let Some(state_uuid) = statuses
        .get("wsrep_cluster_state_uuid")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ClusterScopeDecision::Unknown;
    };
    let healthy = statuses
        .get("wsrep_cluster_status")
        .is_some_and(|value| value.eq_ignore_ascii_case("Primary"))
        && statuses
            .get("wsrep_connected")
            .is_some_and(|value| value.eq_ignore_ascii_case("ON"))
        && statuses
            .get("wsrep_ready")
            .is_some_and(|value| value.eq_ignore_ascii_case("ON"));
    if healthy {
        ClusterScopeDecision::Cluster {
            scope: "mariadb-wsrep",
            id: state_uuid.to_string(),
        }
    } else {
        ClusterScopeDecision::Unknown
    }
}

fn canonical_database_identity(driver: &str, scope: &str, id: &str, database: &str) -> String {
    serde_json::json!({
        "driver": driver,
        "scope": scope,
        "id": id,
        "database": database,
    })
    .to_string()
}

pub struct MysqlDriver {
    pub(crate) pools: RwLock<HashMap<String, MySqlPool>>,
    /// Active schema selected via `use_database` (or connect config), keyed by pool_id.
    /// Applied with `USE` on each acquired connection so pooled sessions stay consistent.
    pub(crate) active_databases: RwLock<HashMap<String, String>>,
    /// Open transactions: connection held for the lifetime of BEGIN…COMMIT/ROLLBACK, keyed by handle.id.
    pub(crate) fixed_states: RwLock<HashMap<String, datazen_driver_api::session::TransactionState>>,
    pub(crate) fixed_sessions: RwLock<HashMap<String, u64>>,
    pub(crate) transactions: Mutex<HashMap<String, std::sync::Arc<Mutex<PoolConnection<MySql>>>>>,
    /// Exact execution target registry. The MySQL thread id never leaves the
    /// driver; the host/UI only sees QueryExecutionId.
    pub(crate) query_executions: Mutex<HashMap<QueryExecutionId, MysqlQueryExecution>>,
    /// Independent control connections used only for out-of-band work
    /// (`KILL QUERY`, transaction instrumentation), keyed by connection identity
    /// **and database** rather than by resource: every resource that resolves to
    /// the same server and database shares one connection in here.
    ///
    /// These connections never execute user SQL and are deliberately **not** part
    /// of a resource's declared cost. `KILL QUERY <thread_id>` is instance-scoped,
    /// so charging a connection per resource would bill the same physical
    /// connection once per resource on that server. The database stays in the key
    /// because a pool reconnects with the options it was built from, so a pool
    /// keyed without it would be reused for a database that may since have been
    /// dropped.
    pub(crate) control_pools: RwLock<HashMap<String, ControlPoolEntry>>,
    /// Which control-pool identity each resource belongs to, so a release can find
    /// the shared entry from the handle alone. Removed with the resource.
    pub(crate) control_keys: RwLock<HashMap<String, String>>,
    pub(crate) is_mariadb: bool,
}

/// One shared control connection plus the number of resources still holding it.
pub(crate) struct ControlPoolEntry {
    pub(crate) pool: MySqlPool,
    pub(crate) refs: usize,
}

impl MysqlDriver {
    /// Identity of the shared control connection.
    ///
    /// The `KILL QUERY` itself is database-agnostic — it names a thread id on the
    /// whole instance — but the **pool is not**: a `MySqlPool` reconnects with the
    /// connect options it was built from, so the database is part of its identity.
    /// Keying without it would let a pool built for one database serve a resource on
    /// another, and a database that has since been dropped would take every later
    /// acquisition down with it (the PostgreSQL driver learned this from the desktop
    /// E2E, which re-creates the worker database before every spec file).
    ///
    /// The password is excluded (a secret has no business in a map key); the
    /// connection is dropped once the last resource of this identity is released.
    pub(crate) fn control_pool_key(config: &ConnectionConfig) -> String {
        format!(
            "myctl|{}|{}|{}|{}|{:?}|{}",
            config.id,
            config.host.as_deref().unwrap_or(DEFAULT_HOST),
            config.port.unwrap_or(DEFAULT_PORT),
            config.username.as_deref().unwrap_or(""),
            config.ssl_mode,
            config
                .database
                .as_deref()
                .map(str::trim)
                .unwrap_or_default(),
        )
    }

    /// Get-or-create the shared control connection for this connection identity,
    /// adding one reference for the resource being opened.
    ///
    /// Held under the write lock across the connect on purpose: two resources
    /// racing for the same identity must not open two control connections, and the
    /// wait is bounded by one connection attempt.
    async fn acquire_control_pool(
        &self,
        config: &ConnectionConfig,
        timeout: Duration,
    ) -> Result<(), DriverError> {
        let key = Self::control_pool_key(config);
        let mut pools = self.control_pools.write().await;
        if let Some(entry) = pools.get_mut(&key) {
            entry.refs = entry.refs.saturating_add(1);
            return Ok(());
        }
        match Self::open_pool(
            build_mysql_options(config)?,
            timeout,
            CONTROL_POOL_CONNECTIONS,
            0,
        )
        .await
        {
            Ok(pool) => {
                pools.insert(key, ControlPoolEntry { pool, refs: 1 });
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Drop one reference to the shared control connection of `pool_id`, closing
    /// the physical connection once the last resource is gone.
    ///
    /// Also forgets which identity the resource belonged to, so a double release
    /// cannot decrement a stranger's count.
    async fn release_control_pool(&self, pool_id: &str) {
        let key = self.control_keys.write().await.remove(pool_id);
        let Some(key) = key else {
            return;
        };
        let mut pools = self.control_pools.write().await;
        let Some(entry) = pools.get_mut(&key) else {
            return;
        };
        entry.refs = entry.refs.saturating_sub(1);
        if entry.refs == 0 {
            if let Some(entry) = pools.remove(&key) {
                entry.pool.close().await;
            }
        }
    }

    /// The shared control connection serving `handle`'s resource.
    ///
    /// Resolved through the resource's identity, not through the pool itself,
    /// because one control connection serves every resource of a (connection
    /// identity, database). `None` means this resource has no usable control
    /// connection, which is a supported state — acquisition never depends on it, so
    /// `KILL QUERY` and the transaction-evidence probe degrade instead of refusing
    /// the resource.
    pub(crate) async fn control_pool_for(&self, handle: &ConnectionHandle) -> Option<MySqlPool> {
        let key = self
            .control_keys
            .read()
            .await
            .get(&handle.pool_id)
            .cloned()?;
        self.control_pools
            .read()
            .await
            .get(&key)
            .map(|entry| entry.pool.clone())
    }

    pub(crate) fn is_table_not_found_error(err_msg: &str) -> bool {
        let lower = err_msg.to_ascii_lowercase();
        lower.contains("1146")
            || lower.contains("doesn't exist")
            || lower.contains("does not exist")
    }

    pub fn new(is_mariadb: bool) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
            fixed_sessions: RwLock::new(HashMap::new()),
            fixed_states: RwLock::new(HashMap::new()),
            query_executions: Mutex::new(HashMap::new()),
            control_pools: RwLock::new(HashMap::new()),
            control_keys: RwLock::new(HashMap::new()),
            is_mariadb,
        }
    }

    fn get_pool<'a>(
        pools: &'a HashMap<String, MySqlPool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a MySqlPool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    /// Bare table name: a caller-supplied `db.table` contributes only its table
    /// part, because the database always arrives as an explicit argument.
    pub(crate) fn bare_table_name(table: &str) -> &str {
        let stripped = match table.rsplit_once('.') {
            Some((_, name)) if !name.is_empty() => name,
            _ => table,
        };
        stripped.trim().trim_matches('`')
    }

    /// `` `db`.`table` `` for the `SHOW` family. MySQL accepts a qualified name
    /// there, so a foreign database can be read without a `USE` — and therefore
    /// without re-pointing a connection that goes back into the pool.
    pub(crate) fn qualified_table_ref(database: &str, table: &str) -> String {
        let bare = Self::bare_table_name(table);
        let db = database.trim();
        if db.is_empty() {
            Self::quote_identifier(bare)
        } else {
            format!(
                "{}.{}",
                Self::quote_identifier(db),
                Self::quote_identifier(bare)
            )
        }
    }

    /// Database a metadata read targets: the explicit argument wins, otherwise
    /// the database this handle's pool was connected to.
    pub(crate) async fn effective_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> String {
        let explicit = database.trim();
        if !explicit.is_empty() {
            return explicit.to_string();
        }
        self.active_databases
            .read()
            .await
            .get(&handle.pool_id)
            .cloned()
            .unwrap_or_default()
    }

    async fn fetch_columns_with_db<'e, E>(
        executor: E,
        current_db: &str,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
        let cols = sqlx::query(
            r#"
            SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
                   COLUMN_COMMENT, COLUMN_KEY, EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY ORDINAL_POSITION
            "#,
        )
        .bind(current_db)
        .bind(table)
        .fetch_all(executor)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name = decode_mysql_text(r, "COLUMN_NAME");
                let nullable = decode_mysql_text(r, "IS_NULLABLE");
                let key = decode_mysql_text(r, "COLUMN_KEY");
                let extra = decode_mysql_text(r, "EXTRA");
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: decode_mysql_text(r, "COLUMN_TYPE"),
                    nullable: nullable == "YES",
                    default_value: decode_mysql_text_opt(r, "COLUMN_DEFAULT"),
                    comment: {
                        let s = decode_mysql_text(r, "COLUMN_COMMENT");
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    },
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    /// Batch-fetch columns for ALL tables in the given database in one query.
    async fn fetch_all_columns_with_db<'e, E>(
        executor: E,
        current_db: &str,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
        let cols = sqlx::query(
            r#"
            SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
                   COLUMN_COMMENT, COLUMN_KEY, EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME, ORDINAL_POSITION
            "#,
        )
        .bind(current_db)
        .fetch_all(executor)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut result: HashMap<String, (Vec<ColumnSchema>, Vec<String>)> = HashMap::new();

        for r in &cols {
            let table_name = decode_mysql_text(r, "TABLE_NAME");
            let name = decode_mysql_text(r, "COLUMN_NAME");
            let nullable = decode_mysql_text(r, "IS_NULLABLE");
            let key = decode_mysql_text(r, "COLUMN_KEY");
            let extra = decode_mysql_text(r, "EXTRA");
            let is_pk = key == "PRI";

            let column = ColumnSchema {
                is_primary_key: is_pk,
                name: name.clone(),
                data_type: decode_mysql_text(r, "COLUMN_TYPE"),
                nullable: nullable == "YES",
                default_value: decode_mysql_text_opt(r, "COLUMN_DEFAULT"),
                comment: {
                    let s = decode_mysql_text(r, "COLUMN_COMMENT");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                is_auto_increment: extra.contains("auto_increment"),
            };

            let entry = result.entry(table_name).or_default();
            entry.0.push(column);
            if is_pk {
                entry.1.push(name);
            }
        }

        Ok(result)
    }

    pub(crate) fn quote_identifier(name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    /// Parse CONSTRAINT ... FOREIGN KEY lines from SHOW CREATE TABLE output.
    fn parse_fk_from_create_table(create_sql: &str) -> Vec<ForeignKeyInfo> {
        let mut fks = Vec::new();
        for line in create_sql.lines() {
            let trimmed = line.trim();
            if !trimmed.contains("FOREIGN KEY") {
                continue;
            }
            // Pattern: CONSTRAINT `name` FOREIGN KEY (`cols`) REFERENCES `table` (`cols`) ...
            let fk_name = Self::extract_backtick_after(trimmed, "CONSTRAINT");
            let fk_cols = Self::extract_backtick_list_after(trimmed, "FOREIGN KEY");
            let ref_table = Self::extract_qualified_table_after(trimmed, "REFERENCES");
            let ref_cols = Self::extract_backtick_list_after(trimmed, "REFERENCES");

            let on_delete = Self::extract_rule(trimmed, "ON DELETE");
            let on_update = Self::extract_rule(trimmed, "ON UPDATE");

            if !fk_name.is_empty() && !fk_cols.is_empty() {
                fks.push(ForeignKeyInfo {
                    name: fk_name,
                    columns: fk_cols,
                    referenced_table: ref_table,
                    referenced_columns: ref_cols,
                    on_delete,
                    on_update,
                    deferrability: ForeignKeyDeferrability::NotDeferrable,
                });
            }
        }
        fks.sort_by(|a, b| a.name.cmp(&b.name));
        fks
    }

    /// Parse CHECK clauses from SHOW CREATE TABLE. The scanner skips quoted
    /// SQL and comments before looking for a real CHECK keyword, then balances
    /// the predicate parentheses so literals containing CHECK cannot become
    /// false constraints or truncate an expression.
    fn parse_check_from_create_table(create_sql: &str) -> Vec<CheckConstraint> {
        let mut checks = Vec::new();
        let bytes = create_sql.as_bytes();
        let mut index = 0usize;
        while index + 5 <= bytes.len() {
            let ch = bytes[index] as char;
            if matches!(ch, '\'' | '"' | '`') {
                index = Self::skip_quoted_sql(bytes, index, ch);
                continue;
            }
            if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
                index = Self::skip_block_comment(bytes, index);
                continue;
            }
            if bytes[index..].starts_with(b"--") || bytes[index] == b'#' {
                index = Self::skip_line_comment(bytes, index);
                continue;
            }
            let is_check = bytes[index..index + 5].eq_ignore_ascii_case(b"CHECK")
                && (index == 0 || !Self::is_sql_identifier_byte(bytes[index - 1]))
                && (index + 5 == bytes.len() || !Self::is_sql_identifier_byte(bytes[index + 5]));
            if !is_check {
                index += 1;
                continue;
            }
            let mut open = index + 5;
            while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                open += 1;
            }
            if open >= bytes.len() || bytes[open] != b'(' {
                index += 5;
                continue;
            }
            let Some(close) = Self::scan_sql_parentheses(bytes, open) else {
                break;
            };
            let expression = create_sql[open + 1..close].trim();
            if !expression.is_empty() {
                let line_start = create_sql[..index].rfind('\n').map_or(0, |pos| pos + 1);
                let name =
                    Self::extract_backtick_after(&create_sql[line_start..index], "CONSTRAINT");
                checks.push(CheckConstraint {
                    name: if name.is_empty() {
                        format!("check_{}", checks.len())
                    } else {
                        name
                    },
                    expression: expression.to_string(),
                });
            }
            index = close + 1;
        }
        checks.sort_by(|a, b| a.name.cmp(&b.name));
        checks
    }

    fn skip_quoted_sql(bytes: &[u8], start: usize, quote: char) -> usize {
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' && index + 1 < bytes.len() {
                index += 2;
            } else if bytes[index] as char == quote {
                if index + 1 < bytes.len() && bytes[index + 1] as char == quote {
                    index += 2;
                } else {
                    return index + 1;
                }
            } else {
                index += 1;
            }
        }
        bytes.len()
    }

    fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
        bytes[start + 2..]
            .windows(2)
            .position(|pair| pair == b"*/")
            .map_or(bytes.len(), |offset| start + 2 + offset + 2)
    }

    fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
        bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| start + offset + 1)
    }

    fn is_sql_identifier_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    fn scan_sql_parentheses(bytes: &[u8], open: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut index = open;
        while index < bytes.len() {
            let ch = bytes[index] as char;
            if matches!(ch, '\'' | '"' | '`') {
                index = Self::skip_quoted_sql(bytes, index, ch);
                continue;
            }
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
            index += 1;
        }
        None
    }

    /// Extract the first backtick-quoted identifier after a keyword.
    fn extract_backtick_after(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(start) = after.find('`') {
                let inner = &after[start + 1..];
                if let Some(end) = inner.find('`') {
                    return inner[..end].to_string();
                }
            }
        }
        String::new()
    }

    fn extract_qualified_table_after(s: &str, keyword: &str) -> String {
        let Some(pos) = s.find(keyword) else {
            return String::new();
        };
        let after = s[pos + keyword.len()..].trim_start();
        let bytes = after.as_bytes();
        let mut index = 0;
        let mut parts = Vec::new();
        loop {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() || bytes[index] == b'(' {
                return String::new();
            }
            let part = if bytes[index] == b'`' {
                index += 1;
                let content_start = index;
                let mut value = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    if bytes[index] == b'`' {
                        if index + 1 < bytes.len() && bytes[index + 1] == b'`' {
                            index += 2;
                        } else {
                            value = after[content_start..index].replace("``", "`");
                            index += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
                if !closed {
                    return String::new();
                }
                value
            } else {
                let start = index;
                while index < bytes.len()
                    && bytes[index] != b'.'
                    && bytes[index] != b'('
                    && !bytes[index].is_ascii_whitespace()
                {
                    index += 1;
                }
                after[start..index].to_string()
            };
            if part.is_empty() || part.contains('.') {
                return String::new();
            }
            parts.push(part);
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'.' {
                index += 1;
                continue;
            }
            if index == bytes.len() || bytes[index] == b'(' || bytes[index].is_ascii_whitespace() {
                return parts.join(".");
            }
            return String::new();
        }
    }

    fn grants_prove_server_wide_catalog_visibility(grants: &[String]) -> bool {
        let has_global_select = grants.iter().any(|grant| {
            let upper = grant.trim().to_ascii_uppercase();
            if !upper.starts_with("GRANT ") {
                return false;
            }
            let Some(on) = upper.find(" ON *.* TO ") else {
                return false;
            };
            upper[6..on]
                .split(',')
                .map(str::trim)
                .any(|privilege| matches!(privilege, "SELECT" | "ALL PRIVILEGES"))
        });
        has_global_select
            && !grants.iter().any(|grant| {
                grant
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("REVOKE ")
            })
    }

    /// Extract a parenthesized list of backtick-quoted identifiers after a keyword.
    fn extract_backtick_list_after(s: &str, keyword: &str) -> Vec<String> {
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(paren_start) = after.find('(') {
                let inner = &after[paren_start + 1..];
                if let Some(paren_end) = inner.find(')') {
                    let list_str = &inner[..paren_end];
                    return list_str
                        .split(',')
                        .filter_map(|part| {
                            let t = part.trim();
                            if t.starts_with('`') && t.ends_with('`') && t.len() >= 2 {
                                Some(t[1..t.len() - 1].to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    fn extract_rule(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = s[pos + keyword.len()..].trim_start();
            let rule = after
                .split(|c: char| c == ',' || c == ')' || c == '\n')
                .next()
                .unwrap_or("")
                .trim();
            if !rule.is_empty() {
                return rule.to_uppercase();
            }
        }
        "RESTRICT".to_string()
    }
    fn extract_mysql_plan_metrics(plan_json: &serde_json::Value) -> (Option<f64>, Option<i64>) {
        let query_cost = plan_json
            .get("query_block")
            .and_then(|block| block.get("cost_info"))
            .and_then(|info| info.get("query_cost"))
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        let estimated_rows = plan_json
            .get("query_block")
            .and_then(|block| block.get("table"))
            .and_then(|table| table.get("rows_examined_per_scan"))
            .and_then(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        (query_cost, estimated_rows)
    }

    fn extract_json_from_explain_row(rows: &[sqlx::mysql::MySqlRow]) -> Option<serde_json::Value> {
        rows.first().and_then(|row| {
            row.try_get::<serde_json::Value, _>(0).ok().or_else(|| {
                row.try_get::<String, _>(0)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
        })
    }

    async fn physical_database_node_identity(
        &self,
        pool: &MySqlPool,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        let driver = if self.is_mariadb { "mariadb" } else { "mysql" };
        if let Ok(row) = sqlx::query("SELECT @@server_uuid AS server_uuid")
            .fetch_one(pool)
            .await
        {
            if let Ok(server_uuid) = row.try_get::<String, _>("server_uuid") {
                let server_uuid = server_uuid.trim();
                if !server_uuid.is_empty() {
                    return Ok(Some(canonical_database_identity(
                        driver,
                        "node",
                        server_uuid,
                        database,
                    )));
                }
            }
        }

        let Ok(row) = sqlx::query(
            "SELECT @@hostname AS server_host, CAST(@@port AS CHAR) AS server_port, CAST(@@server_id AS CHAR) AS server_id",
        )
        .fetch_one(pool)
        .await
        else {
            return Ok(None);
        };
        let (Ok(server_host), Ok(server_port), Ok(server_id)) = (
            row.try_get::<String, _>("server_host"),
            row.try_get::<String, _>("server_port"),
            row.try_get::<String, _>("server_id"),
        ) else {
            return Ok(None);
        };
        let id = serde_json::json!({
            "host": server_host,
            "port": server_port,
            "serverId": server_id,
        })
        .to_string();
        Ok(Some(canonical_database_identity(
            driver, "node", &id, database,
        )))
    }
}

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    fn supports_bound_writes(&self) -> bool {
        true
    }

    fn sql_literal_dialect(&self) -> Option<SqlLiteralDialect> {
        Some(SqlLiteralDialect::MySql)
    }

    fn default_host(&self) -> Option<&'static str> {
        Some(crate::mysql::connection::DEFAULT_HOST)
    }

    fn default_port(&self) -> Option<u16> {
        Some(crate::mysql::connection::DEFAULT_PORT)
    }

    fn has_multi_database(&self) -> bool {
        true
    }

    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::MysqlMigrationRenderer))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::MysqlMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::MysqlTypeNormalizer))
    }
    fn driver_type(&self) -> DatabaseType {
        if self.is_mariadb {
            "mariadb".to_string()
        } else {
            "mysql".to_string()
        }
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        DdlAtomicity::AutoCommitPerStatement
    }

    fn sync_family(&self) -> String {
        "mysql".into()
    }

    fn dialect_notes(&self) -> Option<String> {
        Some(
            "MySQL uses LIMIT offset, count (reversed order from PostgreSQL), \
             CONCAT() for string concatenation, LIKE is case-insensitive by default \
             (collation-dependent), ENUM and SET types, INSERT IGNORE and \
             ON DUPLICATE KEY UPDATE for upsert, and each DDL statement auto-commits \
             (no transactional DDL)."
                .into(),
        )
    }

    /// F7: qualify unqualified table references with the target database
    /// (`` `db`.`t` ``), shared by the mysql/mariadb/doris/starrocks/
    /// manticore/ob_oracle variants. Parse failures pass SQL through
    /// unchanged; see `sql_target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn format_sql_literal(&self, value: &Option<super::Value>) -> String {
        match value {
            None | Some(super::Value::Null) => "NULL".to_string(),
            Some(super::Value::Bool(b)) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Some(super::Value::Integer(i)) => i.to_string(),
            Some(super::Value::Float(f)) => f.to_string(),
            Some(super::Value::String(s)) => {
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
            Some(super::Value::Bytes(b)) => format!(
                "X'{}'",
                b.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            Some(super::Value::Timestamp(s)) => {
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
            Some(super::Value::Json(j)) => {
                let s = j.to_string();
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
        }
    }

    async fn connect_fixed_session(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        self.open_fixed_session(config).await
    }
    async fn observe_fixed_session(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<datazen_driver_api::session::SessionContext, DriverError> {
        self.read_fixed_session(handle).await
    }

    async fn query_to_sink(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        parameters: Option<&[Value]>,
        sink: &dyn datazen_driver_api::resource::ResultSink,
    ) -> Result<Vec<StatementResult>, datazen_driver_api::resource::ResourceError> {
        self.stream_resource_query(handle, execution_id, sql, limit, parameters, sink)
            .await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let result = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()));

        pool.close().await;

        let row = result?;
        let version: String = row.try_get(0).unwrap_or_default();

        let server_type = if version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };

        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let max = config.effective_max_pool_size();
        let min = 2u32.min(max);
        let pool = Self::open_pool(opts, timeout, max, min).await?;

        let acquire_result: Result<(), DriverError> = async {
            let _c1 = pool
                .acquire()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            if max >= 2 {
                let _c2 = pool
                    .acquire()
                    .await
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            }
            Ok(())
        }
        .await;

        if let Err(e) = acquire_result {
            pool.close().await;
            return Err(e);
        }

        let pool_id = uuid::Uuid::new_v4().to_string();
        let connection_id = uuid::Uuid::new_v4().to_string();
        self.control_keys
            .write()
            .await
            .insert(pool_id.clone(), Self::control_pool_key(config));
        if let Err(error) = self.acquire_control_pool(config, timeout).await {
            // The cancel-only connection is an auxiliary capability, and the
            // primary path must not depend on it: a resource that cannot cancel is
            // degraded, not unusable. `control_pool_for` answers `None`, `KILL
            // QUERY` reports the unavailable control pool, and the MySQL
            // transaction-evidence probe in `fixed_session` skips the check.
            tracing::warn!(
                %error,
                "cancel-only control connection unavailable; this resource cannot cancel precisely"
            );
        }

        if let Some(db) = config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.active_databases
                .write()
                .await
                .insert(pool_id.clone(), db.to_string());
        }

        self.pools.write().await.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    /// MySQL serves every database from one pool by qualifying names, so it
    /// holds no per-database resource at all: `close_database` can never release
    /// one. Reporting the connection's own database here would make the tree
    /// mark a database the user never opened, so the default empty report
    /// applies — the tree's own expand state is what shows a database as open.
    async fn open_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(Vec::new())
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let removed = self.transactions.lock().await.remove(&handle.id);
        if let Some(pinned) = removed {
            let mut conn = pinned.lock().await;
            let _ = Self::execute_text_on_conn(&mut *conn, "ROLLBACK").await;
        }
        self.fixed_sessions.write().await.remove(&handle.id);
        self.fixed_states.write().await.remove(&handle.id);
        self.active_databases.write().await.remove(&handle.pool_id);
        self.query_executions
            .lock()
            .await
            .retain(|_, execution| execution.session_id != handle.id);
        // Keep the same pools registered across cancellation/timeouts. Removing
        // before close().await would make the next disconnect falsely succeed
        // without waiting for the outstanding physical connections.
        let pool = self.pools.read().await.get(&handle.pool_id).cloned();
        if let Some(pool) = pool {
            pool.close().await;
        }
        // The control connection is shared per (connection identity, database), so
        // this drops one reference: the last resource out closes it.
        self.release_control_pool(&handle.pool_id).await;
        self.pools.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query("SHOW DATABASES")
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows.iter().map(|r| decode_mysql_text_idx(r, 0)).collect())
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let rows = sqlx::query("SHOW GRANTS FOR CURRENT_USER()")
            .fetch_all(pool)
            .await
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        let grants = rows
            .iter()
            .map(|row| decode_mysql_text_idx(row, 0))
            .collect::<Vec<_>>();
        Ok(Self::grants_prove_server_wide_catalog_visibility(&grants))
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        // MySQL's "schema" *is* the database, so a schema argument is a caller bug.
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            r#"
            SELECT TABLE_NAME, TABLE_TYPE
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME
            "#,
        )
        .bind(database)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        if rows.is_empty() {
            // An unknown database and a genuinely empty one both yield zero
            // rows here. Distinguish them so the tree reports a bad target
            // instead of silently presenting an empty database — PostgreSQL
            // errors on the same input, and the two must not disagree.
            let known: Option<String> = sqlx::query_scalar(
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
            )
            .bind(database)
            .fetch_optional(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            if known.is_none() {
                return Err(DriverError::QueryFailed(format!(
                    "Unknown database '{database}'"
                )));
            }
        }

        Ok(rows
            .iter()
            .map(|r| {
                let tt = decode_mysql_text(r, "TABLE_TYPE");
                TableInfo {
                    schema: None,
                    name: decode_mysql_text(r, "TABLE_NAME"),
                    table_type: match tt.as_str() {
                        "VIEW" => TableType::View,
                        "SYSTEM VIEW" => TableType::SystemTable,
                        _ => TableType::Table,
                    },
                    row_count: None,
                }
            })
            .collect())
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        // `information_schema` is filtered by TABLE_SCHEMA, so the borrowed
        // connection keeps whatever default schema it was born with.
        Self::fetch_columns_with_db(&mut *conn, &db, Self::bare_table_name(table)).await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let db = self.effective_database(handle, database).await;
        // Clone the pool (cheap Arc) so no lock is held across the query.
        let pool = {
            let pools = self.pools.read().await;
            Self::get_pool(&pools, handle)?.clone()
        };
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        // The batch query is filtered by TABLE_SCHEMA: an explicit per-call
        // database is honoured and no `USE` is issued, so a stale session
        // default can never leak into the result.
        Self::fetch_all_columns_with_db(&mut *conn, &db).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let pinned = self.transactions.lock().await.get(&handle.id).cloned();

        if let Some(pinned) = pinned {
            let mut conn = pinned.lock().await;
            let db = if database.trim().is_empty() {
                sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                    .fetch_one(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?
                    .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?
            } else {
                database.to_string()
            };
            return self
                .table_schema_on_connection(&mut **conn, table, &db)
                .await;
        }
        let db = self.effective_database(handle, database).await;
        let pool = {
            let pools = self.pools.read().await;
            Self::get_pool(&pools, handle)?.clone()
        };
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.table_schema_on_connection(&mut *conn, table, &db)
            .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let start = Instant::now();
                let rows = sqlx::query(sql)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let elapsed = start.elapsed().as_millis() as u64;
                let (columns, result_rows) = Self::decode_rows(&rows);
                let row_count = result_rows.len() as u64;
                return Ok(QueryResult {
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: elapsed,
                });
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let start = Instant::now();
        let rows = sqlx::query(sql)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let elapsed = start.elapsed().as_millis() as u64;

        let (columns, result_rows) = Self::decode_rows(&rows);
        let row_count = result_rows.len() as u64;

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: Some(row_count),
            execution_time_ms: elapsed,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            return Ok(MultiQueryResult {
                results: Vec::new(),
                total_time_ms: 0,
            });
        }

        let total_start = Instant::now();
        let mut results = Vec::with_capacity(statements.len());

        let pinned = self.transactions.lock().await.get(&handle.id).cloned();

        if let Some(pinned) = pinned {
            let mut conn = pinned.lock().await;
            for stmt in &statements {
                let (effective_sql, applied_limit) = apply_mysql_select_limit(stmt, limit);
                let trimmed_upper = effective_sql.trim().to_ascii_uppercase();
                let is_query = trimmed_upper.starts_with("SELECT")
                    || trimmed_upper.starts_with("WITH")
                    || trimmed_upper.starts_with("SHOW")
                    || trimmed_upper.starts_with("DESCRIBE")
                    || trimmed_upper.starts_with("DESC")
                    || trimmed_upper.starts_with("EXPLAIN");

                let stmt_start = Instant::now();

                if is_query {
                    let rows = sqlx::query(effective_sql.as_str())
                        .fetch_all(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                    let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                    let (columns, mut result_rows) = Self::decode_rows(&rows);
                    let truncated = if let Some(lim) = applied_limit {
                        let fetched = result_rows.len() as u32;
                        if fetched > lim {
                            result_rows.truncate(lim as usize);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    let row_count = result_rows.len() as u64;

                    results.push(StatementResult {
                        sql: stmt.clone(),
                        columns,
                        rows: result_rows,
                        rows_affected: Some(row_count),
                        execution_time_ms: stmt_ms,
                        truncated,
                    });
                } else {
                    let result = Self::execute_text_on_conn(&mut *conn, effective_sql.as_str())
                        .await
                        .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                    let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                    results.push(StatementResult {
                        sql: stmt.clone(),
                        columns: Vec::new(),
                        rows: Vec::new(),
                        rows_affected: Some(result.rows_affected()),
                        execution_time_ms: stmt_ms,
                        truncated: false,
                    });
                }
            }

            return Ok(MultiQueryResult {
                results,
                total_time_ms: total_start.elapsed().as_millis() as u64,
            });
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        for stmt in &statements {
            let (effective_sql, applied_limit) = apply_mysql_select_limit(stmt, limit);
            let trimmed_upper = effective_sql.trim().to_ascii_uppercase();
            let is_query = trimmed_upper.starts_with("SELECT")
                || trimmed_upper.starts_with("WITH")
                || trimmed_upper.starts_with("SHOW")
                || trimmed_upper.starts_with("DESCRIBE")
                || trimmed_upper.starts_with("DESC")
                || trimmed_upper.starts_with("EXPLAIN");

            let stmt_start = Instant::now();

            if is_query {
                let rows = sqlx::query(effective_sql.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                let (columns, mut result_rows) = Self::decode_rows(&rows);
                let truncated = if let Some(lim) = applied_limit {
                    let fetched = result_rows.len() as u32;
                    if fetched > lim {
                        result_rows.truncate(lim as usize);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                let row_count = result_rows.len() as u64;

                results.push(StatementResult {
                    sql: stmt.clone(),
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: stmt_ms,
                    truncated,
                });
            } else {
                let result = Self::execute_text_on_conn(&mut conn, effective_sql.as_str())
                    .await
                    .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                results.push(StatementResult {
                    sql: stmt.clone(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: Some(result.rows_affected()),
                    execution_time_ms: stmt_ms,
                    truncated: false,
                });
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
        })
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }

        let total_start = Instant::now();
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                for (index, stmt) in statements.iter().enumerate() {
                    Self::stream_one_statement(&mut **conn, stmt, limit, index, &on_event).await?;
                }
                on_event(QueryStreamEvent::Done {
                    total_time_ms: total_start.elapsed().as_millis() as u64,
                });
                return Ok(());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;
        for (index, stmt) in statements.iter().enumerate() {
            Self::stream_one_statement(&mut *conn, stmt, limit, index, &on_event).await?;
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        let transactional = self.transactions.lock().await.contains_key(&handle.id);
        let target_pool = self.pools.read().await.get(&handle.pool_id).cloned();
        // The control connection is shared per (connection identity, database), so
        // it is reached through this resource's identity rather than `pool_id`.
        let control_pool = self.control_pool_for(handle).await;
        let mut executions = self.query_executions.lock().await;
        if executions.contains_key(execution_id) {
            return Err(DriverError::QueryFailed(format!(
                "query execution '{}' is already registered",
                execution_id.as_str()
            )));
        }
        executions.insert(
            execution_id.clone(),
            MysqlQueryExecution {
                cancel_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                session_id: handle.id.clone(),
                target_pool,
                control_pool,
                thread_id: self.fixed_sessions.read().await.get(&handle.id).copied(),
                cancel_requested: false,
                transactional,
            },
        );
        Ok(())
    }

    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            self.finish_query_execution(handle, execution_id).await?;
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
        self.stream_registered_execution(handle, execution_id, &statements, limit, &on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let start = Instant::now();
                let rows = Self::bind_values(sqlx::query(sql), params)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let elapsed = start.elapsed().as_millis() as u64;
                let (columns, result_rows) = Self::decode_rows(&rows);
                let row_count = result_rows.len() as u64;
                return Ok(QueryResult {
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: elapsed,
                });
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let start = Instant::now();
        let rows = Self::bind_values(sqlx::query(sql), params)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let elapsed = start.elapsed().as_millis() as u64;

        let (columns, result_rows) = Self::decode_rows(&rows);
        let row_count = result_rows.len() as u64;

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: Some(row_count),
            execution_time_ms: elapsed,
        })
    }

    fn parameter_placeholder(
        &self,
        _index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Ok("?".to_string())
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let result = Self::bind_values(sqlx::query(sql), params)
                    .execute(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                return Ok(result.rows_affected());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let result = Self::bind_values(sqlx::query(sql), params)
            .execute(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let result = Self::execute_text_on_conn(&mut *conn, sql)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                return Ok(result.rows_affected());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let result = Self::execute_text_on_conn(&mut conn, sql)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let txs = self.transactions.lock().await;
        if self.fixed_sessions.read().await.contains_key(&handle.id) {
            let pinned = txs
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            drop(txs);
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(&mut *conn, "BEGIN")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                handle.id.clone(),
                datazen_driver_api::session::TransactionState::Active,
            );
            return Ok(TransactionHandle {
                id: format!("fixed_tx_{}", uuid::Uuid::new_v4()),
                connection_id: handle.id.clone(),
            });
        }
        if txs.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }

        drop(txs);
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        self.apply_active_database(handle, &mut conn).await?;

        Self::execute_text_on_conn(&mut conn, "BEGIN")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;

        self.transactions.lock().await.insert(
            handle.id.clone(),
            std::sync::Arc::new(tokio::sync::Mutex::new(conn)),
        );
        Ok(TransactionHandle {
            id: format!("mysql_tx_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let txs = self.transactions.lock().await;
        if self.fixed_sessions.read().await.contains_key(&handle.id) {
            let pinned = txs
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            drop(txs);
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(
                &mut *conn,
                "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
            )
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            Self::execute_text_on_conn(
                &mut *conn,
                "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
            )
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                handle.id.clone(),
                datazen_driver_api::session::TransactionState::Active,
            );
            return Ok(TransactionHandle {
                id: format!("fixed_tx_{}", uuid::Uuid::new_v4()),
                connection_id: handle.id.clone(),
            });
        }
        if txs.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }

        drop(txs);
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        self.apply_active_database(handle, &mut conn).await?;

        // WITH CONSISTENT SNAPSHOT is only effective with REPEATABLE READ.
        // Scope the isolation override to the next transaction so the pool's
        // session defaults are not changed for later checkouts.
        if let Err(e) =
            Self::execute_text_on_conn(&mut conn, "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .await
        {
            let _ = conn.close().await;
            return Err(DriverError::TransactionError(e.to_string()));
        }

        // MySQL and MariaDB support these transaction characteristics. If a
        // server rejects one, close the connection so a pending one-shot
        // isolation setting cannot leak into a later pooled transaction.
        if let Err(e) = Self::execute_text_on_conn(
            &mut conn,
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
        )
        .await
        {
            let _ = conn.close().await;
            return Err(DriverError::TransactionError(e.to_string()));
        }

        self.transactions.lock().await.insert(
            handle.id.clone(),
            std::sync::Arc::new(tokio::sync::Mutex::new(conn)),
        );
        Ok(TransactionHandle {
            id: format!("mysql_snapshot_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        if self
            .fixed_sessions
            .read()
            .await
            .contains_key(&tx.connection_id)
        {
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&tx.connection_id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(&mut *conn, "COMMIT")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                tx.connection_id.clone(),
                datazen_driver_api::session::TransactionState::Idle,
            );
            return Ok(());
        }
        let pinned = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;
        let mut conn = pinned.lock().await;

        Self::execute_text_on_conn(&mut conn, "COMMIT")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        if self
            .fixed_sessions
            .read()
            .await
            .contains_key(&tx.connection_id)
        {
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&tx.connection_id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            Self::execute_text_on_conn(&mut *conn, "ROLLBACK")
                .await
                .map_err(|e| DriverError::TransactionError(e.to_string()))?;
            self.fixed_states.write().await.insert(
                tx.connection_id.clone(),
                datazen_driver_api::session::TransactionState::Idle,
            );
            return Ok(());
        }
        let pinned = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;
        let mut conn = pinned.lock().await;

        Self::execute_text_on_conn(&mut conn, "ROLLBACK")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        if split_mysql_statements(sql).len() != 1 {
            return Err(DriverError::InvalidConfig(
                "Explain requires one statement".into(),
            ));
        }
        // query() routes retained handles through their pinned checkout. Both
        // plan formats therefore see the editor's transaction and temp objects.
        let json_rows = self
            .query(handle, &format!("EXPLAIN FORMAT=JSON {sql}"))
            .await?;
        let plan_json = json_rows
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(|value| match value {
                Some(Value::Json(json)) => Some(json.clone()),
                Some(Value::String(text)) => serde_json::from_str(text).ok(),
                _ => None,
            });
        let text_rows = self.query(handle, &format!("EXPLAIN {sql}")).await?;
        let columns = text_rows.columns;
        let result_rows = text_rows.rows;
        let plan_lines: Vec<String> = if columns.is_empty() {
            Vec::new()
        } else {
            result_rows
                .iter()
                .map(|row| {
                    row.iter()
                        .zip(columns.iter())
                        .map(|(val, col)| {
                            let v = match val {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Integer(n)) => n.to_string(),
                                Some(Value::Float(f)) => f.to_string(),
                                Some(Value::Bool(b)) => b.to_string(),
                                Some(Value::Json(j)) => j.to_string(),
                                _ => "NULL".to_string(),
                            };
                            format!("{}: {}", col.name, v)
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect()
        };

        let (total_cost, estimated_rows) = plan_json
            .as_ref()
            .map(Self::extract_mysql_plan_metrics)
            .unwrap_or((None, None));

        let plan_tree = plan_json
            .as_ref()
            .and_then(datazen_driver_api::normalize_mysql_explain_plan);

        // Carry the raw classic EXPLAIN result set (columns + rows) so the Host
        // can render it as a DataTable, same as running EXPLAIN in the editor.
        let mut plan_json = plan_json.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = plan_json.as_object_mut() {
            obj.insert(
                "columns".to_string(),
                serde_json::json!(columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()),
            );
            obj.insert("rows".to_string(), serde_json::json!(result_rows));
        }

        Ok(ExplainResult {
            plan_text: plan_lines.join("\n"),
            plan_json: Some(plan_json),
            plan_tree,
            total_cost,
            estimated_rows,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "legacy session-wide query cancellation is disabled; use an execution handle".into(),
        ))
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        // Keep this execution gated until KILL QUERY completes. A MySQL
        // thread id belongs to a pooled connection and can be reused; early
        // cleanup could otherwise let a delayed cancel hit a later query.
        let gate = {
            let executions = self.query_executions.lock().await;
            let execution = executions.get(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            execution.cancel_gate.clone()
        };
        let _cancel_guard = gate.lock().await;
        let mut executions = self.query_executions.lock().await;
        let (thread_id, control_pool) = {
            let execution = executions.get_mut(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            if !std::sync::Arc::ptr_eq(&execution.cancel_gate, &gate) {
                return Err(DriverError::QueryExecutionNotFound(
                    execution_id.as_str().into(),
                ));
            }
            execution.cancel_requested = true;
            let Some(thread_id) = execution.thread_id else {
                // The stream observes this pending request after acquiring its
                // dedicated connection and before executing user SQL.
                return Ok(());
            };
            (thread_id, execution.control_pool.clone())
        };

        drop(executions);

        let Some(control_pool) = control_pool else {
            if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                execution.cancel_requested = false;
            }
            return Err(DriverError::ConnectionFailed(
                "Control pool not found".into(),
            ));
        };
        let mut conn = match control_pool.acquire().await {
            Ok(conn) => conn,
            Err(error) => {
                if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                return Err(DriverError::ConnectionFailed(error.to_string()));
            }
        };
        let kill_sql = build_kill_query_sql(thread_id);
        match Self::execute_text_on_conn(&mut conn, &kill_sql).await {
            Ok(_) => Ok(()),
            Err(error) => {
                if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                Err(DriverError::QueryFailed(error.to_string()))
            }
        }
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.finish_query_execution(handle, execution_id).await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        true
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT version()")
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        let server_type = if self.is_mariadb || version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };
        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Option<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let Ok(database_row) = sqlx::query("SELECT DATABASE() AS database_name")
            .fetch_one(pool)
            .await
        else {
            return Ok(None);
        };
        let database: Option<String> = database_row.try_get("database_name").unwrap_or(None);
        let Some(database) = database else {
            return Ok(None);
        };

        let cluster_scope = if self.is_mariadb {
            let Ok(rows) = sqlx::query("SHOW GLOBAL STATUS").fetch_all(pool).await else {
                return Ok(None);
            };
            let mut statuses = HashMap::new();
            let mut wsrep_detected = false;
            for row in rows {
                let Ok(name) = row.try_get::<String, _>(0) else {
                    return Ok(None);
                };
                let name = name.to_ascii_lowercase();
                if !name.starts_with("wsrep_") {
                    continue;
                }
                wsrep_detected = true;
                if let Ok(value) = row.try_get::<String, _>(1) {
                    statuses.insert(name, value);
                }
            }
            if wsrep_detected && statuses.is_empty() {
                statuses.insert("wsrep_status_incomplete".to_string(), String::new());
            }
            mariadb_wsrep_scope(Some(&statuses))
        } else {
            let Ok(group_row) =
                sqlx::query("SELECT @@GLOBAL.group_replication_group_name AS group_name")
                    .fetch_one(pool)
                    .await
            else {
                // Older or non-GR MySQL builds may not expose the variable at
                // all. Only fall back when SHOW PLUGINS proves GR is absent or
                // inactive; an active plugin with an unreadable variable is
                // ambiguous and must fail closed.
                let Ok(plugin_rows) = sqlx::query("SHOW PLUGINS").fetch_all(pool).await else {
                    return Ok(None);
                };
                let mut plugin_statuses = HashMap::new();
                for row in plugin_rows {
                    let (Ok(name), Ok(status)) =
                        (row.try_get::<String, _>(0), row.try_get::<String, _>(1))
                    else {
                        return Ok(None);
                    };
                    plugin_statuses.insert(name.to_ascii_lowercase(), status);
                }
                return match mysql_group_replication_plugin_active(Some(&plugin_statuses)) {
                    Some(false) => self.physical_database_node_identity(pool, &database).await,
                    Some(true) | None => Ok(None),
                };
            };
            let Ok(group_name) = group_row.try_get::<String, _>("group_name") else {
                return Ok(None);
            };
            let group_name = group_name.trim();
            if group_name.is_empty() {
                mysql_group_replication_scope(Some(group_name), Ok(None))
            } else {
                // A configured group name alone is not proof that this server
                // is a healthy member. Failure to inspect membership must fail
                // closed rather than treating a peer in that group as separate.
                let membership = sqlx::query(
                    "SELECT MEMBER_STATE AS member_state FROM performance_schema.replication_group_members WHERE MEMBER_ID = @@server_uuid",
                )
                .fetch_optional(pool)
                .await;
                match membership {
                    Ok(None) => mysql_group_replication_scope(Some(group_name), Ok(None)),
                    Ok(Some(row)) => match row.try_get::<String, _>("member_state") {
                        Ok(state) => {
                            mysql_group_replication_scope(Some(group_name), Ok(Some(&state)))
                        }
                        Err(_) => ClusterScopeDecision::Unknown,
                    },
                    Err(_) => ClusterScopeDecision::Unknown,
                }
            }
        };

        match cluster_scope {
            ClusterScopeDecision::Cluster { scope, id } => Ok(Some(canonical_database_identity(
                if self.is_mariadb { "mariadb" } else { "mysql" },
                scope,
                &id,
                &database,
            ))),
            ClusterScopeDecision::Unknown => Ok(None),
            ClusterScopeDecision::NodeFallback => {
                self.physical_database_node_identity(pool, &database).await
            }
        }
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let sql = format!(
            "SHOW CREATE TABLE {}",
            Self::qualified_table_ref(&db, table)
        );
        match self.query(handle, &sql).await {
            Ok(result) => {
                if let Some(create) = extract_show_create_table(&result) {
                    let mut ddl = create;
                    let trimmed = ddl.trim_end();
                    if !trimmed.ends_with(';') {
                        ddl = format!("{};\n", trimmed);
                    } else if !ddl.ends_with('\n') {
                        ddl.push('\n');
                    }
                    return Ok(ddl);
                }
                sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
            }
            Err(_) => {
                sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
            }
        }
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let sql = format!("SHOW CREATE VIEW {}", Self::qualified_table_ref(&db, view));
        let result = self.query(handle, &sql).await?;
        let create = extract_named_create_column(&result, "Create View").or_else(|| {
            if result.columns.len() >= 2 {
                let row = result.rows.first()?;
                let cell = row.get(1)?.as_ref()?;
                match cell {
                    Value::String(s) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        let Some(create) = create else {
            return Err(DriverError::QueryFailed(format!(
                "View definition not found: {view}"
            )));
        };
        let trimmed = create.trim_end();
        if trimmed.ends_with(';') {
            Ok(format!("{trimmed}\n"))
        } else {
            Ok(format!("{trimmed};\n"))
        }
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        let snapshot = if opts.single_transaction {
            self.begin_transaction(handle).await.ok()
        } else {
            None
        };
        let result = async {
            let mut out = String::new();
            if opts.create_database {
                let q = self.quote_ident(database);
                out.push_str(&format!(
                    "CREATE DATABASE IF NOT EXISTS {};\nUSE {};\n\n",
                    q, q
                ));
            }
            out.push_str(
                &sql_dump::dump_sql_database_with_progress(
                    self,
                    handle,
                    database,
                    opts,
                    on_progress,
                )
                .await?,
            );
            Ok(out)
        }
        .await;
        if let Some(tx) = snapshot {
            if result.is_ok() {
                let _ = self.commit(tx).await;
            } else {
                let _ = self.rollback(tx).await;
            }
        }
        result
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_routines(self, handle).await)
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_triggers(self, handle).await)
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(structure::baseline_capabilities(&self.driver_type()))
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities(handle).await?;
        structure::plan_structure_changes(&caps, request)
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        crate::admin_commands::mysql_admin_command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        mut input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        if command == "get_table_schema" {
            if let Some(raw) = input.get("relationSql").and_then(serde_json::Value::as_str) {
                let parsed = sqlparser::parser::Parser::parse_sql(
                    &sqlparser::dialect::MySqlDialect {},
                    &format!("SELECT * FROM {raw}"),
                )
                .map_err(|_| DriverError::InvalidConfig("invalid relation identifier".into()))?;
                let [sqlparser::ast::Statement::Query(query)] = parsed.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let sqlparser::ast::SetExpr::Select(select) = &*query.body else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let [relation] = select.from.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let sqlparser::ast::TableFactor::Table { name, .. } = &relation.relation else {
                    return Err(DriverError::InvalidConfig(
                        "one named relation is required".into(),
                    ));
                };
                let parts = name
                    .0
                    .iter()
                    .map(|part| {
                        part.as_ident()
                            .map(|ident| ident.value.clone())
                            .ok_or_else(|| {
                                DriverError::InvalidConfig("named relation is required".into())
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if input
                    .get("database")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    let context = self.read_fixed_session(handle).await?;
                    input["database"] =
                        serde_json::json!(context.namespace.database.ok_or_else(|| {
                            DriverError::InvalidConfig("database is required".into())
                        })?);
                }
                match parts.as_slice() {
                    [table] => {
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::Value::Null;
                    }
                    [database, table] => {
                        input["database"] = serde_json::json!(database);
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::Value::Null;
                    }
                    _ => {
                        return Err(DriverError::InvalidConfig(
                            "unsupported relation namespace".into(),
                        ))
                    }
                }
            }
        }
        let raw_relation_result = if command == "get_table_schema"
            && input.get("relationSql").is_some()
            && self.fixed_sessions.read().await.contains_key(&handle.id)
        {
            let database = input["database"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
            let table = input["table"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            let schema = self
                .table_schema_exact_on_connection(&mut **conn, table, database)
                .await?;
            Some(CommandResult::new(serde_json::json!({"schema": schema})))
        } else {
            None
        };
        if let Some(mut result) = match raw_relation_result {
            Some(result) => Some(result),
            None => {
                try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
            }
        } {
            if command == "get_table_schema"
                && self.fixed_sessions.read().await.contains_key(&handle.id)
            {
                let table = input["table"]
                    .as_str()
                    .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
                let requested = input["database"]
                    .as_str()
                    .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
                let pinned = self
                    .transactions
                    .lock()
                    .await
                    .get(&handle.id)
                    .cloned()
                    .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
                let mut conn = pinned.lock().await;
                let database = if requested.is_empty() {
                    sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                        .fetch_one(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?
                        .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?
                } else {
                    requested.to_string()
                };
                let relation = if input.get("relationSql").is_some() {
                    format!(
                        "`{}`.`{}`",
                        database.replace('`', "``"),
                        table.replace('`', "``")
                    )
                } else {
                    Self::qualified_table_ref(&database, table)
                };
                let create_row = sqlx::query(&format!("SHOW CREATE TABLE {}", relation))
                    .fetch_one(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let temporary = decode_mysql_text_idx(&create_row, 1)
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE TEMPORARY TABLE");
                result.data["resolvedTarget"] = serde_json::json!({ "database": database, "catalog": null, "schema": null, "table": decode_mysql_text_idx(&create_row, 0), "path": [], "sessionScoped": temporary });
            }
            return Ok(result);
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(
                self,
                &self.driver_type(),
                handle,
                command,
                input,
            )
            .await;
        }
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        crate::admin_commands::execute_mysql_admin_command(pool, command, input).await
    }
}
