//! T147-S1: VM placeholders for the segment-2 libs the Data Transfer window
//! reads — **labelled placeholders, to be replaced by their real tasks**:
//!
//! | React source | real task (segment 2) | what lives here |
//! |---|---|---|
//! | `lib/syncTaxonomy.ts` (`normalizeDriverId`) | T625's neighbour | `normalize_driver_id` |
//! | `lib/syncPairing.ts` (`syncCategory`, `normalizeSyncFamily`) | T625 | `sync_category`, `normalize_sync_family` + the driver-meta table |
//! | `lib/transferPairing.ts` | T636 | `resolve_transfer_pairing`, `is_transfer_target_supported` |
//! | `lib/migrationVerification.ts` | T563 | `is_verified_migration_pair` |
//! | `lib/migrationWindowPrefill.ts` (pure parts) | T563 | `MigrationWindowPrefill`, `resolve_default_database`, `pick_prefill_database` |
//!
//! The window routes every pairing/prefill call through this module and no
//! other pairing logic exists in the window — the same shape as the
//! `transfer_mapping_view` placeholder for T305. Each function is the TS
//! source transliterated line for line (doc comments name the lines); when
//! the real task lands it replaces the placeholder here and the window's
//! call sites keep working.
//!
//! The URL-reading halves of `migrationWindowPrefill.ts`
//! (`readMigrationPrefillFromUrl`, `useMigrationEndpointPrefill`) are WebView
//! routing concepts (`getUrlParam` is T644's seam, per `views/app_root.rs`);
//! the window's host feeds a [`MigrationWindowPrefill`] instead, and the
//! connection-id application effect (`appliedRef`) is the Vm's
//! `apply_endpoint_prefill`.

// ---------------------------------------------------------------------------
// `lib/syncTaxonomy.ts` — `normalizeDriverId`, transliterated.
// ---------------------------------------------------------------------------

/// React `normalizeDriverId` (`lib/syncTaxonomy.ts:5-18`): driver id aliases
/// for sync/transfer taxonomy lookup (mirrors driver-api
/// `normalize_driver_id`).
pub fn normalize_driver_id(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "postgres" => "postgresql".to_owned(),
        "mssql" => "sqlserver".to_owned(),
        "presto" => "trino".to_owned(),
        "tidb" | "oceanbase" => "mysql".to_owned(),
        other => other.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// The driver-meta slice the pairing functions read. The Rust crate has no
// driver registry (`backup_db_meta.rs` set the precedent: a static match
// table mirroring the `DB_REGISTRY` slice a window reads). Fields here:
// `label`, `supportsSQL`, `category`, `sqlDialect`, `syncFamily` — exactly
// the five `syncCategory` / `normalizeSyncFamily` / the window's dialect
// filter read. Git drivers (kiwi, superset) are absent from a basic build
// and are handled by the fallback logic, exactly as React's
/// `categoryFromMeta(undefined, id)` does.
// ---------------------------------------------------------------------------

/// The `DB_REGISTRY[…]?` slice the pairing path reads.
#[derive(Clone, Copy, Debug)]
struct DbSyncMeta {
    label: &'static str,
    supports_sql: bool,
    category: Option<SyncCategory>,
    sql_dialect: Option<&'static str>,
    sync_family: Option<&'static str>,
}

/// React `SyncCategory` (`lib/syncPairing.ts:29`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncCategory {
    Sql,
    Document,
    Kv,
    Other,
}

impl SyncCategory {
    /// The TS union's own spelling, for the pairing reason strings.
    fn as_str(self) -> &'static str {
        match self {
            SyncCategory::Sql => "sql",
            SyncCategory::Document => "document",
            SyncCategory::Kv => "kv",
            SyncCategory::Other => "other",
        }
    }
}

/// The driver-meta table: every path driver's meta that declares the fields
/// the pairing path reads (extracted from
/// `crates/drivers/*/ui/meta.ts`, the files `resolve-drivers.mjs` turns into
/// `DRIVER_DB_ENTRIES`). Types absent here are the "optional drivers may be
/// absent from generated UI metadata in a basic build" case of the TS source
/// and fall through the same fallback logic as React.
const DB_SYNC_META: &[(&str, DbSyncMeta)] = &[
    (
        "postgresql",
        DbSyncMeta {
            label: "PostgreSQL",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("postgresql"),
            sync_family: None,
        },
    ),
    (
        "questdb",
        DbSyncMeta {
            label: "QuestDB",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("postgresql"),
            sync_family: None,
        },
    ),
    (
        "cloudberry",
        DbSyncMeta {
            label: "Cloudberry",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("postgresql"),
            sync_family: None,
        },
    ),
    (
        "mysql",
        DbSyncMeta {
            label: "MySQL",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "mariadb",
        DbSyncMeta {
            label: "MariaDB",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "doris",
        DbSyncMeta {
            label: "Doris",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "starrocks",
        DbSyncMeta {
            label: "StarRocks",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "manticore",
        DbSyncMeta {
            label: "ManticoreSearch",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "ob_oracle",
        DbSyncMeta {
            label: "OceanBase (Oracle)",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("mysql"),
            sync_family: None,
        },
    ),
    (
        "sqlite",
        DbSyncMeta {
            label: "SQLite",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("sqlite"),
            sync_family: None,
        },
    ),
    (
        "redis",
        DbSyncMeta {
            label: "Redis",
            supports_sql: false,
            category: Some(SyncCategory::Kv),
            sql_dialect: None,
            sync_family: Some("redis"),
        },
    ),
    (
        "mongodb",
        DbSyncMeta {
            label: "MongoDB",
            supports_sql: false,
            category: Some(SyncCategory::Document),
            sql_dialect: Some("mongodb"),
            sync_family: None,
        },
    ),
    (
        "sqlserver",
        DbSyncMeta {
            label: "SQL Server",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("sqlserver"),
            sync_family: None,
        },
    ),
    (
        "clickhouse",
        DbSyncMeta {
            label: "ClickHouse",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("clickhouse"),
            sync_family: None,
        },
    ),
    (
        "duckdb",
        DbSyncMeta {
            label: "DuckDB",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("duckdb"),
            sync_family: None,
        },
    ),
    (
        "elasticsearch",
        DbSyncMeta {
            label: "Elasticsearch",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("elasticsearch"),
            sync_family: None,
        },
    ),
    (
        "rqlite",
        DbSyncMeta {
            label: "RQLite",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("sqlite"),
            sync_family: None,
        },
    ),
    (
        "turso",
        DbSyncMeta {
            label: "Turso / libSQL",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("sqlite"),
            sync_family: None,
        },
    ),
    (
        "influxdb",
        DbSyncMeta {
            label: "InfluxDB",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("generic"),
            sync_family: Some("influxdb"),
        },
    ),
    (
        "victoriametrics",
        DbSyncMeta {
            label: "VictoriaMetrics",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("generic"),
            sync_family: Some("victoriametrics"),
        },
    ),
    (
        "hbase",
        DbSyncMeta {
            label: "HBase",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("generic"),
            sync_family: Some("hbase"),
        },
    ),
    (
        "vector",
        DbSyncMeta {
            label: "Vector DB",
            supports_sql: true,
            category: Some(SyncCategory::Sql),
            sql_dialect: Some("generic"),
            sync_family: Some("vector"),
        },
    ),
];

/// React `registryMeta` (`lib/syncPairing.ts:13-15`): the raw lookup, no
/// alias fallback.
fn registry_meta(db_type: &str) -> Option<&'static DbSyncMeta> {
    DB_SYNC_META
        .iter()
        .find(|(id, _)| *id == db_type)
        .map(|(_, meta)| meta)
}

/// React `categoryFromMeta` (`lib/syncPairing.ts:31-40`), line for line.
fn category_from_meta(meta: Option<&DbSyncMeta>, db_type: &str) -> SyncCategory {
    if let Some(meta) = meta {
        if let Some(category) = meta.category {
            return category;
        }
    }
    let id = normalize_driver_id(db_type);
    if id == "kiwi" || id == "superset" {
        return SyncCategory::Other;
    }
    SyncCategory::Sql
}

/// React `syncCategory` (`lib/syncPairing.ts:36-39`): the category lookup
/// with the alias fallback.
pub fn sync_category(db_type: &str) -> SyncCategory {
    let meta = registry_meta(db_type).or_else(|| {
        let normalized = normalize_driver_id(db_type);
        DB_SYNC_META
            .iter()
            .find(|(id, _)| *id == normalized)
            .map(|(_, meta)| meta)
    });
    category_from_meta(meta, db_type)
}

/// React `normalizeSyncFamily` (`lib/syncPairing.ts:42-47`): the sync
/// dialect family for Transfer pairing.
pub fn normalize_sync_family(db_type: &str) -> String {
    let meta = registry_meta(db_type).or_else(|| {
        let normalized = normalize_driver_id(db_type);
        DB_SYNC_META
            .iter()
            .find(|(id, _)| *id == normalized)
            .map(|(_, meta)| meta)
    });
    if let Some(family) = meta.and_then(|meta| meta.sync_family) {
        return family.to_owned();
    }
    if let Some(dialect) = meta.and_then(|meta| meta.sql_dialect) {
        if dialect != "generic" {
            return dialect.to_owned();
        }
    }
    normalize_driver_id(db_type)
}

// ---------------------------------------------------------------------------
// `lib/transferPairing.ts`, transliterated.
// ---------------------------------------------------------------------------

/// React `TransferPath` (`lib/transferPairing.ts:7`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferPath {
    Direct,
    Ir,
    Unsupported,
}

/// React `TransferPairingResult` (`lib/transferPairing.ts:9-14`).
#[derive(Clone, Debug, PartialEq)]
pub struct TransferPairingResult {
    pub path: TransferPath,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}

/// React `resolveTransferPairing` (`lib/transferPairing.ts:19-64`), line for
/// line. Allows SQL Direct + IR; rejects cross-category pairs
/// (SQL↔Redis etc.).
pub fn resolve_transfer_pairing(source_type: &str, target_type: &str) -> TransferPairingResult {
    // Optional drivers may be absent from generated UI metadata in a basic
    // build. Keep their known taxonomy stable in that case.
    let transfer_category = |database_type: &str| {
        let id = normalize_driver_id(database_type);
        if id == "redis" {
            return SyncCategory::Kv;
        }
        if id == "mongodb" {
            return SyncCategory::Document;
        }
        sync_category(database_type)
    };
    let src_cat = transfer_category(source_type);
    let tgt_cat = transfer_category(target_type);

    if src_cat != tgt_cat {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some(format!(
                "Transfer between {} ({}) and {} ({}) is not supported",
                source_type,
                src_cat.as_str(),
                target_type,
                tgt_cat.as_str()
            )),
        };
    }

    if src_cat == SyncCategory::Other {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some(format!(
                "Transfer is not supported for database type '{}'",
                source_type
            )),
        };
    }

    // Redis currently has no Data Transfer row adapters. Its Data Sync pairing
    // policy must not imply an executable Data Transfer path.
    if normalize_driver_id(source_type) == "redis" || normalize_driver_id(target_type) == "redis" {
        return TransferPairingResult {
            path: TransferPath::Unsupported,
            supported: false,
            family: None,
            reason: Some("Data Transfer has no Redis source and target adapters".to_owned()),
        };
    }

    let src_family = normalize_sync_family(source_type);
    let tgt_family = normalize_sync_family(target_type);

    if src_family == tgt_family {
        return TransferPairingResult {
            path: TransferPath::Direct,
            supported: true,
            family: Some(src_family),
            reason: None,
        };
    }

    if src_cat == SyncCategory::Sql {
        return TransferPairingResult {
            path: TransferPath::Ir,
            supported: true,
            family: None,
            reason: None,
        };
    }

    TransferPairingResult {
        path: TransferPath::Unsupported,
        supported: false,
        family: None,
        reason: Some(format!(
            "Transfer between {} and {} is not supported",
            source_type, target_type
        )),
    }
}

/// React `isTransferTargetSupported` (`lib/transferPairing.ts:66-68`).
pub fn is_transfer_target_supported(source_type: &str, target_type: &str) -> bool {
    resolve_transfer_pairing(source_type, target_type).supported
}

// ---------------------------------------------------------------------------
// `lib/migrationVerification.ts`, transliterated.
// ---------------------------------------------------------------------------

/// React `isVerifiedMigrationPair` (`lib/migrationVerification.ts:3-6`):
/// pairings covered by live end-to-end migration runs for this release.
pub fn is_verified_migration_pair(source_type: &str, target_type: &str) -> bool {
    let verified = ["postgresql", "mysql"];
    let source_ok = verified.contains(&source_type.to_ascii_lowercase().as_str());
    let target_ok = verified.contains(&target_type.to_ascii_lowercase().as_str());
    source_ok && target_ok
}

// ---------------------------------------------------------------------------
// `lib/migrationWindowPrefill.ts` — the pure halves, transliterated.
// ---------------------------------------------------------------------------

/// React `MigrationWindowPrefill` (`lib/migrationWindowPrefill.ts:8-15`):
/// URL query params shared by Data Sync / Transfer / Schema Diff
/// sub-windows. The host reads them (the WebView URL itself is T644's
/// seam) and hands the struct to the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MigrationWindowPrefill {
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

impl MigrationWindowPrefill {
    /// Whether any endpoint id is prefilled — the guard React's
    /// `useMigrationEndpointPrefill` effect checks before applying.
    pub fn has_endpoint_ids(&self) -> bool {
        self.source_id.is_some() || self.target_id.is_some()
    }
}

/// React `resolveDefaultDatabase`
/// (`lib/migrationWindowPrefill.ts:47-54`), line for line.
pub fn resolve_default_database(
    databases: Option<&[String]>,
    preferred: &str,
    prev: &str,
) -> String {
    let list = databases.unwrap_or(&[]);
    if list.iter().any(|db| db == preferred) {
        return preferred.to_owned();
    }
    if !prev.is_empty() && list.iter().any(|db| db == prev) {
        return prev.to_owned();
    }
    list.first().cloned().unwrap_or_default()
}

/// React `pickPrefillDatabase` (`lib/migrationWindowPrefill.ts:56-69`),
/// line for line, including the consume-on-use: a prefill that fires is
/// cleared from the ref, so it can never fire twice.
pub fn pick_prefill_database(
    prefill: &mut MigrationWindowPrefill,
    source_side: bool,
    databases: &[String],
    fallback: impl Fn(&str) -> String,
    prev: &str,
) -> String {
    let prefilled = if source_side {
        prefill.source_database.clone()
    } else {
        prefill.target_database.clone()
    };
    if let Some(prefilled) = prefilled {
        if databases.contains(&prefilled) {
            if source_side {
                prefill.source_database = None;
            } else {
                prefill.target_database = None;
            }
            return prefilled;
        }
    }
    fallback(prev)
}

