//! MySQL / MariaDB driver backed by sqlx MySqlPool.
//!
//! This module is the driver root: it declares the submodules, owns the
//! [`MysqlDriver`] state and the shared helpers every submodule needs, and
//! re-exports the names the submodules reach through `super::`.  The driver
//! behaviour itself lives in `src/mysql/*.rs`, one responsibility per file.

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

mod cluster_scope;
mod columns;
mod ddl_parser;
mod driver;
mod driver_command;
mod driver_connection;
mod driver_explain;
mod driver_query;
mod driver_structure;
mod driver_transaction;
mod identity;
mod plan_extract;
mod pool;

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

pub(crate) use cluster_scope::{
    canonical_database_identity, mariadb_wsrep_scope, mysql_group_replication_plugin_active,
    mysql_group_replication_scope, ClusterScopeDecision,
};
pub(crate) use type_decode::{decode_mysql_text, decode_mysql_text_idx, decode_mysql_text_opt};

/// The migration renderer, capabilities, and type normalizer live in sibling
/// crate modules; the driver submodules reach them through `super::`, so the
/// root re-exports them under the names the trait impl uses.
pub(crate) use crate::{MysqlMigrationCapabilities, MysqlMigrationRenderer, MysqlTypeNormalizer};

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
    pub(crate) fn is_table_not_found_error(err_msg: &str) -> bool {
        let lower = err_msg.to_ascii_lowercase();
        lower.contains("1146")
            || lower.contains("doesn't exist")
            || lower.contains("does not exist")
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

    pub(crate) fn quote_identifier(name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }
}
