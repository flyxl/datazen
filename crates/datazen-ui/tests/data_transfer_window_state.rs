//! T147-S1: the Data Transfer window's wizard state model — coder tests.
//!
//! Every assertion is checked against
//! `packages/frontend/src/windows/data-transfer/DataTransferWindow.tsx`
//! (and the lib sources the pairing placeholders transliterate); the
//! React line numbers are cited per section. Pure-logic tests drive the
//! Vm directly — the rendering slices have their own modules.

use datazen_ui::i18n::t;
use datazen_ui::views::{
    mapping_gate_allows_advance, mapping_gate_block_reason, normalize_driver_id,
    resolve_default_database, MigrationWindowPrefill, SyncCategory, TransferColumnMapping,
    TransferConnectionVm, TransferDdlPreviewVm, TransferDestinationMode, TransferMode,
    TransferPairingResult, TransferPath, TransferPreviewVm, TransferSqlFileTargetVm,
    TransferTablePatch, TransferTableRow, TransferTableVm, TransferWritePlanVm,
    DataTransferWindowVm, DdlKind, DedicatedSessionVm, NARROW_STEPS, SqlFileCompression,
    SqlFileEncoding, STEPS, WizardStep, WriteMode,
};

// ---------------------------------------------------------------------------
// Small builders.
// ---------------------------------------------------------------------------

fn conn(id: &str, name: &str, db_type: &str) -> TransferConnectionVm {
    TransferConnectionVm {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        ..TransferConnectionVm::default()
    }
}

fn row(source: &str, target: &str, enabled: bool) -> TransferTableRow {
    TransferTableRow {
        vm: TransferTableVm {
            source_table: source.to_owned(),
            target_table: target.to_owned(),
            enabled,
            ..TransferTableVm::default()
        },
        ..TransferTableRow::default()
    }
}

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

fn session(id: &str, db: &str) -> DedicatedSessionVm {
    DedicatedSessionVm {
        connection_id: format!("conn-{id}"),
        database: db.to_owned(),
        db_session_id: format!("session-{id}"),
    }
}

fn preview(can_execute: bool) -> TransferPreviewVm {
    TransferPreviewVm {
        plan_id: "plan-1".to_owned(),
        pairing_path: "direct".to_owned(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![TransferDdlPreviewVm {
            source_table: "t".to_owned(),
            target_table: "t".to_owned(),
            ddl: "CREATE TABLE t".to_owned(),
            kind: Some(DdlKind::Table),
            depends_on: None,
        }],
        write_plans: vec![TransferWritePlanVm {
            source_table: "t".to_owned(),
            target_table: "t".to_owned(),
            write_mode: WriteMode::Insert,
            mapped_columns: vec![],
            estimated_rows: Some(10),
            preamble: vec![],
            source_filter_preview: None,
            recordset_preview: None,
        }],
        warnings: vec![],
        can_execute,
        block_reason: None,
    }
}

// ---------------------------------------------------------------------------
// The steps (React 74-78) and the defaults (React 86-155).
// ---------------------------------------------------------------------------

#[test]
fn steps_order_matches_react() {
    // React 76: ['endpoints', 'setup', 'objects', 'mapping', 'preview', 'result'].
    assert_eq!(
        STEPS,
        [
            WizardStep::Endpoints,
            WizardStep::Setup,
            WizardStep::Objects,
            WizardStep::Mapping,
            WizardStep::Preview,
            WizardStep::Result,
        ]
    );
    // React 78: NARROW_STEPS = ['endpoints', 'setup', 'result'].
    assert_eq!(
        NARROW_STEPS,
        [
            WizardStep::Endpoints,
            WizardStep::Setup,
            WizardStep::Result,
        ]
    );
    for (index, step) in STEPS.iter().enumerate() {
        assert_eq!(step.index(), index, "STEPS.indexOf({step:?})");
    }
    // React 1135: `` t(`transfer.step.${s}`) `` — the label keys spell the
    // React step ids.
    assert_eq!(WizardStep::Endpoints.label_key(), "transfer.step.endpoints");
    assert_eq!(WizardStep::Setup.label_key(), "transfer.step.setup");
    assert_eq!(WizardStep::Objects.label_key(), "transfer.step.objects");
    assert_eq!(WizardStep::Mapping.label_key(), "transfer.step.mapping");
    assert_eq!(WizardStep::Preview.label_key(), "transfer.step.preview");
    assert_eq!(WizardStep::Result.label_key(), "transfer.step.result");
}

#[test]
fn initial_state_matches_react_usestate_defaults() {
    let vm = DataTransferWindowVm::default();
    // React 126: `useState<WizardStep>('endpoints')`.
    assert_eq!(vm.step, WizardStep::Endpoints);
    // React 122-123: `'data'` / `'insert'`.
    assert_eq!(vm.mode, TransferMode::Data);
    assert_eq!(vm.write_mode, WriteMode::Insert);
    // React 127: `DEFAULT_TRANSFER_OPTIONS.batchSize ?? 500` = 500.
    assert_eq!(vm.batch_size(), Some(500));
    // React 128: `useState(true)`.
    assert!(vm.stop_on_error);
    // React 112-118: destination / dialect / encoding / compression.
    assert_eq!(vm.destination_mode, TransferDestinationMode::Database);
    assert_eq!(vm.sql_file_dialect, "source");
    assert_eq!(vm.sql_file_encoding, SqlFileEncoding::Utf8);
    assert_eq!(vm.sql_file_compression, SqlFileCompression::None);
    // React 129-130: `useState(false)` twice.
    assert!(!vm.confirmed_destructive);
    assert!(!vm.use_target_default_collation);
    // React 131-148: the busy / dialog surfaces start closed and empty.
    assert!(!vm.loading);
    assert!(!vm.preparing);
    assert!(!vm.executing);
    assert!(!vm.error_open);
    assert!(!vm.limitations_open);
    assert!(!vm.execute_confirm_open);
    assert!(vm.error_msg.is_empty());
    assert!(vm.preview_error.is_empty());
    assert!(vm.mapping_gate_error.is_empty());
    assert!(vm.tables.is_empty());
    assert!(vm.preview.is_none());
    assert!(vm.sql_file_target.is_none());
    assert!(vm.profile_mappings.is_none());
    assert_eq!(vm.prepare_seq, 0, "prepareSeqRef starts at 0 (React 154)");
}

// ---------------------------------------------------------------------------
// The endpoint derivations (React 250-296).
// ---------------------------------------------------------------------------

#[test]
fn conn_options_are_name_and_type() {
    // React 259-266: `${c.name} (${c.databaseType})`.
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        conn("a", "Alpha", "mysql"),
        conn("b", "Beta", "postgresql"),
    ]);
    let options = vm.conn_options();
    assert_eq!(options[0].value, "a");
    assert_eq!(options[0].label, "Alpha (mysql)");
    assert_eq!(options[1].label, "Beta (postgresql)");
    assert!(!options[0].disabled);
    assert!(options[0].title.is_none());
}

#[test]
fn target_options_flag_unsupported_and_experimental_pairs() {
    // React 268-289: an unsupported pair is hinted and disabled; an
    // unverified-but-supported pair carries the experimental hint; a
    // verified pair (mysql -> postgresql, lib/migrationVerification.ts)
    // carries neither.
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        conn("src", "Source", "mysql"),
        conn("pg", "PG", "postgresql"),
        conn("redis", "RedisTgt", "redis"),
        conn("mysql2", "MyTgt", "mysql"),
        conn("sqlite", "LiteTgt", "sqlite"),
    ]);
    vm.source_id = "src".to_owned();
    let hint = t("common.unsupportedPair").to_owned();
    let experimental = t("common.experimentalPairHint").to_owned();
    let options = vm.target_options();
    // mysql -> redis: cross-category, unsupported (disabled, hinted).
    let redis = &options[2];
    assert!(redis.disabled);
    assert_eq!(redis.label, format!("RedisTgt (redis) — {hint}"));
    assert_eq!(redis.title.as_deref(), Some(hint.as_str()));
    // mysql -> postgresql: supported via IR and verified — plain.
    let pg = &options[1];
    assert!(!pg.disabled);
    assert_eq!(pg.label, "PG (postgresql)");
    assert!(pg.title.is_none());
    // mysql -> sqlite: supported cross-family SQL, unverified ->
    // experimental hint (the verified set is postgresql + mysql only,
    // lib/migrationVerification.ts:1-6).
    let lite = &options[4];
    assert!(!lite.disabled);
    assert_eq!(lite.label, format!("LiteTgt (sqlite) — {experimental}"));
    assert_eq!(lite.title.as_deref(), Some(experimental.as_str()));
    // mysql -> mysql: same family, direct — no hints at all.
    let mine = &options[3];
    assert!(!mine.disabled);
    assert_eq!(mine.label, "MyTgt (mysql)");
    assert!(mine.title.is_none());
}

#[test]
fn target_options_without_a_source_carry_no_hints() {
    // React 271: `sourceConn?.databaseType` — no source selected means
    // no pair can be evaluated, so every option is plain.
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![conn("a", "Alpha", "redis")]);
    let options = vm.target_options();
    assert!(!options[0].disabled);
    assert_eq!(options[0].label, "Alpha (redis)");
    assert!(options[0].title.is_none());
}

#[test]
fn pairing_follows_transfer_pairing_lib() {
    // React 291-294 + lib/transferPairing.ts.
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        conn("mysql", "M", "mysql"),
        conn("pg", "P", "postgresql"),
        conn("maria", "Maria", "mariadb"),
        conn("redis", "R", "redis"),
    ]);
    vm.source_id = "mysql".to_owned();
    vm.target_id = "maria".to_owned();
    let pairing = vm.pairing().expect("pairing resolves");
    assert!(pairing.supported);
    assert_eq!(pairing.path, TransferPath::Direct);
    assert_eq!(pairing.family.as_deref(), Some("mysql"));
    // Cross-category (mysql -> redis) is unsupported with a reason.
    vm.target_id = "redis".to_owned();
    let pairing = vm.pairing().expect("pairing resolves");
    assert!(!pairing.supported);
    assert!(pairing.reason.is_some());
    // No pairing without both endpoints (React 292-293).
    vm.target_id.clear();
    assert!(vm.pairing().is_none());
}

#[test]
fn target_read_only_reads_the_target_connection() {
    // React 296.
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![conn("a", "A", "mysql")]);
    assert!(!vm.target_read_only());
    vm.target_id = "a".to_owned();
    assert!(!vm.target_read_only());
    vm.connections[0].read_only = true;
    assert!(vm.target_read_only());
}

// ---------------------------------------------------------------------------
// The pairing placeholder itself (lib/transferPairing.ts,
// lib/syncTaxonomy.ts, lib/migrationVerification.ts).
// ---------------------------------------------------------------------------

#[test]
fn normalize_driver_id_mirrors_the_aliases() {
    // lib/syncTaxonomy.ts:5-18.
    assert_eq!(normalize_driver_id("Postgres"), "postgresql");
    assert_eq!(normalize_driver_id("MSSQL"), "sqlserver");
    assert_eq!(normalize_driver_id("Presto"), "trino");
    assert_eq!(normalize_driver_id("TiDB"), "mysql");
    assert_eq!(normalize_driver_id("OceanBase"), "mysql");
    assert_eq!(normalize_driver_id("MySQL"), "mysql");
}

#[test]
fn resolve_transfer_pairing_covers_the_branches() {
    // lib/transferPairing.ts:19-64, branch for branch.
    // Same family -> direct with the family name.
    let result = resolve_pairing("mysql", "mariadb");
    assert_eq!(result.path, TransferPath::Direct);
    assert_eq!(result.family.as_deref(), Some("mysql"));
    // Cross-family SQL -> IR.
    let result = resolve_pairing("mysql", "postgresql");
    assert_eq!(result.path, TransferPath::Ir);
    assert!(result.supported);
    // Cross-category -> unsupported with the category reason.
    let result = resolve_pairing("mysql", "redis");
    assert!(!result.supported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer between mysql (sql) and redis (kv) is not supported")
    );
    // Redis on either side has no adapters (lib/transferPairing.ts:46-53),
    // even though redis -> redis is same-family.
    let result = resolve_pairing("redis", "redis");
    assert!(!result.supported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Data Transfer has no Redis source and target adapters")
    );
    // mongodb -> mongodb: same category (document) and family — the
    // doc-store pair is supported-direct per the lib.
    let result = resolve_pairing("mongodb", "mongodb");
    assert_eq!(result.path, TransferPath::Direct);
    // An unknown type falls to the sql category and its normalized id
    // as family: unknown == unknown is direct (the fail-open default).
    let result = resolve_pairing("unknowndb", "unknowndb");
    assert_eq!(result.path, TransferPath::Direct);
    // kiwi / superset are proxy drivers: category 'other' refuses
    // (categoryFromMeta, syncPairing.ts:38-39).
    let result = resolve_pairing("kiwi", "kiwi");
    assert!(!result.supported);
    assert_eq!(
        result.reason.as_deref(),
        Some("Transfer is not supported for database type 'kiwi'")
    );
}

fn resolve_pairing(source: &str, target: &str) -> TransferPairingResult {
    datazen_ui::views::resolve_transfer_pairing(source, target)
}

#[test]
fn is_verified_migration_pair_is_pg_and_mysql() {
    // lib/migrationVerification.ts:1-6.
    assert!(datazen_ui::views::is_verified_migration_pair("postgresql", "mysql"));
    assert!(datazen_ui::views::is_verified_migration_pair("MySQL", "PostgreSQL"));
    assert!(!datazen_ui::views::is_verified_migration_pair("postgresql", "sqlite"));
    assert!(!datazen_ui::views::is_verified_migration_pair("sqlite", "sqlite"));
}

#[test]
fn resolve_default_database_follows_the_lib_order() {
    // lib/migrationWindowPrefill.ts:47-54.
    let list = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    // preferred wins when present.
    assert_eq!(
        resolve_default_database(Some(&list), "b", "c"),
        "b".to_owned()
    );
    // preferred absent, prev present -> prev.
    assert_eq!(
        resolve_default_database(Some(&list), "x", "c"),
        "c".to_owned()
    );
    // Neither -> the first row.
    assert_eq!(
        resolve_default_database(Some(&list), "x", ""),
        "a".to_owned()
    );
    // No databases -> ''.
    assert_eq!(resolve_default_database(None, "b", "c"), "");
    // Empty list -> ''.
    assert_eq!(resolve_default_database(Some(&[]), "b", "c"), "");
}

#[test]
fn pick_prefill_database_consumes_on_use() {
    // lib/migrationWindowPrefill.ts:56-69 — a prefill that fires is
    // cleared, so it can never fire twice.
    let mut prefill = MigrationWindowPrefill {
        source_database: Some("b".to_owned()),
        ..MigrationWindowPrefill::default()
    };
    let list = vec!["a".to_owned(), "b".to_owned()];
    let picked = datazen_ui::views::pick_prefill_database(
        &mut prefill,
        true,
        &list,
        |current| resolve_default_database(Some(&list), "a", current),
        "a",
    );
    assert_eq!(picked, "b");
    assert!(prefill.source_database.is_none(), "consumed on use");
    // The second call falls back (React's ref no longer holds the value).
    let picked = datazen_ui::views::pick_prefill_database(
        &mut prefill,
        true,
        &list,
        |current| resolve_default_database(Some(&list), "a", current),
        "a",
    );
    assert_eq!(picked, "a");
    // A prefill that is NOT in the list does not fire and is kept.
    let mut prefill = MigrationWindowPrefill {
        source_database: Some("z".to_owned()),
        ..MigrationWindowPrefill::default()
    };
    let picked = datazen_ui::views::pick_prefill_database(
        &mut prefill,
        true,
        &list,
        |current| resolve_default_database(Some(&list), "a", current),
        "a",
    );
    assert_eq!(picked, "a");
    assert_eq!(prefill.source_database.as_deref(), Some("z"));
}

// ---------------------------------------------------------------------------
// The step gates (React 879-925).
// ---------------------------------------------------------------------------

fn endpoints_ready_vm() -> DataTransferWindowVm {
    let mut vm = DataTransferWindowVm::default();
    vm.apply_connections_loaded(vec![
        conn("src", "S", "mysql"),
        conn("tgt", "T", "mysql"),
    ]);
    vm.source_id = "src".to_owned();
    vm.target_id = "tgt".to_owned();
    vm.source_database = "db1".to_owned();
    vm.target_database = "db2".to_owned();
    vm
}

#[test]
fn can_next_endpoints_requires_both_endpoints_and_a_supported_pair() {
    // React 884-891.
    let vm = endpoints_ready_vm();
    assert!(vm.can_next());
    // A read-only target does not block the endpoints step (that is
    // canExecute's rule, React 919-925).
    // Missing target database blocks.
    let mut vm = endpoints_ready_vm();
    vm.target_database.clear();
    assert!(!vm.can_next());
    // An unsupported pair blocks (pairing?.supported, React 890).
    let mut vm = endpoints_ready_vm();
    vm.connections[1].database_type = "redis".to_owned();
    assert!(!vm.can_next());
    // The sql-file branch needs a picked file instead (React 888-890).
    let mut vm = endpoints_ready_vm();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(!vm.can_next());
    vm.choose_sql_file(Some(TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        ..TransferSqlFileTargetVm::default()
    }));
    assert!(vm.can_next());
}

#[test]
fn can_next_setup_checks_batch_and_destructive_confirm() {
    // React 892-895.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Setup;
    assert!(vm.can_next());
    // An invalid batch size blocks (React 893).
    vm.batch_size_edit = "0".to_owned();
    assert!(!vm.can_next());
    vm.batch_size_edit = "501".to_owned();
    assert!(!vm.can_next());
    vm.batch_size_edit = "".to_owned();
    assert!(!vm.can_next());
    vm.batch_size_edit = "500".to_owned();
    assert!(vm.can_next());
    // A destructive write mode needs the confirm (React 894).
    vm.write_mode = WriteMode::TruncateInsert;
    assert!(!vm.can_next());
    vm.confirmed_destructive = true;
    assert!(vm.can_next());
}

#[test]
fn can_next_objects_needs_an_enabled_table() {
    // React 896-897.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Objects;
    assert!(!vm.can_next());
    vm.tables = vec![row("t1", "t1", false)];
    assert!(!vm.can_next());
    vm.tables.push(row("t2", "t2", true));
    assert!(vm.can_next());
}

#[test]
fn can_next_mapping_is_the_mapping_gate() {
    // React 898-900: the same predicate goNext re-checks (React 949).
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Mapping;
    assert!(!vm.can_next(), "no rows: gate shut");
    // An enabled, named, mapped row clears it.
    let mut table = row("t1", "t1", true);
    table.vm.source_columns = Some(vec!["c1".to_owned()]);
    table.vm.column_mappings = vec![mapping("c1", "c1", false)];
    vm.tables = vec![table];
    assert!(vm.can_next());
    // A disabled row cannot carry the gate (rowClearsGate, React 69-71).
    vm.tables[0].vm.enabled = false;
    assert!(!vm.can_next());
    // A create-new row behind whitespace is the refused shape (React 72).
    let mut table = row("t1", "  ", true);
    table.vm.create_new = true;
    table.vm.source_columns = Some(vec!["c1".to_owned()]);
    table.vm.column_mappings = vec![mapping("c1", "c1", false)];
    vm.tables = vec![table];
    assert!(!vm.can_next());
}

#[test]
fn can_next_is_false_on_preview_and_result() {
    // React 901-902: `default: return false`.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Preview;
    assert!(!vm.can_next());
    vm.step = WizardStep::Result;
    assert!(!vm.can_next());
}

#[test]
fn can_execute_reads_the_review_and_the_read_only_target() {
    // React 919-925: `preview?.canExecute === true && (sqlFile ||
    // !targetReadOnly) && !loading`.
    let mut vm = endpoints_ready_vm();
    assert!(!vm.can_execute(), "no preview yet");
    vm.preview = Some(preview(true));
    assert!(vm.can_execute());
    // A read-only target blocks (unless the destination is a SQL file).
    vm.connections[1].read_only = true;
    assert!(!vm.can_execute());
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(vm.can_execute());
    vm.destination_mode = TransferDestinationMode::Database;
    // A blocklisted review cannot execute.
    vm.preview = Some(preview(false));
    assert!(!vm.can_execute());
    vm.preview = Some(preview(true));
    // Loading blocks.
    vm.loading = true;
    assert!(!vm.can_execute());
}

// ---------------------------------------------------------------------------
// The mapping payload (React 431-451) and buildJob (React 595-657).
// ---------------------------------------------------------------------------

#[test]
fn all_tables_to_mappings_normalizes_and_trims() {
    // React 431-446.
    let mut vm = DataTransferWindowVm::default();
    let mut table = row("t1", "tgt1", true);
    table.vm.source_columns = Some(vec!["a".to_owned(), "b".to_owned()]);
    // Only 'a' has a stored mapping; 'b' is synthesised skipped.
    table.vm.column_mappings = vec![mapping("a", "a2", false)];
    table.ddl_override = Some("  \n".to_owned());
    let empty = row("", "x", true); // dropped: no sourceTable (React 434).
    vm.tables = vec![table, empty];
    let mappings = vm.all_tables_to_mappings();
    assert_eq!(mappings.len(), 1);
    let table = &mappings[0];
    assert_eq!(table.source_table, "t1");
    assert_eq!(table.target_table, "tgt1");
    assert_eq!(table.create_new, Some(false));
    assert_eq!(table.enabled, Some(true));
    // normalizeColumnMappings ran (React 440).
    assert_eq!(table.column_mappings.as_ref().unwrap().len(), 2);
    assert_eq!(table.column_mappings.as_ref().unwrap()[1].skip, true);
    // `ddlOverride?.trim() ? ddlOverride.trim() : undefined` (React 441).
    assert!(table.ddl_override.is_none());
}

#[test]
fn tables_to_mappings_keeps_the_enabled_subset() {
    // React 448-451: `filter((table) => table.enabled !== false)`.
    let mut vm = DataTransferWindowVm::default();
    vm.tables = vec![row("t1", "t1", true), row("t2", "t2", false)];
    assert_eq!(vm.tables_to_mappings().len(), 1);
    assert_eq!(vm.tables_to_mappings()[0].source_table, "t1");
}

#[test]
fn build_job_database_mode_needs_both_sessions() {
    // React 600-602, 621.
    let mut vm = endpoints_ready_vm();
    assert!(vm.build_job(None, None).is_none());
    let src = session("src", "db1");
    assert!(vm.build_job(Some(&src), None).is_none(), "no target session");
    let tgt = session("tgt", "db2");
    let job = vm.build_job(Some(&src), Some(&tgt)).expect("job builds");
    assert_eq!(job.source.db_session_id, "session-src");
    assert_eq!(job.source.database, "db1");
    assert_eq!(job.target.as_ref().unwrap().db_session_id, "session-tgt");
    assert_eq!(job.target.as_ref().unwrap().database, "db2");
    assert!(job.sql_file_target.is_none());
    assert_eq!(job.mode, TransferMode::Data);
    assert_eq!(job.write_mode, WriteMode::Insert);
    assert_eq!(job.options.batch_size, Some(500));
    assert!(job.options.stop_on_error);
    assert!(!job.options.confirmed_destructive);
}

#[test]
fn build_job_sql_file_mode_spreads_the_destination_config() {
    // React 603-619: the picked token is spread and the wizard's config
    // overrides; omitted-means-default fields are dropped exactly as the
    // React spreads do.
    let mut vm = endpoints_ready_vm();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(vm.build_job(Some(&session("src", "db1")), None).is_none(), "no file");
    vm.sql_file_target = Some(TransferSqlFileTargetVm {
        file_token: "tok".to_owned(),
        database_type: Some("stale".to_owned()),
        ..TransferSqlFileTargetVm::default()
    });
    let job = vm
        .build_job(Some(&session("src", "db1")), None)
        .expect("sql-file job builds");
    let target = job.sql_file_target.as_ref().unwrap();
    assert_eq!(target.file_token, "tok");
    // `databaseType: sqlFileDialect === 'source' ? undefined : dialect`.
    assert_eq!(target.database_type.as_deref(), None);
    // `encoding: encoding === 'utf8' ? undefined : encoding`.
    assert_eq!(target.encoding, None);
    // `compression: compression === 'none' ? undefined : compression`.
    assert_eq!(target.compression, None);
    // The empty database/schema strings are omitted (React 612-613).
    assert!(target.database.is_none());
    assert!(target.schema.is_none());
    // The target endpoint is absent on the SQL-file branch (React 605-619).
    assert!(job.target.is_none());
    // Changing the config carries through.
    vm.sql_file_dialect = "postgresql".to_owned();
    vm.sql_file_encoding = SqlFileEncoding::Utf16Le;
    vm.sql_file_compression = SqlFileCompression::Gzip;
    vm.sql_file_database = "  db  ".to_owned();
    vm.sql_file_schema = "sch".to_owned();
    let job = vm
        .build_job(Some(&session("src", "db1")), None)
        .expect("sql-file job builds");
    let target = job.sql_file_target.as_ref().unwrap();
    assert_eq!(target.database_type.as_deref(), Some("postgresql"));
    assert_eq!(target.encoding, Some(SqlFileEncoding::Utf16Le));
    assert_eq!(target.compression, Some(SqlFileCompression::Gzip));
    // The database/schema are trimmed on the wire (React 612-613 spreads
    // `sqlFileDatabase.trim()`).
    assert_eq!(target.database.as_deref(), Some("db"));
    assert_eq!(target.schema.as_deref(), Some("sch"));
}

#[test]
fn refresh_sessions_guard_matches_react() {
    // React 566-571.
    let mut vm = DataTransferWindowVm::default();
    assert!(!vm.refresh_sessions_guard());
    let mut vm = endpoints_ready_vm();
    assert!(vm.refresh_sessions_guard());
    vm.target_database.clear();
    assert!(!vm.refresh_sessions_guard());
    // The sql-file branch only needs the source (React 570-580).
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(vm.refresh_sessions_guard());
}

// ---------------------------------------------------------------------------
// The idempotency keys (React 155-179).
// ---------------------------------------------------------------------------

#[test]
fn prepare_keys_are_per_call_and_plan_scoped() {
    // React 175-178: seq pre-increments; 179: apply keys are plan-scoped.
    let mut vm = DataTransferWindowVm::default();
    vm.prepare_scope = "scope-1".to_owned();
    assert_eq!(vm.next_prepare_key(), "data-transfer/prepare/scope-1/1");
    assert_eq!(vm.next_prepare_key(), "data-transfer/prepare/scope-1/2");
    assert_eq!(vm.prepare_seq, 2);
    assert_eq!(
        DataTransferWindowVm::apply_key_for_plan("plan-9"),
        "data-transfer/apply/plan-9"
    );
}

// ---------------------------------------------------------------------------
// The host-fed applications (React 202-429).
// ---------------------------------------------------------------------------

#[test]
fn connections_prefill_applies_once() {
    // React 227 + useMigrationEndpointPrefill (migrationWindowPrefill.ts:95-113).
    let mut vm = DataTransferWindowVm::default();
    vm.endpoint_prefill = MigrationWindowPrefill {
        source_id: Some("src".to_owned()),
        target_id: Some("missing".to_owned()),
        ..MigrationWindowPrefill::default()
    };
    // No connections yet: nothing applies (React 98: connections.length === 0).
    assert!(!vm.apply_connections_loaded(Vec::new()));
    assert!(vm.source_id.is_empty());
    // The first real load applies the known source but not the unknown
    // target (React 105-108), and marks the effect applied.
    assert!(vm.apply_connections_loaded(vec![
        conn("src", "S", "mysql"),
        conn("tgt", "T", "mysql"),
    ]));
    assert_eq!(vm.source_id, "src");
    assert!(vm.target_id.is_empty(), "unknown target id is not applied");
    // A second load never re-applies (appliedRef, React 97).
    vm.endpoint_prefill.target_id = Some("tgt".to_owned());
    assert!(!vm.apply_connections_loaded(vec![conn("tgt", "T", "mysql")]));
    assert!(vm.target_id.is_empty());
    // A prefill with no ids at all does not arm the effect (React 99-100).
    let mut vm = DataTransferWindowVm::default();
    vm.endpoint_prefill = MigrationWindowPrefill::default();
    assert!(!vm.apply_connections_loaded(vec![conn("a", "A", "mysql")]));
    assert!(!vm.endpoint_prefill_applied);
}

#[test]
fn source_databases_effect_resolves_the_default() {
    // React 325-355.
    let mut vm = endpoints_ready_vm();
    vm.connections[0].database = Some("preferred".to_owned());
    vm.apply_source_databases(Some(vec![
        "preferred".to_owned(),
        "other".to_owned(),
    ]));
    assert_eq!(vm.source_databases, vec!["preferred".to_owned(), "other".to_owned()]);
    // The preferred database wins (React 338-346).
    assert_eq!(vm.source_database, "preferred");
    // Without a preferred match, the previous selection survives if
    // present, else the first row (resolveDefaultDatabase).
    vm.apply_source_databases(Some(vec!["other".to_owned()]));
    assert_eq!(vm.source_database, "other");
    vm.source_database = "gone".to_owned();
    vm.apply_source_databases(Some(vec!["other".to_owned()]));
    assert_eq!(vm.source_database, "other", "prev no longer present -> first");
    // The catch path clears the list (React 348-350).
    vm.apply_source_databases(None);
    assert!(vm.source_databases.is_empty());
}

#[test]
fn source_databases_prefill_leg_fires_once() {
    // React 340-347 via pickPrefillDatabase.
    let mut vm = endpoints_ready_vm();
    vm.endpoint_prefill = MigrationWindowPrefill {
        source_database: Some("b".to_owned()),
        ..MigrationWindowPrefill::default()
    };
    vm.apply_source_databases(Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(vm.source_database, "b");
    // Consumed: a second effect run cannot re-fire it.
    vm.apply_source_databases(Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(vm.source_database, "b");
}

#[test]
fn target_databases_effect_resolves_the_default() {
    // React 378-408 — the target twin.
    let mut vm = endpoints_ready_vm();
    vm.connections[1].database = Some("tpref".to_owned());
    vm.apply_target_databases(Some(vec!["x".to_owned(), "tpref".to_owned()]));
    assert_eq!(vm.target_database, "tpref");
    vm.apply_target_databases(None);
    assert!(vm.target_databases.is_empty());
    vm.target_database = "kept".to_owned();
    vm.apply_target_databases(Some(vec!["kept".to_owned()]));
    assert_eq!(vm.target_database, "kept");
}

#[test]
fn available_sql_dialects_keeps_sql_category_drivers() {
    // React 212-225: `supportsSQL === true && category === 'sql'`.
    let mut vm = DataTransferWindowVm::default();
    vm.set_available_sql_dialects(vec![
        "mysql".to_owned(),
        "redis".to_owned(),
        "postgresql".to_owned(),
        "mongodb".to_owned(),
    ]);
    assert_eq!(vm.available_sql_dialects, vec!["mysql", "postgresql"]);
}

#[test]
fn connection_closed_clears_only_the_matching_sessions() {
    // React 230-239.
    let mut vm = endpoints_ready_vm();
    vm.source_session = Some(session("src", "db1"));
    vm.target_session = Some(session("tgt", "db2"));
    vm.on_connection_closed("session-src");
    assert!(vm.source_session.is_none());
    assert!(vm.target_session.is_some());
    vm.on_connection_closed("session-tgt");
    assert!(vm.target_session.is_none());
    // A non-matching id clears nothing.
    vm.source_session = Some(session("src", "db1"));
    vm.on_connection_closed("session-other");
    assert!(vm.source_session.is_some());
    // An empty id is a no-op (the bus payload guard, React 233).
    vm.on_connection_closed("");
    assert!(vm.source_session.is_some());
}

// ---------------------------------------------------------------------------
// The SQL-file config edits (React 1288-1396).
// ---------------------------------------------------------------------------

#[test]
fn sql_file_dialect_change_drops_the_inspected_tables() {
    // React 1288-1294: overrides are dialect-specific, so a fresh
    // source-only inspection is forced.
    let mut vm = endpoints_ready_vm();
    vm.preview = Some(preview(true));
    vm.tables = vec![row("t1", "t1", true)];
    vm.set_sql_file_dialect("postgresql".to_owned());
    assert_eq!(vm.sql_file_dialect, "postgresql");
    assert!(vm.preview.is_none());
    assert!(vm.tables.is_empty());
}

#[test]
fn sql_file_encoding_and_compression_drop_the_preview() {
    // React 1327-1330 / 1350-1353.
    let mut vm = endpoints_ready_vm();
    vm.preview = Some(preview(true));
    vm.set_sql_file_encoding(SqlFileEncoding::Utf16Be);
    assert_eq!(vm.sql_file_encoding, SqlFileEncoding::Utf16Be);
    assert!(vm.preview.is_none());
    vm.preview = Some(preview(true));
    vm.set_sql_file_compression(SqlFileCompression::Gzip);
    assert_eq!(vm.sql_file_compression, SqlFileCompression::Gzip);
    assert!(vm.preview.is_none());
    vm.preview = Some(preview(true));
    vm.set_sql_file_database("db".to_owned());
    assert_eq!(vm.sql_file_database, "db");
    assert!(vm.preview.is_none());
    vm.preview = Some(preview(true));
    vm.set_sql_file_schema("sch".to_owned());
    assert_eq!(vm.sql_file_schema, "sch");
    assert!(vm.preview.is_none());
}

// ---------------------------------------------------------------------------
// Profiles (React 150-153, 453-558).
// ---------------------------------------------------------------------------

fn profile(id: &str, name: &str) -> datazen_ui::views::TransferProfileVm {
    datazen_ui::views::TransferProfileVm {
        version: 1,
        id: id.to_owned(),
        name: name.to_owned(),
        source_connection_id: "src".to_owned(),
        target_connection_id: Some("tgt".to_owned()),
        source_database: Some("sdb".to_owned()),
        target_database: Some("tdb".to_owned()),
        source_schema: None,
        target_schema: None,
        destination_mode: TransferDestinationMode::Database,
        sql_file_dialect: None,
        sql_file_encoding: None,
        sql_file_compression: None,
        sql_file_database: None,
        sql_file_schema: None,
        mode: TransferMode::StructureAndData,
        write_mode: WriteMode::TruncateInsert,
        tables: vec![],
        options: datazen_ui::views::DEFAULT_TRANSFER_OPTIONS,
        created_at: "2026-01-01T00:00:00.000Z".to_owned(),
        updated_at: "2026-01-02T00:00:00.000Z".to_owned(),
    }
}

#[test]
fn load_selected_profile_applies_every_field() {
    // React 526-558.
    let mut vm = endpoints_ready_vm();
    vm.step = WizardStep::Preview;
    vm.tables = vec![row("t1", "t1", true)];
    vm.preview = Some(preview(true));
    vm.batch_size_edit = "10".to_owned();
    let mut saved = profile("p1", "My Profile");
    saved.options.stop_on_error = false;
    vm.transfer_profiles = vec![saved];
    vm.selected_profile_id = "p1".to_owned();
    let choose_file = vm.load_selected_profile();
    assert!(!choose_file, "database profile does not ask for a file");
    assert_eq!(vm.profile_name, "My Profile");
    assert_eq!(vm.source_id, "src");
    assert_eq!(vm.target_id, "tgt");
    assert_eq!(vm.source_database, "sdb");
    assert_eq!(vm.target_database, "tdb");
    assert_eq!(vm.mode, TransferMode::StructureAndData);
    assert_eq!(vm.write_mode, WriteMode::TruncateInsert);
    // `batchSize ?? DEFAULT (500)` (React 536).
    assert_eq!(vm.batch_size(), Some(500));
    // The profile's tables are stashed for the first inspect (React 547).
    assert!(vm.profile_mappings.is_some());
    // The tables and preview are dropped (React 548-549).
    assert!(vm.tables.is_empty());
    assert!(vm.preview.is_none());
    // The step returns to endpoints (React 553).
    assert_eq!(vm.step, WizardStep::Endpoints);
    // The sql-file destination asks for a file (React 554-557).
    let mut vm = endpoints_ready_vm();
    let mut sql_file = profile("p2", "SQL Profile");
    sql_file.destination_mode = TransferDestinationMode::SqlFile;
    vm.transfer_profiles = vec![sql_file];
    vm.selected_profile_id = "p2".to_owned();
    assert!(vm.load_selected_profile());
    assert_eq!(vm.sql_file_dialect, "source", "sqlFileDialect ?? 'source'");
    assert_eq!(vm.sql_file_encoding, SqlFileEncoding::Utf8);
    assert_eq!(vm.sql_file_compression, SqlFileCompression::None);
}

#[test]
fn load_selected_profile_without_a_selection_is_a_no_op() {
    // React 527-528: `find` returns undefined -> return.
    let mut vm = endpoints_ready_vm();
    assert!(!vm.load_selected_profile());
    assert_eq!(vm.step, WizardStep::Endpoints);
}

#[test]
fn profile_missing_fields_checks_the_database_destination() {
    // React 454-464.
    let mut vm = endpoints_ready_vm();
    vm.profile_name = "  ".to_owned();
    assert!(vm.profile_missing_fields());
    vm.profile_name = "p".to_owned();
    assert!(!vm.profile_missing_fields());
    vm.target_database.clear();
    assert!(vm.profile_missing_fields());
    // The sql-file destination only needs the source (React 460).
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(!vm.profile_missing_fields());
}

#[test]
fn profile_save_payload_reuses_the_existing_id_and_timestamp() {
    // React 465-489.
    let mut vm = endpoints_ready_vm();
    vm.profile_name = "  My Profile  ".to_owned();
    vm.use_target_default_collation = true;
    vm.batch_size_edit = "250".to_owned();
    let mut existing = profile("p1", "Old Name");
    existing.created_at = "2026-01-01T00:00:00.000Z".to_owned();
    vm.transfer_profiles = vec![existing];
    vm.selected_profile_id = "p1".to_owned();
    let payload = vm.profile_save_payload("2026-02-02T00:00:00.000Z", "fresh-id");
    assert_eq!(payload.id, "p1", "the existing id is reused (React 469)");
    assert_eq!(payload.name, "My Profile", "the name is trimmed (React 454)");
    assert_eq!(payload.version, 1);
    assert_eq!(payload.source_connection_id, "src");
    assert_eq!(payload.target_connection_id.as_deref(), Some("tgt"));
    assert_eq!(payload.source_database.as_deref(), Some("db1"));
    assert_eq!(payload.target_database.as_deref(), Some("db2"));
    assert_eq!(payload.destination_mode, TransferDestinationMode::Database);
    // The sql-file fields are nulled on the database branch (React 478-482).
    assert!(payload.sql_file_dialect.is_none());
    assert!(payload.sql_file_encoding.is_none());
    assert!(payload.sql_file_compression.is_none());
    assert!(payload.sql_file_database.is_none());
    assert!(payload.sql_file_schema.is_none());
    assert_eq!(payload.created_at, "2026-01-01T00:00:00.000Z");
    assert_eq!(payload.updated_at, "2026-02-02T00:00:00.000Z");
    assert_eq!(payload.options.batch_size, Some(250));
    assert!(payload.options.use_target_default_collation);
    // A fresh save keeps the host's id and stamps both times.
    let mut vm = endpoints_ready_vm();
    vm.profile_name = "New".to_owned();
    let payload = vm.profile_save_payload("2026-02-02T00:00:00.000Z", "new-id");
    assert_eq!(payload.id, "new-id");
    assert_eq!(payload.created_at, "2026-02-02T00:00:00.000Z");
    assert_eq!(payload.updated_at, "2026-02-02T00:00:00.000Z");
}

#[test]
fn profile_saved_selects_the_saved_id_and_name() {
    // React 490-494.
    let mut vm = endpoints_ready_vm();
    let saved = profile("p9", "Saved Name");
    vm.profile_saved(&saved);
    assert_eq!(vm.selected_profile_id, "p9");
    assert_eq!(vm.profile_name, "Saved Name");
}

// ---------------------------------------------------------------------------
// The table reducers (React 560-564, 967-1042).
// ---------------------------------------------------------------------------

#[test]
fn update_table_clears_the_preview_and_the_gate_error() {
    // React 969-972.
    let mut vm = endpoints_ready_vm();
    vm.step = WizardStep::Mapping;
    vm.preview = Some(preview(true));
    vm.mapping_gate_error = "gate lost".to_owned();
    vm.tables = vec![row("t1", "t1", true)];
    vm.update_table(
        "t1",
        &TransferTablePatch {
            target_table: Some("t9".to_owned()),
            ..TransferTablePatch::default()
        },
    );
    assert!(vm.preview.is_none());
    assert!(vm.mapping_gate_error.is_empty());
    assert_eq!(vm.tables[0].vm.target_table, "t9");
}

#[test]
fn update_table_with_mappings_clears_the_ddl_override() {
    // React 977-980.
    let mut vm = endpoints_ready_vm();
    let mut table = row("t1", "t1", true);
    table.ddl_override = Some("CREATE t".to_owned());
    vm.tables = vec![table];
    vm.update_table(
        "t1",
        &TransferTablePatch {
            column_mappings: Some(vec![mapping("a", "a", false)]),
            ..TransferTablePatch::default()
        },
    );
    assert!(vm.tables[0].ddl_override.is_none());
    // A non-mapping patch does not clear it.
    let mut table = row("t1", "t1", true);
    table.ddl_override = Some("CREATE t".to_owned());
    let mut vm = endpoints_ready_vm();
    vm.tables = vec![table];
    vm.update_table(
        "t1",
        &TransferTablePatch {
            enabled: Some(false),
            ..TransferTablePatch::default()
        },
    );
    assert_eq!(vm.tables[0].ddl_override.as_deref(), Some("CREATE t"));
}

#[test]
fn update_table_recordset_distinguishes_clear_from_leave() {
    // The Option<Option<…>> shape: Some(None) clears (the enable toggle
    // switching the recordset off), None leaves it.
    let mut vm = endpoints_ready_vm();
    let mut table = row("t1", "t1", true);
    table.vm.recordset = Some(datazen_ui::views::TransferRecordset {
        order_by: Some("id".to_owned()),
        ..datazen_ui::views::TransferRecordset::default()
    });
    vm.tables = vec![table];
    vm.update_table(
        "t1",
        &TransferTablePatch {
            recordset: Some(None),
            ..TransferTablePatch::default()
        },
    );
    assert!(vm.tables[0].vm.recordset.is_none());
    vm.update_table(
        "t1",
        &TransferTablePatch {
            recordset: None,
            ..TransferTablePatch::default()
        },
    );
    assert!(vm.tables[0].vm.recordset.is_none(), "leave-it is a no-op");
}

#[test]
fn update_table_ddl_override_targets_one_row() {
    // React 560-564.
    let mut vm = endpoints_ready_vm();
    vm.tables = vec![row("t1", "t1", true), row("t2", "t2", true)];
    vm.update_table_ddl_override("t2", "CREATE t2".to_owned());
    assert!(vm.tables[0].ddl_override.is_none());
    assert_eq!(vm.tables[1].ddl_override.as_deref(), Some("CREATE t2"));
}

#[test]
fn toggle_table_flips_only_the_named_row_and_keeps_the_preview() {
    // React 1036-1042 — a toggle is not a mapping edit; the preview
    // survives (unlike updateTable).
    let mut vm = endpoints_ready_vm();
    vm.preview = Some(preview(true));
    vm.tables = vec![row("t1", "t1", true), row("t2", "t2", false)];
    vm.toggle_table("t1");
    assert!(!vm.tables[0].vm.enabled);
    assert!(!vm.tables[1].vm.enabled);
    assert!(vm.preview.is_some());
}

#[test]
fn apply_inspect_rows_normalizes_and_spends_the_profile_stash() {
    // React 674-707.
    let mut vm = endpoints_ready_vm();
    vm.profile_mappings = Some(vec![]);
    let mut table = row("t1", "t1", false);
    table.vm.source_columns = Some(vec!["a".to_owned(), "b".to_owned()]);
    table.vm.column_mappings = vec![mapping("a", "a", false)];
    let empty = row("", "x", true);
    vm.apply_inspect_rows(vec![table, empty]);
    // Rows without a source name are dropped (React 694).
    assert_eq!(vm.tables.len(), 1);
    // Mappings are normalized: 'b' is synthesised skipped (React 697).
    assert_eq!(vm.tables[0].vm.column_mappings.len(), 2);
    assert!(vm.tables[0].vm.column_mappings[1].skip);
    // The selected table falls back to the first row when no row is
    // enabled (React 701-705: find(enabled) ?? enabled[0]).
    assert_eq!(vm.selected_mapping_table, "t1");
    // The profile stash is spent (React 707).
    assert!(vm.profile_mappings.is_none());
}

#[test]
fn apply_inspect_rows_keeps_a_still_present_selection() {
    // React 700-705: prev kept while still present; first enabled when not.
    let mut vm = endpoints_ready_vm();
    vm.selected_mapping_table = "t2".to_owned();
    vm.apply_inspect_rows(vec![row("t1", "t1", true), row("t2", "t2", true)]);
    assert_eq!(vm.selected_mapping_table, "t2");
    vm.apply_inspect_rows(vec![row("t1", "t1", true), row("t3", "t3", true)]);
    assert_eq!(vm.selected_mapping_table, "t1", "t2 gone -> first enabled");
}

#[test]
fn apply_table_refresh_updates_server_fields_only() {
    // React 1005-1021: status/targetColumns/sourceColumns/
    // incompatibleReason from the refresh; createNew/targetTable/
    // columnMappings survive.
    let mut vm = endpoints_ready_vm();
    let mut table = row("t1", "local-name", true);
    table.vm.create_new = true;
    table.vm.column_mappings = vec![mapping("a", "a", false)];
    table.incompatible_reason = Some("old".to_owned());
    vm.tables = vec![table];
    let mut inspected = row("t1", "server-name", true);
    inspected.vm.target_columns = Some(vec!["x".to_owned()]);
    inspected.vm.source_columns = Some(vec!["a".to_owned()]);
    inspected.incompatible_reason = Some("new".to_owned());
    vm.apply_table_refresh("t1", Some(&inspected));
    assert_eq!(vm.tables[0].vm.target_columns, Some(vec!["x".to_owned()]));
    assert_eq!(vm.tables[0].vm.source_columns, Some(vec!["a".to_owned()]));
    assert_eq!(vm.tables[0].incompatible_reason.as_deref(), Some("new"));
    assert_eq!(vm.tables[0].vm.target_table, "local-name", "local edit kept");
    assert!(vm.tables[0].vm.create_new, "local edit kept");
    assert_eq!(vm.tables[0].vm.column_mappings.len(), 1, "local edit kept");
    // An absent response row leaves the table untouched (React 1009).
    vm.apply_table_refresh("missing", Some(&inspected));
    assert_eq!(vm.tables[0].vm.target_table, "local-name");
    // None leaves the table untouched (React 1022-1024: keep local edits).
    vm.apply_table_refresh("t1", None);
    assert_eq!(vm.tables[0].vm.target_columns, Some(vec!["x".to_owned()]));
}

// ---------------------------------------------------------------------------
// The inspect / preview reducers (React 659-771).
// ---------------------------------------------------------------------------

#[test]
fn preview_reducer_brackets_the_round_trip() {
    // React 725-771.
    let mut vm = endpoints_ready_vm();
    vm.preview_error = "old".to_owned();
    vm.mapping_gate_error = "gate".to_owned();
    vm.begin_preview();
    assert!(vm.preview_error.is_empty(), "React 734: setPreviewError('')");
    assert!(vm.mapping_gate_error.is_empty(), "React 735");
    assert!(vm.loading);
    vm.finish_preview(preview(true));
    assert!(vm.preview.is_some());
    assert!(!vm.loading);
    // The quiet failure does not open the dialog (React 761-763).
    vm.begin_preview();
    vm.fail_preview("boom", true);
    assert!(vm.preview.is_none());
    assert_eq!(vm.preview_error, "boom");
    assert!(!vm.error_open);
    // The loud failure does (React 762-764).
    vm.begin_preview();
    vm.fail_preview("boom", false);
    assert!(vm.error_open);
    assert_eq!(vm.error_msg, "boom");
    // A classified refusal records its own words on the preview surface
    // (React 749-753).
    vm.begin_preview();
    vm.preview_refused("backend refused");
    assert_eq!(vm.preview_error, "backend refused");
    assert!(!vm.loading);
}

#[test]
fn inspect_reducer_brackets_the_round_trip() {
    // React 672-713.
    let mut vm = endpoints_ready_vm();
    vm.begin_inspect();
    assert!(vm.loading);
    vm.apply_inspect_rows(vec![row("t1", "t1", true)]);
    vm.finish_inspect();
    assert!(!vm.loading);
    assert_eq!(vm.tables.len(), 1);
}

#[test]
fn inspect_missing_endpoints_matches_the_guard() {
    // React 663-670.
    let mut vm = endpoints_ready_vm();
    vm.source_session = Some(session("src", "db1"));
    vm.target_session = Some(session("tgt", "db2"));
    assert!(!vm.inspect_missing_endpoints());
    vm.target_session = None;
    assert!(vm.inspect_missing_endpoints());
    // The sql-file branch has no target requirement.
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(!vm.inspect_missing_endpoints());
    vm.source_session = None;
    assert!(vm.inspect_missing_endpoints());
}

// ---------------------------------------------------------------------------
// Execute (React 773-845).
// ---------------------------------------------------------------------------

#[test]
fn execute_gates_mirror_react() {
    // React 777-792.
    let mut vm = endpoints_ready_vm();
    assert!(!vm.execute_read_only_blocked());
    vm.connections[1].read_only = true;
    assert!(vm.execute_read_only_blocked());
    // The sql-file branch ignores the read-only target (React 777).
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(!vm.execute_read_only_blocked());
    // The stale-plan refusal (React 785-792).
    let mut vm = endpoints_ready_vm();
    assert!(vm.execute_stale_plan(), "no preview: stale");
    vm.preview = Some(preview(true));
    assert!(!vm.execute_stale_plan());
    // The destructive write mode routes through the confirm (React
    // 834-840).
    assert!(!vm.handle_execute_click_confirm());
    vm.write_mode = WriteMode::DropCreateInsert;
    assert!(vm.handle_execute_click_confirm());
}

#[test]
fn execute_table_count_and_source_tables() {
    // React 794-805.
    let mut vm = endpoints_ready_vm();
    vm.preview = Some(preview(true));
    vm.tables = vec![row("t1", "t1", true), row("t2", "t2", false)];
    let job = vm
        .build_job(
            Some(&session("src", "db1")),
            Some(&session("tgt", "db2")),
        )
        .expect("job");
    // With rows: the job's enabled tables (React 795-797).
    assert_eq!(vm.execute_table_count(&job), 1);
    // The selection lists the enabled source tables (React 802-805).
    assert_eq!(
        vm.execute_source_tables(&job),
        Some(vec!["t1".to_owned()])
    );
    // A SQL-file run with no inspected rows selects nothing — every
    // table the plan froze (React 802-804).
    vm.tables.clear();
    vm.destination_mode = TransferDestinationMode::SqlFile;
    assert!(vm.execute_source_tables(&job).is_none());
    // The table count then falls back to the plan's own size (React
    // 797-798: max(writePlans, ddl) = 1).
    assert_eq!(vm.execute_table_count(&job), 1);
}

#[test]
fn execute_confirm_closes_the_dialog() {
    // React 842-845.
    let mut vm = endpoints_ready_vm();
    vm.execute_confirm_open = true;
    vm.handle_execute_confirm();
    assert!(!vm.execute_confirm_open);
}

// ---------------------------------------------------------------------------
// The step arithmetic (React 879-965).
// ---------------------------------------------------------------------------

#[test]
fn go_back_steps_down_and_stops_at_the_first() {
    // React 962-965.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Result;
    vm.go_back();
    assert_eq!(vm.step, WizardStep::Preview);
    vm.go_back();
    assert_eq!(vm.step, WizardStep::Mapping);
    vm.step = WizardStep::Endpoints;
    vm.go_back();
    assert_eq!(vm.step, WizardStep::Endpoints, "index 0: no prev");
}

#[test]
fn go_next_target_advances_and_needs_inspect_only_into_objects() {
    // React 935-960.
    let mut vm = DataTransferWindowVm::default();
    assert_eq!(vm.go_next_target(), Some(WizardStep::Setup));
    assert!(!vm.go_next_needs_inspect());
    vm.step = WizardStep::Objects;
    assert_eq!(vm.go_next_target(), Some(WizardStep::Mapping));
    vm.step = WizardStep::Setup;
    vm.tables.clear();
    assert!(vm.go_next_needs_inspect(), "React 956-958");
    vm.tables = vec![row("t", "t", true)];
    assert!(!vm.go_next_needs_inspect());
    // The last step has no next.
    vm.step = WizardStep::Result;
    assert_eq!(vm.go_next_target(), None);
    // advance_step applies the target.
    vm.step = WizardStep::Endpoints;
    vm.advance_step();
    assert_eq!(vm.step, WizardStep::Setup);
}

#[test]
fn mapping_gate_lost_records_the_message_and_blocks() {
    // React 949-953: re-decided against the post-await rows.
    let mut vm = DataTransferWindowVm::default();
    vm.step = WizardStep::Mapping;
    // No rows: the gate is lost.
    assert!(!vm.go_next_after_mapping_gate(&[]));
    assert_eq!(vm.mapping_gate_error, t("transfer.mapping.gateLost"));
    // A clearing row set passes and clears nothing.
    let mut table = row("t1", "t1", true);
    table.vm.source_columns = Some(vec!["c1".to_owned()]);
    table.vm.column_mappings = vec![mapping("c1", "c1", false)];
    vm.mapping_gate_error.clear();
    assert!(vm.go_next_after_mapping_gate(&[table]));
    assert!(vm.mapping_gate_error.is_empty());
}

#[test]
fn handle_re_review_plan_returns_to_mapping_without_the_preview() {
    // React 872-877.
    let mut vm = endpoints_ready_vm();
    vm.step = WizardStep::Result;
    vm.preview = Some(preview(true));
    vm.execute_progress = "running".to_owned();
    vm.handle_re_review_plan();
    assert_eq!(vm.step, WizardStep::Mapping);
    assert!(vm.preview.is_none());
    assert!(vm.execute_progress.is_empty());
}

// ---------------------------------------------------------------------------
// The mapping-gate placeholder (transferMappingView.ts:58-100).
// ---------------------------------------------------------------------------

#[test]
fn mapping_gate_rules() {
    let gate_table = |enabled: bool, create_new: bool, target: &str, mapped: bool| {
        let mut table = TransferTableVm {
            enabled,
            create_new,
            target_table: target.to_owned(),
            ..TransferTableVm::default()
        };
        if mapped {
            table.source_columns = Some(vec!["c".to_owned()]);
            table.column_mappings = vec![mapping("c", "c", false)];
        }
        table
    };
    // An enabled, named, mapped row clears the gate.
    assert!(mapping_gate_allows_advance(&[gate_table(true, false, "t", true)]));
    // A disabled row cannot (React 69-70).
    assert!(!mapping_gate_allows_advance(&[gate_table(false, false, "t", true)]));
    // A create-new row behind whitespace is the refused shape (React 72).
    assert!(!mapping_gate_allows_advance(&[gate_table(true, true, " ", true)]));
    assert!(mapping_gate_allows_advance(&[gate_table(true, true, "typed", true)]));
    // No active mappings: shut (tableHasActiveMappings, React 58-60).
    assert!(!mapping_gate_allows_advance(&[gate_table(true, false, "t", false)]));
    // The block reason: only the unnamed create-new case is reported
    // (React 92-100).
    let mut rows = vec![gate_table(true, true, " ", true)];
    assert_eq!(
        mapping_gate_block_reason(&rows),
        Some("transfer.mapping.targetNameRequired")
    );
    // A table-less gate is "still working", not a dead end: no reason.
    rows.clear();
    assert_eq!(mapping_gate_block_reason(&rows), None);
    // An unmapped enabled non-create row reports nothing either.
    assert_eq!(
        mapping_gate_block_reason(&[gate_table(true, false, "t", false)]),
        None
    );
}

// ---------------------------------------------------------------------------
// The sync-category placeholder table (lib/syncPairing.ts:31-47).
// ---------------------------------------------------------------------------

#[test]
fn sync_category_and_family_fallbacks() {
    // categoryFromMeta defaults unknown types to sql (syncPairing.ts:40),
    // and kiwi/superset to other (syncPairing.ts:38-39).
    assert_eq!(
        datazen_ui::views::sync_category("mysql"),
        SyncCategory::Sql
    );
    assert_eq!(
        datazen_ui::views::sync_category("redis"),
        SyncCategory::Kv
    );
    assert_eq!(
        datazen_ui::views::sync_category("mongodb"),
        SyncCategory::Document
    );
    assert_eq!(
        datazen_ui::views::sync_category("kiwi"),
        SyncCategory::Other
    );
    assert_eq!(
        datazen_ui::views::sync_category("superset"),
        SyncCategory::Other
    );
    assert_eq!(
        datazen_ui::views::sync_category("unknown-db"),
        SyncCategory::Sql
    );
    // normalizeSyncFamily: meta syncFamily, else non-generic sqlDialect,
    // else the normalized id (syncPairing.ts:42-47).
    assert_eq!(
        datazen_ui::views::normalize_sync_family("redis"),
        "redis".to_owned()
    );
    assert_eq!(
        datazen_ui::views::normalize_sync_family("hbase"),
        "hbase".to_owned()
    );
    assert_eq!(
        datazen_ui::views::normalize_sync_family("mysql"),
        "mysql".to_owned()
    );
    // A generic dialect falls through to the normalized id.
    assert_eq!(
        datazen_ui::views::normalize_sync_family("influxdb"),
        "influxdb".to_owned()
    );
    // The driver-id alias resolves through the registry fallback.
    assert_eq!(
        datazen_ui::views::normalize_sync_family("postgres"),
        "postgresql".to_owned()
    );
}
