//! PostgreSQL binding of the shared real-driver contract template.
//!
//! The template itself lives in
//! `packages/drivers/http-support/tests/support/real_driver_contract.rs` and is
//! pulled in with `#[path]`; nothing about PostgreSQL is written there. This file
//! only declares what PostgreSQL *is* — its driver type, its capabilities, and
//! its SQL dialect — and hands back a driver instance. Every real-dialect
//! assertion about PostgreSQL therefore still lives inside this driver crate
//! (`fake-runtime-fixtures.md` §10.1), never in Host.
//!
//! Run with `-- --nocapture` to see the unverified-scope report.

use datazen_driver_api::DdlAtomicity;
use datazen_driver_postgres::PostgresDriver;

#[path = "../../http-support/tests/support/real_driver_contract.rs"]
mod support;

use support::{Capability, Contract, Dialect};

/// PostgreSQL is a schema-qualified, multi-database engine reached over one
/// protocol. Its per-database routing is a **separate pooled resource**: a
/// statement addressed to another database runs over a pool opened for that
/// database, and the session's own database is deliberately absent from
/// `open_databases`, because the session reaches it by name in its connection
/// string rather than by selecting it per statement. `CM-16` pins that down.
const CONTRACT: Contract = Contract {
    label: "postgresql",
    driver_types: &["postgresql"],
    env_prefix: "TEST_PG_",
    default_port: 5432,
    default_user: "postgres",
    has_schema_level: true,
    default_schema: Some("public"),
    has_multi_database: true,
    ddl_atomicity: DdlAtomicity::Transactional,
    supports_offset: true,
    per_database_resource: true,
    // A session keeps its own `search_path` and its own temporary schema; nothing
    // else in a second session may observe either.
    session_scoped_state: Capability::Supported,
    // `BEGIN ... ISOLATION LEVEL REPEATABLE READ` gives a stable read view.
    read_snapshots: Capability::Supported,
    // `cancel_query` is deliberately disabled; `cancel_query_with_execution`
    // addresses one `pg_cancel_backend` PID.
    precise_cancel: Capability::Supported,
    transactions: Capability::Supported,
    // Nothing in the contract hands a closed database back to the pool.
    reset_for_reuse: Capability::Supported,
    dialect: Dialect {
        marker: "dz_fixture_marker",
        // `{table}` is filled in per live case: every live case in this crate
        // runs concurrently against the same two fixture targets, so the marker
        // relation must not be a name fixed in the dialect.
        create_marker: "CREATE TABLE IF NOT EXISTS {table} (dz_fixture_marker_value TEXT NOT NULL)",
        insert_marker: "INSERT INTO {table} (dz_fixture_marker_value) VALUES ('{marker}')",
        select_marker: "SELECT dz_fixture_marker_value FROM {table}",
        drop_marker: "DROP TABLE IF EXISTS {table}",
        current_namespace: "current_database()",
        // The switch keyword inside a string literal must not move the context.
        decoy_text_select: "SELECT 'USE dz_fixture_other' AS dz_fixture_decoy",
        missing_object: "dz_fixture_absent_object",
        create_named: "CREATE TABLE {name} (dz_fixture_value INTEGER NOT NULL)",
        insert_named: "INSERT INTO {name} (dz_fixture_value) VALUES (42)",
        select_named: "SELECT dz_fixture_value FROM {name}",
        drop_named: "DROP TABLE IF EXISTS {name}",
        create_temp: "CREATE TEMP TABLE {name} (dz_fixture_value INTEGER NOT NULL)",
        insert_temp: "INSERT INTO {name} (dz_fixture_value) VALUES (7)",
        select_temp: "SELECT dz_fixture_value FROM {name}",
        switch_keyword: "USE",
    },
};

fn contract_driver() -> PostgresDriver {
    PostgresDriver::new()
}
