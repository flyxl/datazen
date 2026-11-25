//! T582: `navigator/utils.ts` — the formal port's tests. Two React suites
//! pin this file: `navigator/__tests__/utils.test.ts` (the namespace leaf
//! context) and `navigatorTreeKeyContract.test.ts` (the row-key contract —
//! every case below is a bug that actually shipped once). The helpers that
//! arrived with T570/T578 (grouping, quoting, fallbacks) keep their coverage
//! in the builder and menu suites; the unit contracts live here now.

use std::collections::HashMap;

use datazen_ui::views::navigator::build_flat_rows::inputs::{TableInfo, TableType};
use datazen_ui::views::navigator::types::{row_key as get_unified_row_key, NavigatorRow};
use datazen_ui::views::navigator::utils::{
    create_drag_ghost, depth_padding, namespace_leaf_context, resolve_drop_database_fallback,
    should_use_multi_database_tree, DRAG_GHOST_ID,
};

// ---------------------------------------------------------------------------
// namespaceLeafContext (React utils.test.ts, case for case)
// ---------------------------------------------------------------------------

#[test]
fn maps_superset_style_segments_to_the_driver_fetch_path() {
    let aliases = HashMap::from([("presto_afi_data".to_owned(), "558".to_owned())]);
    let ctx = namespace_leaf_context(
        &["presto_afi_data", "hive", "snap", "orders"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<String>>(),
        &aliases,
    );
    assert_eq!(ctx.table_name, "orders");
    assert_eq!(ctx.schema.as_deref(), Some("snap"));
    assert_eq!(ctx.database.as_deref(), Some("558/hive/snap"));
}

#[test]
fn returns_the_table_name_only_for_single_segment_leaves() {
    let ctx = namespace_leaf_context(
        &["users".to_owned()],
        &HashMap::<String, String>::new(),
    );
    assert_eq!(ctx.table_name, "users");
    assert_eq!(ctx.schema, None);
    assert_eq!(ctx.database, None);
}

#[test]
fn the_empty_path_and_the_alias_less_root_follow_the_react_rules() {
    // No segments at all: the empty context.
    let ctx = namespace_leaf_context(&[], &HashMap::new());
    assert_eq!(ctx.table_name, "");
    assert_eq!(ctx.schema, None);
    assert_eq!(ctx.database, None);

    // Two segments without an alias: the root passes through unchanged and
    // no schema is named (the schema needs depth 2+).
    let ctx = namespace_leaf_context(
        &["root".to_owned(), "orders".to_owned()],
        &HashMap::new(),
    );
    assert_eq!(ctx.table_name, "orders");
    assert_eq!(ctx.schema, None);
    assert_eq!(ctx.database.as_deref(), Some("root"));

    // Three segments: the middle joins the path and the last parent is the
    // schema.
    let ctx = namespace_leaf_context(
        &["root".to_owned(), "area".to_owned(), "orders".to_owned()],
        &HashMap::new(),
    );
    assert_eq!(ctx.database.as_deref(), Some("root/area"));
    assert_eq!(ctx.schema.as_deref(), Some("area"));
}

// ---------------------------------------------------------------------------
// the row-key contract (React navigatorTreeKeyContract.test.ts)
// ---------------------------------------------------------------------------

fn dp(depth: usize) -> (usize, usize) {
    (depth, depth)
}

fn section_row(section: &'static str) -> NavigatorRow {
    NavigatorRow::Section {
        depth_pair: dp(0),
        section,
        display_name: section.to_owned(),
        count: 0,
        expanded: true,
    }
}

fn group_row(group_name: &str) -> NavigatorRow {
    NavigatorRow::Group {
        depth_pair: dp(0),
        group_name: group_name.to_owned(),
        display_name: group_name.to_owned(),
        count: 0,
        expanded: true,
    }
}

fn connection_row(id: &str, section_group: &str) -> NavigatorRow {
    NavigatorRow::Connection {
        depth_pair: dp(1),
        id: id.to_owned(),
        name: id.to_owned(),
        engine: "postgresql".to_owned(),
        section_group: section_group.to_owned(),
        is_selected: false,
        status: "disconnected".to_owned(),
        expanded: false,
        match_payload: None,
    }
}

fn db_row(db_name: &str) -> NavigatorRow {
    NavigatorRow::Db {
        depth_pair: dp(2),
        connection_id: "c1".to_owned(),
        db_session_id: "sess-c1".to_owned(),
        db_name: db_name.to_owned(),
        expanded: false,
        loading: false,
        is_open: false,
    }
}

fn loading_row(owner_key: &str) -> NavigatorRow {
    NavigatorRow::DbLoading { depth_pair: dp(2), owner_key: owner_key.to_owned() }
}

fn object_row(
    connection_id: &str,
    db_name: &str,
    schema_name: Option<&str>,
    cat_id: &'static str,
) -> NavigatorRow {
    NavigatorRow::Object {
        depth_pair: dp(4),
        obj: datazen_ui::views::navigator::build_flat_rows::inputs::DatabaseObject {
            kind: "function".to_owned(),
            schema: None,
            name: "fn_calc".to_owned(),
        },
        cat_id,
        connection_id: connection_id.to_owned(),
        db_name: db_name.to_owned(),
        schema_name: schema_name.map(str::to_owned),
    }
}

#[test]
fn gives_the_same_row_the_same_key_wherever_it_is_in_the_list() {
    // A list rebuild allocates fresh objects; identity is in the fields.
    let row = object_row("c1", "db1", Some("public"), "routines");
    let clone = row.clone();
    assert_eq!(get_unified_row_key(&clone), get_unified_row_key(&row));
}

#[test]
fn object_rows_separate_by_owner() {
    // The shipped bug: keys were `(catId, name)`, so every connection holding
    // `fn_calc` produced one and the same key.
    assert_ne!(
        get_unified_row_key(&object_row("c1", "db1", Some("public"), "routines")),
        get_unified_row_key(&object_row("c2", "db1", Some("public"), "routines"))
    );
    // Two databases of one connection.
    assert_ne!(
        get_unified_row_key(&object_row("c1", "db1", Some("public"), "routines")),
        get_unified_row_key(&object_row("c1", "db2", Some("public"), "routines"))
    );
    // Two schemas of one database.
    assert_ne!(
        get_unified_row_key(&object_row("c1", "db1", Some("public"), "routines")),
        get_unified_row_key(&object_row("c1", "db1", Some("private"), "routines"))
    );
    // Two categories.
    assert_ne!(
        get_unified_row_key(&object_row("c1", "db1", Some("public"), "routines")),
        get_unified_row_key(&object_row("c1", "db1", Some("public"), "views"))
    );
    // A missing schema and an empty one read the same in the key — the same
    // place on screen — and the assertion pins that they stay that way.
    assert_eq!(
        get_unified_row_key(&object_row("c1", "db1", None, "routines")),
        get_unified_row_key(&object_row("c1", "db1", Some(""), "routines"))
    );
}

#[test]
fn loading_rows_are_named_for_the_database_they_stand_in_for() {
    // The shipped bug: keyed by list position, so database A's spinner became
    // database B's as soon as A scrolled out of the window.
    assert_eq!(
        get_unified_row_key(&loading_row("db:c1::db1")),
        "loading:db:c1::db1"
    );
    // Two loading databases keep two spinners apart.
    assert_ne!(
        get_unified_row_key(&loading_row("db:c1::db1")),
        get_unified_row_key(&loading_row("db:c1::db2"))
    );
    // `db:` and `loading:` are distinct namespaces on purpose: the
    // placeholder and the real row never share a node, so the swap cannot
    // strand state.
    assert_ne!(
        get_unified_row_key(&loading_row("db:c1::db1")),
        get_unified_row_key(&db_row("db1"))
    );
}

#[test]
fn empty_group_rows_are_named_for_the_group() {
    // The shipped bug: `empty:${row.groupName ?? index}` meant the hint row
    // renamed itself whenever anything above it appeared or disappeared.
    let row = NavigatorRow::EmptyGroup {
        depth_pair: dp(1),
        group_name: "recent".to_owned(),
    };
    assert_eq!(get_unified_row_key(&row), "empty:recent");
    let other = NavigatorRow::EmptyGroup {
        depth_pair: dp(1),
        group_name: "pinned".to_owned(),
    };
    assert_ne!(get_unified_row_key(&row), get_unified_row_key(&other));
}

#[test]
fn every_row_variant_keys_in_its_own_namespace() {
    // The prefixes are written out because they are not derivable from the
    // type name — `group` keys as `grp:`, `namespace-node` as `ns:` — and a
    // placeholder that accidentally matches a real one is the failure worth
    // catching.
    let rows: Vec<(NavigatorRow, &str)> = vec![
        (section_row("pinned"), "sec:"),
        (group_row("g"), "grp:"),
        (connection_row("c1", "g"), "conn:"),
        (db_row("db1"), "db:"),
        (
            NavigatorRow::Schema {
                depth_pair: dp(3),
                connection_id: "c1".to_owned(),
                db_name: "db1".to_owned(),
                schema_name: "public".to_owned(),
                expanded: true,
            },
            "schema:",
        ),
        (
            NavigatorRow::KvDb {
                depth_pair: dp(2),
                connection_id: "c1".to_owned(),
                db_session_id: "sess-c1".to_owned(),
                db_name: "db1".to_owned(),
                is_selected: false,
                db_counts_command: None,
            },
            "kv:",
        ),
        (loading_row("db:c1::db1"), "loading:"),
        (NavigatorRow::NoConnections { depth_pair: dp(0) }, "no-connections"),
        (
            NavigatorRow::EmptyGroup { depth_pair: dp(1), group_name: "g".to_owned() },
            "empty:",
        ),
        (
            NavigatorRow::NamespaceNode {
                depth_pair: dp(3),
                name: "tbl".to_owned(),
                expanded: false,
                is_leaf: false,
                leaf_kind: None,
                segments: vec!["tbl".to_owned()],
                key: "ns1".to_owned(),
                connection_id: "c1".to_owned(),
                db_session_id: "sess-c1".to_owned(),
            },
            "ns:",
        ),
        (object_row("c1", "db1", Some("public"), "routines"), "obj:"),
        // The table row, whose key carries the owner tuple like the object's.
        (
            NavigatorRow::Table {
                depth_pair: dp(5),
                item: TableInfo {
                    name: "users".to_owned(),
                    schema: Some("public".to_owned()),
                    table_type: TableType::Table,
                },
                cat_id: "tables",
                is_selected: false,
                connection_id: "c1".to_owned(),
                db_session_id: "sess-c1".to_owned(),
                db_name: "db1".to_owned(),
            },
            "tbl:",
        ),
        // The error row, sharing the loading row's owner-key namespace with
        // its own prefix.
        (
            NavigatorRow::DbError {
                depth_pair: dp(2),
                owner_key: "db:c1::db1".to_owned(),
                message: "boom".to_owned(),
            },
            "error:",
        ),
    ];
    for (row, prefix) in rows {
        let key = get_unified_row_key(&row);
        assert!(!key.is_empty(), "no key for {}", row.kind());
        assert!(
            key.starts_with(prefix),
            "prefix for {}: {key}",
            row.kind()
        );
    }
}

#[test]
fn two_rows_in_one_list_get_two_different_keys() {
    // The property the shell actually depends on: keys unique within the
    // painted list, without consulting the list itself.
    let list = vec![
        section_row("pinned"),
        group_row("g"),
        connection_row("c1", "g"),
        db_row("db1"),
        loading_row("db:c1::db1"),
        object_row("c1", "db1", Some("public"), "routines"),
        object_row("c1", "db1", Some("public"), "views"),
        NavigatorRow::EmptyGroup { depth_pair: dp(1), group_name: "g".to_owned() },
    ];
    let keys: Vec<String> = list.iter().map(get_unified_row_key).collect();
    let unique: std::collections::HashSet<&String> = keys.iter().collect();
    assert_eq!(unique.len(), keys.len());
}

// ---------------------------------------------------------------------------
// the remaining exports' unit contracts
// ---------------------------------------------------------------------------

#[test]
fn depth_padding_is_the_chevron_gutter_plus_one_rung_per_level() {
    assert_eq!(depth_padding(0), 0.375);
    assert_eq!(depth_padding(1), 1.375);
    assert_eq!(depth_padding(4), 4.375);
}

#[test]
fn the_multi_db_decision_follows_the_domain_rule() {
    let dbs = |d: Option<&str>| d.map(str::to_owned);
    // Not multi-db: the driver cannot, or a logical database is pinned.
    assert!(!should_use_multi_database_tree(false, "name", None));
    assert!(!should_use_multi_database_tree(true, "name", dbs(Some("db1")).as_deref()));
    // Multi-db: nothing pinned (a blank or whitespace-only database reads
    // as unpinned — React's `!initialDatabase?.trim()`), or the field is a
    // domain (Kiwi).
    assert!(should_use_multi_database_tree(true, "name", dbs(Some("  ")).as_deref()));
    assert!(should_use_multi_database_tree(true, "name", None));
    assert!(should_use_multi_database_tree(true, "name", dbs(Some("")).as_deref()));
    assert!(should_use_multi_database_tree(true, "domain", dbs(Some("my-domain")).as_deref()));
}

#[test]
fn the_drop_fallback_picks_postgres_first() {
    // postgres wins even when another database would do.
    let dbs = vec!["db1".to_owned(), "db2".to_owned(), "postgres".to_owned()];
    assert_eq!(
        resolve_drop_database_fallback(&dbs, "db1", None),
        Some("postgres".to_owned())
    );
    // Dropping postgres itself: the configured database, then any other —
    // never the dropped one.
    assert_eq!(
        resolve_drop_database_fallback(&dbs, "postgres", Some("db1")),
        Some("db1".to_owned())
    );
    assert_eq!(
        resolve_drop_database_fallback(&dbs, "postgres", None),
        Some("db1".to_owned())
    );
    // No postgres, the configured database is the dropped one: whatever
    // else remains — not the dropped one, not nothing.
    let no_pg = vec!["db1".to_owned(), "db2".to_owned()];
    assert_eq!(
        resolve_drop_database_fallback(&no_pg, "db1", Some("db1")),
        Some("db2".to_owned())
    );
    // A configured database the list does not hold reads as absent.
    assert_eq!(
        resolve_drop_database_fallback(&no_pg, "db1", Some("gone")),
        Some("db2".to_owned())
    );
    // The last database standing: nowhere to go.
    let lone = vec!["db1".to_owned()];
    assert_eq!(resolve_drop_database_fallback(&lone, "db1", None), None);
}

#[test]
fn the_drag_ghost_spec_is_the_pill_the_host_mounts() {
    let ghost = create_drag_ghost("My Connection");
    assert_eq!(ghost.id, DRAG_GHOST_ID);
    assert_eq!(ghost.label, "My Connection");
    assert_eq!(ghost.padding, (4.0, 10.0));
    assert_eq!(ghost.radius, 6.0);
    assert_eq!(ghost.font_size, 12.0);
}
