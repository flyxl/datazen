//! T026-S1: the Backup window's state model and selection logic.
//!
//! Ported from `packages/frontend/src/windows/backup/BackupWindow.tsx`
//! (564 lines). This slice owns the view model — every `useState` field —
//! plus the pure derivations and the synchronous head of connection
//! selection. It renders nothing except the empty keyed mount for the
//! `ProgressLog` child (see [`progress_log_mount`]).
//!
//! ## S1 / S2 boundary
//!
//! | piece | slice | where |
//! |---|---|---|
//! | `BackupWindowVm` and all state fields | S1 | this module |
//! | `DatabaseInfo`, `BackupConnection`, `BackupOption`, `BackupProgress` | S1 | this module |
//! | `backupOptions` derivation | S1 | [`BackupWindowVm::backup_options`] |
//! | `grouped` grouping + search ranking | S1 | [`super::backup_grouping::group_connections`], [`super::backup_grouping::rank_connections`] |
//! | `filteredDbs` | S1 | [`BackupWindowVm::filtered_dbs`] |
//! | `toggleGroup` / `toggleOption` | S1 | [`BackupWindowVm::toggle_group`], [`BackupWindowVm::toggle_option`] |
//! | `dbTypeDisplayLabel` | S1 | [`BackupWindowVm::db_type_display_label`] |
//! | file-name / extension derivation | S1 | [`BackupWindowVm::default_file_name`], [`BackupWindowVm::file_extension`] |
//! | `handleSelectConnection` wiring (reset, unsupported-type branch, session apply) | S1 | [`BackupWindowVm::begin_select_connection`], [`BackupWindowVm::apply_session_connected`] |
//! | URL prefill (`connectionId` / `database`) | S1 | [`BackupWindowVm::take_prefill`] |
//! | `datazen:connection-closed` listener | S1 | [`BackupWindowVm::on_connection_closed`] |
//! | start-button enablement (pure predicate, no button) | S1 | [`BackupWindowVm::can_start`] |
//! | `handleBackup` / `handleRestore` execution | S2 | — |
//! | three-column rendering, `WindowChromeSlot`, start button | S2 | — |
//! | `ProgressLog` component | own task | empty mount [`progress_log_mount`] below |
//!
//! The async middle of `handleSelectConnection` — the `connect` invoke,
//! the best-effort `get_connection_info`, and `schemaClient.listDatabases`
//! — lives on the host: [`BackupWindowVm::begin_select_connection`]
//! returns [`SelectConnectionOutcome::NeedsConnect`] with the connection id,
//! and the host feeds the fetched values back through
//! [`BackupWindowVm::apply_session_connected`] (or
//! [`BackupWindowVm::apply_select_error`] on failure). The Vm never
//! performs IPC itself, so every transition here is a pure function of
//! its inputs and is testable without a runtime.
//!
//! ## `ProgressLog` mount
//!
//! `ProgressLog` (`./ProgressLog`) is a separate component with its own
//! port task. Until then this slice exposes [`progress_log_mount`]: an
//! empty keyed `col` (`backup-progress-log`, internal key — React puts no
//! `data-testid` on it) that S2 places in column 3 where React renders
//! `<ProgressLog lines={progressLog} />` (or the `flex-1` spacer while
//! the log is empty). The mount carries the `flex-1` geometry so the
//! column keeps its height before the child fills it.
//!
//! ## Documented deviations from React
//!
//! * **Driver registry is a static table.** React reads `DB_REGISTRY`
//!   (driver-injected `label` / `supportsBackup` / `sqlDialect` /
//!   `sqlDialectStrategy`) and `getSqlDialect`. The Rust crate has no
//!   registry, so `db_type_meta` (in `backup_db_meta`) mirrors the shipped driver metas
//!   (`crates/drivers/*/ui/meta.ts`) as a match table. An unknown type
//!   behaves exactly like React's missing entry: label falls back to the
//!   type id, `supportsBackup` is false, and the dialect is absent, so
//!   [`super::backup_db_meta::backup_options_for`] returns `[]`. A new driver type needs one
//!   row here.
//! * **`enabledOptions` / `expandedGroups` are `BTreeSet`s.** React
//!   `Set`s preserve insertion order; the Rust sets iterate sorted, so
//!   S2 renders the selected-option chips in dialect order
//!   (`backup_options` filtered by membership) rather than React's
//!   toggle-insertion order. Membership semantics are identical.
//! * **Search ranking mirrors `rankConnections` with empty usage state.**
//!   The Backup window calls `groupConnections` with no usage state, so
//!   schema / table / view / object / namespace / path candidates never
//!   occur; [`super::backup_grouping::connection_match`] only scores name, host, database,
//!   database type, and the type label. The rank offsets (0 / 10 / 20 /
//!   30), the exact / prefix / contains steps (+0 / +1 / +2), and the
//!   `compareResults` tie-breaks (match rank, pinned, `lastConnectedAt`
//!   desc with nulls last, group, name, id) are copied verbatim.
//! * **`localeCompare` is approximated by lowercased `Ord`.** React
//!   compares group / name / context with `localeCompare` (base
//!   sensitivity: case- and accent-insensitive). The Rust mirror
//!   lowercases both sides and uses byte order, which agrees on the
//!   ASCII data this window ranks; the `id` tie-break uses plain `Ord`,
//!   matching `localeCompare` on ASCII ids.
//! * **`lastConnectedAt` parsing is ISO-8601 only.** React's
//!   `Date.parse` accepts any date string; the shared connection timestamp parser
//!   accepts `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SS` with an optional
//!   fractional part and a `Z` or `±HH:MM` offset — the shapes the app
//!   stores — and returns `None` (sorts last, like React's `null`) for
//!   anything else.
//! * **`format-custom` is kept even though no dialect offers it.**
//!   `PG_OPTIONS` removed `format-custom`, but `handleBackup` still
//!   maps it to the `.dump` extension; [`BackupWindowVm::file_extension`]
//!   mirrors that branch so a manually-enabled option still behaves
//!   the same.

// The production consumer is the Backup window (T026-S2); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::collections::BTreeSet;

use rune::{col, El};

use crate::i18n::t;

use super::backup_db_meta::{backup_options_for, db_label, db_supports_backup, BackupOption};
use super::backup_grouping::{group_connections, initial_expanded_groups, ConnectionGroup};
/// React `DatabaseInfo` (`BackupWindow.tsx:30-32`): one database name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseInfo {
    pub name: String,
}

/// The `ConnectionConfig` fields the Backup window reads
/// (`BackupWindow.tsx`, `connectionStore.ts` `groupConnections`, and
/// `connectionLocator.ts` `rankConnections`).
///
/// The full `ConnectionConfig` (credentials, tunnels, SSL, …) is host
/// data; the window only needs identity, display, grouping, and the
/// search/ranking inputs, so only those travel here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupConnection {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub host: Option<String>,
    pub database: Option<String>,
    pub group: Option<String>,
    pub pinned: bool,
    pub last_connected_at: Option<String>,
}

/// React `BackupDumpPhase` (`lib/backupProgress.ts:3`):
/// `'object' | 'writing' | 'done'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackupProgressPhase {
    #[default]
    Object,
    Writing,
    Done,
}

/// React `BackupProgressPayload` (`lib/backupProgress.ts:5-10`).
/// `formatBackupProgress` / `backupProgressRatio` live with S2, which
/// renders the status line and the progress bar; S1 only carries the
/// shape the run state sets and clears.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupProgress {
    pub current: u64,
    pub total: u64,
    pub object_name: String,
    pub phase: BackupProgressPhase,
}

/// What [`BackupWindowVm::begin_select_connection`] needs the host to
/// do next. `UnsupportedType` means the Vm already set the status
/// message to `t('backup.unsupportedType')` and cleared the session;
/// `NeedsConnect` means the host must run the `connect` invoke (then
/// `get_connection_info` and `listDatabases`) and feed the result back
/// through [`BackupWindowVm::apply_session_connected`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectConnectionOutcome {
    UnsupportedType,
    NeedsConnect { connection_id: String },
}

/// The values the host feeds back after `connect` +
/// `get_connection_info` + `schemaClient.listDatabases`.
///
/// `default_database` is `preferredDatabase ?? conn.database`,
/// resolved by the caller (React's `dbPick`); an empty string or a
/// name missing from `databases` selects nothing, matching React's
/// `if (dbPick && dbs.includes(dbPick))`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionConnected {
    pub db_session_id: String,
    pub server_version: Option<String>,
    pub databases: Vec<String>,
    pub default_database: Option<String>,
}

/// The URL-prefill target [`BackupWindowVm::take_prefill`] hands the
/// host: the connection to select and the preferred database
/// (`database` URL param, if any).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefillTarget {
    pub connection: BackupConnection,
    pub preferred_database: Option<String>,
}

/// The Backup window's controlled state: every `useState` field of
/// `BackupWindow.tsx`, plus `prefill_applied` for the `prefillAppliedRef`
/// guard. `is_restore` is `getUrlParam('mode') === 'restore'`, read by
/// the host and passed to [`BackupWindowVm::new`].
#[derive(Clone, Debug)]
pub struct BackupWindowVm {
    pub is_restore: bool,
    pub connections: Vec<BackupConnection>,
    pub groups: Vec<String>,
    pub expanded_groups: BTreeSet<String>,
    pub selected_conn_id: Option<String>,
    pub db_session_id: Option<String>,
    pub server_version: String,
    pub databases: Vec<DatabaseInfo>,
    pub selected_db: Option<String>,
    pub enabled_options: BTreeSet<String>,
    pub compress_gzip: bool,
    pub file_name: String,
    pub option_dropdown_open: bool,
    pub backing: bool,
    pub status_message: String,
    pub progress: Option<BackupProgress>,
    pub progress_log: Vec<String>,
    pub search_conn: String,
    pub search_db: String,
    pub prefill_applied: bool,
}

impl Default for BackupWindowVm {
    fn default() -> Self {
        Self {
            is_restore: false,
            connections: Vec::new(),
            groups: Vec::new(),
            expanded_groups: BTreeSet::new(),
            selected_conn_id: None,
            db_session_id: None,
            server_version: String::new(),
            databases: Vec::new(),
            selected_db: None,
            enabled_options: BTreeSet::new(),
            compress_gzip: false,
            file_name: "untitled".to_owned(),
            option_dropdown_open: false,
            backing: false,
            status_message: String::new(),
            progress: None,
            progress_log: Vec::new(),
            search_conn: String::new(),
            search_db: String::new(),
            prefill_applied: false,
        }
    }
}

impl BackupWindowVm {
    pub fn new(is_restore: bool) -> Self {
        Self {
            is_restore,
            ..Self::default()
        }
    }

    /// React's `selectedConn` memo: the connection matching
    /// `selectedConnId`, or `None`.
    pub fn selected_conn(&self) -> Option<&BackupConnection> {
        let id = self.selected_conn_id.as_deref()?;
        self.connections.iter().find(|c| c.id == id)
    }

    /// React's `backupOptions` memo: `[]` without a selected
    /// connection, otherwise the selected type's dialect options.
    pub fn backup_options(&self) -> &'static [BackupOption] {
        match self.selected_conn() {
            Some(conn) => backup_options_for(&conn.database_type),
            None => &[],
        }
    }

    /// React's `grouped` memo: [`super::backup_grouping::group_connections`] over the current
    /// connections, groups, and connection search.
    pub fn grouped(&self) -> Vec<ConnectionGroup> {
        group_connections(&self.connections, &self.groups, &self.search_conn)
    }

    /// React's `filteredDbs` memo: a trimmed, case-insensitive
    /// substring match on the database name; an empty query returns
    /// every loaded database.
    pub fn filtered_dbs(&self) -> Vec<&DatabaseInfo> {
        let q = self.search_db.trim().to_lowercase();
        if q.is_empty() {
            return self.databases.iter().collect();
        }
        self.databases
            .iter()
            .filter(|d| d.name.to_lowercase().contains(&q))
            .collect()
    }

    /// React's `dbTypeDisplayLabel`: the type label, plus the server
    /// version when one was reported; `""` without a selected
    /// connection.
    pub fn db_type_display_label(&self) -> String {
        let Some(conn) = self.selected_conn() else {
            return String::new();
        };
        let label = db_label(&conn.database_type);
        if self.server_version.is_empty() {
            label
        } else {
            format!("{label} {}", self.server_version)
        }
    }

    /// React's `toggleGroup`: flip one group in `expandedGroups`.
    pub fn toggle_group(&mut self, group: &str) {
        if !self.expanded_groups.remove(group) {
            self.expanded_groups.insert(group.to_owned());
        }
    }

    /// React's `toggleOption`: flip one option id in `enabledOptions`.
    pub fn toggle_option(&mut self, option_id: &str) {
        if !self.enabled_options.remove(option_id) {
            self.enabled_options.insert(option_id.to_owned());
        }
    }

    /// The `handleBackup` extension rule: gzip wins, then the
    /// (dialect-removed but still honored) `format-custom` option
    /// means `.dump`, otherwise `.sql`.
    pub fn file_extension(&self) -> &'static str {
        if self.compress_gzip {
            "gz"
        } else if self.enabled_options.contains("format-custom") {
            "dump"
        } else {
            "sql"
        }
    }

    /// The `handleBackup` default file name: `{fileName}.sql.gz` when
    /// gzipped, otherwise `{fileName}.{extension}`.
    pub fn default_file_name(&self) -> String {
        if self.compress_gzip {
            format!("{}.sql.gz", self.file_name)
        } else {
            format!("{}.{}", self.file_name, self.file_extension())
        }
    }

    /// The start button's `disabled` condition, as a pure predicate
    /// for S2: a live session and a selected database, and no run in
    /// progress.
    pub fn can_start(&self) -> bool {
        self.db_session_id.is_some() && self.selected_db.is_some() && !self.backing
    }

    /// The `get_connections` / `get_groups` load effect: store the
    /// lists and expand every group (see [`super::backup_grouping::initial_expanded_groups`]).
    pub fn apply_connections_loaded(
        &mut self,
        connections: Vec<BackupConnection>,
        groups: Vec<String>,
    ) {
        self.expanded_groups = initial_expanded_groups(&groups, &connections);
        self.connections = connections;
        self.groups = groups;
    }

    /// The synchronous head of `handleSelectConnection`: reset the
    /// per-connection state (selection, databases, server version,
    /// options, status, progress). For a type without backup support
    /// the session is cleared, the status becomes
    /// `t('backup.unsupportedType')`, and the host must not connect.
    /// Otherwise the host must connect and feed the session back
    /// through [`BackupWindowVm::apply_session_connected`].
    pub fn begin_select_connection(&mut self, conn: &BackupConnection) -> SelectConnectionOutcome {
        self.selected_conn_id = Some(conn.id.clone());
        self.selected_db = None;
        self.databases.clear();
        self.server_version.clear();
        self.enabled_options.clear();
        self.status_message.clear();
        self.progress = None;
        if !db_supports_backup(&conn.database_type) {
            self.db_session_id = None;
            self.status_message = t("backup.unsupportedType").to_owned();
            return SelectConnectionOutcome::UnsupportedType;
        }
        SelectConnectionOutcome::NeedsConnect {
            connection_id: conn.id.clone(),
        }
    }

    /// The tail of `handleSelectConnection`, fed with the host's
    /// fetched values: store the session, keep a non-empty server
    /// version (`get_connection_info` is best-effort — a missing or
    /// empty version leaves the previous one, which the reset cleared
    /// to `""`), load the database list, pick the default database
    /// when it is among them, and enable the dialect's default
    /// options (`routines` / `triggers` when the dialect offers them).
    pub fn apply_session_connected(&mut self, session: SessionConnected) {
        self.db_session_id = Some(session.db_session_id);
        if let Some(version) = session.server_version {
            if !version.is_empty() {
                self.server_version = version;
            }
        }
        self.databases = session
            .databases
            .into_iter()
            .map(|name| DatabaseInfo { name })
            .collect();
        if let Some(pick) = session.default_database {
            if !pick.is_empty() && self.databases.iter().any(|d| d.name == pick) {
                self.selected_db = Some(pick);
            }
        }
        let db_type = self.selected_conn().map(|c| c.database_type.clone());
        if let Some(db_type) = db_type {
            for opt in backup_options_for(&db_type) {
                if opt.id == "routines" || opt.id == "triggers" {
                    self.enabled_options.insert(opt.id.to_owned());
                }
            }
        }
    }

    /// The `handleSelectConnection` catch branch: the connect /
    /// list-databases error becomes the status message.
    pub fn apply_select_error(&mut self, message: &str) {
        self.status_message = message.to_owned();
    }

    /// The `datazen:connection-closed` listener: a close for the
    /// current session clears it; any other session id (or an empty
    /// payload) leaves the state alone.
    pub fn on_connection_closed(&mut self, closed_db_session_id: &str) {
        if closed_db_session_id.is_empty() {
            return;
        }
        if self.db_session_id.as_deref() == Some(closed_db_session_id) {
            self.db_session_id = None;
        }
    }

    /// The URL-prefill effect (`connectionId` / `database` params).
    /// Returns the connection to select the first time the params
    /// resolve — and marks the prefill applied, so a second call
    /// returns `None`. When the params are absent, the connection
    /// list is still empty, or the id is unknown, it returns `None`
    /// *without* marking, so a later connections load retries —
    /// exactly like React's early returns before
    /// `prefillAppliedRef.current = true`.
    pub fn take_prefill(
        &mut self,
        connection_id: Option<&str>,
        database: Option<&str>,
    ) -> Option<PrefillTarget> {
        if self.prefill_applied || self.connections.is_empty() {
            return None;
        }
        let id = connection_id?;
        let connection = self.connections.iter().find(|c| c.id == id)?.clone();
        self.prefill_applied = true;
        Some(PrefillTarget {
            connection,
            preferred_database: database.map(str::to_owned),
        })
    }
}

/// The empty mount for the `ProgressLog` child (own task): S2 places
/// this where React renders `<ProgressLog lines={progressLog} />` (or
/// the `flex-1` spacer while the log is empty). The key is internal —
/// React puts no `data-testid` on the log — and the mount keeps the
/// column's `flex-1` geometry until the child fills it.
pub fn progress_log_mount() -> El {
    col()
        .key("backup-progress-log")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
}
