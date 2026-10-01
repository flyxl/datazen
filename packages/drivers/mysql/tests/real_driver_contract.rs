//! MySQL binding of the shared real-driver contract template.
//!
//! Second binding on purpose: MySQL and PostgreSQL share the *shape* of the
//! contract (two sessions, two databases, A/B markers) but not the semantic
//! model — MySQL is schema-less, the database *is* the namespace, DDL is
//! auto-committed, and every database is served from one pool rather than from a
//! per-database resource. One shared template, two genuinely different
//! contracts, and every dialect difference stays in this file, which is why the
//! template can hold no SQL of its own.
//!
//! Run with `-- --nocapture` to see the unverified-scope report.

use datazen_driver_api::DdlAtomicity;
use datazen_driver_mysql::MysqlDriver;

#[path = "../../http-support/tests/support/real_driver_contract.rs"]
mod support;

use support::{Capability, Contract, Dialect};

const CONTRACT: Contract = Contract {
    label: "mysql",
    // One implementation serves both wire dialects.
    driver_types: &["mysql", "mariadb"],
    env_prefix: "TEST_MYSQL_",
    default_port: 3306,
    default_user: "root",
    // No schemas: `USE` moves between databases, and the default schema is absent.
    has_schema_level: false,
    default_schema: None,
    has_multi_database: true,
    ddl_atomicity: DdlAtomicity::AutoCommitPerStatement,
    supports_offset: true,
    // One pool serves every database, so there is no per-database resource to
    // hand out; target routing happens through the session.
    per_database_resource: false,
    // Temporary tables live in the session's own temporary namespace.
    session_scoped_state: Capability::Supported,
    // InnoDB gives a repeatable read within a transaction, but the driver
    // refuses rather than pretending a bare snapshot exists.
    read_snapshots: Capability::Unsupported,
    precise_cancel: Capability::Supported,
    transactions: Capability::Supported,
    reset_for_reuse: Capability::Supported,
    dialect: Dialect {
        marker: "dz_fixture_marker",
        // `{table}` is filled in per live case: every live case in this crate runs
        // concurrently against the same two fixture targets, so the marker relation
        // must not be a name fixed in the dialect.
        create_marker:
            "CREATE TABLE IF NOT EXISTS {table} (dz_fixture_marker_value VARCHAR(255) NOT NULL)",
        insert_marker:
            "INSERT INTO {table} (dz_fixture_marker_value) VALUES ('{marker}')",
        select_marker: "SELECT dz_fixture_marker_value FROM {table}",
        drop_marker: "DROP TABLE IF EXISTS {table}",
        current_namespace: "DATABASE()",
        decoy_text_select: "SELECT 'USE dz_fixture_other' AS dz_fixture_decoy",
        missing_object: "dz_fixture_absent_object",
        create_named: "CREATE TABLE {name} (dz_fixture_value INT NOT NULL)",
        insert_named: "INSERT INTO {name} (dz_fixture_value) VALUES (42)",
        select_named: "SELECT dz_fixture_value FROM {name}",
        drop_named: "DROP TABLE IF EXISTS {name}",
        create_temp: "CREATE TEMPORARY TABLE {name} (dz_fixture_value INT NOT NULL)",
        insert_temp: "INSERT INTO {name} (dz_fixture_value) VALUES (7)",
        select_temp: "SELECT dz_fixture_value FROM {name}",
        switch_keyword: "USE",
    },
};

fn contract_driver() -> MysqlDriver {
    MysqlDriver::new(false)
}
