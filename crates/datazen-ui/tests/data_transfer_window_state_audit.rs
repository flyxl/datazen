//! T147-S1 independent audit: fresh vectors against the React sources.
//!
//! Written as the tester round for the state-model slice — none of the
//! builders reuse the coder's; every expectation is derived from the TS
//! sources named in the assertions:
//!
//! * the driver-meta table behind the pairing placeholder (all 22 rows
//!   vs `crates/drivers/*/ui/meta.ts`);
//! * the pairing branch matrix of `lib/transferPairing.ts`;
//! * `targetOptions` fail-open behaviour for an unknown source type
//!   (`DB_REGISTRY[unknown]` is undefined in React too);
//! * `canNext`'s endpoints arm for a proxy driver (category `other`);
//! * `buildJob`'s file-token preservation and the database branch's
//!   indifference to a picked file;
//! * the inspect application's first-enabled fallback for mixed rows;
//! * the profile round-trip (save payload -> load application) for the
//!   sql-file destination;
//! * the write-plan/ddl `max` in the execute progress count;
//! * the i18n keys the step rail asks for exist.
//!
//! Mutation red-proof (run by hand, transcript in the commit message):
//! inverting `row_clears_gate`'s enabled guard or `is_verified_migration_pair`'s
//! set membership turns these red; the restore returns them green.

use datazen_ui::i18n::has;
use datazen_ui::views::{
    is_transfer_target_supported, is_verified_migration_pair, resolve_transfer_pairing,
    sync_category, normalize_sync_family, ConnectionOption, DdlKind, DedicatedSessionVm,
    MigrationWindowPrefill, SqlFileCompression, SqlFileEncoding, SyncCategory, TransferColumnMapping,
    TransferConnectionVm, TransferDdlPreviewVm, TransferDestinationMode, TransferMode,
    TransferPairingResult, TransferPath, TransferPreviewVm, TransferSqlFileTargetVm,
    TransferTablePatch, TransferTableRow, TransferTableVm, TransferWritePlanVm,
    DataTransferWindowVm, WizardStep, WriteMode,
};

fn connection(id: &str, db_type: &str) -> TransferConnectionVm {
    TransferConnectionVm {
        id: id.to_owned(),
        name: format!("name-{id}"),
        database_type: db_type.to_owned(),
        database: None,
        schema: None,
        read_only: false,
    }
}

fn table_row(source: &str, target: &str) -> TransferTableRow {
    TransferTableRow {
        vm: TransferTableVm {
            source_table: source.to_owned(),
            target_table: target.to_owned(),
            ..TransferTableVm::default()
        },
        ddl_override: None,
        incompatible_reason: None,
        source_row_count: None,
        source_filter: None,
    }
}

fn session_vm(id: &str) -> DedicatedSessionVm {
    DedicatedSessionVm {
        connection_id: format!("c-{id}"),
        database: "db".to_owned(),
        db_session_id: format!("s-{id}"),
    }
}

// ---------------------------------------------------------------------------
// The driver-meta table (crates/drivers/*/ui/meta.ts, extracted verbatim).
// ---------------------------------------------------------------------------

/// (id, label, category, sqlDialect, syncFamily) for every path driver —
/// read straight out of the driver meta sources; the placeholder table
/// must agree row for row.
const DRIVER_META: &[(&str, &str, &str, Option<&str>, Option<&str>)] = &[
    ("postgresql", "PostgreSQL", "sql", Some("postgresql"), None),
    ("questdb", "QuestDB", "sql", Some("postgresql"), None),
    ("cloudberry", "Cloudberry", "sql", Some("postgresql"), None),
    ("mysql", "MySQL", "sql", Some("mysql"), None),
    ("mariadb", "MariaDB", "sql", Some("mysql"), None),
    ("doris", "Doris", "sql", Some("mysql"), None),
    ("starrocks", "StarRocks", "sql", Some("mysql"), None),
    ("manticore", "ManticoreSearch", "sql", Some("mysql"), None),
    ("ob_oracle", "OceanBase (Oracle)", "sql", Some("mysql"), None),
    ("sqlite", "SQLite", "sql", Some("sqlite"), None),
    ("redis", "Redis", "kv", None, Some("redis")),
    ("mongodb", "MongoDB", "document", Some("mongodb"), None),
    ("sqlserver", "SQL Server", "sql", Some("sqlserver"), None),
    ("clickhouse", "ClickHouse", "sql", Some("clickhouse"), None),
    ("duckdb", "DuckDB", "sql", Some("duckdb"), None),
    ("elasticsearch", "Elasticsearch", "sql", Some("elasticsearch"), None),
    ("rqlite", "RQLite", "sql", Some("sqlite"), None),
    ("turso", "Turso / libSQL", "sql", Some("sqlite"), None),
    ("influxdb", "InfluxDB", "sql", Some("generic"), Some("influxdb")),
    ("victoriametrics", "VictoriaMetrics", "sql", Some("generic"), Some("victoriametrics")),
    ("hbase", "HBase", "sql", Some("generic"), Some("hbase")),
    ("vector", "Vector DB", "sql", Some("generic"), Some("vector")),
];

#[test]
fn driver_meta_table_matches_the_sources_row_for_row() {
    for (id, _label, category, sql_dialect, sync_family) in DRIVER_META {
        let expected_category = match *category {
            "sql" => SyncCategory::Sql,
            "kv" => SyncCategory::Kv,
            "document" => SyncCategory::Document,
            other => panic!("unexpected category {other}"),
        };
        assert_eq!(sync_category(id), expected_category, "category of {id}");
        let family = normalize_sync_family(id);
        // syncFamily wins; else a non-generic sqlDialect; else the id.
        let expected_family = sync_family
            .or(*sql_dialect)
            .filter(|dialect| *dialect != "generic")
            .unwrap_or(id)
            .to_owned();
        assert_eq!(family, expected_family, "family of {id}");
    }
    // The registry ids the table must cover (the path drivers of
    // drivers-registry.json's basic set + the wire-reuse metas).
    let table_ids: Vec<&str> = DRIVER_META.iter().map(|(id, ..)| *id).collect();
    for id in ["postgresql", "mysql", "sqlite", "redis", "mongodb", "sqlserver"] {
        assert!(table_ids.contains(&id), "{id} covered");
    }
}

#[test]
fn unknown_types_default_open_like_the_registry_lookup() {
    // registryMeta('nope') is undefined in React; categoryFromMeta then
    // defaults to 'sql' (syncPairing.ts:40) and the family falls to the
    // normalized id (syncPairing.ts:46-47).
    assert_eq!(sync_category("nope"), SyncCategory::Sql);
    assert_eq!(normalize_sync_family("nope"), "nope".to_owned());
    // Alias resolution happens before the family fallback.
    assert_eq!(normalize_sync_family("MSSQL"), "sqlserver".to_owned());
    // The proxy drivers are their own category (syncPairing.ts:38-39).
    assert_eq!(sync_category("superset"), SyncCategory::Other);
}

// ---------------------------------------------------------------------------
// The pairing branch matrix (lib/transferPairing.ts:19-64).
// ---------------------------------------------------------------------------

#[test]
fn pairing_matrix_covers_every_branch_of_the_lib() {
    let cases: &[(&str, &str, TransferPath, bool)] = &[
        // Same family, same type: direct.
        ("mysql", "mysql", TransferPath::Direct, true),
        // Same family via wire reuse: direct, family is the shared dialect.
        ("mariadb", "doris", TransferPath::Direct, true),
        // Cross-family SQL: IR.
        ("sqlite", "sqlserver", TransferPath::Ir, true),
        ("clickhouse", "duckdb", TransferPath::Ir, true),
        // Cross-category: unsupported.
        ("redis", "mongodb", TransferPath::Unsupported, false),
        ("mongodb", "mysql", TransferPath::Unsupported, false),
        // Redis adapters refuse even same-family redis.
        ("redis", "redis", TransferPath::Unsupported, false),
    ];
    for (source, target, path, supported) in cases {
        let result: TransferPairingResult = resolve_transfer_pairing(source, target);
        assert_eq!(result.path, *path, "path of {source}->{target}");
        assert_eq!(result.supported, *supported, "supported of {source}->{target}");
        if !*supported {
            assert!(result.reason.is_some(), "unsupported pairs name a reason");
        } else {
            assert!(result.reason.is_none(), "supported pairs carry no reason");
        }
    }
    // The category reason spells the categories (transferPairing.ts:30-33).
    let result = resolve_transfer_pairing("mongodb", "mysql");
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer between mongodb (document) and mysql (sql) is not supported")
    );
    // isTransferTargetSupported is the .supported accessor (line 66-68).
    assert!(is_transfer_target_supported("mysql", "mysql"));
    assert!(!is_transfer_target_supported("redis", "mongodb"));
    // The verified set is exactly postgresql + mysql, case-insensitive
    // (migrationVerification.ts:2-6).
    for (source, target, verified) in [
        ("mysql", "postgresql", true),
        ("PostgreSQL", "MySQL", true),
        ("sqlite", "mysql", false),
        ("postgresql", "postgresql", true),
        ("postgresql", "sqlite", false),
    ] {
        assert_eq!(
            is_verified_migration_pair(source, target),
            verified,
            "verified({source},{target})"
        );
    }
}

// ---------------------------------------------------------------------------
// The endpoint derivations' fail-open shapes (React 268-296).
// ---------------------------------------------------------------------------

#[test]
fn unknown_source_type_leaves_target_options_undecorated() {
    // React's DB_REGISTRY lookup misses for an unknown source; the
    // unsupported/experimental guards then short to false and every option
    // is plain (React 273-287).
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        connection("weird", "sometype"),
        connection("tgt", "mysql"),
    ]);
    vm.source_id = "weird".to_owned();
    let options: Vec<ConnectionOption> = vm.target_options();
    let tgt = options.iter().find(|o| o.value == "tgt").unwrap();
    assert!(!tgt.disabled);
    // React shows the experimental hint for an unverified-but-supported pair,
    // and an unknown source type is exactly that: pairing resolves fail-open
    // (sql -> sql IR) while the verified set is postgresql + mysql only.
    let experimental = datazen_ui::i18n::t("common.experimentalPairHint").to_owned();
    assert_eq!(tgt.title.as_deref(), Some(experimental.as_str()));
    assert_eq!(tgt.label, format!("name-tgt (mysql) — {experimental}"));
    // An unknown source still pairs fail-open (sql -> sql, cross-family IR)
    // so the endpoints step passes with both endpoints picked (React
    // 884-891: the guard reads pairing?.supported, not a known registry).
    vm.source_database = "db".to_owned();
    vm.target_database = "db".to_owned();
    vm.target_id = "tgt".to_owned();
    assert!(vm.can_next(), "unknown source type pairs open");
}

#[test]
fn proxy_source_category_other_shuts_the_endpoints_gate() {
    // kiwi is category 'other': pairing refuses, so canNext's endpoints arm
    // stays shut even with every field picked (React 890: pairing?.supported).
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![connection("kiwi", "kiwi")]);
    vm.source_id = "kiwi".to_owned();
    vm.target_id = "kiwi".to_owned();
    vm.source_database = "db".to_owned();
    vm.target_database = "db".to_owned();
    let pairing = vm.pairing().expect("pairing resolves");
    assert!(!pairing.supported);
    assert_eq!(vm.step, WizardStep::Endpoints);
    assert!(!vm.can_next());
}

// ---------------------------------------------------------------------------
// buildJob's file-token contract (React 595-657).
// ---------------------------------------------------------------------------

fn sql_file_vm() -> DataTransferWindowVm {
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        connection("src", "mysql"),
        connection("tgt", "mysql"),
    ]);
    vm.source_id = "src".to_owned();
    vm.target_id = "tgt".to_owned();
    vm.source_database = "sdb".to_owned();
    vm.target_database = "tdb".to_owned();
    vm.source_session = Some(session_vm("src"));
    vm.target_session = Some(session_vm("tgt"));
    vm
}

#[test]
fn build_job_keeps_the_picked_token_and_ignores_it_on_the_database_branch() {
    let mut vm = sql_file_vm();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.sql_file_target = Some(TransferSqlFileTargetVm {
        file_token: "opaque-token-42".to_owned(),
        database_type: Some("mysql".to_owned()),
        encoding: Some(SqlFileEncoding::Utf16Be),
        compression: Some(SqlFileCompression::Gzip),
        database: Some("stale".to_owned()),
        schema: Some("stale".to_owned()),
    });
    let job = vm
        .build_job(vm.source_session.as_ref(), vm.target_session.as_ref())
        .expect("the sql-file job builds");
    let target = job.sql_file_target.as_ref().unwrap();
    assert_eq!(target.file_token, "opaque-token-42");
    // The wizard's current config overrides the picked target's own
    // decoration fields (React 608-613 spreads).
    assert_eq!(target.database_type.as_deref(), None);
    assert_eq!(target.encoding, None);
    assert_eq!(target.compression, None);
    assert!(target.database.is_none());
    assert!(target.schema.is_none());

    // Back on the database branch a picked file is simply not part of the
    // job (React 622-634 has no sqlFileTarget field).
    vm.destination_mode = TransferDestinationMode::Database;
    let job = vm
        .build_job(vm.source_session.as_ref(), vm.target_session.as_ref())
        .expect("the database job builds");
    assert!(job.sql_file_target.is_none());
    assert_eq!(
        job.target.as_ref().unwrap().db_session_id,
        "s-tgt",
        "the refreshed target session id is used"
    );
}

// ---------------------------------------------------------------------------
// The inspect application's first-enabled fallback (React 700-705).
// ---------------------------------------------------------------------------

#[test]
fn inspect_picks_the_first_enabled_row_when_prev_is_gone() {
    let mut vm = DataTransferWindowVm::default();
    vm.selected_mapping_table = "gone".to_owned();
    let mut first = table_row("a", "a");
    first.vm.enabled = false;
    let mut second = table_row("b", "b");
    second.vm.enabled = true;
    vm.apply_inspect_rows(vec![first, second]);
    assert_eq!(vm.selected_mapping_table, "b");
    // No enabled row at all -> the first row (enabled[0], React 705).
    let mut first = table_row("a", "a");
    first.vm.enabled = false;
    let mut vm = DataTransferWindowVm::default();
    vm.apply_inspect_rows(vec![first]);
    assert_eq!(vm.selected_mapping_table, "a");
}

// ---------------------------------------------------------------------------
// The profile round-trip (React 453-558).
// ---------------------------------------------------------------------------

#[test]
fn sql_file_profile_round_trips_through_save_and_load() {
    let mut vm = sql_file_vm();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.profile_name = "SQL dump".to_owned();
    vm.sql_file_target = Some(TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..TransferSqlFileTargetVm::default()
    });
    vm.sql_file_dialect = "postgresql".to_owned();
    vm.sql_file_encoding = SqlFileEncoding::Utf8Bom;
    vm.sql_file_compression = SqlFileCompression::Gzip;
    vm.sql_file_database = "dumpdb".to_owned();
    vm.sql_file_schema = "dumpschema".to_owned();
    vm.mode = TransferMode::Structure;
    vm.write_mode = WriteMode::DropCreateInsert;
    vm.batch_size_edit = "128".to_owned();
    vm.stop_on_error = false;
    vm.confirmed_destructive = true;
    vm.use_target_default_collation = true;

    let payload = vm.profile_save_payload("2026-03-01T00:00:00.000Z", "id-1");
    // The save payload carries the sql-file decoration (React 478-483).
    assert_eq!(payload.destination_mode, TransferDestinationMode::SqlFile);
    assert_eq!(payload.sql_file_dialect.as_deref(), Some("postgresql"));
    assert_eq!(payload.sql_file_encoding, Some(SqlFileEncoding::Utf8Bom));
    assert_eq!(payload.sql_file_compression, Some(SqlFileCompression::Gzip));
    assert_eq!(payload.sql_file_database.as_deref(), Some("dumpdb"));
    assert_eq!(payload.sql_file_schema.as_deref(), Some("dumpschema"));
    // A sql-file profile has no target connection (React 472).
    assert!(payload.target_connection_id.is_none());
    assert!(payload.target_database.is_none());
    assert!(payload.target_schema.is_none());
    assert_eq!(payload.options.batch_size, Some(128));
    assert!(!payload.options.stop_on_error);
    assert!(payload.options.confirmed_destructive);
    assert!(payload.options.use_target_default_collation);

    // Load it back into a fresh window: every field re-applies (React
    // 526-547) and the load reports the choose-a-file error (React
    // 554-557).
    let mut fresh = DataTransferWindowVm::default();
    fresh.transfer_profiles = vec![payload.clone()];
    fresh.selected_profile_id = "id-1".to_owned();
    fresh.step = WizardStep::Objects;
    assert!(fresh.load_selected_profile());
    assert_eq!(fresh.destination_mode, TransferDestinationMode::SqlFile);
    assert_eq!(fresh.sql_file_dialect, "postgresql");
    assert_eq!(fresh.sql_file_encoding, SqlFileEncoding::Utf8Bom);
    assert_eq!(fresh.sql_file_compression, SqlFileCompression::Gzip);
    assert_eq!(fresh.sql_file_database, "dumpdb");
    assert_eq!(fresh.sql_file_schema, "dumpschema");
    assert_eq!(fresh.mode, TransferMode::Structure);
    assert_eq!(fresh.write_mode, WriteMode::DropCreateInsert);
    assert_eq!(fresh.batch_size(), Some(128));
    assert!(!fresh.stop_on_error);
    assert!(fresh.confirmed_destructive);
    assert!(fresh.use_target_default_collation);
    // The picked file does not ride the profile: the load clears it
    // (React 546) and asks for a new one.
    assert!(fresh.sql_file_target.is_none());
    assert_eq!(fresh.step, WizardStep::Endpoints);
}

// ---------------------------------------------------------------------------
// The execute progress count's max rule (React 794-798).
// ---------------------------------------------------------------------------

#[test]
fn execute_table_count_takes_the_plan_max_when_no_rows_were_inspected() {
    let mut vm = sql_file_vm();
    // A plan with 3 write plans and 2 DDL items: the max is 3.
    let mut preview = TransferPreviewVm {
        plan_id: "p".to_owned(),
        pairing_path: "direct".to_owned(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![
            TransferDdlPreviewVm {
                source_table: "a".to_owned(),
                target_table: "a".to_owned(),
                ddl: String::new(),
                kind: Some(DdlKind::Table),
                depends_on: None,
            };
            2
        ],
        write_plans: vec![TransferWritePlanVm {
            source_table: String::new(),
            target_table: String::new(),
            write_mode: WriteMode::Insert,
            mapped_columns: vec![],
            estimated_rows: None,
            preamble: vec![],
            source_filter_preview: None,
            recordset_preview: None,
        }],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let single = TransferWritePlanVm {
        source_table: String::new(),
        target_table: String::new(),
        write_mode: WriteMode::Insert,
        mapped_columns: vec![],
        estimated_rows: None,
        preamble: vec![],
        source_filter_preview: None,
        recordset_preview: None,
    };
    preview.write_plans = vec![single.clone(), single.clone(), single];
    vm.preview = Some(preview);
    vm.tables.clear();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    vm.sql_file_target = Some(TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..TransferSqlFileTargetVm::default()
    });
    let job = vm
        .build_job(vm.source_session.as_ref(), None)
        .expect("job builds");
    assert_eq!(vm.execute_table_count(&job), 3, "max(3 write plans, 2 ddl)");
    // With inspected rows the count is the job's enabled tables instead
    // (React 795-797). Rebuild the job so its table list reflects the rows.
    vm.tables = vec![table_row("t1", "t1"), table_row("t2", "t2")];
    vm.tables[0].vm.enabled = true;
    vm.tables[1].vm.enabled = false;
    let job = vm
        .build_job(vm.source_session.as_ref(), None)
        .expect("job builds");
    assert_eq!(vm.execute_table_count(&job), 1);
}

// ---------------------------------------------------------------------------
// The step rail's i18n keys (React 1135).
// ---------------------------------------------------------------------------

#[test]
fn every_step_label_key_the_rail_asks_for_exists() {
    for step in datazen_ui::views::STEPS {
        assert!(
            has(step.label_key()),
            "the rail labels step {:?} with {}",
            step,
            step.label_key()
        );
    }
    // The gate messages the window reports exist too (React 950, the
    // MappingGateNotice key).
    assert!(has("transfer.mapping.gateLost"));
    assert!(has("transfer.mapping.targetNameRequired"));
}

// ---------------------------------------------------------------------------
// The once-only prefill consume across both legs (React 227, 325-355).
// ---------------------------------------------------------------------------

#[test]
fn prefill_database_legs_fire_once_each() {
    let mut vm = sql_file_vm();
    vm.endpoint_prefill = MigrationWindowPrefill {
        source_database: Some("sdb".to_owned()),
        target_database: Some("tdb".to_owned()),
        ..MigrationWindowPrefill::default()
    };
    vm.apply_source_databases(Some(vec!["sdb".to_owned(), "other".to_owned()]));
    assert_eq!(vm.source_database, "sdb", "the source prefill fires");
    // A later source effect run cannot re-fire it (consume-on-use).
    vm.apply_source_databases(Some(vec!["other".to_owned()]));
    assert_eq!(vm.source_database, "other");
    // The target leg is independent and still armed.
    vm.apply_target_databases(Some(vec!["tdb".to_owned(), "other".to_owned()]));
    assert_eq!(vm.target_database, "tdb", "the target prefill fires");
}

// ---------------------------------------------------------------------------
// updateTable's spread rule for target_table and create_new (React 973-982).
// ---------------------------------------------------------------------------

#[test]
fn update_table_spreads_target_and_create_new() {
    let mut vm = sql_file_vm();
    vm.tables = vec![table_row("t", "old")];
    vm.update_table(
        "t",
        &TransferTablePatch {
            target_table: Some("new".to_owned()),
            create_new: Some(true),
            ..TransferTablePatch::default()
        },
    );
    assert_eq!(vm.tables[0].vm.target_table, "new");
    assert!(vm.tables[0].vm.create_new);
    // A patch for another table leaves this one alone.
    vm.update_table(
        "other",
        &TransferTablePatch {
            target_table: Some("nope".to_owned()),
            ..TransferTablePatch::default()
        },
    );
    assert_eq!(vm.tables[0].vm.target_table, "new");
}

// ---------------------------------------------------------------------------
// The mapping-gate placeholder's normalized view (transferMappingView.ts).
// ---------------------------------------------------------------------------

#[test]
fn gate_reads_the_normalized_mappings_not_the_stored_ones() {
    // tableHasActiveMappings normalizes first (transferMappingView.ts:58-60):
    // a stored mapping for a column that sourceColumns does not list is
    // not the view the gate sees; the synthesized rows for listed columns
    // are skipped unless a stored row unmasks them.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Mapping;
    let mut table = table_row("t", "t");
    table.vm.enabled = true;
    table.vm.source_columns = Some(vec!["listed".to_owned()]);
    // A stored mapping for an UNLISTED column only.
    table.vm.column_mappings = vec![TransferColumnMapping {
        source_column: "unlisted".to_owned(),
        target_column: "unlisted_target".to_owned(),
        skip: false,
        target_native_type: None,
    }];
    vm.tables = vec![table];
    assert!(!vm.can_next(), "an unlisted mapping does not clear the gate");
    // A stored mapping for the listed column does.
    vm.tables[0].vm.column_mappings = vec![TransferColumnMapping {
        source_column: "listed".to_owned(),
        target_column: "listed_target".to_owned(),
        skip: false,
        target_native_type: None,
    }];
    assert!(vm.can_next());
}
