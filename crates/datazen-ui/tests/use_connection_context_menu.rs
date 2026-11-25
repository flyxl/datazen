//! T608: `useConnectionContextMenu` logic — every menu family's item set,
//! order, gates and SQL/danger decisions as pure tests over host-owned state.

use std::collections::{BTreeMap, BTreeSet};

use datazen_ui::views::use_connection_context_menu::{
    build_menu, derive_context_flags, drop_sql, entry_ids, may_run_dangerous, show_export_for_kind,
    truncate_sql, ContextMetaVm, FeatureFlags, MenuArgs, MenuEntry, MenuKind,
};

fn args(kind: MenuKind, wired: &[&'static str]) -> MenuArgs {
    MenuArgs {
        kind,
        wired: wired.iter().map(|id| *id).collect::<BTreeSet<_>>(),
        show_open_structure: true,
        show_er_focus: true,
        ..MenuArgs::default()
    }
}

fn ids(entries: &[MenuEntry]) -> Vec<&str> {
    entry_ids(entries)
}

// ── table family ─────────────────────────────────────────────────────────────

#[test]
fn table_menu_uses_tableplus_order_with_danger_section() {
    let mut a = args(MenuKind::Table, &[
        "open", "open-structure", "focus-er", "new-query", "copy-name", "copy-ddl",
        "export", "import", "truncate", "drop",
    ]);
    assert_eq!(ids(&build_menu(&a)), [
        "open", "open-structure", "focus-er", "new-query", "copy-name", "copy-ddl",
        "export", "import", "separator", "truncate", "drop",
    ]);
    // Optional structure/ER items disappear when their flags are off.
    a.show_open_structure = false;
    a.show_er_focus = false;
    assert_eq!(ids(&build_menu(&a)), [
        "open", "new-query", "copy-name", "copy-ddl", "export",
        "import", "separator", "truncate", "drop",
    ]);
}

#[test]
fn table_read_only_hides_import_truncate_drop() {
    let mut a = args(MenuKind::Table, &[
        "open", "copy-name", "copy-ddl", "new-query", "export",
        "import", "truncate", "drop",
    ]);
    a.read_only = true;
    assert_eq!(ids(&build_menu(&a)), [
        "open", "new-query", "copy-name", "copy-ddl", "export",
    ]);
}

#[test]
fn table_generate_sql_submenu_has_fixed_order() {
    let mut a = args(MenuKind::Table, &[
        "open", "generate-select", "generate-insert", "generate-update", "generate-delete", "generate-ddl",
    ]);
    a.generate_labels.insert("generate-sql", "Generate SQL".to_owned());
    let entries = build_menu(&a);
    assert!(entries.iter().any(|e| matches!(e, MenuEntry::Submenu { id: "generate-sql", .. })));
    let sub = entries.iter().find(|e| matches!(e, MenuEntry::Submenu { id: "generate-sql", .. })).unwrap();
    let MenuEntry::Submenu { items, .. } = sub else { unreachable!() };
    assert_eq!(ids(items), ["generate-select", "generate-insert", "generate-update", "generate-delete", "generate-ddl"]);
}

#[test]
fn table_export_flags_hide_export_and_batch() {
    let mut a = args(MenuKind::Table, &[
        "open", "copy-name", "copy-ddl", "new-query", "export", "batch-export", "import", "truncate", "drop",
    ]);
    a.show_export = Some(false);
    a.show_batch_export = Some(false);
    let menu = build_menu(&a);
    let rendered = ids(&menu);
    assert!(!rendered.contains(&"export"));
    assert!(!rendered.contains(&"batch-export"));
    assert!(rendered.contains(&"truncate"));
}

#[test]
fn unwired_handlers_omit_their_items() {
    let a = args(MenuKind::Table, &["open"]);
    assert_eq!(ids(&build_menu(&a)), ["open"]);
}

// ── view family ──────────────────────────────────────────────────────────────

#[test]
fn view_menu_export_needs_explicit_flag_and_drop_view_is_last() {
    let mut a = args(MenuKind::View, &[
        "open", "copy-name", "copy-ddl", "export", "batch-export", "drop-view",
    ]);
    assert_eq!(ids(&build_menu(&a)), ["open", "copy-name", "copy-ddl", "separator", "drop-view"]);
    a.show_export = Some(true);
    assert_eq!(ids(&build_menu(&a)), [
        "open", "copy-name", "copy-ddl", "export", "batch-export", "separator", "drop-view",
    ]);
    a.read_only = true;
    assert_eq!(ids(&build_menu(&a)), ["open", "copy-name", "copy-ddl", "export", "batch-export"]);
}

// ── database / schema families ───────────────────────────────────────────────

#[test]
fn database_menu_groups_sync_backup_close_drop() {
    let mut a = args(MenuKind::Database, &[
        "refresh", "new-query", "query-history", "copy-database-name", "view-er-diagram",
        "batch-export", "import", "new-table", "create-schema", "data-transfer",
        "compare-schema", "compare-data", "backup", "restore", "close-database-connection", "drop-database",
    ]);
    a.show_new_table = true;
    a.features = FeatureFlags { data_transfer: true, schema_diff: true, data_sync: true };
    assert_eq!(ids(&build_menu(&a)), [
        "refresh", "new-query", "query-history", "copy-database-name", "view-er-diagram",
        "batch-export", "import", "new-table", "create-schema",
        "separator", "data-transfer", "compare-schema", "compare-data",
        "separator", "backup", "restore", "separator", "close-database-connection",
        "separator", "drop-database",
    ]);
}

#[test]
fn database_execute_sql_file_is_gated_by_read_only_and_safe_mode() {
    let mut a = args(MenuKind::Database, &["refresh", "execute-sql-file"]);
    assert!(ids(&build_menu(&a)).contains(&"execute-sql-file"));
    a.safe_mode = true;
    assert!(!ids(&build_menu(&a)).contains(&"execute-sql-file"));
    a.safe_mode = false;
    a.read_only = true;
    assert!(!ids(&build_menu(&a)).contains(&"execute-sql-file"));
}

#[test]
fn schema_menu_uses_copy_name_label_and_hides_new_table_when_read_only() {
    let mut a = args(MenuKind::Schema, &[
        "refresh", "new-query", "query-history", "copy-schema-name", "view-er-diagram", "new-table", "drop-schema",
    ]);
    assert_eq!(ids(&build_menu(&a)), [
        "refresh", "new-query", "query-history", "copy-schema-name", "view-er-diagram",
        "new-table", "separator", "drop-schema",
    ]);
    a.read_only = true;
    let menu = build_menu(&a);
    let rendered = ids(&menu);
    assert!(!rendered.contains(&"new-table"));
    assert!(!rendered.contains(&"drop-schema"));
}

// ── category / blank / default ───────────────────────────────────────────────

#[test]
fn category_tables_puts_refresh_first_and_new_table_only_for_tables() {
    let all = ["refresh", "import", "new-table"];
    let mut a = args(MenuKind::Category, &all);
    a.category_id = Some("tables".to_owned());
    assert_eq!(ids(&build_menu(&a)), ["refresh", "new-table", "import"]);
    a.category_id = Some("views".to_owned());
    assert_eq!(ids(&build_menu(&a)), ["refresh", "import"]);
}

#[test]
fn blank_menu_hides_import_and_new_table_when_read_only() {
    let all = ["refresh", "new-query", "batch-export", "import", "new-table"];
    let mut a = args(MenuKind::Blank, &all);
    a.show_new_table = true;
    assert_eq!(ids(&build_menu(&a)), ["refresh", "new-query", "batch-export", "import", "new-table"]);
    a.read_only = true;
    assert_eq!(ids(&build_menu(&a)), ["refresh", "new-query", "batch-export"]);
}

#[test]
fn other_kinds_only_offer_copy_name() {
    let a = args(MenuKind::Other, &["copy-name", "open"]);
    assert_eq!(ids(&build_menu(&a)), ["copy-name"]);
}

// ── hook flag derivation + danger SQL ────────────────────────────────────────

#[test]
fn context_flags_follow_read_only_and_capabilities() {
    let meta = ContextMetaVm {
        driver_read_only: false,
        saved_read_only: true,
        can_open_structure_editor: true,
        supports_er_diagram: true,
        supports_any_export: true,
        supports_full_table_export: true,
    };
    let flags = derive_context_flags(&meta);
    assert!(flags.read_only);
    assert!(flags.show_open_structure);
    assert!(flags.show_er_focus);
    assert!(flags.show_new_table == false, "read-only blocks new-table");
    assert_eq!(show_export_for_kind(&meta, MenuKind::Table), Some(false));
    assert_eq!(show_export_for_kind(&meta, MenuKind::View), Some(true));

    let capable = ContextMetaVm { saved_read_only: false, ..meta };
    assert!(derive_context_flags(&capable).show_new_table);
}

#[test]
fn danger_sql_and_confirm_gate() {
    assert_eq!(truncate_sql("\"users\"", None), "TRUNCATE TABLE \"users\"");
    assert_eq!(truncate_sql("\"users\"", Some("TRUNCATE \"users\"".to_owned())), "TRUNCATE \"users\"");
    assert_eq!(drop_sql("\"users\"", false), "DROP TABLE \"users\"");
    assert_eq!(drop_sql("\"v\"", true), "DROP VIEW \"v\"");
    assert!(may_run_dangerous(false, false, true));
    assert!(!may_run_dangerous(false, false, false), "decline blocks");
    assert!(!may_run_dangerous(false, true, true), "safe mode blocks");
    assert!(!may_run_dangerous(true, false, true), "read-only blocks");
}

#[test]
fn labels_are_caller_supplied_not_invented() {
    let mut a = args(MenuKind::Table, &["open"]);
    a.labels.insert("open", "自定义标签".to_owned());
    assert_eq!(build_menu(&a), vec![MenuEntry::Item { id: "open", label: "自定义标签".to_owned() }]);
    let _ = BTreeMap::<&str, String>::new();
}

// ── T608 independent audit (appended; reuses this module's helpers) ──────

/// React `useConnectionContextMenu.ts:326-327`:
/// `showBatchExport: kind === 'table' ? false : ctxBatchExportSupported`.
/// The port derives `show_batch_export` once from the driver meta with no
/// per-kind override (unlike `show_export_for_kind`), so a table keeps batch
/// export when the driver supports full-table export — which React forbids.
#[test]
fn audit_table_batch_export_is_suppressed_like_react() {
    let meta = ContextMetaVm {
        supports_any_export: true,
        supports_full_table_export: true,
        ..ContextMetaVm::default()
    };
    let flags = derive_context_flags(&meta);
    let mut a = args(MenuKind::Table, &["open", "batch-export", "truncate"]);
    a.show_export = show_export_for_kind(&meta, MenuKind::Table); // Some(false)
    // React forces showBatchExport = false for a table, whatever the driver says.
    a.show_batch_export = flags.show_batch_export;
    let menu = build_menu(&a);
    let rendered = ids(&menu);
    assert!(
        !rendered.contains(&"batch-export"),
        "React forces showBatchExport=false for tables; got {rendered:?}"
    );
}
