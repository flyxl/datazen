//! T147-S1: the Data Transfer window's wizard state model.
//!
//! Ported from `packages/frontend/src/windows/data-transfer/DataTransferWindow.tsx`
//! (1,901 lines). S1 owns the state machine and every pure decision the
//! window makes; S2 ([`super::transfer_job_run`]) owns the Job run machine
//! (`useTransferJobRun`) and the run-dependent gates; S3
//! ([`super::data_transfer_window`]) owns the rendering.
//!
//! ## What S1 covers (React lines cited per item)
//!
//! * the wizard steps and step order (`WizardStep`, `STEPS`,
//!   `NARROW_STEPS`, React 74-78);
//! * every `useState` slot the window holds (the `DataTransferWindowVm`
//!   fields, React 86-155);
//! * the endpoint derivations — `sourceConn` / `targetConn` / `connOptions`
//!   / `targetOptions` (with the unsupported / experimental pair hints) /
//!   `pairing` / `targetReadOnly` (React 250-296);
//! * the SQL-file destination config and its option lists (React 112-121,
//!   1250-1398) including the dialect-change reset (React 1288-1294);
//! * `tablesToMappings` / `allTablesToMappings` (React 431-451) and the
//!   inspect application that normalizes rows (React 674-707);
//! * the profile save payload and the load application (React 453-558);
//! * `buildJob`'s payload assembly (React 595-657) — the pure half: the
//!   sessions are a host-fed input, exactly as React's `buildJob` takes
//!   them as a parameter;
//! * the idempotency-key derivations `nextPrepareKey` / `applyKeyForPlan`
//!   (React 155-179);
//! * the step gates `canNext` / `canExecute` / `validBatchSize` (React
//!   879-925), the `goNext` / `goBack` step arithmetic (React 935-965) and
//!   the mapping-gate error slot (React 949-951);
//! * the table reducers `updateTable` (with the `columnMappings` =>
//!   `ddlOverride` clearing), `updateTableDdlOverride`, `toggleTable`
//!   (React 560-564, 967-983, 1036-1042) and the mapping-refresh merge
//!   (React 985-1034);
//! * the preview state and the `runPreview` reducer (React 725-771);
//! * the profile-mappings stash `profileMappingsRef` (React 153, 547,
//!   674, 707) and the `tablesRef` mirror contract (React 161-164);
//! * the `datazen:connection-closed` session clearing (React 229-240);
//! * the SQL-file dialect list the endpoints effect builds (React
//!   212-225) and the `refreshEndpointSessions` guard (React 566-593).
//!
//! ## Host contract (the Vm never performs IPC)
//!
//! * **Connections / databases / dialects.** The host loads the connection
//!   list, the per-connection database lists and the available SQL drivers,
//!   then feeds them via `apply_connections_loaded`,
//!   `apply_source_databases` / `apply_target_databases` and
//!   `set_available_sql_dialects`. The database prefill application lives
//!   here so the host does not re-implement React's effect body.
//! * **Endpoint prefill.** The host reads the window URL's prefill params
//!   (the WebView URL is T644's seam) into a
//!   [`MigrationWindowPrefill`] and calls
//!   `apply_connections_loaded`, whose prefill leg applies the ids at
//!   most once, exactly as `useMigrationEndpointPrefill`'s `appliedRef`
//!   effect.
//! * **Sessions.** The dedicated side sessions (`lib/dedicatedDbSession.ts`
//!   is T467's, segment 2) are host-owned IPC, but their window state is
//!   the Vm's: the host feeds `set_source_session` /
//!   `set_target_session` (the `ensureDedicatedSession` results) and
//!   `on_connection_closed` for the `datazen:connection-closed` bus
//!   event. [`refresh_sessions_guard`] decides whether the refresh may
//!   run at all, mirroring `refreshEndpointSessions`' head (React
//!   566-571).
//! * **File pick.** `chooseSqlFile` is a save dialog: the host resolves
//!   `transferCommands.pickSqlFile()` and feeds the opaque
//!   [`TransferSqlFileTargetVm`] token with `choose_sql_file`.
//! * **Profiles.** The host loads the profile list
//!   (`apply_transfer_profiles`) and performs the save; the payload the
//!   window sends is [`profile_save_payload`] so the host never
//!   re-derives it.
//! * **Inspect / preview / execute.** The host drives the async middle;
//!   every outcome application is a Vm method (`apply_inspect_rows`,
//!   `begin_preview` / `finish_preview` / `fail_preview`, the run-state
//!   methods in S2) so the reducer semantics cannot drift into two
//!   places.
//!
//! ## Documented deviations from React
//!
//! * **`batchSize` is a `String` edit.** React binds a number input
//!   (`Number(e.target.value)`, React 1486) — an empty or non-numeric
//!   string becomes `NaN`, which `validBatchSize` then rejects. The Vm
//!   keeps the raw edit text and `batch_size()` parses it the same way
//!   (`trim().parse::<i64>().ok()`; `1e2` is a React `100` but a Rust
//!   parse miss — the number input itself normalizes it before `value`
//!   reports it, so the visible behaviour matches).
//! * **`crypto.randomUUID` ids are host-minted.** The prepare scope is
//!   taken from the host (`new(scope)`, React 155 mints it per mount);
//!   the profile id stand-in derives from the scope so tests stay
//!   deterministic.
//! * **`sourceRowCount` mirrors as `Option<i64>`.** React's `number | null`
//!   has no integer bound; the inspect wire is integral rows.
//! * **`sourceColumnTypes` is the ordered association list** the mapping VM
//!   already carries (see [`super::transfer_mapping_view::TransferTableVm`])
//!   — a handful of columns; linear lookup is enough, per the T023-S2
//!   precedent.
//! * **The `sort` of the write plans / warnings / DDL is the wire's**; the
//!   preview mirrors preserve it.

use crate::i18n::t;
use crate::table_data_connection_state::{FilterCondition, FilterLogic};
use crate::views::data_transfer::column_mapping_row::TransferColumnMapping;
use crate::views::data_transfer::recordset_editor::TransferRecordset;
use crate::views::data_transfer::transfer_mapping_view::{
    mapping_gate_allows_advance, normalize_column_mappings, TransferMappingStatus, TransferTableVm,
};
use crate::views::data_transfer::transfer_job_run::{TransferJobRunVm, TransferRunPhase};
use crate::views::data_transfer::transfer_pairing_vm::{
    is_transfer_target_supported, is_verified_migration_pair, pick_prefill_database,
    resolve_default_database, resolve_transfer_pairing, sync_category, MigrationWindowPrefill,
    SyncCategory, TransferPairingResult,
};

// ---------------------------------------------------------------------------
// The wizard steps (React 74-78).
// ---------------------------------------------------------------------------

/// React `WizardStep` (`DataTransferWindow.tsx:74`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WizardStep {
    Endpoints,
    Setup,
    Objects,
    Mapping,
    Preview,
    Result,
}

impl WizardStep {
    /// React `STEPS.indexOf(step)` (React 879) — the step rail's index.
    pub(crate) fn index(self) -> usize {
        STEPS
            .iter()
            .position(|step| *step == self)
            .unwrap_or(0)
    }

    /// The i18n key the step rail's label uses (React 1135:
    /// `` t(`transfer.step.${s}`) ``).
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            WizardStep::Endpoints => "transfer.step.endpoints",
            WizardStep::Setup => "transfer.step.setup",
            WizardStep::Objects => "transfer.step.objects",
            WizardStep::Mapping => "transfer.step.mapping",
            WizardStep::Preview => "transfer.step.preview",
            WizardStep::Result => "transfer.step.result",
        }
    }
}

/// React `STEPS` (React 76).
pub(crate) const STEPS: [WizardStep; 6] = [
    WizardStep::Endpoints,
    WizardStep::Setup,
    WizardStep::Objects,
    WizardStep::Mapping,
    WizardStep::Preview,
    WizardStep::Result,
];

/// React `NARROW_STEPS` (React 78) — the steps whose body uses the narrow
/// (`max-w-2xl`) column.
pub(crate) const NARROW_STEPS: [WizardStep; 3] = [
    WizardStep::Endpoints,
    WizardStep::Setup,
    WizardStep::Result,
];

// ---------------------------------------------------------------------------
// Contract mirrors (`commands/transfer.ts`), transliterated.
// ---------------------------------------------------------------------------

/// React `TransferMode` (`commands/transfer.ts:4`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TransferMode {
    Structure,
    #[default]
    Data,
    StructureAndData,
}

impl TransferMode {
    /// The TS union's own spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TransferMode::Structure => "structure",
            TransferMode::Data => "data",
            TransferMode::StructureAndData => "structureAndData",
        }
    }

    /// Parse the TS union's own spelling.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "structure" => Some(TransferMode::Structure),
            "data" => Some(TransferMode::Data),
            "structureAndData" => Some(TransferMode::StructureAndData),
            _ => None,
        }
    }
}

/// React `WriteMode` (`commands/transfer.ts:5`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WriteMode {
    #[default]
    Insert,
    TruncateInsert,
    DropCreateInsert,
}

impl WriteMode {
    /// The TS union's own spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WriteMode::Insert => "insert",
            WriteMode::TruncateInsert => "truncateInsert",
            WriteMode::DropCreateInsert => "dropCreateInsert",
        }
    }

    /// Parse the TS union's own spelling.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "insert" => Some(WriteMode::Insert),
            "truncateInsert" => Some(WriteMode::TruncateInsert),
            "dropCreateInsert" => Some(WriteMode::DropCreateInsert),
            _ => None,
        }
    }
}

/// The window's SQL-file encoding union (React 115-118):
/// `'utf8' | 'utf8Bom' | 'utf16Le' | 'utf16Be'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SqlFileEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl SqlFileEncoding {
    /// The TS union's own spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SqlFileEncoding::Utf8 => "utf8",
            SqlFileEncoding::Utf8Bom => "utf8Bom",
            SqlFileEncoding::Utf16Le => "utf16Le",
            SqlFileEncoding::Utf16Be => "utf16Be",
        }
    }
}

/// The window's SQL-file compression union (React 118): `'none' | 'gzip'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SqlFileCompression {
    #[default]
    None,
    Gzip,
}

impl SqlFileCompression {
    /// The TS union's own spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SqlFileCompression::None => "none",
            SqlFileCompression::Gzip => "gzip",
        }
    }
}

/// React `TransferEndpoint` (`commands/transfer.ts:15-19`) as the window's
/// `buildJob` assembles it; the schema rides the connection, not the
/// session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TransferEndpointVm {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
}

/// React `DedicatedSideSession` (`lib/dedicatedDbSession.ts:3-7`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DedicatedSessionVm {
    pub connection_id: String,
    pub database: String,
    pub db_session_id: String,
}

/// React `TransferSqlFileTarget` (`commands/transfer.ts:94-106`) as the
/// window's state holds it. The optional fields are `Option` because the
/// wire distinguishes "omitted" (the default) from an explicit value.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferSqlFileTargetVm {
    /// Opaque host token returned by the native SQL save dialog.
    pub file_token: String,
    /// Registered SQL driver used to render the artifact; omitted means
    /// source dialect.
    pub database_type: Option<String>,
    /// Output text encoding; omitted means UTF-8 without a BOM.
    pub encoding: Option<SqlFileEncoding>,
    /// Output compression; omitted means an uncompressed `.sql` artifact.
    pub compression: Option<SqlFileCompression>,
    /// Optional target catalog/database qualifier for the generated SQL.
    pub database: Option<String>,
    /// Optional target schema qualifier for the generated SQL.
    pub schema: Option<String>,
}

/// React `TransferSourceFilter` (`commands/transfer.ts:43-46`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferSourceFilter {
    pub filters: Vec<FilterCondition>,
    pub logic: Option<FilterLogic>,
}

/// React `TransferTableMapping` (`commands/transfer.ts:29-41`) — what the
/// window sends for every table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferTableMapping {
    pub source_table: String,
    pub target_table: String,
    pub create_new: Option<bool>,
    pub enabled: Option<bool>,
    pub column_mappings: Option<Vec<TransferColumnMapping>>,
    /// Execute this CREATE instead of auto-generated DDL.
    pub ddl_override: Option<String>,
    /// Structured, parameterized source row filter.
    pub source_filter: Option<TransferSourceFilter>,
    /// Stable source recordset selection; never a restart checkpoint.
    pub recordset: Option<TransferRecordset>,
}

/// React `TransferOptions` (`commands/transfer.ts:75-81`) — the four
/// option fields the window reads; `batchSize` is the parsed number, the
/// raw edit text lives in the Vm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TransferOptionsVm {
    pub batch_size: Option<i64>,
    pub stop_on_error: bool,
    pub confirmed_destructive: bool,
    pub use_target_default_collation: bool,
}

/// React `DEFAULT_TRANSFER_OPTIONS` (`commands/transfer.ts:228-233`).
pub(crate) const DEFAULT_TRANSFER_OPTIONS: TransferOptionsVm = TransferOptionsVm {
    batch_size: Some(500),
    stop_on_error: true,
    confirmed_destructive: false,
    use_target_default_collation: false,
};

/// React `TransferJob` (`commands/transfer.ts:83-92`) as the window's
/// `buildJob` assembles it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferJobVm {
    pub source: TransferEndpointVm,
    pub target: Option<TransferEndpointVm>,
    pub sql_file_target: Option<TransferSqlFileTargetVm>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TransferTableMapping>,
    pub options: TransferOptionsVm,
}

/// React `TransferProfile` (`commands/transfer.ts:108-130`) — the save
/// payload's shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferProfileVm {
    pub version: i64,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    pub target_connection_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub destination_mode: TransferDestinationMode,
    pub sql_file_dialect: Option<String>,
    pub sql_file_encoding: Option<SqlFileEncoding>,
    pub sql_file_compression: Option<SqlFileCompression>,
    pub sql_file_database: Option<String>,
    pub sql_file_schema: Option<String>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TransferTableMapping>,
    pub options: TransferOptionsVm,
    pub created_at: String,
    pub updated_at: String,
}

/// The window's destination union (React 112): `'database' | 'sqlFile'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TransferDestinationMode {
    #[default]
    Database,
    SqlFile,
}

/// React `TransferDdlPreview` (`commands/transfer.ts:150-156`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferDdlPreviewVm {
    pub source_table: String,
    pub target_table: String,
    pub ddl: String,
    /// React `kind?: 'table' | 'index' | 'foreignKey' | 'dropTable'`.
    pub kind: Option<DdlKind>,
    /// React `dependsOn?: string[]`.
    pub depends_on: Option<Vec<String>>,
}

/// The DDL item kind union (`commands/transfer.ts:154`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DdlKind {
    Table,
    Index,
    ForeignKey,
    DropTable,
}

impl DdlKind {
    /// React's `item.kind ?? 'table'` default (React 1631).
    pub(crate) fn or_table(self) -> &'static str {
        match self {
            DdlKind::Table => "table",
            DdlKind::Index => "index",
            DdlKind::ForeignKey => "foreignKey",
            DdlKind::DropTable => "dropTable",
        }
    }
}

/// React `TransferWritePlan` (`commands/transfer.ts:158-167`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferWritePlanVm {
    pub source_table: String,
    pub target_table: String,
    pub write_mode: WriteMode,
    pub mapped_columns: Vec<TransferColumnMapping>,
    pub estimated_rows: Option<i64>,
    pub preamble: Vec<String>,
    pub source_filter_preview: Option<String>,
    pub recordset_preview: Option<String>,
}

/// React `TransferPreview` (`commands/transfer.ts:169-179`) — the review
/// the prepare Job returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferPreviewVm {
    pub plan_id: String,
    pub pairing_path: String,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub ddl: Vec<TransferDdlPreviewVm>,
    pub write_plans: Vec<TransferWritePlanVm>,
    pub warnings: Vec<String>,
    pub can_execute: bool,
    pub block_reason: Option<String>,
}

/// React `TransferTableResult` (`commands/transfer.ts:132-148`): the
/// mapping VM's [`TransferTableVm`] plus the four fields only the window
/// reads — `ddlOverride`, `incompatibleReason`, `sourceRowCount`,
/// `sourceFilter`. The split keeps the T023/T305 placeholder model
/// untouched (its own doc forbids extending it with new semantics).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferTableRow {
    pub vm: TransferTableVm,
    /// React `ddlOverride?: string` — execute this CREATE instead of the
    /// auto-generated DDL.
    pub ddl_override: Option<String>,
    /// React `incompatibleReason?: string | null`.
    pub incompatible_reason: Option<String>,
    /// React `sourceRowCount?: number | null`.
    pub source_row_count: Option<i64>,
    /// React `sourceFilter?: TransferSourceFilter`.
    pub source_filter: Option<TransferSourceFilter>,
}

impl TransferTableRow {
    /// React `tbl.sourceTable` / `tbl.targetTable` / `tbl.status` reads.
    pub(crate) fn source_table(&self) -> &str {
        &self.vm.source_table
    }

    pub(crate) fn target_table(&self) -> &str {
        &self.vm.target_table
    }

    pub(crate) fn status(&self) -> TransferMappingStatus {
        self.vm.status
    }
}

/// React `Partial<TransferTableResult>` as the window's `updateTable`
/// commits it (React 967-983): the mapping-step patch plus the DDL
/// override and source filter the preview step writes. Fields React never
/// patches from the window (status, column lists, row count) are absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferTablePatch {
    pub target_table: Option<String>,
    pub create_new: Option<bool>,
    pub enabled: Option<bool>,
    pub column_mappings: Option<Vec<TransferColumnMapping>>,
    pub ddl_override: Option<String>,
    pub recordset: Option<Option<TransferRecordset>>,
    pub source_filter: Option<TransferSourceFilter>,
}

impl TransferTablePatch {
    /// Apply the patch the way the parent's `setTables` spread does
    /// (React 973-982) — including the rule that a `columnMappings` patch
    /// clears `ddlOverride` (React 977-980).
    pub(crate) fn apply_to(&self, row: &mut TransferTableRow) {
        if let Some(target_table) = &self.target_table {
            row.vm.target_table = target_table.clone();
        }
        if let Some(create_new) = self.create_new {
            row.vm.create_new = create_new;
        }
        if let Some(enabled) = self.enabled {
            row.vm.enabled = enabled;
        }
        if let Some(column_mappings) = &self.column_mappings {
            row.vm.column_mappings = column_mappings.clone();
            row.ddl_override = None;
        }
        if let Some(ddl_override) = &self.ddl_override {
            row.ddl_override = Some(ddl_override.clone());
        }
        if let Some(recordset) = &self.recordset {
            row.vm.recordset = recordset.clone();
        }
        if let Some(source_filter) = &self.source_filter {
            row.source_filter = Some(source_filter.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// The connections list (React 86, 202-210, 250-296).
// ---------------------------------------------------------------------------

/// The `ConnectionConfig` slice the window reads (`id`, `name`,
/// `databaseType`, `database`, `schema`, `readOnly`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferConnectionVm {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub read_only: bool,
}

/// One `{ value, label, disabled?, title? }` option the endpoints bar
/// renders (React 259-289).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectionOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
    /// React `title` — the hint the option carries for unsupported /
    /// experimental pairs.
    pub title: Option<String>,
}

// ---------------------------------------------------------------------------
// The window Vm.
// ---------------------------------------------------------------------------

/// The Data Transfer window's state — every `useState` slot React holds
/// (React 86-155) as one struct, with the derivations as methods.
#[derive(Clone, Debug)]
pub(crate) struct DataTransferWindowVm {
    // -- endpoints (React 86-121) -----------------------------------------
    pub connections: Vec<TransferConnectionVm>,
    pub source_id: String,
    pub target_id: String,
    pub source_databases: Vec<String>,
    pub target_databases: Vec<String>,
    pub source_database: String,
    pub target_database: String,
    pub destination_mode: TransferDestinationMode,
    pub sql_file_target: Option<TransferSqlFileTargetVm>,
    pub sql_file_dialect: String,
    pub sql_file_encoding: SqlFileEncoding,
    pub sql_file_compression: SqlFileCompression,
    pub sql_file_database: String,
    pub sql_file_schema: String,
    pub available_sql_dialects: Vec<String>,
    // -- sessions (React 104-105) -----------------------------------------
    pub source_session: Option<DedicatedSessionVm>,
    pub target_session: Option<DedicatedSessionVm>,
    // -- transfer options (React 122-131) --------------------------------
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    /// The batch-size edit text (React 127 binds a number input; see the
    /// deviations).
    pub batch_size_edit: String,
    pub stop_on_error: bool,
    pub confirmed_destructive: bool,
    pub use_target_default_collation: bool,
    // -- the inspected rows (React 124) ------------------------------------
    pub tables: Vec<TransferTableRow>,
    pub selected_mapping_table: String,
    // -- the review (React 125) --------------------------------------------
    pub preview: Option<TransferPreviewVm>,
    // -- profiles (React 150-153) ------------------------------------------
    pub transfer_profiles: Vec<TransferProfileVm>,
    pub selected_profile_id: String,
    pub profile_name: String,
    /// React `profileMappingsRef` (React 153): the profile's table
    /// mappings, spent by the first inspect (React 674, 707).
    pub profile_mappings: Option<Vec<TransferTableMapping>>,
    /// React `prepareScope` / `prepareSeqRef` (React 154-155, 175-179).
    pub prepare_scope: String,
    pub prepare_seq: u64,
    /// React `migrationPrefillRef` (`useMigrationEndpointPrefill`, React
    /// 227) — the host feeds it; the database legs consume it.
    pub endpoint_prefill: MigrationWindowPrefill,
    /// React `useMigrationEndpointPrefill`'s `appliedRef`: the connection
    /// ids apply at most once.
    pub endpoint_prefill_applied: bool,
    // -- busy / error surfaces (React 131-146) -----------------------------
    pub loading: bool,
    /// True for exactly the window between "the user asked to prepare a
    /// plan" and "the prepare resolved" (React 132-139): the mapping
    /// editor is inert for the whole round trip.
    pub preparing: bool,
    pub executing: bool,
    pub execute_progress: String,
    pub error_msg: String,
    pub error_open: bool,
    pub preview_error: String,
    /// Set only by the post-prepare re-check on the mapping step (React
    /// 146).
    pub mapping_gate_error: String,
    // -- dialog surfaces (React 147-148) ------------------------------------
    pub limitations_open: bool,
    pub execute_confirm_open: bool,
    // -- the wizard position (React 126) -----------------------------------
    pub step: WizardStep,
    // -- the Job run machine (React 93, the `jobRun` hook) -----------------
    /// React `useTransferJobRun()` (React 93): the prepare → apply →
    /// verdict lifecycle, ported as [`TransferJobRunVm`]. The host drives
    /// its async hops; the window reads its slots.
    pub job_run: TransferJobRunVm,
}

impl Default for DataTransferWindowVm {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            source_id: String::new(),
            target_id: String::new(),
            source_databases: Vec::new(),
            target_databases: Vec::new(),
            source_database: String::new(),
            target_database: String::new(),
            destination_mode: TransferDestinationMode::Database,
            sql_file_target: None,
            // React 114: `useState('source')`.
            sql_file_dialect: "source".to_owned(),
            sql_file_encoding: SqlFileEncoding::Utf8,
            sql_file_compression: SqlFileCompression::None,
            sql_file_database: String::new(),
            sql_file_schema: String::new(),
            available_sql_dialects: Vec::new(),
            source_session: None,
            target_session: None,
            // React 122-123: `useState<TransferMode>('data')`,
            // `useState<WriteMode>('insert')`.
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            // React 127: `DEFAULT_TRANSFER_OPTIONS.batchSize ?? 500` = 500.
            batch_size_edit: "500".to_owned(),
            // React 128: `useState(true)`.
            stop_on_error: true,
            confirmed_destructive: false,
            use_target_default_collation: false,
            tables: Vec::new(),
            selected_mapping_table: String::new(),
            preview: None,
            transfer_profiles: Vec::new(),
            selected_profile_id: String::new(),
            profile_name: String::new(),
            profile_mappings: None,
            prepare_scope: String::new(),
            prepare_seq: 0,
            endpoint_prefill: MigrationWindowPrefill::default(),
            endpoint_prefill_applied: false,
            loading: false,
            preparing: false,
            executing: false,
            execute_progress: String::new(),
            error_msg: String::new(),
            error_open: false,
            preview_error: String::new(),
            mapping_gate_error: String::new(),
            // React 196-200: the limitations dialog opens unless the
            // dismissal flag is set — the host reads the flag
            // (`lib/transferLimitationsPrefs.ts`, T636's) and feeds the
            // initial state.
            limitations_open: false,
            execute_confirm_open: false,
            step: WizardStep::Endpoints,
            job_run: TransferJobRunVm::default(),
        }
    }
}

impl DataTransferWindowVm {
    /// The parsed batch size (React 127 binds `batchSize`; the deviation
    /// above records why the raw edit text is the field).
    pub(crate) fn batch_size(&self) -> Option<i64> {
        self.batch_size_edit.trim().parse::<i64>().ok()
    }

    // -- endpoint derivations (React 250-296) ------------------------------

    /// React `sourceConn` (React 250-253).
    pub(crate) fn source_conn(&self) -> Option<&TransferConnectionVm> {
        self.connections.iter().find(|c| c.id == self.source_id)
    }

    /// React `targetConn` (React 254-257).
    pub(crate) fn target_conn(&self) -> Option<&TransferConnectionVm> {
        self.connections.iter().find(|c| c.id == self.target_id)
    }

    /// React `connOptions` (React 259-266): `` `${c.name} (${c.databaseType})` ``.
    pub(crate) fn conn_options(&self) -> Vec<ConnectionOption> {
        self.connections
            .iter()
            .map(|c| ConnectionOption {
                value: c.id.clone(),
                label: format!("{} ({})", c.name, c.database_type),
                disabled: false,
                title: None,
            })
            .collect()
    }

    /// React `targetOptions` (React 268-289), line for line: the target
    /// select's options with the unsupported pair hint and the
    /// experimental pair hint.
    pub(crate) fn target_options(&self) -> Vec<ConnectionOption> {
        let hint = t("common.unsupportedPair");
        let experimental_hint = t("common.experimentalPairHint");
        let src_type = self.source_conn().map(|c| c.database_type.clone());
        self.connections
            .iter()
            .map(|c| {
                let unsupported = src_type
                    .as_deref()
                    .map(|src| !is_transfer_target_supported(src, &c.database_type))
                    .unwrap_or(false);
                let base = format!("{} ({})", c.name, c.database_type);
                let experimental = src_type
                    .as_deref()
                    .map(|src| {
                        !unsupported && !is_verified_migration_pair(src, &c.database_type)
                    })
                    .unwrap_or(false);
                ConnectionOption {
                    value: c.id.clone(),
                    label: if unsupported {
                        format!("{} — {}", base, hint)
                    } else if experimental {
                        format!("{} — {}", base, experimental_hint)
                    } else {
                        base
                    },
                    disabled: unsupported,
                    title: if unsupported {
                        Some(hint.to_owned())
                    } else if experimental {
                        Some(experimental_hint.to_owned())
                    } else {
                        None
                    },
                }
            })
            .collect()
    }

    /// React `pairing` (React 291-294).
    pub(crate) fn pairing(&self) -> Option<TransferPairingResult> {
        let source = self.source_conn()?;
        let target = self.target_conn()?;
        Some(resolve_transfer_pairing(
            &source.database_type,
            &target.database_type,
        ))
    }

    /// React `targetReadOnly` (React 296).
    pub(crate) fn target_read_only(&self) -> bool {
        self.target_conn().map(|c| c.read_only).unwrap_or(false)
    }

    // -- the step gates (React 879-925) --------------------------------------

    /// React `validBatchSize` (React 880):
    /// `Number.isInteger(batchSize) && batchSize >= 1 && batchSize <= 500`.
    /// The parsed edit is always integral, so only the range remains.
    pub(crate) fn valid_batch_size(&self) -> bool {
        match self.batch_size() {
            Some(batch_size) => (1..=500).contains(&batch_size),
            None => false,
        }
    }

    /// React `canNext` (React 882-917), line for line, switch by step.
    pub(crate) fn can_next(&self) -> bool {
        match self.step {
            WizardStep::Endpoints => {
                let endpoints_ready = match self.destination_mode {
                    TransferDestinationMode::SqlFile => self.sql_file_target.is_some(),
                    TransferDestinationMode::Database => {
                        !self.target_id.is_empty()
                            && !self.target_database.is_empty()
                            && self.pairing().map(|p| p.supported).unwrap_or(false)
                    }
                };
                !self.source_id.is_empty()
                    && !self.source_database.is_empty()
                    && endpoints_ready
            }
            WizardStep::Setup => {
                if !self.valid_batch_size() {
                    return false;
                }
                if self.write_mode != WriteMode::Insert && !self.confirmed_destructive {
                    return false;
                }
                true
            }
            WizardStep::Objects => {
                !self.tables.is_empty() && self.tables.iter().any(|tbl| tbl.vm.enabled)
            }
            // Same predicate goNext re-checks after the prepare round trip
            // (React 899-900).
            WizardStep::Mapping => mapping_gate_allows_advance(&self.table_vms()),
            WizardStep::Preview | WizardStep::Result => false,
        }
    }

    /// React `canExecute` (React 919-925), line for line.
    pub(crate) fn can_execute(&self) -> bool {
        self.preview
            .as_ref()
            .map(|preview| preview.can_execute)
            .unwrap_or(false)
            && (self.destination_mode == TransferDestinationMode::SqlFile
                || !self.target_read_only())
            && !self.loading
    }

    /// The table rows as the mapping-gate VM sees them (the row's VM half).
    pub(crate) fn table_vms(&self) -> Vec<TransferTableVm> {
        self.tables.iter().map(|row| row.vm.clone()).collect()
    }

    // -- the mapping payload (React 431-451) ---------------------------------

    /// React `allTablesToMappings` (React 431-446), line for line: every
    /// table with a source name, mappings normalized, `ddlOverride`
    /// trimmed-to-Some.
    pub(crate) fn all_tables_to_mappings(&self) -> Vec<TransferTableMapping> {
        self.tables
            .iter()
            .filter(|tbl| !tbl.vm.source_table.is_empty())
            .map(|tbl| TransferTableMapping {
                source_table: tbl.vm.source_table.clone(),
                target_table: tbl.vm.target_table.clone(),
                create_new: Some(tbl.vm.create_new),
                enabled: Some(tbl.vm.enabled),
                column_mappings: Some(normalize_column_mappings(&tbl.vm)),
                ddl_override: tbl
                    .ddl_override
                    .as_deref()
                    .map(str::trim)
                    .filter(|ddl| !ddl.is_empty())
                    .map(str::to_owned),
                source_filter: tbl.source_filter.clone(),
                recordset: tbl.vm.recordset.clone(),
            })
            .collect()
    }

    /// React `tablesToMappings` (React 448-451): the enabled subset.
    pub(crate) fn tables_to_mappings(&self) -> Vec<TransferTableMapping> {
        self.all_tables_to_mappings()
            .into_iter()
            .filter(|table| table.enabled.unwrap_or(true))
            .collect()
    }

    // -- buildJob (React 595-657) ---------------------------------------------

    /// React `buildJob` (React 595-657), the pure half: the sessions are
    /// parameters exactly as React's `buildJob` takes them. `None` is
    /// every refusal React's guard returns `null` for.
    pub(crate) fn build_job(
        &self,
        source_session: Option<&DedicatedSessionVm>,
        target_session: Option<&DedicatedSessionVm>,
    ) -> Option<TransferJobVm> {
        let src_conn_id = source_session
            .map(|s| s.db_session_id.clone())
            .unwrap_or_default();
        if src_conn_id.is_empty() || self.source_database.is_empty() {
            return None;
        }
        let options = TransferOptionsVm {
            batch_size: self.batch_size(),
            stop_on_error: self.stop_on_error,
            confirmed_destructive: self.confirmed_destructive,
            use_target_default_collation: self.use_target_default_collation,
        };
        if self.destination_mode == TransferDestinationMode::SqlFile {
            let picked = self.sql_file_target.clone()?;
            let database = self.sql_file_database.trim().to_owned();
            let schema = self.sql_file_schema.trim().to_owned();
            return Some(TransferJobVm {
                source: TransferEndpointVm {
                    db_session_id: src_conn_id,
                    database: self.source_database.clone(),
                    schema: self.source_conn().and_then(|c| c.schema.clone()),
                },
                target: None,
                sql_file_target: Some(TransferSqlFileTargetVm {
                    database_type: if self.sql_file_dialect == "source" {
                        None
                    } else {
                        Some(self.sql_file_dialect.clone())
                    },
                    encoding: (self.sql_file_encoding != SqlFileEncoding::Utf8)
                        .then_some(self.sql_file_encoding),
                    compression: (self.sql_file_compression != SqlFileCompression::None)
                        .then_some(self.sql_file_compression),
                    database: (!database.is_empty()).then_some(database),
                    schema: (!schema.is_empty()).then_some(schema),
                    ..picked
                }),
                mode: self.mode,
                write_mode: self.write_mode,
                tables: self.tables_to_mappings(),
                options,
            });
        }
        let tgt_conn_id = target_session
            .map(|s| s.db_session_id.clone())
            .unwrap_or_default();
        if tgt_conn_id.is_empty() || self.target_database.is_empty() {
            return None;
        }
        Some(TransferJobVm {
            source: TransferEndpointVm {
                db_session_id: src_conn_id,
                database: self.source_database.clone(),
                schema: self.source_conn().and_then(|c| c.schema.clone()),
            },
            target: Some(TransferEndpointVm {
                db_session_id: tgt_conn_id,
                database: self.target_database.clone(),
                schema: self.target_conn().and_then(|c| c.schema.clone()),
            }),
            sql_file_target: None,
            mode: self.mode,
            write_mode: self.write_mode,
            tables: self.tables_to_mappings(),
            options,
        })
    }

    /// React `refreshEndpointSessions`' guard (React 566-571): whether the
    /// refresh may run at all. The host performs the session IPC.
    pub(crate) fn refresh_sessions_guard(&self) -> bool {
        !self.source_id.is_empty()
            && !self.source_database.is_empty()
            && (self.destination_mode == TransferDestinationMode::SqlFile
                || (!self.target_id.is_empty() && !self.target_database.is_empty()))
    }

    // -- the idempotency keys (React 155-179) ---------------------------------

    /// React `nextPrepareKey` (React 175-178):
    /// `data-transfer/prepare/<scope>/<seq>`, seq pre-incremented.
    pub(crate) fn next_prepare_key(&mut self) -> String {
        self.prepare_seq += 1;
        format!(
            "data-transfer/prepare/{}/{}",
            self.prepare_scope, self.prepare_seq
        )
    }

    /// React `applyKeyForPlan` (React 179):
    /// `data-transfer/apply/<planId>`.
    pub(crate) fn apply_key_for_plan(plan_id: &str) -> String {
        format!("data-transfer/apply/{}", plan_id)
    }

    // -----------------------------------------------------------------------
    // Host-fed applications (the effects' bodies).
    // -----------------------------------------------------------------------

    /// React `loadConnections` effect body (React 202-210): the host feeds
    /// the loaded list; the prefill connection ids apply here (React
    /// 227's `useMigrationEndpointPrefill` effect). Returns whether the
    /// prefill applied.
    pub(crate) fn apply_connections_loaded(
        &mut self,
        connections: Vec<TransferConnectionVm>,
    ) -> bool {
        self.connections = connections;
        if self.endpoint_prefill_applied || self.connections.is_empty() {
            return false;
        }
        if !self.endpoint_prefill.has_endpoint_ids() {
            return false;
        }
        self.endpoint_prefill_applied = true;
        let mut applied = false;
        if let Some(source_id) = self.endpoint_prefill.source_id.clone() {
            if self.connections.iter().any(|c| c.id == source_id) {
                self.source_id = source_id;
                applied = true;
            }
        }
        if let Some(target_id) = self.endpoint_prefill.target_id.clone() {
            if self.connections.iter().any(|c| c.id == target_id) {
                self.target_id = target_id;
                applied = true;
            }
        }
        applied
    }

    /// React `setSourceDatabases` effect body (React 325-355): the host
    /// feeds `listDatabasesDedicated`'s result (or `None` for its catch);
    /// the prefill database leg and the default resolution live here.
    pub(crate) fn apply_source_databases(&mut self, databases: Option<Vec<String>>) {
        let preferred = self
            .source_conn()
            .and_then(|c| c.database.clone())
            .unwrap_or_default();
        let prev = self.source_database.clone();
        let list: Vec<String> = databases.unwrap_or_default();
        self.source_databases = list.clone();
        let list_ref: &[String] = &list;
        self.source_database = pick_prefill_database(
            &mut self.endpoint_prefill,
            true,
            &list,
            |current| resolve_default_database(Some(list_ref), &preferred, current),
            &prev,
        );
    }

    /// React `setTargetDatabases` effect body (React 378-408): the target
    /// twin of [`apply_source_databases`].
    pub(crate) fn apply_target_databases(&mut self, databases: Option<Vec<String>>) {
        let preferred = self
            .target_conn()
            .and_then(|c| c.database.clone())
            .unwrap_or_default();
        let prev = self.target_database.clone();
        let list: Vec<String> = databases.unwrap_or_default();
        self.target_databases = list.clone();
        let list_ref: &[String] = &list;
        self.target_database = pick_prefill_database(
            &mut self.endpoint_prefill,
            false,
            &list,
            |current| resolve_default_database(Some(list_ref), &preferred, current),
            &prev,
        );
    }

    /// React's available-dialects effect body (React 212-225): keep the
    /// SQL drivers whose meta `supportsSQL === true && category === 'sql'`
    /// (the meta's `supportsSQL` gate rides the `Sql` category filter —
    /// every `Sql`-category driver in the table declares it).
    pub(crate) fn set_available_sql_dialects(&mut self, drivers: Vec<String>) {
        self.available_sql_dialects = drivers
            .into_iter()
            .filter(|driver| sync_category(driver) == SyncCategory::Sql)
            .collect();
    }

    /// React `chooseSqlFile` (React 298-309): a picked file switches the
    /// destination to `sqlFile`; the dialog error is the host's.
    pub(crate) fn choose_sql_file(&mut self, picked: Option<TransferSqlFileTargetVm>) {
        if let Some(picked) = picked {
            self.sql_file_target = Some(picked);
            self.destination_mode = TransferDestinationMode::SqlFile;
        }
    }

    /// The `datazen:connection-closed` bus listener's body (React
    /// 230-239): drop the side sessions whose id matches. The host owns
    /// the listener and maps a missing payload to `""` (an empty id is a
    /// no-op by design, per the BackupWindow N4 precedent).
    pub(crate) fn on_connection_closed(&mut self, closed_db_session_id: &str) {
        if closed_db_session_id.is_empty() {
            return;
        }
        if self
            .source_session
            .as_ref()
            .map(|s| s.db_session_id == closed_db_session_id)
            .unwrap_or(false)
        {
            self.source_session = None;
        }
        if self
            .target_session
            .as_ref()
            .map(|s| s.db_session_id == closed_db_session_id)
            .unwrap_or(false)
        {
            self.target_session = None;
        }
    }

    /// React's endpoint-clearing effects (React 326-330, 358-360): an
    /// empty source endpoint clears its databases and session; a source
    /// without a database clears the session only. The host calls these
    /// when its own effect tracking sees the change.
    pub(crate) fn source_endpoint_cleared(&mut self) {
        self.source_databases.clear();
        self.source_session = None;
    }

    pub(crate) fn source_database_cleared(&mut self) {
        self.source_session = None;
    }

    /// React's target twins (React 379-383, 411-413).
    pub(crate) fn target_endpoint_cleared(&mut self) {
        self.target_databases.clear();
        self.target_session = None;
    }

    pub(crate) fn target_database_cleared(&mut self) {
        self.target_session = None;
    }

    // -- the SQL-file config edits (React 1288-1396) --------------------------

    /// React's dialect `onChange` (React 1288-1294): target-native type
    /// overrides are dialect-specific, so the preview and the inspected
    /// tables are dropped to force a fresh source-only inspection.
    pub(crate) fn set_sql_file_dialect(&mut self, dialect: String) {
        self.sql_file_dialect = dialect;
        self.preview = None;
        self.tables.clear();
    }

    /// React's encoding `onChange` (React 1327-1330).
    pub(crate) fn set_sql_file_encoding(&mut self, encoding: SqlFileEncoding) {
        self.sql_file_encoding = encoding;
        self.preview = None;
    }

    /// React's compression `onChange` (React 1350-1353).
    pub(crate) fn set_sql_file_compression(&mut self, compression: SqlFileCompression) {
        self.sql_file_compression = compression;
        self.preview = None;
    }

    /// React's target-database input `onChange` (React 1364-1367).
    pub(crate) fn set_sql_file_database(&mut self, database: String) {
        self.sql_file_database = database;
        self.preview = None;
    }

    /// React's target-schema input `onChange` (React 1379-1382).
    pub(crate) fn set_sql_file_schema(&mut self, schema: String) {
        self.sql_file_schema = schema;
        self.preview = None;
    }

    // -- profiles (React 150-153, 453-558) -------------------------------------

    /// React `loadTransferProfiles` (React 185-190): the host loads; a
    /// failure feeds an empty list.
    pub(crate) fn apply_transfer_profiles(&mut self, profiles: Vec<TransferProfileVm>) {
        self.transfer_profiles = profiles;
    }

    /// React `loadSelectedProfile` (React 526-558), line for line — minus
    /// the job-run reset (`resetJobRun`, S2's field) which the caller
    /// applies after this. Returns the "choose a file" flag so the host
    /// can raise the dialog error exactly as React does (React 554-557).
    pub(crate) fn load_selected_profile(&mut self) -> bool {
        let Some(profile) = self
            .transfer_profiles
            .iter()
            .find(|candidate| candidate.id == self.selected_profile_id)
            .cloned()
        else {
            return false;
        };
        self.profile_name = profile.name.clone();
        self.source_id = profile.source_connection_id.clone();
        self.target_id = profile.target_connection_id.clone().unwrap_or_default();
        self.source_database = profile.source_database.clone().unwrap_or_default();
        self.target_database = profile.target_database.clone().unwrap_or_default();
        self.mode = profile.mode;
        self.write_mode = profile.write_mode;
        self.batch_size_edit = profile
            .options
            .batch_size
            .or(DEFAULT_TRANSFER_OPTIONS.batch_size)
            .map(|batch| batch.to_string())
            .unwrap_or_else(|| "500".to_owned());
        self.stop_on_error = profile.options.stop_on_error;
        self.confirmed_destructive = profile.options.confirmed_destructive;
        self.use_target_default_collation = profile.options.use_target_default_collation;
        self.destination_mode = profile.destination_mode;
        self.sql_file_dialect = profile
            .sql_file_dialect
            .clone()
            .unwrap_or_else(|| "source".to_owned());
        self.sql_file_encoding = profile.sql_file_encoding.unwrap_or(SqlFileEncoding::Utf8);
        self.sql_file_compression = profile
            .sql_file_compression
            .unwrap_or(SqlFileCompression::None);
        self.sql_file_database = profile.sql_file_database.clone().unwrap_or_default();
        self.sql_file_schema = profile.sql_file_schema.clone().unwrap_or_default();
        self.sql_file_target = None;
        self.profile_mappings = Some(profile.tables.clone());
        self.tables.clear();
        self.preview = None;
        // A different profile is a different plan: any run state from the
        // previous one must go with it (the S2 reset the caller applies,
        // React 550-552). The step always returns to endpoints (React
        // 553).
        self.step = WizardStep::Endpoints;
        profile.destination_mode == TransferDestinationMode::SqlFile
    }

    /// React `saveCurrentProfile`'s missing-fields refusal (React 454-464)
    /// — the pure check; the host raises the dialog error when this
    /// returns `false`.
    pub(crate) fn profile_missing_fields(&self) -> bool {
        self.profile_name.trim().is_empty()
            || self.source_id.is_empty()
            || self.source_database.is_empty()
            || (self.destination_mode == TransferDestinationMode::Database
                && (self.target_id.is_empty() || self.target_database.is_empty()))
    }

    /// React `saveCurrentProfile`'s payload assembly (React 465-489). The
    /// host performs the save with its own id mint; `now` is the host's
    /// clock (React 465).
    pub(crate) fn profile_save_payload(&self, now: &str, profile_id: &str) -> TransferProfileVm {
        let existing = self
            .transfer_profiles
            .iter()
            .find(|profile| profile.id == self.selected_profile_id);
        let database_mode = self.destination_mode == TransferDestinationMode::Database;
        let sql_file_mode = self.destination_mode == TransferDestinationMode::SqlFile;
        TransferProfileVm {
            version: 1,
            id: existing
                .map(|profile| profile.id.clone())
                .unwrap_or_else(|| profile_id.to_owned()),
            name: self.profile_name.trim().to_owned(),
            source_connection_id: self.source_id.clone(),
            target_connection_id: database_mode.then(|| self.target_id.clone()),
            source_database: Some(self.source_database.clone()),
            target_database: database_mode.then(|| self.target_database.clone()),
            source_schema: self.source_conn().and_then(|c| c.schema.clone()),
            target_schema: if database_mode {
                self.target_conn().and_then(|c| c.schema.clone())
            } else {
                None
            },
            destination_mode: self.destination_mode,
            sql_file_dialect: sql_file_mode.then(|| self.sql_file_dialect.clone()),
            sql_file_encoding: sql_file_mode.then_some(self.sql_file_encoding),
            sql_file_compression: sql_file_mode.then_some(self.sql_file_compression),
            sql_file_database: sql_file_mode
                .then(|| {
                    let database = self.sql_file_database.clone();
                    (!database.is_empty()).then_some(database)
                })
                .flatten(),
            sql_file_schema: sql_file_mode
                .then(|| {
                    let schema = self.sql_file_schema.clone();
                    (!schema.is_empty()).then_some(schema)
                })
                .flatten(),
            mode: self.mode,
            write_mode: self.write_mode,
            tables: self.all_tables_to_mappings(),
            options: TransferOptionsVm {
                batch_size: self.batch_size(),
                stop_on_error: self.stop_on_error,
                confirmed_destructive: self.confirmed_destructive,
                use_target_default_collation: self.use_target_default_collation,
            },
            created_at: existing
                .map(|profile| profile.created_at.clone())
                .unwrap_or_else(|| now.to_owned()),
            updated_at: now.to_owned(),
        }
    }

    /// The save-success application (React 490-494): select the saved id,
    /// show its name; the host reloads the profile list.
    pub(crate) fn profile_saved(&mut self, profile: &TransferProfileVm) {
        self.selected_profile_id = profile.id.clone();
        self.profile_name = profile.name.clone();
    }

    // -- the table reducers (React 560-564, 967-1042) ---------------------------

    /// React `updateTableDdlOverride` (React 560-564).
    pub(crate) fn update_table_ddl_override(&mut self, source_table: &str, ddl: String) {
        for row in &mut self.tables {
            if row.vm.source_table == source_table {
                row.ddl_override = Some(ddl.clone());
            }
        }
    }

    /// React `updateTable` (React 967-983): mapping edits — including a
    /// recordset change — invalidate the opaque server preview immediately
    /// (React 969-970: `setPreview(null)`) and clear the gate error the
    /// user is re-answering (React 972).
    pub(crate) fn update_table(&mut self, source_table: &str, patch: &TransferTablePatch) {
        self.preview = None;
        self.mapping_gate_error.clear();
        for row in &mut self.tables {
            if row.vm.source_table == source_table {
                patch.apply_to(row);
            }
        }
    }

    /// React `toggleTable` (React 1036-1042) — a toggle does not
    /// invalidate the preview.
    pub(crate) fn toggle_table(&mut self, source_table: &str) {
        for row in &mut self.tables {
            if row.vm.source_table == source_table {
                row.vm.enabled = !row.vm.enabled;
            }
        }
    }

    /// React `runInspect`'s success application (React 674-707): keep the
    /// rows with a source name, normalize their mappings, spend the
    /// profile stash, and re-pick the selected mapping table (prev kept
    /// only while still present; else the first enabled row, else the
    /// first row).
    pub(crate) fn apply_inspect_rows(&mut self, rows: Vec<TransferTableRow>) {
        let enabled: Vec<TransferTableRow> = rows
            .into_iter()
            .filter(|row| !row.vm.source_table.is_empty())
            .map(|mut row| {
                row.vm.column_mappings = normalize_column_mappings(&row.vm);
                row
            })
            .collect();
        self.tables = enabled;
        if !self.tables.is_empty() {
            let prev = self.selected_mapping_table.clone();
            self.selected_mapping_table = if !prev.is_empty()
                && self.tables.iter().any(|row| row.vm.source_table == prev)
            {
                prev
            } else {
                self.tables
                    .iter()
                    .find(|row| row.vm.enabled)
                    .map(|row| row.vm.source_table.clone())
                    .unwrap_or_else(|| self.tables[0].vm.source_table.clone())
            };
        }
        self.profile_mappings = None;
    }

    /// React `refreshTableMapping`'s merge (React 1005-1021): a refresh
    /// only touches the server-known fields (`status`, `targetColumns`,
    /// `sourceColumns`, `incompatibleReason`); local edits (`createNew`,
    /// `targetTable`, `columnMappings`) survive. An absent response row
    /// or a failed refresh leaves the table untouched (React 1009,
    /// 1022-1024).
    pub(crate) fn apply_table_refresh(&mut self, source_table: &str, inspected: Option<&TransferTableRow>) {
        let Some(inspected) = inspected else {
            return;
        };
        for row in &mut self.tables {
            if row.vm.source_table != source_table {
                continue;
            }
            row.vm.status = inspected.vm.status;
            row.vm.target_columns = inspected.vm.target_columns.clone();
            row.vm.source_columns = inspected.vm.source_columns.clone();
            row.incompatible_reason = inspected.incompatible_reason.clone();
        }
    }

    // -- inspect / preview reducers (React 659-771) ------------------------------

    /// React `runInspect`'s head (React 659-671): the refusal message when
    /// the endpoints are not both picked (the host raises the dialog).
    pub(crate) fn inspect_missing_endpoints(&self) -> bool {
        !(self.source_session.is_some()
            && !self.source_database.is_empty()
            && (self.destination_mode == TransferDestinationMode::SqlFile
                || (self.target_session.is_some() && !self.target_database.is_empty())))
    }

    /// React `runInspect`'s busy/error brackets (React 672-713): `loading`
    /// around the round trip, the error dialog on a refusal.
    pub(crate) fn begin_inspect(&mut self) {
        self.loading = true;
    }

    pub(crate) fn finish_inspect(&mut self) {
        self.loading = false;
    }

    /// React `runPreview` (React 725-771), the reducer half. The host
    /// performs the prepare; the refusals classify in S2.
    pub(crate) fn begin_preview(&mut self) {
        self.preview_error.clear();
        self.mapping_gate_error.clear();
        self.loading = true;
    }

    pub(crate) fn finish_preview(&mut self, review: TransferPreviewVm) {
        self.preview = Some(review);
        self.loading = false;
    }

    /// React `runPreview`'s catch (React 757-765): the preview is dropped,
    /// the message recorded, and — unless `quiet` — the error dialog
    /// opens.
    pub(crate) fn fail_preview(&mut self, message: &str, quiet: bool) {
        self.preview = None;
        self.preview_error = message.to_owned();
        if !quiet {
            self.error_msg = message.to_owned();
            self.error_open = true;
        }
        self.loading = false;
    }

    /// React `runPreview`'s non-advance path (React 744-754): the prepare
    /// hook classified the refusal and re-rendered with `failure`; the
    /// wizard must not advance past a plan the backend declined to admit.
    /// The backend's own words are read synchronously into the preview
    /// error surface.
    pub(crate) fn preview_refused(&mut self, detail: &str) {
        self.preview_error = detail.to_owned();
        self.loading = false;
    }

    // -- execute (React 773-845) ------------------------------------------------

    /// React `runExecute`'s readOnly refusal (React 777-780) — the pure
    /// check; the host raises the dialog error.
    pub(crate) fn execute_read_only_blocked(&self) -> bool {
        self.destination_mode == TransferDestinationMode::Database && self.target_read_only()
    }

    /// React `runExecute`'s stale-plan refusal (React 785-792): the plan
    /// must come from `prepare`. The host raises the dialog error, drops
    /// the preview and steps back to mapping.
    pub(crate) fn execute_stale_plan(&self) -> bool {
        self.preview.is_none()
    }

    /// React `runExecute`'s progress line (React 794-798): the table count
    /// — the job's enabled tables when rows were inspected, else the
    /// plan's own size.
    pub(crate) fn execute_table_count(&self, job: &TransferJobVm) -> usize {
        if !self.tables.is_empty() {
            job.tables
                .iter()
                .filter(|tbl| tbl.enabled.unwrap_or(true))
                .count()
        } else {
            let write_plans = self
                .preview
                .as_ref()
                .map(|preview| preview.write_plans.len())
                .unwrap_or(0);
            let ddl = self
                .preview
                .as_ref()
                .map(|preview| preview.ddl.len())
                .unwrap_or(0);
            write_plans.max(ddl)
        }
    }

    /// React `runExecute`'s source-tables selection (React 799-805): an
    /// absent selection means "every table the plan froze" (the SQL-file
    /// case); an explicit list would disable every table.
    pub(crate) fn execute_source_tables(&self, job: &TransferJobVm) -> Option<Vec<String>> {
        if self.destination_mode == TransferDestinationMode::SqlFile && self.tables.is_empty() {
            None
        } else {
            Some(
                job.tables
                    .iter()
                    .filter(|table| table.enabled.unwrap_or(true))
                    .map(|table| table.source_table.clone())
                    .collect(),
            )
        }
    }

    /// React `handleExecuteClick`'s gate (React 834-840): a destructive
    /// write mode routes through the confirm dialog first.
    pub(crate) fn handle_execute_click_confirm(&self) -> bool {
        self.write_mode != WriteMode::Insert
    }

    /// React `handleExecuteConfirm` (React 842-845) — the dialog's
    /// confirmation closes it; the host then runs the execute.
    pub(crate) fn handle_execute_confirm(&mut self) {
        self.execute_confirm_open = false;
    }

    // -- the step arithmetic (React 879-965) --------------------------------------

    /// React `goBack` (React 962-965).
    pub(crate) fn go_back(&mut self) {
        let index = self.step.index();
        if index > 0 {
            self.step = STEPS[index - 1];
        }
    }

    /// React `goNext`'s step arithmetic (React 935-960) — the pure half.
    /// The mapping→preview round trip (the prepare and the gate re-check
    /// on the post-await rows) is the S2 host contract:
    /// [`go_next_after_mapping_gate`] re-decides the gate against the
    /// rows that exist *after* the prepare.
    pub(crate) fn go_next_target(&self) -> Option<WizardStep> {
        STEPS.get(self.step.index() + 1).copied()
    }

    /// React `goNext`'s mapping→preview gate (React 949-953): re-decided
    /// against the rows that exist *after* the prepare round trip. A lost
    /// gate records the i18n message (React 950) and stays on mapping.
    pub(crate) fn go_next_after_mapping_gate(&mut self, rows: &[TransferTableRow]) -> bool {
        let vms: Vec<TransferTableVm> = rows.iter().map(|row| row.vm.clone()).collect();
        if mapping_gate_allows_advance(&vms) {
            true
        } else {
            self.mapping_gate_error = t("transfer.mapping.gateLost").to_owned();
            false
        }
    }

    /// React `goNext`'s objects leg (React 956-958): stepping into objects
    /// with no rows inspects first — the host performs the inspect; this
    /// reports whether it must.
    pub(crate) fn go_next_needs_inspect(&self) -> bool {
        self.go_next_target() == Some(WizardStep::Objects) && self.tables.is_empty()
    }

    /// React `if (next) setStep(next)` (React 959).
    pub(crate) fn advance_step(&mut self) {
        if let Some(next) = self.go_next_target() {
            self.step = next;
        }
    }

    /// The dialog surfaces (React 147-148, 196-200).
    pub(crate) fn set_limitations_open(&mut self, open: bool) {
        self.limitations_open = open;
    }

    pub(crate) fn set_execute_confirm_open(&mut self, open: bool) {
        self.execute_confirm_open = open;
    }

    /// React's error dialog (React 142-143, 1881-1897).
    pub(crate) fn raise_error(&mut self, message: &str) {
        self.error_msg = message.to_owned();
        self.error_open = true;
    }

    pub(crate) fn dismiss_error(&mut self) {
        self.error_open = false;
    }

    // -----------------------------------------------------------------------
    // The Job run wiring (React 773-877) — the window-side halves of the
    // hook's callbacks; the host performs the IPC hops through
    // [`TransferJobRunVm`]'s own methods.
    // -----------------------------------------------------------------------

    /// React `reconcilePending` (React 933): a re-review may only be
    /// offered while nothing is pending reconcile. A `null` verdict means
    /// no Job settled at all (a refusal before any write), so there is no
    /// half-written range to protect and the affordance stays open.
    pub(crate) fn reconcile_pending(&self) -> bool {
        self.job_run
            .verdict()
            .map(|verdict| verdict.requires_reconcile)
            .unwrap_or(false)
    }

    /// React `handleCancel`'s decision half (React 847-865): the host reads
    /// the phase *before* the request (the apply-in-flight flag), performs
    /// the cancel, and applies the answer here. A cancel the backend
    /// acknowledged while the plan was merely prepared has disposed that
    /// plan: no write was ever in flight, so there is nothing to show a
    /// verdict for. Dropping it back to mapping is the honest outcome —
    /// the next review mints a fresh planId. A cancel that could not be
    /// addressed changes nothing and must leave the review untouched.
    pub(crate) fn handle_cancel(&mut self, was_applying: bool, acknowledged: bool) {
        if acknowledged && !was_applying {
            self.job_run.reset();
            self.preview = None;
            self.step = WizardStep::Mapping;
            self.execute_progress.clear();
        }
    }

    /// React `handleReReviewPlan`'s full body (React 872-877) — the run
    /// reset joins the state reset now that the machine is a field.
    pub(crate) fn handle_re_review_plan(&mut self) {
        self.job_run.reset();
        self.preview = None;
        self.step = WizardStep::Mapping;
        self.execute_progress.clear();
    }

    /// React `runExecute`'s executing brackets (React 793, 816-819): the
    /// progress line is armed with the table count, and the outcome's step
    /// transition — a settled apply steps to `result`; a refusal (scope,
    /// budget, consumed plan) stays on `preview` so the fail-closed
    /// notices are shown where the user still has the review in front of
    /// them (React 813-815).
    pub(crate) fn begin_execute(&mut self, table_count: usize) {
        self.executing = true;
        // React 798: `t('transfer.executingProgress', { count })`.
        self.execute_progress = t("transfer.executingProgress").replace("{count}", &table_count.to_string());
    }

    pub(crate) fn finish_execute(&mut self, settled: bool) {
        self.executing = false;
        self.execute_progress.clear();
        self.step = if settled {
            WizardStep::Result
        } else {
            WizardStep::Preview
        };
    }

    /// React `goNext`'s mapping→preview round trip brackets (React
    /// 942-943): the editor is disabled for the whole round trip
    /// (`preparing`), and the decision is re-made against the rows that
    /// exist *after* it (the host calls
    /// [`go_next_after_mapping_gate`] with the post-await rows).
    pub(crate) fn begin_go_next_prepare(&mut self) {
        self.preparing = true;
    }

    pub(crate) fn finish_go_next_prepare(&mut self) {
        self.preparing = false;
    }

    /// The prepare refusal path of `goNext` (React 944-947): prepare
    /// itself refused; the preview step owns that error surface.
    pub(crate) fn go_next_prepare_refused(&mut self) {
        self.preparing = false;
        self.step = WizardStep::Preview;
    }

    /// `loadSelectedProfile`'s run reset (React 550-552): a different
    /// profile is a different plan; any run state from the previous one
    /// must go with it, or a spent planId would look reusable.
    pub(crate) fn profile_run_reset(&mut self) {
        self.job_run.reset();
    }

    /// `runPreview`'s non-advance detail read (React 751-753): the
    /// backend's own words, read synchronously from the run machine's
    /// same-tick mirror.
    pub(crate) fn last_run_failure_message(&self) -> Option<&str> {
        self.job_run.last_failure().map(|failure| failure.message.as_str())
    }

    /// The window's own `preparing`/`executing`/`loading` gating the
    /// footer's Next button honours (React 1801, 1851-1852): back is
    /// disabled while executing; next while loading.
    pub(crate) fn back_disabled(&self) -> bool {
        self.step.index() == 0 || self.executing
    }

    pub(crate) fn next_disabled(&self) -> bool {
        !self.can_next() || self.loading
    }

    /// The run phase the window surfaces (the footer's cancel buttons and
    /// the pending-id notice key off it, React 1767-1807).
    pub(crate) fn run_phase(&self) -> TransferRunPhase {
        self.job_run.phase
    }
}
