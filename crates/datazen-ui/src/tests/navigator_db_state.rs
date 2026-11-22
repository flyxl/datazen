//! T580: `navigator/useNavigatorDbState.ts` port — the per-database caches
//! and the refresh ladder. No React suite pins the hook in isolation (its
//! coverage lives in the host-level `ConnectionNavigatorTree.test.tsx`
//! blocks: refresh routing, drop-database fallback, session-focus rules);
//! these tests pin the same contracts in the machine's vocabulary: the
//! pointer-first toggle, background-neutral loads, the error row that is
//! not "no tables", the refresh routes by driver shape, and the
//! fingerprint-driven cache-refresh wave.

use std::collections::{HashMap, HashSet};

use crate::views::navigator::build_flat_rows::inputs::{
    ActiveEntrySnapshot, ConnectionSnapshot, DatabaseObject, DriverMeta, SchemaStateSnapshot,
    TableInfo, TableType,
};
use crate::views::navigator::use_navigator_db_state::{
    database_name_for_category, DbEffect, DbStateInputs, LoadForConnectionOpts, NavigatorDbState,
};

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn tbl(name: &str) -> TableInfo {
    TableInfo { name: name.to_owned(), schema: None, table_type: TableType::Table }
}

fn obj(name: &str) -> DatabaseObject {
    DatabaseObject { kind: "function".to_owned(), schema: None, name: name.to_owned() }
}

fn connected(session: &str) -> ActiveEntrySnapshot {
    ActiveEntrySnapshot {
        db_session_id: Some(session.to_owned()),
        status: "connected".to_owned(),
    }
}

fn multi_db_meta() -> DriverMeta {
    DriverMeta { has_multi_database: true, ..Default::default() }
}

fn conn(id: &str) -> ConnectionSnapshot {
    ConnectionSnapshot::new(id, id, "postgresql")
}

fn schema_state(databases: &[&str], epoch: u64) -> SchemaStateSnapshot {
    SchemaStateSnapshot {
        databases: databases.iter().map(|s| (*s).to_owned()).collect(),
        schema_epoch: epoch,
        ..Default::default()
    }
}

struct Inputs {
    active: HashMap<String, ActiveEntrySnapshot>,
    connections: Vec<ConnectionSnapshot>,
    expanded_dbs: HashSet<String>,
    expanded_cats: HashSet<String>,
    schemas: HashMap<String, SchemaStateSnapshot>,
    registry: HashMap<String, DriverMeta>,
}

impl Inputs {
    fn new() -> Self {
        Self {
            active: HashMap::from([("c1".to_owned(), connected("s1"))]),
            connections: vec![conn("c1")],
            expanded_dbs: HashSet::new(),
            expanded_cats: HashSet::new(),
            schemas: HashMap::from([("s1".to_owned(), schema_state(&["db1", "db2"], 0))]),
            registry: HashMap::from([("postgresql".to_owned(), multi_db_meta())]),
        }
    }

    fn inputs(&self) -> DbStateInputs<'_> {
        DbStateInputs {
            active_connections: &self.active,
            connections: &self.connections,
            expanded_dbs: &self.expanded_dbs,
            expanded_cats: &self.expanded_cats,
            schemas: &self.schemas,
            registry: &self.registry,
        }
    }
}

fn list_tables(session: &str, db: &str, owner: Option<&str>) -> DbEffect {
    DbEffect::ListTables {
        db_session_id: session.to_owned(),
        db_name: db.to_owned(),
        owner_connection_id: owner.map(str::to_owned),
    }
}

fn load_for(session: &str, opts: LoadForConnectionOpts) -> DbEffect {
    DbEffect::LoadForConnection { db_session_id: session.to_owned(), opts }
}

// ---------------------------------------------------------------------------
// databaseNameForCategory
// ---------------------------------------------------------------------------

#[test]
fn the_category_database_matches_the_longest_prefix_from_both_sources() {
    let expanded = HashSet::from(["c1::db_extra".to_owned(), "c1::db1".to_owned()]);
    let known = vec!["db".to_owned(), "db1".to_owned()];
    // "db1" wins over "db" (longest prefix), and both sources contribute.
    assert_eq!(
        database_name_for_category("c1", "c1::db1::public::routines", &expanded, &known),
        Some("db1".to_owned())
    );
    // Only the expanded keys know about db_extra.
    assert_eq!(
        database_name_for_category("c1", "c1::db_extra::routines", &expanded, &known),
        Some("db_extra".to_owned())
    );
    // Another connection's expanded keys do not leak in.
    let expanded_b = HashSet::from(["c2::db1".to_owned()]);
    assert_eq!(
        database_name_for_category("c1", "c1::db1::public::routines", &expanded_b, &[]),
        None
    );
    // A category key with no database in it resolves to nothing.
    assert_eq!(
        database_name_for_category("c1", "c1::routines", &expanded, &known),
        None
    );
    // A database name that itself contains `::` is a longer prefix than its
    // own head segment — the length sort exists for exactly this shape.
    let nested = vec!["a".to_owned(), "a::b".to_owned()];
    assert_eq!(
        database_name_for_category("c1", "c1::a::b::routines", &HashSet::new(), &nested),
        Some("a::b".to_owned())
    );
}

// ---------------------------------------------------------------------------
// table loads and errors
// ---------------------------------------------------------------------------

#[test]
fn a_successful_load_caches_clears_the_error_and_stays_neutral() {
    let mut state = NavigatorDbState::default();
    // A stale error from a previous failed read.
    let _ = state.apply_list_tables_err("s1", "db1", "boom".to_owned());
    let effects = state.apply_list_tables_ok("s1", "db1", vec![tbl("users")]);
    assert_eq!(
        effects,
        vec![
            DbEffect::SetLoadedTables {
                db_name: "db1".to_owned(),
                tables: vec![tbl("users")],
                db_session_id: "s1".to_owned(),
                // Background fills never pin: the pointer belongs to the
                // user's last click, not to whichever fan-out finishes last.
                pin_current_database: false,
            },
            DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() },
        ]
    );
    assert_eq!(state.tables_of("s1", "db1"), Some(&vec![tbl("users")]));
    assert!(state.db_errors().is_empty());
}

#[test]
fn a_failed_load_records_the_error_and_keeps_the_previous_list() {
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users")]);
    let effects = state.apply_list_tables_err("s1", "db1", "driver said no".to_owned());
    assert_eq!(
        effects,
        vec![
            DbEffect::RecordSessionError {
                db_session_id: "s1".to_owned(),
                message: "driver said no".to_owned(),
            },
            DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() },
        ]
    );
    // A cached list beats a failed refresh.
    assert_eq!(state.tables_of("s1", "db1"), Some(&vec![tbl("users")]));
    assert_eq!(state.db_errors().get("s1::db1").map(String::as_str), Some("driver said no"));

    // And on a session that had nothing: the key exists with an empty list,
    // paired with the error — a failed read must not read as "no tables".
    let mut fresh = NavigatorDbState::default();
    let _ = fresh.apply_list_tables_err("s2", "db9", "boom".to_owned());
    assert_eq!(fresh.tables_of("s2", "db9"), Some(&Vec::new()));
    assert!(fresh.db_errors().contains_key("s2::db9"));
}

#[test]
fn a_toggle_load_marks_loading_and_the_apply_clears_it() {
    let mut state = NavigatorDbState::default();
    let effects = state.toggle_db("s1", "db1");
    assert_eq!(
        effects,
        vec![
            // The pin happens first, synchronously — even before the read.
            DbEffect::SetCurrentDatabase {
                db_name: "db1".to_owned(),
                db_session_id: "s1".to_owned(),
            },
            list_tables("s1", "db1", None),
        ]
    );
    assert!(state.loading_dbs().contains("s1::db1"));
    // The read resolving clears the mark (React's `finally`), either way.
    let _ = state.apply_list_tables_ok("s1", "db1", vec![tbl("users")]);
    assert!(!state.loading_dbs().contains("s1::db1"));
    state.toggle_db("s1", "db2");
    assert!(state.loading_dbs().contains("s1::db2"));
    let _ = state.apply_list_tables_err("s1", "db2", "boom".to_owned());
    assert!(!state.loading_dbs().contains("s1::db2"));
}

#[test]
fn a_toggle_on_an_in_flight_load_only_pins() {
    let mut state = NavigatorDbState::default();
    let _ = state.toggle_db("s1", "db1"); // marks loading
    let effects = state.toggle_db("s1", "db1"); // second click while reading
    assert_eq!(
        effects,
        vec![DbEffect::SetCurrentDatabase {
            db_name: "db1".to_owned(),
            db_session_id: "s1".to_owned(),
        }]
    );
}

#[test]
fn a_toggle_on_a_cached_database_publishes_the_catalog_pinned() {
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users")]);
    let effects = state.toggle_db("s1", "db1");
    assert_eq!(
        effects,
        vec![
            DbEffect::SetCurrentDatabase {
                db_name: "db1".to_owned(),
                db_session_id: "s1".to_owned(),
            },
            DbEffect::SetLoadedTables {
                db_name: "db1".to_owned(),
                tables: vec![tbl("users")],
                db_session_id: "s1".to_owned(),
                // The user clicked: pin.
                pin_current_database: true,
            },
        ]
    );
}

// ---------------------------------------------------------------------------
// activation and the raw setters
// ---------------------------------------------------------------------------

#[test]
fn activate_database_publishes_the_cache_or_just_moves_the_pointer() {
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users")]);
    assert_eq!(
        state.activate_database("s1", "db1"),
        vec![DbEffect::SetLoadedTables {
            db_name: "db1".to_owned(),
            tables: vec![tbl("users")],
            db_session_id: "s1".to_owned(),
            pin_current_database: true,
        }]
    );
    // Cache miss: only the pointer moves (the store validates known dbs).
    assert_eq!(
        state.activate_database("s1", "db2"),
        vec![DbEffect::SetCurrentDatabase {
            db_name: "db2".to_owned(),
            db_session_id: "s1".to_owned(),
        }]
    );
}

#[test]
fn the_menu_refresh_after_mutation_drops_one_row() {
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users"), tbl("logs")]);
    assert!(state.drop_relation_row("s1", "db1", "users"));
    assert_eq!(state.tables_of("s1", "db1"), Some(&vec![tbl("logs")]));
    assert!(!state.drop_relation_row("s1", "db1", "gone"));
    assert!(!state.drop_relation_row("s9", "db1", "logs"));
}

// ---------------------------------------------------------------------------
// open databases and cache clearing
// ---------------------------------------------------------------------------

#[test]
fn the_open_databases_marker_follows_the_read_result() {
    let mut state = NavigatorDbState::default();
    assert_eq!(
        state.refresh_open_databases("s1"),
        vec![DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() }]
    );
    state.apply_open_databases("s1", Ok(vec!["db1".to_owned(), "db2".to_owned()]));
    assert_eq!(
        state.open_dbs().get("s1"),
        Some(&HashSet::from(["db1".to_owned(), "db2".to_owned()]))
    );
    // A failed read (the session is gone) drops the entry: a stale marker
    // cannot outlive the pool it described.
    state.apply_open_databases("s1", Err("session gone".to_owned()));
    assert!(!state.open_dbs().contains_key("s1"));
}

#[test]
fn clearing_a_database_releases_everything_it_owned() {
    let mut state = NavigatorDbState::default();
    state.apply_open_databases("s1", Ok(vec!["db1".to_owned()]));
    state.set_tables("s1", "db1", vec![tbl("users")]);
    let _ = state.apply_list_tables_err("s1", "db2", "boom".to_owned());
    state.apply_database_objects("c1::db1::public::routines", Ok(vec![obj("fn_a")]));
    state.apply_database_objects("c1::db2::public::routines", Ok(vec![obj("fn_b")]));

    state.clear_db_local_cache("c1", "s1", "db1");
    assert!(state.open_dbs().get("s1").is_none_or(|set| !set.contains("db1")));
    assert_eq!(state.tables_of("s1", "db1"), None);
    // db2's caches are untouched.
    assert!(state.tables_of("s1", "db2").is_some());
    assert!(state.db_errors().contains_key("s1::db2"));
    // db1's object caches are gone, prefix-scoped; db2's survive.
    assert!(!state.db_objects_map().contains_key("c1::db1::public::routines"));
    assert!(state.db_objects_map().contains_key("c1::db2::public::routines"));
}

#[test]
fn clearing_a_session_clears_its_tables_and_optionally_its_connection_objects() {
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users")]);
    state.set_tables("s2", "db1", vec![tbl("logs")]);
    state.apply_database_objects("c1::db1::public::routines", Ok(vec![obj("fn_a")]));
    state.apply_database_objects("c2::db1::public::routines", Ok(vec![obj("fn_b")]));

    state.clear_session_caches("s1", Some("c1"));
    assert_eq!(state.tables_of("s1", "db1"), None);
    assert!(state.tables_of("s2", "db1").is_some());
    assert!(!state.db_objects_map().contains_key("c1::db1::public::routines"));
    assert!(state.db_objects_map().contains_key("c2::db1::public::routines"));
}

// ---------------------------------------------------------------------------
// object categories
// ---------------------------------------------------------------------------

#[test]
fn object_categories_load_once_and_never_for_tables_or_views() {
    let mut state = NavigatorDbState::default();
    // Tables and views live in the table cache.
    assert!(state.reload_db_object_category("s1", "k", "tables", "db1").is_empty());
    assert!(state.reload_db_object_category("s1", "k", "views", "db1").is_empty());
    let effects = state.reload_db_object_category("s1", "c1::db1::public::routines", "routines", "db1");
    assert_eq!(
        effects,
        vec![DbEffect::GetDatabaseObjects {
            db_session_id: "s1".to_owned(),
            cat_id: "routines".to_owned(),
            db_name: "db1".to_owned(),
        }]
    );
    state.apply_database_objects("c1::db1::public::routines", Ok(vec![obj("fn_a")]));
    // Cached: the toggle does nothing.
    assert!(state
        .toggle_category_load("s1", "c1::db1::public::routines", "routines", "db1")
        .is_empty());
    // A failed read caches an empty list — empty, not spinning forever.
    state.apply_database_objects("c1::db1::public::triggers", Err("boom".to_owned()));
    assert_eq!(
        state.db_objects_map().get("c1::db1::public::triggers"),
        Some(&Vec::new())
    );
}

// ---------------------------------------------------------------------------
// the refresh ladder
// ---------------------------------------------------------------------------

#[test]
fn refresh_connection_routes_by_driver_shape() {
    let mut inputs = Inputs::new();
    inputs.connections[0].database = None; // multi-db (no pinned database)
    inputs.expanded_dbs = HashSet::from(["c1::db1".to_owned(), "c1::db2".to_owned()]);
    inputs.expanded_cats = HashSet::from(["c1::db1::public::routines".to_owned()]);
    let state = NavigatorDbState::default();

    // Multi-db: skip load-tables in the loader, reload each expanded db,
    // then the object categories.
    let effects = state.refresh_connection(&inputs.inputs(), "c1");
    assert_eq!(
        effects,
        vec![
            load_for(
                "s1",
                LoadForConnectionOpts {
                    preferred_database: None,
                    skip_load_tables: true,
                    database_type: "postgresql".to_owned(),
                    activate: false,
                },
            ),
            DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() },
            list_tables("s1", "db1", Some("c1")),
            list_tables("s1", "db2", Some("c1")),
            DbEffect::GetDatabaseObjects {
                db_session_id: "s1".to_owned(),
                cat_id: "routines".to_owned(),
                db_name: "db1".to_owned(),
            },
        ]
    );

    // Single-db (a pinned logical database): the loader runs with tables
    // and no per-db reloads.
    let mut single = Inputs::new();
    single.connections[0].database = Some("db1".to_owned());
    single.expanded_dbs = HashSet::from(["c1::db1".to_owned()]);
    let effects = state.refresh_connection(&single.inputs(), "c1");
    assert_eq!(
        effects,
        vec![
            load_for(
                "s1",
                LoadForConnectionOpts {
                    preferred_database: Some("db1".to_owned()),
                    skip_load_tables: false,
                    database_type: "postgresql".to_owned(),
                    activate: false,
                },
            ),
            DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() },
        ]
    );
}

#[test]
fn a_path_hierarchy_driver_ensures_its_root_path_on_refresh() {
    let mut inputs = Inputs::new();
    let mut meta = multi_db_meta();
    meta.namespace_ensure = Some("path-hierarchy".to_owned());
    inputs.registry.insert("postgresql".to_owned(), meta);
    let state = NavigatorDbState::default();
    let effects = state.refresh_connection(&inputs.inputs(), "c1");
    assert_eq!(effects[2], DbEffect::EnsureNamespacePath {
        segments: Vec::new(),
        db_session_id: "s1".to_owned(),
    });
    // And its loader skips tables (plugin-managed).
    assert!(matches!(
        &effects[0],
        DbEffect::LoadForConnection { opts, .. } if opts.skip_load_tables
    ));
}

#[test]
fn a_custom_tree_driver_skips_table_loads_without_the_namespace_ensure() {
    let mut inputs = Inputs::new();
    let mut meta = multi_db_meta();
    meta.schema_tree_mode = Some("custom".to_owned());
    inputs.registry.insert("postgresql".to_owned(), meta);
    let state = NavigatorDbState::default();
    let effects = state.refresh_connection(&inputs.inputs(), "c1");
    assert!(matches!(
        &effects[0],
        DbEffect::LoadForConnection { opts, .. } if opts.skip_load_tables
    ));
    assert!(!effects.iter().any(|e| matches!(e, DbEffect::EnsureNamespacePath { .. })));
}

#[test]
fn refresh_connection_requires_a_connected_session_and_a_known_connection() {
    let mut inputs = Inputs::new();
    let state = NavigatorDbState::default();
    // Idle entry: nothing.
    inputs.active.insert(
        "c1".to_owned(),
        ActiveEntrySnapshot { db_session_id: Some("s1".to_owned()), status: "idle".to_owned() },
    );
    assert!(state.refresh_connection(&inputs.inputs(), "c1").is_empty());
    // Connected but the config is gone: nothing.
    inputs.active.insert("c1".to_owned(), connected("s1"));
    inputs.connections.clear();
    assert!(state.refresh_connection(&inputs.inputs(), "c1").is_empty());
    // Unknown connection id: nothing.
    inputs.connections = vec![conn("c1")];
    assert!(state.refresh_connection(&inputs.inputs(), "c9").is_empty());
}

#[test]
fn refresh_all_fans_out_to_the_connected_entries_only() {
    let mut inputs = Inputs::new();
    inputs.active.insert(
        "c2".to_owned(),
        ActiveEntrySnapshot { db_session_id: Some("s2".to_owned()), status: "connected".to_owned() },
    );
    inputs.active.insert(
        "c3".to_owned(),
        ActiveEntrySnapshot { db_session_id: Some("s3".to_owned()), status: "idle".to_owned() },
    );
    inputs.connections = vec![conn("c1"), conn("c2")];
    let state = NavigatorDbState::default();
    let effects = state.refresh_all_connections(&inputs.inputs());
    // Two loaders (c1 and c2) and nothing for the idle c3.
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, DbEffect::LoadForConnection { .. }))
            .count(),
        2
    );
    assert!(effects
        .iter()
        .all(|e| !matches!(e, DbEffect::LoadForConnection { db_session_id, .. } if db_session_id == "s3")));
}

#[test]
fn refresh_database_routes_by_driver_shape_and_reloads_its_categories() {
    let mut inputs = Inputs::new();
    inputs.connections[0].database = None; // multi-db
    inputs.expanded_cats = HashSet::from([
        "c1::db1::public::routines".to_owned(),
        "c1::db2::public::routines".to_owned(),
        "c1::db1::public::tables".to_owned(), // tables: not an object read
    ]);
    let state = NavigatorDbState::default();
    let effects = state.refresh_database(&inputs.inputs(), "c1", "db1");
    assert_eq!(
        effects,
        vec![
            list_tables("s1", "db1", Some("c1")),
            DbEffect::GetDatabaseObjects {
                db_session_id: "s1".to_owned(),
                cat_id: "routines".to_owned(),
                db_name: "db1".to_owned(),
            },
        ]
    );

    // Single-db driver: the loader with the database preferred, activate
    // false — refreshing one database must not re-focus the session.
    let mut single = Inputs::new();
    single.connections[0].database = Some("db1".to_owned());
    let effects = state.refresh_database(&single.inputs(), "c1", "db9");
    assert_eq!(
        effects,
        vec![
            load_for(
                "s1",
                LoadForConnectionOpts {
                    preferred_database: Some("db9".to_owned()),
                    skip_load_tables: false,
                    database_type: "postgresql".to_owned(),
                    activate: false,
                },
            ),
            DbEffect::GetOpenDatabases { db_session_id: "s1".to_owned() },
        ]
    );
}

#[test]
fn refresh_schema_reloads_the_tables_and_only_its_own_categories() {
    let mut inputs = Inputs::new();
    inputs.expanded_cats = HashSet::from([
        "c1::db1::public::routines".to_owned(),
        "c1::db1::other::triggers".to_owned(),
        "c1::db2::public::routines".to_owned(),
    ]);
    let state = NavigatorDbState::default();
    let effects = state.refresh_schema(&inputs.inputs(), "c1", "db1", "public");
    assert_eq!(
        effects,
        vec![
            list_tables("s1", "db1", Some("c1")),
            DbEffect::GetDatabaseObjects {
                db_session_id: "s1".to_owned(),
                cat_id: "routines".to_owned(),
                db_name: "db1".to_owned(),
            },
        ]
    );
}

#[test]
fn reload_expanded_object_categories_resolves_each_databases_categories() {
    let mut inputs = Inputs::new();
    inputs.expanded_cats = HashSet::from([
        "c1::db1::public::routines".to_owned(),
        "c1::db1::public::tables".to_owned(),
        "c1::db1::public::triggers".to_owned(),
        "c2::db1::public::routines".to_owned(),
        "c1::routines".to_owned(), // no database in the key: skipped
    ]);
    inputs.expanded_dbs = HashSet::from(["c1::db1".to_owned()]);
    let state = NavigatorDbState::default();
    let effects = state.reload_expanded_object_categories(&inputs.inputs(), "c1", "s1");
    let mut loaded: Vec<&str> = effects
        .iter()
        .filter_map(|e| match e {
            DbEffect::GetDatabaseObjects { cat_id, .. } => Some(cat_id.as_str()),
            _ => None,
        })
        .collect();
    loaded.sort_unstable();
    assert_eq!(loaded, vec!["routines", "triggers"]);
}

// ---------------------------------------------------------------------------
// the expanded-cache-refresh watch
// ---------------------------------------------------------------------------

#[test]
fn the_cache_refresh_watch_records_first_and_reloads_only_on_change() {
    let mut inputs = Inputs::new();
    inputs.expanded_dbs = HashSet::from(["c1::db1".to_owned()]);
    inputs.expanded_cats = HashSet::from(["c1::db1::public::routines".to_owned()]);
    let mut state = NavigatorDbState::default();
    state.set_tables("s1", "db1", vec![tbl("users")]);

    // First pass: records the fingerprint, reloads nothing.
    assert!(state.cache_refresh_pass(&inputs.inputs()).is_empty());
    assert_eq!(state.tables_of("s1", "db1"), Some(&vec![tbl("users")]));

    // Same fingerprint: nothing.
    assert!(state.cache_refresh_pass(&inputs.inputs()).is_empty());

    // A bumped epoch: the caches clear and the recovery wave is scheduled
    // in the same pass — tables for the expanded db, objects for the
    // expanded category.
    inputs.schemas.insert("s1".to_owned(), schema_state(&["db1", "db2"], 1));
    let effects = state.cache_refresh_pass(&inputs.inputs());
    assert_eq!(
        effects,
        vec![
            list_tables("s1", "db1", None),
            DbEffect::GetDatabaseObjects {
                db_session_id: "s1".to_owned(),
                cat_id: "routines".to_owned(),
                db_name: "db1".to_owned(),
            },
        ]
    );
    // The wipe already happened when the wave was scheduled.
    assert_eq!(state.tables_of("s1", "db1"), None);
}

#[test]
fn a_changed_database_list_also_trips_the_fingerprint() {
    let mut inputs = Inputs::new();
    let mut state = NavigatorDbState::default();
    let _ = state.cache_refresh_pass(&inputs.inputs()); // record
    state.set_tables("s1", "db1", vec![tbl("users")]);
    // Nothing is expanded, so the recovery wave is empty — but the wipe the
    // wave rides on still happened.
    inputs.schemas.insert("s1".to_owned(), schema_state(&["db1", "db2", "db3"], 0));
    assert!(state.cache_refresh_pass(&inputs.inputs()).is_empty());
    assert_eq!(state.tables_of("s1", "db1"), None);
}

#[test]
fn a_session_that_disappears_is_simply_forgotten() {
    let mut inputs = Inputs::new();
    let mut state = NavigatorDbState::default();
    let _ = state.cache_refresh_pass(&inputs.inputs()); // record s1
    // The session drops out of the active map entirely: no effects, no
    // clear for a session that no longer exists.
    inputs.active.remove("c1");
    assert!(state.cache_refresh_pass(&inputs.inputs()).is_empty());
}
