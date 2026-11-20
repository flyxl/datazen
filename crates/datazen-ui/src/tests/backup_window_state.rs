//! T026-S1: `BackupWindowVm` state model + selection logic
//! (`BackupWindow.tsx`).
//!
//! Pure-logic tests (no runtime except the `ProgressLog` mount probe):
//! the dialect option tables, the `supportsBackup` gate, the select /
//! session-apply journey, grouping + search ranking, database filtering,
//! file-name derivation, URL prefill, and the cross-window close
//! listener. Every assertion is checked against the React source named
//! in the module docs.

use rune::{Cx, El, Env, Runtime};

use crate::i18n::{has, t};
use crate::views::{
    backup_options_for, connection_match, db_label, db_supports_backup, format_group_label,
    group_connections, group_display_label, initial_expanded_groups, progress_log_mount,
    rank_connections, BackupConnection, BackupProgressPhase, BackupWindowVm, ConnectionMatch,
    MatchReason, SelectConnectionOutcome, SessionConnected,
};

fn conn(id: &str, name: &str, db_type: &str) -> BackupConnection {
    BackupConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        ..BackupConnection::default()
    }
}

fn conn_full(
    id: &str,
    name: &str,
    db_type: &str,
    group: Option<&str>,
    pinned: bool,
    last: Option<&str>,
) -> BackupConnection {
    BackupConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        group: group.map(str::to_owned),
        pinned,
        last_connected_at: last.map(str::to_owned),
        ..BackupConnection::default()
    }
}

fn ids(conns: &[BackupConnection]) -> Vec<&str> {
    conns.iter().map(|c| c.id.as_str()).collect()
}

fn option_ids(options: &[crate::views::BackupOption]) -> Vec<&str> {
    options.iter().map(|o| o.id).collect()
}

// ---- fresh state ----------------------------------------------------------

#[test]
fn fresh_vm_has_untitled_file_name_and_empty_state() {
    let vm = BackupWindowVm::default();
    assert!(!vm.is_restore);
    assert_eq!(vm.file_name, "untitled");
    assert!(vm.connections.is_empty());
    assert!(vm.groups.is_empty());
    assert!(vm.expanded_groups.is_empty());
    assert_eq!(vm.selected_conn_id, None);
    assert_eq!(vm.db_session_id, None);
    assert_eq!(vm.server_version, "");
    assert!(vm.databases.is_empty());
    assert_eq!(vm.selected_db, None);
    assert!(vm.enabled_options.is_empty());
    assert!(!vm.compress_gzip);
    assert!(!vm.option_dropdown_open);
    assert!(!vm.backing);
    assert_eq!(vm.status_message, "");
    assert_eq!(vm.progress, None);
    assert!(vm.progress_log.is_empty());
    assert_eq!(vm.search_conn, "");
    assert_eq!(vm.search_db, "");
    assert!(!vm.prefill_applied);
    assert!(!vm.can_start());
    assert_eq!(vm.backup_options(), &[] as &[crate::views::BackupOption]);
    assert_eq!(vm.db_type_display_label(), "");
    assert_eq!(vm.grouped(), vec![]);
    assert!(vm.filtered_dbs().is_empty());
    assert_eq!(BackupWindowVm::new(true).is_restore, true);
}

#[test]
fn the_i18n_keys_the_slice_asks_for_resolve() {
    for key in [
        "backup.unsupportedType",
        "main.ungrouped",
        "newConn.groupProd",
        "newConn.groupDev",
        "newConn.groupTest",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

// ---- driver meta table ----------------------------------------------------

#[test]
fn db_labels_come_from_the_driver_metas_with_id_fallback() {
    assert_eq!(db_label("mysql"), "MySQL");
    assert_eq!(db_label("postgresql"), "PostgreSQL");
    assert_eq!(db_label("sqlite"), "SQLite");
    assert_eq!(db_label("ob_oracle"), "OceanBase (Oracle)");
    assert_eq!(db_label("turso"), "Turso / libSQL");
    assert_eq!(db_label("manticore"), "ManticoreSearch");
    // Unknown type: React's `DB_REGISTRY[type]?.label ?? type`.
    assert_eq!(db_label("presto"), "presto");
    assert_eq!(db_label(""), "");
}

#[test]
fn supports_backup_matches_the_driver_metas() {
    for db_type in [
        "mysql",
        "mariadb",
        "doris",
        "starrocks",
        "manticore",
        "ob_oracle",
        "postgresql",
        "questdb",
        "cloudberry",
        "sqlite",
        "rqlite",
        "turso",
        "sqlserver",
        "clickhouse",
        "duckdb",
    ] {
        assert!(db_supports_backup(db_type), "{db_type} supports backup");
    }
    for db_type in [
        "redis",
        "mongodb",
        "elasticsearch",
        "hbase",
        "influxdb",
        "vector",
        "victoriametrics",
        "presto",
        "kiwi",
        "",
    ] {
        assert!(
            !db_supports_backup(db_type),
            "{db_type} does not support backup"
        );
    }
}

#[test]
fn backup_option_tables_match_the_dialect_sources() {
    assert_eq!(
        option_ids(backup_options_for("mysql")),
        [
            "no-data",
            "add-drop-table",
            "single-transaction",
            "routines",
            "triggers",
            "no-create-info"
        ]
    );
    assert_eq!(
        option_ids(backup_options_for("mariadb")),
        option_ids(backup_options_for("mysql")),
        "mariadb shares the mysql dialect strategy"
    );
    assert_eq!(
        option_ids(backup_options_for("postgresql")),
        [
            "data-only",
            "clean",
            "create",
            "no-owner",
            "schema-only",
            "routines",
            "triggers"
        ]
    );
    assert_eq!(
        option_ids(backup_options_for("cloudberry")),
        option_ids(backup_options_for("postgresql")),
        "cloudberry shares the postgresql dialect strategy"
    );
    assert_eq!(
        option_ids(backup_options_for("sqlite")),
        [
            "schema-only",
            "data-only",
            "clean",
            "no-owner",
            "single-transaction"
        ]
    );
    assert_eq!(
        option_ids(backup_options_for("rqlite")),
        option_ids(backup_options_for("sqlite")),
        "rqlite uses the sqlite family"
    );
    assert_eq!(
        option_ids(backup_options_for("sqlserver")),
        [
            "schema-only",
            "data-only",
            "clean",
            "no-owner",
            "single-transaction"
        ]
    );
    assert_eq!(
        option_ids(backup_options_for("clickhouse")),
        ["schema-only", "data-only", "clean", "no-owner"]
    );
    assert_eq!(
        option_ids(backup_options_for("duckdb")),
        [
            "schema-only",
            "data-only",
            "clean",
            "no-owner",
            "single-transaction"
        ]
    );
    for db_type in ["redis", "mongodb", "elasticsearch", "hbase", "presto", ""] {
        assert!(
            backup_options_for(db_type).is_empty(),
            "{db_type} has no backup options"
        );
    }
}

#[test]
fn backup_options_follow_the_selected_connection() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![
        conn("pg", "PG", "postgresql"),
        conn("lite", "Lite", "sqlite"),
        conn("kv", "KV", "redis"),
    ];
    vm.selected_conn_id = Some("pg".to_owned());
    assert_eq!(
        option_ids(vm.backup_options()),
        [
            "data-only",
            "clean",
            "create",
            "no-owner",
            "schema-only",
            "routines",
            "triggers"
        ]
    );
    vm.selected_conn_id = Some("lite".to_owned());
    assert_eq!(
        option_ids(vm.backup_options()),
        [
            "schema-only",
            "data-only",
            "clean",
            "no-owner",
            "single-transaction"
        ]
    );
    // Unsupported type: the memo returns [] even though a connection
    // is selected (`!meta?.supportsBackup`).
    vm.selected_conn_id = Some("kv".to_owned());
    assert!(vm.backup_options().is_empty());
    vm.selected_conn_id = Some("missing".to_owned());
    assert!(vm.backup_options().is_empty());
}

// ---- grouping -------------------------------------------------------------

#[test]
fn grouped_declared_groups_first_then_extras_then_ungrouped() {
    let conns = vec![
        conn_full("u1", "U1", "mysql", None, false, None),
        conn_full("a1", "A1", "mysql", Some("alpha"), false, None),
        conn_full("x1", "X1", "mysql", Some("extra"), false, None),
        conn_full("a2", "A2", "mysql", Some("alpha"), false, None),
        conn_full("u2", "U2", "mysql", Some(""), false, None),
    ];
    let groups = group_connections(&conns, &["alpha".to_owned(), "empty".to_owned()], "");
    assert_eq!(
        groups.iter().map(|g| g.group.as_str()).collect::<Vec<_>>(),
        ["alpha", "empty", "extra", ""]
    );
    assert_eq!(ids(&groups[0].connections), ["a1", "a2"]);
    assert!(
        groups[1].connections.is_empty(),
        "declared empty group stays"
    );
    assert_eq!(ids(&groups[2].connections), ["x1"]);
    assert_eq!(ids(&groups[3].connections), ["u1", "u2"]);
}

#[test]
fn grouped_keeps_pinned_first_within_each_group_when_not_searching() {
    let conns = vec![
        conn_full("c1", "First", "mysql", Some("g"), false, None),
        conn_full("c2", "Second", "mysql", Some("g"), true, None),
        conn_full("c3", "Third", "mysql", Some("g"), false, None),
        conn_full("c4", "Fourth", "mysql", Some("g"), true, None),
    ];
    let groups = group_connections(&conns, &["g".to_owned()], "");
    assert_eq!(ids(&groups[0].connections), ["c2", "c4", "c1", "c3"]);
}

#[test]
fn grouped_search_keeps_ranked_order_not_pinned_order() {
    // Both match "app" by name; c1 ranks better (prefix) than c2
    // (contains) even though c2 is pinned.
    let conns = vec![
        conn_full("c1", "apple", "mysql", Some("g"), false, None),
        conn_full("c2", "pineapple", "mysql", Some("g"), true, None),
        conn_full("c3", "banana", "mysql", Some("g"), false, None),
    ];
    let groups = group_connections(&conns, &["g".to_owned()], "app");
    assert_eq!(ids(&groups[0].connections), ["c1", "c2"]);
}

// ---- search ranking -------------------------------------------------------

#[test]
fn connection_match_ranks_exact_prefix_contains() {
    let exact = conn("a", "app", "mysql");
    let prefix = conn("b", "apple", "mysql");
    let contains = conn("c", "pineapple", "mysql");
    assert_eq!(
        connection_match(&exact, "app"),
        Some(ConnectionMatch {
            rank: 0,
            reason: MatchReason::Name,
            context: "app".to_owned()
        })
    );
    assert_eq!(
        connection_match(&prefix, "app"),
        Some(ConnectionMatch {
            rank: 1,
            reason: MatchReason::Name,
            context: "apple".to_owned()
        })
    );
    assert_eq!(
        connection_match(&contains, "app"),
        Some(ConnectionMatch {
            rank: 2,
            reason: MatchReason::Name,
            context: "pineapple".to_owned()
        })
    );
    assert_eq!(connection_match(&exact, "zzz"), None);
    assert_eq!(
        connection_match(&exact, "  "),
        None,
        "blank query matches nothing"
    );
    assert_eq!(connection_match(&exact, ""), None);
}

#[test]
fn connection_match_scores_host_database_and_type_after_name() {
    let mut c = conn("a", "zzz", "mysql");
    c.host = Some("dbhost".to_owned());
    assert_eq!(
        connection_match(&c, "dbhost"),
        Some(ConnectionMatch {
            rank: 10,
            reason: MatchReason::Host,
            context: "dbhost".to_owned()
        })
    );
    c.host = None;
    c.database = Some("appdb".to_owned());
    assert_eq!(
        connection_match(&c, "appdb"),
        Some(ConnectionMatch {
            rank: 20,
            reason: MatchReason::Database,
            context: "appdb".to_owned()
        })
    );
    // Database type id and label both score at offset 30; "postgres"
    // is a prefix of "postgresql", so both score 31 — the id candidate
    // is pushed first and wins the context tie-break.
    assert_eq!(
        connection_match(&conn("b", "zzz", "postgresql"), "postgres"),
        Some(ConnectionMatch {
            rank: 31,
            reason: MatchReason::DatabaseType,
            context: "postgresql".to_owned()
        })
    );
    assert_eq!(
        connection_match(&conn("b", "zzz", "postgresql"), "postgresql"),
        Some(ConnectionMatch {
            rank: 30,
            reason: MatchReason::DatabaseType,
            context: "postgresql".to_owned()
        })
    );
}

#[test]
fn rank_connections_dedupes_filters_and_sorts() {
    let conns = vec![
        conn("c2", "pineapple", "mysql"),
        conn("c1", "apple", "mysql"),
        conn("c1", "apple-dup", "mysql"),
        conn("c3", "banana", "mysql"),
    ];
    assert_eq!(ids(&rank_connections(&conns, "app")), ["c1", "c2"]);
    assert_eq!(rank_connections(&conns, "zzz").len(), 0);
}

#[test]
fn rank_connections_empty_query_returns_all_sorted_by_recency() {
    let conns = vec![
        conn_full(
            "old",
            "Old",
            "mysql",
            Some("g"),
            false,
            Some("2026-08-29T10:00:00Z"),
        ),
        conn_full(
            "new",
            "New",
            "mysql",
            Some("g"),
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
        conn_full(
            "pin",
            "Pin",
            "mysql",
            Some("g"),
            true,
            Some("2026-08-01T10:00:00Z"),
        ),
        conn_full("never", "Never", "mysql", Some("g"), false, None),
        conn_full("bad", "Bad", "mysql", Some("g"), false, Some("not-a-date")),
    ];
    // Pinned first, then most recently connected, then null/invalid
    // dates last — and among those, the group/name/id tie-breaks
    // still apply ("Bad" sorts before "Never").
    assert_eq!(
        ids(&rank_connections(&conns, "")),
        ["pin", "new", "old", "bad", "never"]
    );
}

#[test]
fn rank_connections_parses_date_only_fractional_and_negative_offsets() {
    let conns = vec![
        conn_full(
            "badday",
            "BadDay",
            "mysql",
            None,
            false,
            Some("2026-08-32T00:00:00Z"),
        ),
        conn_full(
            "fra",
            "Frac",
            "mysql",
            None,
            false,
            Some("2026-08-30T00:00:00.500Z"),
        ),
        conn_full(
            "badmonth",
            "BadMonth",
            "mysql",
            None,
            false,
            Some("2026-13-01T00:00:00Z"),
        ),
        conn_full(
            "dateonly",
            "DateOnly",
            "mysql",
            None,
            false,
            Some("2026-08-30"),
        ),
        conn_full(
            "badHour",
            "BadHour",
            "mysql",
            None,
            false,
            Some("2026-08-30T24:00:00Z"),
        ),
        conn_full(
            "negoff",
            "NegOff",
            "mysql",
            None,
            false,
            Some("2026-08-30T05:00:00-05:00"),
        ),
    ];
    // 05:00 at -05:00 is 10:00Z (newest); the date-only and fractional
    // stamps are the same midnight instant (fraction ignored) and tie
    // on name; the three invalid stamps sort last, by name.
    assert_eq!(
        ids(&rank_connections(&conns, "")),
        ["negoff", "dateonly", "fra", "badday", "badHour", "badmonth"]
    );
}

#[test]
fn grouped_duplicates_a_declared_group_name_like_react() {
    // React pushes one result row per `groups` entry (the map dedupes,
    // the result list does not), so a duplicate name appears twice.
    let conns = vec![conn_full("a", "A", "mysql", Some("g"), false, None)];
    let groups = group_connections(&conns, &["g".to_owned(), "g".to_owned()], "");
    assert_eq!(
        groups.iter().map(|g| g.group.as_str()).collect::<Vec<_>>(),
        ["g", "g"]
    );
    assert_eq!(ids(&groups[0].connections), ["a"]);
    assert_eq!(ids(&groups[1].connections), ["a"]);
}

#[test]
fn grouped_declared_empty_ungrouped_group_stays_singular() {
    let conns = vec![conn_full("a", "A", "mysql", Some("g"), false, None)];
    let groups = group_connections(&conns, &["".to_owned()], "");
    assert_eq!(
        groups.iter().map(|g| g.group.as_str()).collect::<Vec<_>>(),
        ["", "g"]
    );
    assert!(groups[0].connections.is_empty());
    assert_eq!(ids(&groups[1].connections), ["a"]);
}

#[test]
fn apply_session_connected_without_a_selected_connection_sets_no_defaults() {
    let mut vm = BackupWindowVm::default();
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-9".to_owned(),
        server_version: Some("15.2".to_owned()),
        databases: vec!["app".to_owned()],
        default_database: Some("app".to_owned()),
    });
    assert_eq!(vm.db_session_id.as_deref(), Some("sess-9"));
    assert_eq!(vm.selected_db.as_deref(), Some("app"));
    assert!(
        vm.enabled_options.is_empty(),
        "no selected connection, no dialect defaults"
    );
}

#[test]
fn apply_session_connected_ignores_an_empty_default_database() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("pg", "PG", "postgresql")];
    let pg = conn("pg", "PG", "postgresql");
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-1".to_owned(),
        server_version: None,
        databases: vec!["app".to_owned()],
        default_database: Some("".to_owned()),
    });
    assert_eq!(
        vm.selected_db, None,
        "React's `if (dbPick && …)` skips \"\""
    );
}

#[test]
fn rank_connections_tie_breaks_fall_through_group_name_id() {
    let conns = vec![
        conn_full(
            "b2",
            "Same",
            "mysql",
            Some("zeta"),
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
        conn_full(
            "b1",
            "Same",
            "mysql",
            Some("alpha"),
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
        conn_full(
            "a9",
            "Aardvark",
            "mysql",
            Some("alpha"),
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
        conn_full(
            "a1",
            "Same",
            "mysql",
            Some("alpha"),
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
    ];
    assert_eq!(ids(&rank_connections(&conns, "same")), ["a1", "b1", "b2"]);
    assert_eq!(ids(&rank_connections(&conns, "")), ["a9", "a1", "b1", "b2"]);
}

#[test]
fn rank_connections_offset_timestamps_compare_as_instants() {
    // 10:00 at +08:00 is 02:00Z — older than 03:00Z.
    let conns = vec![
        conn_full(
            "plus8",
            "Plus8",
            "mysql",
            None,
            false,
            Some("2026-08-30T10:00:00+08:00"),
        ),
        conn_full(
            "utc",
            "Utc",
            "mysql",
            None,
            false,
            Some("2026-08-30T03:00:00Z"),
        ),
    ];
    assert_eq!(ids(&rank_connections(&conns, "")), ["utc", "plus8"]);
}

// ---- database filtering / labels / toggles --------------------------------

#[test]
fn filtered_dbs_is_a_trimmed_case_insensitive_substring() {
    let mut vm = BackupWindowVm::default();
    vm.databases = ["App", "app_logs", "Other", "MyDB"]
        .iter()
        .map(|name| crate::views::DatabaseInfo {
            name: (*name).to_owned(),
        })
        .collect();
    assert_eq!(vm.filtered_dbs().len(), 4);
    vm.search_db = "  APP ".to_owned();
    assert_eq!(
        vm.filtered_dbs()
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["App", "app_logs"]
    );
    vm.search_db = "db".to_owned();
    assert_eq!(
        vm.filtered_dbs()
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["MyDB"]
    );
    vm.search_db = "zzz".to_owned();
    assert!(vm.filtered_dbs().is_empty());
}

#[test]
fn db_type_display_label_appends_the_server_version() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("pg", "PG", "postgresql")];
    vm.selected_conn_id = Some("pg".to_owned());
    assert_eq!(vm.db_type_display_label(), "PostgreSQL");
    vm.server_version = "15.2".to_owned();
    assert_eq!(vm.db_type_display_label(), "PostgreSQL 15.2");
}

#[test]
fn group_labels_localize_presets_and_pass_custom_keys_through() {
    assert_eq!(
        format_group_label("preset:production"),
        t("newConn.groupProd")
    );
    assert_eq!(
        format_group_label("preset:development"),
        t("newConn.groupDev")
    );
    assert_eq!(format_group_label("preset:testing"), t("newConn.groupTest"));
    assert_eq!(format_group_label("my-team"), "my-team");
    assert_eq!(group_display_label(""), t("main.ungrouped"));
    assert_eq!(group_display_label("my-team"), "my-team");
}

#[test]
fn toggles_flip_membership() {
    let mut vm = BackupWindowVm::default();
    vm.toggle_group("g1");
    assert!(vm.expanded_groups.contains("g1"));
    vm.toggle_group("g1");
    assert!(!vm.expanded_groups.contains("g1"));
    vm.toggle_option("routines");
    assert!(vm.enabled_options.contains("routines"));
    vm.toggle_option("routines");
    assert!(!vm.enabled_options.contains("routines"));
}

#[test]
fn file_name_derivation_follows_the_gzip_and_format_custom_rules() {
    let mut vm = BackupWindowVm::default();
    assert_eq!(vm.file_extension(), "sql");
    assert_eq!(vm.default_file_name(), "untitled.sql");
    vm.enabled_options.insert("format-custom".to_owned());
    assert_eq!(vm.file_extension(), "dump");
    assert_eq!(vm.default_file_name(), "untitled.dump");
    vm.compress_gzip = true;
    assert_eq!(vm.file_extension(), "gz");
    assert_eq!(vm.default_file_name(), "untitled.sql.gz");
    vm.file_name = "mydb-2026".to_owned();
    assert_eq!(vm.default_file_name(), "mydb-2026.sql.gz");
}

// ---- load / select journey --------------------------------------------------

#[test]
fn apply_connections_loaded_expands_every_group() {
    let mut vm = BackupWindowVm::default();
    vm.apply_connections_loaded(
        vec![
            conn_full("a", "A", "mysql", Some("alpha"), false, None),
            conn_full("b", "B", "mysql", None, false, None),
        ],
        vec!["alpha".to_owned(), "beta".to_owned()],
    );
    assert_eq!(vm.connections.len(), 2);
    assert_eq!(vm.groups, ["alpha", "beta"]);
    let expanded: Vec<&str> = vm.expanded_groups.iter().map(String::as_str).collect();
    assert_eq!(expanded, ["", "alpha", "beta"]);
}

#[test]
fn initial_expanded_groups_include_every_connection_group() {
    let expanded = initial_expanded_groups(
        &["alpha".to_owned()],
        &[conn_full("a", "A", "mysql", Some("extra"), false, None)],
    );
    let expanded: Vec<&str> = expanded.iter().map(String::as_str).collect();
    assert_eq!(expanded, ["", "alpha", "extra"]);
}

#[test]
fn begin_select_on_unsupported_type_resets_and_reports() {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("old-session".to_owned());
    vm.selected_db = Some("app".to_owned());
    vm.server_version = "7.0".to_owned();
    vm.enabled_options.insert("routines".to_owned());
    vm.status_message = "previous".to_owned();
    vm.progress = Some(crate::views::BackupProgress {
        current: 1,
        total: 2,
        object_name: "t".to_owned(),
        phase: BackupProgressPhase::Writing,
    });
    let redis = conn("kv", "KV", "redis");
    assert_eq!(
        vm.begin_select_connection(&redis),
        SelectConnectionOutcome::UnsupportedType
    );
    assert_eq!(vm.selected_conn_id.as_deref(), Some("kv"));
    assert_eq!(vm.selected_db, None);
    assert!(vm.databases.is_empty());
    assert_eq!(vm.server_version, "");
    assert!(vm.enabled_options.is_empty());
    assert_eq!(vm.progress, None);
    assert_eq!(vm.db_session_id, None);
    assert_eq!(vm.status_message, t("backup.unsupportedType"));
}

#[test]
fn begin_select_on_supported_type_resets_and_asks_the_host_to_connect() {
    let mut vm = BackupWindowVm::default();
    vm.selected_db = Some("app".to_owned());
    vm.status_message = "previous".to_owned();
    let pg = conn("pg", "PG", "postgresql");
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    assert_eq!(vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(vm.selected_db, None);
    assert_eq!(vm.status_message, "");
}

#[test]
fn apply_session_connected_loads_databases_picks_default_and_options() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("pg", "PG", "postgresql")];
    let pg = conn("pg", "PG", "postgresql");
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-1".to_owned(),
        server_version: Some("15.2".to_owned()),
        databases: vec!["app".to_owned(), "other".to_owned()],
        default_database: Some("app".to_owned()),
    });
    assert_eq!(vm.db_session_id.as_deref(), Some("sess-1"));
    assert_eq!(vm.server_version, "15.2");
    assert_eq!(
        vm.databases
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["app", "other"]
    );
    assert_eq!(vm.selected_db.as_deref(), Some("app"));
    // Postgres offers routines + triggers, so both default on.
    assert!(vm.enabled_options.contains("routines"));
    assert!(vm.enabled_options.contains("triggers"));
    assert_eq!(vm.enabled_options.len(), 2);
    assert!(vm.can_start());
}

#[test]
fn apply_session_connected_falls_back_to_the_connection_database() {
    let mut vm = BackupWindowVm::default();
    let mut pg = conn("pg", "PG", "postgresql");
    pg.database = Some("fallback".to_owned());
    vm.connections = vec![pg.clone()];
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    // No preferred database: the caller passes `preferred ?? conn.database`.
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-1".to_owned(),
        server_version: None,
        databases: vec!["fallback".to_owned()],
        default_database: pg.database.clone(),
    });
    assert_eq!(vm.selected_db.as_deref(), Some("fallback"));
}

#[test]
fn apply_session_connected_skips_a_default_that_is_not_listed() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("lite", "Lite", "sqlite")];
    let lite = conn("lite", "Lite", "sqlite");
    assert_eq!(
        vm.begin_select_connection(&lite),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "lite".to_owned()
        }
    );
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-2".to_owned(),
        server_version: Some("".to_owned()),
        databases: vec!["main".to_owned()],
        default_database: Some("missing".to_owned()),
    });
    assert_eq!(vm.selected_db, None);
    // Empty server version is falsy in React: the version stays "".
    assert_eq!(vm.server_version, "");
    // SQLite offers neither routines nor triggers: no defaults.
    assert!(vm.enabled_options.is_empty());
    assert!(!vm.can_start(), "no database selected yet");
}

#[test]
fn apply_select_error_becomes_the_status_message() {
    let mut vm = BackupWindowVm::default();
    vm.apply_select_error("connection refused");
    assert_eq!(vm.status_message, "connection refused");
}

#[test]
fn can_start_needs_a_session_a_database_and_no_run() {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("s".to_owned());
    assert!(!vm.can_start());
    vm.selected_db = Some("app".to_owned());
    assert!(vm.can_start());
    vm.backing = true;
    assert!(!vm.can_start());
}

// ---- cross-window close / prefill -----------------------------------------

#[test]
fn connection_closed_clears_only_the_matching_session() {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("sess-1".to_owned());
    vm.on_connection_closed("");
    assert_eq!(vm.db_session_id.as_deref(), Some("sess-1"));
    vm.on_connection_closed("sess-2");
    assert_eq!(vm.db_session_id.as_deref(), Some("sess-1"));
    vm.on_connection_closed("sess-1");
    assert_eq!(vm.db_session_id, None);
}

#[test]
fn prefill_applies_once_for_a_known_connection() {
    let mut vm = BackupWindowVm::default();
    vm.apply_connections_loaded(
        vec![conn("pg", "PG", "postgresql"), conn("my", "MY", "mysql")],
        vec![],
    );
    let target = vm.take_prefill(Some("pg"), Some("app"));
    assert_eq!(
        target.as_ref().map(|t| t.connection.id.as_str()),
        Some("pg")
    );
    assert_eq!(
        target
            .as_ref()
            .and_then(|t| t.preferred_database.as_deref()),
        Some("app")
    );
    assert!(vm.prefill_applied);
    assert_eq!(
        vm.take_prefill(Some("my"), None),
        None,
        "second prefill is a no-op"
    );
}

#[test]
fn prefill_without_a_match_does_not_mark_applied_so_it_retries() {
    let mut vm = BackupWindowVm::default();
    assert_eq!(
        vm.take_prefill(Some("pg"), None),
        None,
        "no connections yet"
    );
    assert!(!vm.prefill_applied);
    vm.apply_connections_loaded(vec![conn("pg", "PG", "postgresql")], vec![]);
    assert_eq!(vm.take_prefill(None, None), None, "no connectionId param");
    assert!(!vm.prefill_applied);
    assert_eq!(vm.take_prefill(Some("missing"), None), None, "unknown id");
    assert!(!vm.prefill_applied);
    assert!(
        vm.take_prefill(Some("pg"), None).is_some(),
        "retry after load works"
    );
}

// ---- ProgressLog mount ------------------------------------------------------

fn mount_view(_s: &(), _cx: &mut Cx<()>) -> El {
    rune_kit::root(progress_log_mount(), Vec::new())
}

#[test]
fn progress_log_mount_is_an_empty_keyed_mount() {
    let mut rt = Runtime::new(
        (),
        mount_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| !e.exiting && e.key.as_deref() == Some("backup-progress-log")),
        "the empty backup-progress-log mount is laid out"
    );
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|e| e.key.as_deref() == Some("kit-root"))
            .count(),
        1,
        "a single kit root"
    );
}
