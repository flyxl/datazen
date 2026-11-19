//! T026-S1 independent audit: `BackupWindowVm` state model + selection
//! logic (`BackupWindow.tsx`).
//!
//! Written against the React sources, not against the coder's tests:
//! the full driver-label table, every dialect alias, the select /
//! session journey's untouched fields, grouping quirks React itself
//! has (duplicate declared groups), and the timestamp parser's
//! edges. Where the port pins a known limitation instead of fixing
//! it, the test name says so and the finding is listed in the audit
//! report (finding N1: bare fractional timestamps).

use rune::{Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{
    backup_options_for, connection_match, db_label, db_supports_backup, group_connections,
    initial_expanded_groups, progress_log_mount, rank_connections, BackupConnection,
    BackupProgress, BackupProgressPhase, BackupWindowVm, ConnectionMatch, MatchReason,
    SelectConnectionOutcome, SessionConnected,
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

// ---- full driver-meta table -----------------------------------------------

#[test]
fn audit_full_driver_label_table() {
    // Every label straight from crates/drivers/*/ui/meta.ts (and the
    // redis shared meta); React's getDbLabel falls back to the id.
    for (db_type, label) in [
        ("mysql", "MySQL"),
        ("mariadb", "MariaDB"),
        ("doris", "Doris"),
        ("starrocks", "StarRocks"),
        ("manticore", "ManticoreSearch"),
        ("ob_oracle", "OceanBase (Oracle)"),
        ("postgresql", "PostgreSQL"),
        ("questdb", "QuestDB"),
        ("cloudberry", "Cloudberry"),
        ("sqlite", "SQLite"),
        ("rqlite", "RQLite"),
        ("turso", "Turso / libSQL"),
        ("sqlserver", "SQL Server"),
        ("clickhouse", "ClickHouse"),
        ("duckdb", "DuckDB"),
        ("redis", "Redis"),
        ("mongodb", "MongoDB"),
        ("elasticsearch", "Elasticsearch"),
        ("hbase", "HBase"),
        ("influxdb", "InfluxDB"),
        ("vector", "Vector DB"),
        ("victoriametrics", "VictoriaMetrics"),
    ] {
        assert_eq!(db_label(db_type), label, "{db_type} label");
    }
    assert_eq!(db_label("kiwi"), "kiwi", "unshipped type falls back to id");
    assert_eq!(db_label("superset"), "superset");
}

#[test]
fn audit_all_alias_types_share_dialect_options() {
    let mysql = option_ids(backup_options_for("mysql"));
    for alias in ["mariadb", "doris", "starrocks", "manticore", "ob_oracle"] {
        assert_eq!(
            option_ids(backup_options_for(alias)),
            mysql,
            "{alias} shares the mysql dialect strategy"
        );
    }
    let pg = option_ids(backup_options_for("postgresql"));
    for alias in ["questdb", "cloudberry"] {
        assert_eq!(
            option_ids(backup_options_for(alias)),
            pg,
            "{alias} shares the postgresql dialect strategy"
        );
    }
    let sqlite = option_ids(backup_options_for("sqlite"));
    for alias in ["rqlite", "turso"] {
        assert_eq!(
            option_ids(backup_options_for(alias)),
            sqlite,
            "{alias} uses the sqlite family"
        );
    }
}

#[test]
fn audit_oracle_dialect_has_no_driver() {
    // extra.ts declares an oracle dialect (3 options) but no driver
    // meta selects it, so the Backup window never sees it.
    assert_eq!(db_label("oracle"), "oracle");
    assert!(!db_supports_backup("oracle"));
    assert!(backup_options_for("oracle").is_empty());
}

// ---- select journey: what begin does and does not touch --------------------

#[test]
fn audit_unknown_type_select_is_unsupported() {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("old-session".to_owned());
    let presto = conn("p", "P", "presto");
    assert_eq!(
        vm.begin_select_connection(&presto),
        SelectConnectionOutcome::UnsupportedType
    );
    assert_eq!(vm.db_session_id, None, "unsupported clears the session");
    assert_eq!(vm.status_message, t("backup.unsupportedType"));
    assert_eq!(vm.selected_conn_id.as_deref(), Some("p"));
}

#[test]
fn audit_begin_select_preserves_unrelated_state() {
    let mut vm = BackupWindowVm::default();
    vm.file_name = "mydb".to_owned();
    vm.compress_gzip = true;
    vm.search_conn = "app".to_owned();
    vm.search_db = "logs".to_owned();
    vm.progress_log = vec!["old line".to_owned()];
    vm.backing = true;
    vm.db_session_id = Some("old-session".to_owned());
    // React's handleSelectConnection resets only the per-connection
    // fields; file name, gzip, searches, log and backing survive.
    let pg = conn("pg", "PG", "postgresql");
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    assert_eq!(vm.file_name, "mydb");
    assert!(vm.compress_gzip);
    assert_eq!(vm.search_conn, "app");
    assert_eq!(vm.search_db, "logs");
    assert_eq!(vm.progress_log, ["old line"]);
    assert!(vm.backing);
    // The old session stays until connect succeeds: React only clears
    // it on the unsupported-type branch.
    assert_eq!(vm.db_session_id.as_deref(), Some("old-session"));
}

#[test]
fn audit_apply_select_error_keeps_old_session() {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("old-session".to_owned());
    let pg = conn("pg", "PG", "postgresql");
    assert_eq!(
        vm.begin_select_connection(&pg),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "pg".to_owned()
        }
    );
    vm.apply_select_error("connection refused");
    assert_eq!(vm.status_message, "connection refused");
    assert_eq!(
        vm.db_session_id.as_deref(),
        Some("old-session"),
        "React's catch only sets the status message"
    );
}

#[test]
fn audit_mysql_defaults_enable_routines_and_triggers() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("my", "MY", "mysql")];
    let my = conn("my", "MY", "mysql");
    assert_eq!(
        vm.begin_select_connection(&my),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "my".to_owned()
        }
    );
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-1".to_owned(),
        server_version: Some("8.0.36".to_owned()),
        databases: vec!["app".to_owned()],
        default_database: Some("app".to_owned()),
    });
    assert!(vm.enabled_options.contains("routines"));
    assert!(vm.enabled_options.contains("triggers"));
    assert_eq!(vm.enabled_options.len(), 2);
}

#[test]
fn audit_clickhouse_defaults_enable_nothing() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("ch", "CH", "clickhouse")];
    let ch = conn("ch", "CH", "clickhouse");
    assert_eq!(
        vm.begin_select_connection(&ch),
        SelectConnectionOutcome::NeedsConnect {
            connection_id: "ch".to_owned()
        }
    );
    vm.apply_session_connected(SessionConnected {
        db_session_id: "sess-1".to_owned(),
        server_version: None,
        databases: vec!["default".to_owned()],
        default_database: Some("default".to_owned()),
    });
    assert!(
        vm.enabled_options.is_empty(),
        "clickhouse offers neither routines nor triggers"
    );
    assert_eq!(vm.selected_db.as_deref(), Some("default"));
}

#[test]
fn audit_default_database_none_selects_nothing() {
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
        default_database: None,
    });
    assert_eq!(vm.selected_db, None);
    assert_eq!(vm.databases.len(), 1, "databases still load");
    assert!(!vm.can_start(), "no database selected yet");
}

#[test]
fn audit_db_type_label_falls_back_to_id_with_version() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![conn("p", "P", "presto")];
    vm.selected_conn_id = Some("p".to_owned());
    assert_eq!(vm.db_type_display_label(), "presto");
    vm.server_version = "1.0".to_owned();
    assert_eq!(vm.db_type_display_label(), "presto 1.0");
}

// ---- filtering / grouping ---------------------------------------------------

#[test]
fn audit_filtered_dbs_blank_query_returns_all() {
    let mut vm = BackupWindowVm::default();
    vm.databases = ["a".to_owned(), "b".to_owned()]
        .iter()
        .map(|name| crate::views::DatabaseInfo { name: name.clone() })
        .collect();
    vm.search_db = "   ".to_owned();
    assert_eq!(vm.filtered_dbs().len(), 2, "trimmed query is empty");
}

#[test]
fn audit_grouped_search_keeps_declared_empty_groups() {
    let conns = vec![
        conn_full("c1", "apple", "mysql", Some("g"), false, None),
        conn_full("c2", "banana", "mysql", Some("g"), false, None),
    ];
    let groups = group_connections(&conns, &["g".to_owned(), "empty".to_owned()], "app");
    assert_eq!(
        groups.iter().map(|g| g.group.as_str()).collect::<Vec<_>>(),
        ["g", "empty"],
        "React pushes every declared group even while searching"
    );
    assert_eq!(ids(&groups[0].connections), ["c1"]);
    assert!(groups[1].connections.is_empty());
}

#[test]
fn audit_declared_ungrouped_group_duplicates_like_react() {
    // React pushes '' once in the declared-groups loop and again in
    // the trailing ungrouped block; the port mirrors the duplication.
    let conns = vec![conn_full("u1", "U1", "mysql", None, false, None)];
    let groups = group_connections(&conns, &["".to_owned()], "");
    assert_eq!(
        groups.iter().map(|g| g.group.as_str()).collect::<Vec<_>>(),
        ["", ""]
    );
    assert_eq!(ids(&groups[0].connections), ["u1"]);
    assert_eq!(ids(&groups[1].connections), ["u1"]);
}

#[test]
fn audit_grouped_vm_method_uses_search_conn() {
    let mut vm = BackupWindowVm::default();
    vm.connections = vec![
        conn_full("c1", "apple", "mysql", Some("g"), false, None),
        conn_full("c2", "pineapple", "mysql", Some("g"), false, None),
        conn_full("c3", "banana", "mysql", Some("g"), false, None),
    ];
    vm.groups = vec!["g".to_owned()];
    vm.search_conn = "app".to_owned();
    let grouped = vm.grouped();
    assert_eq!(grouped.len(), 1);
    assert_eq!(ids(&grouped[0].connections), ["c1", "c2"]);
}

#[test]
fn audit_initial_expanded_groups_dedupes() {
    let expanded = initial_expanded_groups(
        &["g".to_owned(), "g".to_owned()],
        &[conn_full("a", "A", "mysql", None, false, None)],
    );
    let expanded: Vec<&str> = expanded.iter().map(String::as_str).collect();
    assert_eq!(expanded, ["", "g"]);
}

#[test]
fn audit_toggle_group_on_ungrouped_key() {
    let mut vm = BackupWindowVm::default();
    vm.toggle_group("");
    assert!(vm.expanded_groups.contains(""));
    vm.toggle_group("");
    assert!(!vm.expanded_groups.contains(""));
}

// ---- search ranking ---------------------------------------------------------

#[test]
fn audit_rank_matches_type_label() {
    // "libsql" only occurs in the Turso label, at the offset-30
    // database-type tier, as a contains match.
    assert_eq!(
        connection_match(&conn("t", "zzz", "turso"), "libsql"),
        Some(ConnectionMatch {
            rank: 32,
            reason: MatchReason::DatabaseType,
            context: "Turso / libSQL".to_owned()
        })
    );
}

#[test]
fn audit_connection_match_is_case_insensitive_and_trims() {
    assert_eq!(
        connection_match(&conn("a", "app", "mysql"), "  APP "),
        Some(ConnectionMatch {
            rank: 0,
            reason: MatchReason::Name,
            context: "app".to_owned()
        })
    );
}

#[test]
fn audit_connection_match_unknown_type_label_candidate() {
    // React scores no label candidate for an unknown type; the Rust
    // fallback label is the id itself, so the observable match is
    // identical: rank 30, context the id.
    assert_eq!(
        connection_match(&conn("p", "zzz", "presto"), "presto"),
        Some(ConnectionMatch {
            rank: 30,
            reason: MatchReason::DatabaseType,
            context: "presto".to_owned()
        })
    );
}

#[test]
fn audit_best_candidate_context_tie_prefers_label_with_space() {
    // Both the id ("sqlserver") and the label ("SQL Server") start
    // with "sql" at rank 31; the lowercased contexts sort the label
    // first ("sql server" < "sqlserver"), matching React's
    // localeCompare order for this pair.
    assert_eq!(
        connection_match(&conn("s", "zzz", "sqlserver"), "sql"),
        Some(ConnectionMatch {
            rank: 31,
            reason: MatchReason::DatabaseType,
            context: "SQL Server".to_owned()
        })
    );
}

// ---- timestamp parsing ------------------------------------------------------

#[test]
fn audit_timestamp_leap_day_and_bare_datetime() {
    let conns = vec![
        conn_full("bad", "Bad", "mysql", None, false, Some("2025-02-29")),
        conn_full("good", "Good", "mysql", None, false, Some("2024-02-29")),
        conn_full(
            "bare",
            "Bare",
            "mysql",
            None,
            false,
            Some("2026-08-30T10:00:00"),
        ),
        conn_full("never", "Never", "mysql", None, false, None),
    ];
    // 2024 is a leap year (valid), 2025-02-29 is not; a bare datetime
    // without Z/offset is rejected by the ISO-only parser, like an
    // invalid date, and sorts last with the nulls by name.
    assert_eq!(
        ids(&rank_connections(&conns, "")),
        ["good", "bad", "bare", "never"]
    );
}

#[test]
fn audit_bare_fractional_timestamp_sorts_last_known_limitation() {
    // Finding N1 (non-blocking): "2026-08-30T10:00:00.500" is valid
    // ISO-8601 and Date.parse accepts it, but parse_iso_timestamp
    // returns None when the fractional part runs to the end of the
    // string with no Z/offset suffix, so it sorts last. Stored
    // lastConnectedAt values come from toISOString() (always Z and a
    // 3-digit fraction), so no live data hits this path.
    let conns = vec![
        conn_full(
            "bare",
            "Bare",
            "mysql",
            None,
            false,
            Some("2026-08-30T10:00:00.500"),
        ),
        conn_full(
            "zulu",
            "Zulu",
            "mysql",
            None,
            false,
            Some("2026-08-30T10:00:00Z"),
        ),
    ];
    assert_eq!(ids(&rank_connections(&conns, "")), ["zulu", "bare"]);
}

// ---- misc state -------------------------------------------------------------

#[test]
fn audit_file_name_empty_stays_empty_prefix() {
    let mut vm = BackupWindowVm::default();
    vm.file_name = String::new();
    assert_eq!(vm.default_file_name(), ".sql");
    vm.compress_gzip = true;
    assert_eq!(vm.default_file_name(), ".sql.gz");
}

#[test]
fn audit_vm_progress_default_phase_is_object() {
    let progress = BackupProgress::default();
    assert_eq!(progress.current, 0);
    assert_eq!(progress.total, 0);
    assert_eq!(progress.object_name, "");
    assert_eq!(progress.phase, BackupProgressPhase::Object);
    let mut vm = BackupWindowVm::new(true);
    assert!(vm.is_restore);
    vm.progress = Some(BackupProgress {
        current: 1,
        total: 2,
        object_name: "t".to_owned(),
        phase: BackupProgressPhase::Done,
    });
    assert_eq!(
        vm.progress,
        Some(BackupProgress {
            current: 1,
            total: 2,
            object_name: "t".to_owned(),
            phase: BackupProgressPhase::Done,
        })
    );
}

// ---- ProgressLog mount ------------------------------------------------------

fn audit_mount_view(_s: &(), _cx: &mut Cx<()>) -> El {
    rune_kit::root(progress_log_mount(), Vec::new())
}

#[test]
fn audit_progress_log_mount_renders_in_light_theme() {
    let mut rt = Runtime::new(
        (),
        audit_mount_view,
        (800.0, 600.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| !e.exiting && e.key.as_deref() == Some("backup-progress-log")),
        "the empty backup-progress-log mount is laid out in light theme too"
    );
}
