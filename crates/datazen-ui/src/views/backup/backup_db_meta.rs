//! T026-S1: the driver-meta table behind the Backup window's option derivation.
//!
//! Mirrors the `DB_REGISTRY` slice the window reads (`label`,
//! `supportsBackup`, `sqlDialect` / `sqlDialectStrategy`) plus the
//! `getSqlDialect` backup-option tables (`lib/sqlDialects/*`), as a
//! static match table — the Rust crate has no driver registry. See
//! `backup_window_state` for the S1/S2 boundary and the documented
//! deviations; a new driver type needs one row in `db_type_meta`.

// The production consumer is the Backup window (T026-S2); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

/// One dialect backup option (`sqlDialects`: `{ id, label }`, e.g.
/// `{ id: 'routines', label: '--routines' }`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupOption {
    pub id: &'static str,
    pub label: &'static str,
}

/// The `sqlDialect` family a driver meta selects
/// (`getSqlDialect` in `lib/sqlDialects/index.ts`: an explicit
/// `sqlDialectStrategy` wins, otherwise the `sqlDialect` family name
/// indexes `DIALECTS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialectFamily {
    Mysql,
    Postgres,
    Sqlite,
    SqlServer,
    ClickHouse,
    DuckDb,
    Oracle,
    Elasticsearch,
    Mongodb,
    Generic,
}

/// The driver-meta slice the Backup window reads: `label`
/// (`getDbLabel`), `supportsBackup` (`DB_REGISTRY[…]?.supportsBackup`),
/// and the dialect family (`getSqlDialect`).
#[derive(Clone, Copy, Debug)]
struct DbTypeMeta {
    label: &'static str,
    supports_backup: bool,
    dialect: Option<DialectFamily>,
}

const MYSQL_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "no-data",
        label: "--no-data",
    },
    BackupOption {
        id: "add-drop-table",
        label: "--add-drop-table",
    },
    BackupOption {
        id: "single-transaction",
        label: "--single-transaction",
    },
    BackupOption {
        id: "routines",
        label: "--routines",
    },
    BackupOption {
        id: "triggers",
        label: "--triggers",
    },
    BackupOption {
        id: "no-create-info",
        label: "--no-create-info",
    },
];

const PG_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
    BackupOption {
        id: "create",
        label: "--create",
    },
    BackupOption {
        id: "no-owner",
        label: "--no-owner",
    },
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "routines",
        label: "--routines",
    },
    BackupOption {
        id: "triggers",
        label: "--triggers",
    },
];

const SQLITE_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
    BackupOption {
        id: "no-owner",
        label: "--no-owner",
    },
    BackupOption {
        id: "single-transaction",
        label: "--single-transaction",
    },
];

const SQLSERVER_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
    BackupOption {
        id: "no-owner",
        label: "--no-owner",
    },
    BackupOption {
        id: "single-transaction",
        label: "--single-transaction",
    },
];

const ORACLE_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
];

const CLICKHOUSE_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
    BackupOption {
        id: "no-owner",
        label: "--no-owner",
    },
];

const DUCKDB_BACKUP_OPTIONS: &[BackupOption] = &[
    BackupOption {
        id: "schema-only",
        label: "--schema-only",
    },
    BackupOption {
        id: "data-only",
        label: "--data-only",
    },
    BackupOption {
        id: "clean",
        label: "--clean",
    },
    BackupOption {
        id: "no-owner",
        label: "--no-owner",
    },
    BackupOption {
        id: "single-transaction",
        label: "--single-transaction",
    },
];

/// The `DB_REGISTRY` slice for one database type, mirrored from the
/// shipped driver metas (`crates/drivers/*/ui/meta.ts`).
///
/// `None` is an unknown type: React's `DB_REGISTRY[type]` is
/// `undefined`, so the label falls back to the id, `supportsBackup`
/// is falsy, and `getSqlDialect` returns `null`.
fn db_type_meta(db_type: &str) -> Option<DbTypeMeta> {
    Some(match db_type {
        // mysql crate: all six metas spread `mysqlDialectStrategy`.
        "mysql" => DbTypeMeta {
            label: "MySQL",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        "mariadb" => DbTypeMeta {
            label: "MariaDB",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        "doris" => DbTypeMeta {
            label: "Doris",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        "starrocks" => DbTypeMeta {
            label: "StarRocks",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        "manticore" => DbTypeMeta {
            label: "ManticoreSearch",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        "ob_oracle" => DbTypeMeta {
            label: "OceanBase (Oracle)",
            supports_backup: true,
            dialect: Some(DialectFamily::Mysql),
        },
        // postgres crate: all three metas spread `postgresqlDialectStrategy`.
        "postgresql" => DbTypeMeta {
            label: "PostgreSQL",
            supports_backup: true,
            dialect: Some(DialectFamily::Postgres),
        },
        "questdb" => DbTypeMeta {
            label: "QuestDB",
            supports_backup: true,
            dialect: Some(DialectFamily::Postgres),
        },
        "cloudberry" => DbTypeMeta {
            label: "Cloudberry",
            supports_backup: true,
            dialect: Some(DialectFamily::Postgres),
        },
        // sqlite family: `sqlDialect: 'sqlite'`.
        "sqlite" => DbTypeMeta {
            label: "SQLite",
            supports_backup: true,
            dialect: Some(DialectFamily::Sqlite),
        },
        "rqlite" => DbTypeMeta {
            label: "RQLite",
            supports_backup: true,
            dialect: Some(DialectFamily::Sqlite),
        },
        "turso" => DbTypeMeta {
            label: "Turso / libSQL",
            supports_backup: true,
            dialect: Some(DialectFamily::Sqlite),
        },
        "sqlserver" => DbTypeMeta {
            label: "SQL Server",
            supports_backup: true,
            dialect: Some(DialectFamily::SqlServer),
        },
        "clickhouse" => DbTypeMeta {
            label: "ClickHouse",
            supports_backup: true,
            dialect: Some(DialectFamily::ClickHouse),
        },
        "duckdb" => DbTypeMeta {
            label: "DuckDB",
            supports_backup: true,
            dialect: Some(DialectFamily::DuckDb),
        },
        // No backup; dialect options are empty in every case, but the
        // gate in `backup_options_for` still applies.
        "redis" => DbTypeMeta {
            label: "Redis",
            supports_backup: false,
            dialect: None,
        },
        "mongodb" => DbTypeMeta {
            label: "MongoDB",
            supports_backup: false,
            dialect: Some(DialectFamily::Mongodb),
        },
        "elasticsearch" => DbTypeMeta {
            label: "Elasticsearch",
            supports_backup: false,
            dialect: Some(DialectFamily::Elasticsearch),
        },
        "hbase" => DbTypeMeta {
            label: "HBase",
            supports_backup: false,
            dialect: Some(DialectFamily::Generic),
        },
        "influxdb" => DbTypeMeta {
            label: "InfluxDB",
            supports_backup: false,
            dialect: Some(DialectFamily::Generic),
        },
        "vector" => DbTypeMeta {
            label: "Vector DB",
            supports_backup: false,
            dialect: Some(DialectFamily::Generic),
        },
        "victoriametrics" => DbTypeMeta {
            label: "VictoriaMetrics",
            supports_backup: false,
            dialect: Some(DialectFamily::Generic),
        },
        _ => return None,
    })
}

/// React `getDbLabel`: `DB_REGISTRY[dbType]?.label ?? dbType`.
pub fn db_label(db_type: &str) -> String {
    match db_type_meta(db_type) {
        Some(meta) => meta.label.to_owned(),
        None => db_type.to_owned(),
    }
}

/// React `DB_REGISTRY[dbType]?.supportsBackup` (falsy when absent).
pub fn db_supports_backup(db_type: &str) -> bool {
    db_type_meta(db_type).is_some_and(|meta| meta.supports_backup)
}

/// React `backupOptions`: `[]` without a selected connection, without
/// `supportsBackup`, or without a dialect; otherwise the dialect's
/// `backupOptions` table, in dialect order.
pub fn backup_options_for(db_type: &str) -> &'static [BackupOption] {
    let Some(meta) = db_type_meta(db_type) else {
        return &[];
    };
    if !meta.supports_backup {
        return &[];
    }
    match meta.dialect {
        Some(DialectFamily::Mysql) => MYSQL_BACKUP_OPTIONS,
        Some(DialectFamily::Postgres) => PG_BACKUP_OPTIONS,
        Some(DialectFamily::Sqlite) => SQLITE_BACKUP_OPTIONS,
        Some(DialectFamily::SqlServer) => SQLSERVER_BACKUP_OPTIONS,
        Some(DialectFamily::ClickHouse) => CLICKHOUSE_BACKUP_OPTIONS,
        Some(DialectFamily::DuckDb) => DUCKDB_BACKUP_OPTIONS,
        Some(DialectFamily::Oracle) => ORACLE_BACKUP_OPTIONS,
        // elasticsearch / mongodb / generic declare no options.
        Some(DialectFamily::Elasticsearch)
        | Some(DialectFamily::Mongodb)
        | Some(DialectFamily::Generic)
        | None => &[],
    }
}
