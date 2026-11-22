//! T570: the navigator flat-row builder, ported from React's
//! `navigator/buildFlatRows.ts`. The tests pin the same contracts the React
//! suite pins (its `buildFlatRows.dbError.test.ts` runs here almost
//! case-for-case), plus the contracts its deps carry: the depth ladder, the
//! search rules, the locator ranking, the object filter, and the stable row
//! keys.

use std::collections::{HashMap, HashSet};

use crate::views::navigator::build_flat_rows::deps::sql_namespace::SqlNamespace;
use crate::views::navigator::build_flat_rows::inputs::{
    ActiveEntrySnapshot, ConnectionSnapshot, DatabaseObject, DriverMeta, GroupedSection,
    SchemaStateSnapshot, TableInfo, TableType,
};
use crate::views::navigator::build_flat_rows::row_model::{
    row_key as navigator_row_key, NavigatorRow,
};
use crate::views::navigator::build_flat_rows::{build_navigator_flat_rows, BuildFlatRowsParams};

// ---------------------------------------------------------------------------
// fixtures (the shapes React's own tests build)
// ---------------------------------------------------------------------------

fn tbl(name: &str, table_type: TableType) -> TableInfo {
    TableInfo {
        name: name.to_owned(),
        schema: None,
        table_type,
    }
}

fn tbl_in(schema: &str, name: &str, table_type: TableType) -> TableInfo {
    TableInfo {
        name: name.to_owned(),
        schema: Some(schema.to_owned()),
        table_type,
    }
}

fn obj(kind: &str, name: &str) -> DatabaseObject {
    DatabaseObject {
        kind: kind.to_owned(),
        schema: None,
        name: name.to_owned(),
    }
}

fn connected_entry(session: &str) -> ActiveEntrySnapshot {
    ActiveEntrySnapshot {
        db_session_id: Some(session.to_owned()),
        status: "connected".to_owned(),
    }
}

fn mysql_meta() -> DriverMeta {
    DriverMeta {
        has_multi_database: true,
        ..Default::default()
    }
}

fn base_params() -> BuildFlatRowsParams {
    BuildFlatRowsParams {
        expanded_groups: HashSet::from([String::new()]),
        expanded_connections: HashSet::from(["::c1".to_owned()]),
        db_registry: HashMap::from([("mysql".to_owned(), mysql_meta())]),
        ..Default::default()
    }
}

fn rows_of(params: &BuildFlatRowsParams) -> Vec<NavigatorRow> {
    build_navigator_flat_rows(params)
}

fn kinds(rows: &[NavigatorRow]) -> Vec<&'static str> {
    rows.iter().map(|row| row.kind()).collect()
}

fn first_of<'a>(rows: &'a [NavigatorRow], kind: &str) -> &'a NavigatorRow {
    rows.iter()
        .find(|row| row.kind() == kind)
        .unwrap_or_else(|| panic!("no {kind} row"))
}

fn mysql_conn() -> ConnectionSnapshot {
    ConnectionSnapshot::new("c1", "MySQL", "mysql")
}

// ---------------------------------------------------------------------------
// the section/group ladder
// ---------------------------------------------------------------------------

#[test]
fn empty_grouped_emits_the_no_connections_row() {
    let mut params = base_params();
    params.grouped = Vec::new();
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["no-connections"]);
    assert_eq!(navigator_row_key(&rows[0]), "no-connections");
}

#[test]
fn a_group_header_precedes_its_connections_and_respects_the_expand_set() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: "g1".to_owned(),
        connections: vec![mysql_conn()],
    }];
    // Not in the expand set: header only, connection itself is not expanded.
    params.expanded_groups = HashSet::new();
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group"]);
    let NavigatorRow::Group {
        group_name,
        display_name,
        count,
        expanded,
        depth_pair,
    } = &rows[0]
    else {
        panic!()
    };
    assert_eq!(group_name, "g1");
    assert_eq!(display_name, "g1"); // custom keys pass through verbatim
    assert_eq!(*count, 1);
    assert!(!*expanded);
    assert_eq!(*depth_pair, (0, 0));
}

#[test]
fn preset_group_keys_get_their_localized_labels() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: "production".to_owned(),
        connections: vec![mysql_conn()],
    }];
    params.expanded_groups = HashSet::new();
    let rows = rows_of(&params);
    let NavigatorRow::Group { display_name, .. } = &rows[0] else {
        panic!()
    };
    // The generated zh-CN table supplies the label; the key never leaks.
    assert_ne!(display_name, "production");
    assert!(!display_name.is_empty());
}

#[test]
fn an_expanded_empty_group_shows_the_hint_row() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: "g1".to_owned(),
        connections: vec![],
    }];
    params.expanded_groups = HashSet::from(["g1".to_owned()]);
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "empty-group"]);
    let NavigatorRow::EmptyGroup {
        group_name,
        depth_pair,
    } = &rows[1]
    else {
        panic!()
    };
    assert_eq!(group_name, "g1");
    assert_eq!(*depth_pair, (1, 1));
    assert_eq!(navigator_row_key(&rows[1]), "empty:g1");
}

#[test]
fn pinned_and_recent_sections_use_section_rows_not_group_rows() {
    let mut params = base_params();
    params.grouped = vec![
        GroupedSection {
            group: "__pinned__".to_owned(),
            connections: vec![mysql_conn()],
        },
        GroupedSection {
            group: "__recent__".to_owned(),
            connections: vec![ConnectionSnapshot::new("c2", "Recent", "mysql")],
        },
    ];
    params.expanded_groups = HashSet::from(["__pinned__".to_owned(), "__recent__".to_owned()]);
    let rows = rows_of(&params);
    let sections: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "section").collect();
    assert_eq!(sections.len(), 2);
    assert_eq!(navigator_row_key(sections[0]), "sec:pinned");
    assert_eq!(navigator_row_key(sections[1]), "sec:recent");
    // Under a search the sections collapse into one unnamed flat section, so
    // neither section nor group headers appear — the connections stand alone.
    let mut searching = params.clone();
    searching.query = "mysql".to_owned();
    let rows = rows_of(&searching);
    assert!(!rows.iter().any(|row| row.kind() == "section"));
    assert!(!rows.iter().any(|row| row.kind() == "group"));
    assert!(rows.iter().any(|row| row.kind() == "connection"));
}

#[test]
fn connections_dedupe_across_sections_by_id() {
    let mut params = base_params();
    params.grouped = vec![
        GroupedSection {
            group: "__pinned__".to_owned(),
            connections: vec![mysql_conn()],
        },
        GroupedSection {
            group: String::new(),
            connections: vec![mysql_conn()],
        },
    ];
    params.expanded_groups = HashSet::from(["__pinned__".to_owned(), String::new()]);
    let rows = rows_of(&params);
    let connection_rows: Vec<&NavigatorRow> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .collect();
    // The pinned section comes first; the ungrouped copy is a different
    // section's *view* of the same connection, and both appear (React
    // deduplicates for the *locator*, not for section membership).
    assert_eq!(connection_rows.len(), 2);
    assert_eq!(navigator_row_key(connection_rows[0]), "conn:__pinned__:c1");
    assert_eq!(navigator_row_key(connection_rows[1]), "conn::c1");
}

// ---------------------------------------------------------------------------
// connection rows and their placeholders
// ---------------------------------------------------------------------------

#[test]
fn an_idle_connection_row_emits_no_children() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params.active_connections.clear();
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "connection"]);
    let NavigatorRow::Connection {
        status,
        expanded,
        depth_pair,
        ..
    } = &rows[1]
    else {
        panic!()
    };
    assert_eq!(status, "idle");
    assert!(!expanded); // not connected: announces nothing open
    assert_eq!(*depth_pair, (1, 1)); // CONNECTION_CHILD_DEPTH - 1
}

#[test]
fn a_connecting_connection_shows_the_connection_spinner() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params.active_connections.insert(
        "c1".into(),
        ActiveEntrySnapshot {
            db_session_id: None,
            status: "connecting".into(),
        },
    );
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "connection", "db-loading"]);
    let NavigatorRow::DbLoading {
        owner_key,
        depth_pair,
    } = &rows[2]
    else {
        panic!()
    };
    assert_eq!(owner_key, "conn:c1");
    assert_eq!(*depth_pair, (2, 2));
    assert_eq!(navigator_row_key(&rows[2]), "loading:conn:c1");
}

#[test]
fn a_connected_connection_without_a_schema_shows_the_session_spinner() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    // No entry in params.schemas for sess-1.
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "connection", "db-loading"]);
    let NavigatorRow::DbLoading { owner_key, .. } = &rows[2] else {
        panic!()
    };
    assert_eq!(owner_key, "session:sess-1");
}

#[test]
fn connection_rows_carry_the_locator_match_under_a_search() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    params.query = "mysql".to_owned();
    let rows = rows_of(&params);
    let connection = first_of(&rows, "connection");
    let NavigatorRow::Connection {
        match_payload,
        depth_pair,
        expanded,
        ..
    } = connection
    else {
        panic!()
    };
    let payload = match_payload
        .as_ref()
        .expect("a search must attach its best match");
    // The connection is *named* mysql: exact name match, rank 0.
    assert_eq!(payload.rank, 0);
    assert_eq!(
        payload.reason,
        crate::views::navigator::build_flat_rows::deps::locator::MatchReason::Name
    );
    assert_eq!(payload.context, "MySQL");
    // Search suppresses the group header: the connection paints at 0. Only
    // rows at or below CONNECTION_CHILD_DEPTH lose an announced level, and 0
    // is above it — the connection announces its own rung (level 1 after the
    // aria conversion adds TREE_TOP_LEVEL).
    assert_eq!(*depth_pair, (0, 0));
    // And force-expands it.
    assert!(expanded);
}

// ---------------------------------------------------------------------------
// the dbError contract (React buildFlatRows.dbError.test.ts, case for case)
// ---------------------------------------------------------------------------

fn db_error_params() -> BuildFlatRowsParams {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    params.schemas.insert(
        "sess-1".into(),
        SchemaStateSnapshot {
            database_type: Some("mysql".into()),
            databases: vec!["orders".into()],
            ..Default::default()
        },
    );
    params.expanded_dbs = HashSet::from(["c1::orders".to_owned()]);
    params
}

#[test]
fn a_failed_database_load_becomes_a_visible_error_row() {
    let mut params = db_error_params();
    params
        .db_errors
        .insert("sess-1::orders".into(), "connection refused".into());
    let rows = rows_of(&params);

    let error_row = first_of(&rows, "db-error");
    let NavigatorRow::DbError {
        owner_key,
        message,
        depth_pair,
    } = error_row
    else {
        panic!()
    };
    assert_eq!(owner_key, "db:c1::orders");
    assert_eq!(message, "connection refused");
    assert_eq!(*depth_pair, (3, 3));
    // The error is the database's only child: no category list below it.
    let after_db = rows.iter().position(|row| row.kind() == "db").unwrap();
    let between = &rows[after_db + 1..];
    assert!(!between.iter().any(|row| row.kind() == "category"));
    assert_eq!(navigator_row_key(&between[0]), "error:db:c1::orders");
}

#[test]
fn a_cached_table_list_beats_a_failed_refresh() {
    let mut params = db_error_params();
    params
        .db_errors
        .insert("sess-1::orders".into(), "stale refresh failed".into());
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![tbl("invoices", TableType::Table)],
    );
    let rows = rows_of(&params);
    assert!(!rows.iter().any(|row| row.kind() == "db-error"));
    assert!(rows.iter().any(|row| row.kind() == "category"));
}

#[test]
fn a_single_database_connection_shows_its_session_error() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot {
            database: Some("shop".into()),
            ..mysql_conn()
        }],
    }];
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    params.schemas.insert(
        "sess-1".into(),
        SchemaStateSnapshot {
            database_type: Some("mysql".into()),
            current_database: Some("shop".into()),
            error: Some("Typed command result unavailable".into()),
            ..Default::default()
        },
    );
    params.expanded_dbs = HashSet::from(["c1::shop".to_owned()]);
    let rows = rows_of(&params);
    let NavigatorRow::DbError {
        owner_key, message, ..
    } = first_of(&rows, "db-error")
    else {
        panic!()
    };
    assert_eq!(owner_key, "db:c1::shop");
    assert_eq!(message, "Typed command result unavailable");
}

#[test]
fn no_error_row_while_the_load_is_in_flight() {
    let mut params = db_error_params();
    params.loading_dbs.insert("sess-1::orders".to_owned());
    params
        .db_errors
        .insert("sess-1::orders".into(), "a previous attempt failed".into());
    let rows = rows_of(&params);
    assert!(!rows.iter().any(|row| row.kind() == "db-error"));
    assert!(rows.iter().any(|row| row.kind() == "db-loading"));
}

// ---------------------------------------------------------------------------
// the multi-database ladder
// ---------------------------------------------------------------------------

#[test]
fn databases_come_from_the_schema_state_and_carry_open_and_loading_flags() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases = vec!["orders".into(), "billing".into()];
    params
        .open_dbs
        .insert("sess-1".into(), HashSet::from(["billing".into()]));
    let rows = rows_of(&params);
    let dbs: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "db").collect();
    assert_eq!(dbs.len(), 2);
    let NavigatorRow::Db {
        db_name, is_open, ..
    } = dbs[0]
    else {
        panic!()
    };
    assert_eq!(db_name, "orders");
    assert!(*is_open, "the tree's own expand set marks orders open");
    let NavigatorRow::Db {
        db_name, is_open, ..
    } = dbs[1]
    else {
        panic!()
    };
    assert_eq!(db_name, "billing");
    assert!(*is_open, "the driver's open resource marks billing open");
}

#[test]
fn a_collapsed_database_hides_its_tables_but_still_paints_the_row() {
    let mut params = db_error_params();
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![tbl("invoices", TableType::Table)],
    );
    params.expanded_dbs = HashSet::new(); // collapsed
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "connection", "db"]);
    let NavigatorRow::Db { expanded, .. } = &rows[2] else {
        panic!()
    };
    assert!(!expanded);
}

#[test]
fn tables_and_views_split_into_their_categories_with_counts() {
    let mut params = db_error_params();
    params.expanded_cats = HashSet::from([
        "c1::orders::tables".to_owned(),
        "c1::orders::views".to_owned(),
    ]);
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![
            tbl("invoices", TableType::Table),
            tbl("logs", TableType::SystemTable),
            tbl("daily", TableType::View),
            tbl("rollup", TableType::MaterializedView),
            tbl("", TableType::Table), // nameless: dropped
        ],
    );
    let rows = rows_of(&params);
    let categories: Vec<&NavigatorRow> =
        rows.iter().filter(|row| row.kind() == "category").collect();
    assert_eq!(categories.len(), 2);
    let NavigatorRow::Category { cat_id, count, .. } = categories[0] else {
        panic!()
    };
    assert_eq!(*cat_id, "tables");
    assert_eq!(*count, 2, "table + systemTable");
    let NavigatorRow::Category { cat_id, count, .. } = categories[1] else {
        panic!()
    };
    assert_eq!(*cat_id, "views");
    assert_eq!(*count, 2, "view + materializedView");
    // Both categories list their rows as `table` rows (React's UnifiedRow has
    // one variant for items, whatever the category) at database depth + 1.
    let tables: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "table").collect();
    assert_eq!(tables.len(), 4);
    let by_cat: Vec<(&str, &str)> = tables
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, cat_id, .. } => Some((*cat_id, item.name.as_str())),
            _ => None,
        })
        .collect();
    assert!(by_cat.contains(&("tables", "invoices")));
    assert!(by_cat.contains(&("tables", "logs")));
    assert!(by_cat.contains(&("views", "daily")));
    assert!(by_cat.contains(&("views", "rollup")));
    assert_eq!(tables[0].depth(), 4);
    assert_eq!(tables[0].depth_pair(), (4, 4));
}

#[test]
fn other_kinds_read_the_shared_object_cache_scoped_to_the_owner_schema() {
    let mut params = db_error_params();
    // PG-shaped: one database, schemas from the table list.
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            supported_object_kinds: vec!["function".to_owned()],
            ..Default::default()
        },
    );
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("pg1", "PG", "postgresql")],
    }];
    params
        .active_connections
        .insert("pg1".into(), connected_entry("pgs"));
    params.schemas.insert(
        "pgs".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::pg1".to_owned());
    params.expanded_dbs.insert("pg1::app".to_owned());
    params
        .expanded_schemas
        .insert("pg1::app::public".to_owned());
    params
        .expanded_cats
        .insert("pg1::app::public::function".to_owned());
    params.db_tables_map.insert(
        "pgs::app".into(),
        vec![
            tbl_in("public", "events", TableType::Table),
            tbl_in("audit", "trail", TableType::Table),
        ],
    );
    // The driver command lists both schemas' functions; each schema's category
    // shows only its own.
    params.db_objects_map.insert(
        "pg1::app::public::function".to_owned(),
        vec![
            obj("function", "fn_pub"),
            obj("function", "fn_audit"),
            obj("function", "fn_anon"),
        ],
    );
    // Objects with no schema metadata stay (some drivers do not expose it).
    let mut fn_anon = obj("function", "fn_anon");
    fn_anon.schema = None;
    params
        .db_objects_map
        .get_mut("pg1::app::public::function")
        .unwrap()[2] = fn_anon;
    // The audit schema's own cache key (the sibling category).
    params.db_objects_map.insert(
        "pg1::app::audit::function".to_owned(),
        vec![{
            let mut f = obj("function", "fn_audit");
            f.schema = Some("audit".into());
            f
        }],
    );

    let rows = rows_of(&params);
    // Schemas sort: extras absent, both come from the tables; preferred none.
    let schema_rows: Vec<&NavigatorRow> =
        rows.iter().filter(|row| row.kind() == "schema").collect();
    assert_eq!(schema_rows.len(), 2);
    let objects: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "object").collect();
    // public's category holds fn_pub (schema=null stays? no: filter keeps
    // objects with schema==null OR schema==public; fn_audit has schema=null in
    // the shared list and so also stays — the scope filter only drops objects
    // whose schema names *another* schema).
    let names: Vec<&str> = objects
        .iter()
        .map(|o| match o {
            NavigatorRow::Object { obj, .. } => obj.name.as_str(),
            _ => "",
        })
        .collect();
    assert!(names.contains(&"fn_pub"));
    // An object that declares the *other* schema is dropped from public's list.
    let mut dropped = obj("function", "fn_other");
    dropped.schema = Some("audit".into());
    params
        .db_objects_map
        .get_mut("pg1::app::public::function")
        .unwrap()
        .push(dropped);
    let rows = rows_of(&params);
    let objects: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "object").collect();
    let names: Vec<&str> = objects
        .iter()
        .map(|o| match o {
            NavigatorRow::Object { obj, .. } => obj.name.as_str(),
            _ => "",
        })
        .collect();
    assert!(
        !names.contains(&"fn_other"),
        "an object naming another schema does not leak in"
    );
    // The audit schema's own category lists its own function.
    assert!(names.contains(&"fn_audit"));
}

#[test]
fn a_single_schema_matching_the_database_name_folds_away() {
    let mut params = db_error_params();
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            ..Default::default()
        },
    );
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("pg1", "PG", "postgresql")],
    }];
    params
        .active_connections
        .insert("pg1".into(), connected_entry("pgs"));
    params.schemas.insert(
        "pgs".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::pg1".to_owned());
    params.expanded_dbs.insert("pg1::app".to_owned());
    // Every table carries the database's own name as its schema: no ladder.
    params.db_tables_map.insert(
        "pgs::app".into(),
        vec![tbl_in("app", "events", TableType::Table)],
    );
    let rows = rows_of(&params);
    assert!(!rows.iter().any(|row| row.kind() == "schema"));
    assert!(rows.iter().any(|row| row.kind() == "category"));
    // Two schemas would keep the ladder.
    params
        .db_tables_map
        .get_mut("pgs::app")
        .unwrap()
        .push(tbl_in("audit", "trail", TableType::Table));
    let rows = rows_of(&params);
    assert!(rows.iter().any(|row| row.kind() == "schema"));
}

#[test]
fn the_preferred_schema_sorts_first_and_the_rest_alphabetically() {
    let mut params = db_error_params();
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            default_schema: Some("public".to_owned()),
            ..Default::default()
        },
    );
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("pg1", "PG", "postgresql")],
    }];
    params
        .active_connections
        .insert("pg1".into(), connected_entry("pgs"));
    params.schemas.insert(
        "pgs".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::pg1".to_owned());
    params.expanded_dbs.insert("pg1::app".to_owned());
    params.db_tables_map.insert(
        "pgs::app".to_owned(),
        vec![
            tbl_in("zeta", "t1", TableType::Table),
            tbl_in("public", "t2", TableType::Table),
            tbl_in("alpha", "t3", TableType::Table),
        ],
    );
    let rows = rows_of(&params);
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(schemas, vec!["public", "alpha", "zeta"]);
}

#[test]
fn a_schema_less_item_builds_the_empty_name_group_beside_named_ones() {
    let mut params = db_error_params();
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::pg1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("pg1", "PG", "postgresql")],
    }];
    params
        .active_connections
        .insert("pg1".into(), connected_entry("pgs"));
    params.schemas.insert(
        "pgs".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_dbs.insert("pg1::app".to_owned());
    params.db_tables_map.insert(
        "pgs::app".to_owned(),
        vec![
            tbl("loose", TableType::Table),
            tbl_in("public", "events", TableType::Table),
        ],
    );
    let rows = rows_of(&params);
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    // React keys a schema-less item under '' — it is its own group, sorted
    // first, and it shows its own categories when expanded.
    assert_eq!(schemas, vec!["", "public"]);
    params.expanded_schemas.insert("pg1::app::".to_owned());
    params.expanded_cats.insert("pg1::app::::tables".to_owned());
    let rows = rows_of(&params);
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tables,
        vec!["loose"],
        "the '' group lists the schema-less item"
    );
}

#[test]
fn the_schema_ladder_skips_nameless_items_and_pg_prefixed_schemas() {
    let mut params = db_error_params();
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            ..Default::default()
        },
    );
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("pg1", "PG", "postgresql")],
    }];
    params
        .active_connections
        .insert("pg1".into(), connected_entry("pgs"));
    params.schemas.insert(
        "pgs".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::pg1".to_owned());
    params.expanded_dbs.insert("pg1::app".to_owned());
    params.db_tables_map.insert(
        "pgs::app".into(),
        vec![
            tbl_in("public", "events", TableType::Table),
            tbl_in("public", "", TableType::Table), // nameless: not in any group
            tbl_in("pg_temp", "orphan", TableType::Table),
        ],
    );
    // Without the pref: all three schemas show (public, pg_temp, and the
    // nameless item's key '' -- it belongs to no group, so it is dropped).
    let rows = rows_of(&params);
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        schemas,
        vec!["pg_temp", "public"],
        "the nameless item builds no empty group"
    );
    // With hideSystemSchemas: pg_temp drops (the pg_ prefix is system too).
    let mut conn = ConnectionSnapshot::new("pg1", "PG", "postgresql");
    conn.options = Some(serde_json::json!({ "objectFilter": { "hideSystemSchemas": true } }));
    params.grouped[0].connections[0] = conn;
    let rows = rows_of(&params);
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(schemas, vec!["public"]);
    // And a lone nameless schema-less item conjures no ladder at all, pref or not.
    params
        .db_tables_map
        .insert("pgs::app".to_owned(), vec![tbl("", TableType::Table)]);
    let rows = rows_of(&params);
    assert!(
        !rows.iter().any(|row| row.kind() == "schema"),
        "a lone nameless item is not a schema group"
    );
    // A schema whose only member is nameless still shows as a group: React's
    // dbSchemaNames (the extras) collect schema names off every item, and the
    // nameless filter in groupBySchema is defense-in-depth the category list
    // re-applies — so the empty group is visible, exactly as in React.
    params.db_tables_map.insert(
        "pgs::app".to_owned(),
        vec![
            tbl_in("public", "events", TableType::Table),
            tbl_in("ghost", "", TableType::Table),
        ],
    );
    params.grouped[0].connections[0] = ConnectionSnapshot::new("pg1", "PG", "postgresql");
    let rows = rows_of(&params);
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(schemas, vec!["ghost", "public"]);
}

// ---------------------------------------------------------------------------
// the object filter
// ---------------------------------------------------------------------------

fn filtered_conn_params(filter_json: serde_json::Value) -> BuildFlatRowsParams {
    let mut params = db_error_params();
    params.expanded_cats.insert("c1::orders::tables".to_owned());
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![
            tbl("invoices", TableType::Table),
            tbl("pg_temp", TableType::Table),
            tbl("sys_logs", TableType::SystemTable),
        ],
    );
    let mut conn = mysql_conn();
    conn.options = Some(serde_json::json!({ "objectFilter": filter_json }));
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![conn],
    }];
    params
}

#[test]
fn hiding_system_schemas_drops_system_tables_and_system_schemas() {
    let params = filtered_conn_params(serde_json::json!({ "hideSystemSchemas": true }));
    let rows = rows_of(&params);
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tables, vec!["invoices", "pg_temp"]);
}

#[test]
fn include_and_exclude_patterns_filter_table_names() {
    // Glob include: only names matching `* invoices *`-shaped patterns.
    let params = filtered_conn_params(serde_json::json!({ "tableNameInclude": "inv*" }));
    let rows = rows_of(&params);
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tables, vec!["invoices"]);

    // Substring exclude.
    let params = filtered_conn_params(serde_json::json!({ "tableNameExclude": "temp" }));
    let rows = rows_of(&params);
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(!tables.contains(&"pg_temp"));
    assert!(tables.contains(&"invoices"));
}

#[test]
fn wrong_typed_filter_fields_read_as_absent() {
    let params = filtered_conn_params(
        serde_json::json!({ "hideSystemSchemas": "yes", "tableNameInclude": 42 }),
    );
    let rows = rows_of(&params);
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tables.len(),
        3,
        "nothing filtered: every field had the wrong type"
    );
}

#[test]
fn system_databases_hide_under_the_same_pref() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases =
        vec!["orders".into(), "mysql".into(), "performance_schema".into()];
    let mut conn = mysql_conn();
    conn.options = Some(serde_json::json!({ "objectFilter": { "hideSystemSchemas": true } }));
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![conn],
    }];
    let rows = rows_of(&params);
    let dbs: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Db { db_name, .. } => Some(db_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(dbs, vec!["orders"]);
}

// ---------------------------------------------------------------------------
// the key-value branch
// ---------------------------------------------------------------------------

#[test]
fn key_value_connections_list_databases_as_kv_rows_with_their_command() {
    let mut params = base_params();
    params.db_registry.insert(
        "redis".to_owned(),
        DriverMeta {
            is_key_value: true,
            has_multi_database: true,
            db_counts_command: Some("DBSIZE".to_owned()),
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::r1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("r1", "Cache", "redis")],
    }];
    params
        .active_connections
        .insert("r1".into(), connected_entry("rss"));
    params.schemas.insert(
        "rss".into(),
        SchemaStateSnapshot {
            databases: vec!["db0".into(), "cache".into()],
            ..Default::default()
        },
    );
    let rows = rows_of(&params);
    let kvs: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "kv-db").collect();
    assert_eq!(kvs.len(), 2);
    let NavigatorRow::KvDb {
        db_name,
        db_counts_command,
        depth_pair,
        ..
    } = kvs[0]
    else {
        panic!()
    };
    assert_eq!(db_name, "db0");
    assert_eq!(db_counts_command.as_deref(), Some("DBSIZE"));
    assert_eq!(*depth_pair, (2, 2));
    assert_eq!(navigator_row_key(kvs[0]), "kv:r1:db0");
}

#[test]
fn a_kv_list_that_is_refreshing_but_loaded_still_shows_its_dbs() {
    let mut params = base_params();
    params.db_registry.insert(
        "redis".to_owned(),
        DriverMeta {
            is_key_value: true,
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::r1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("r1", "Cache", "redis")],
    }];
    params
        .active_connections
        .insert("r1".into(), connected_entry("rss"));
    // Loading, but the list already has entries: rows, not a spinner.
    params.schemas.insert(
        "rss".into(),
        SchemaStateSnapshot {
            loading: true,
            databases: vec!["db0".into()],
            ..Default::default()
        },
    );
    let rows = rows_of(&params);
    assert!(rows.iter().any(|row| row.kind() == "kv-db"));
    assert!(
        !rows.iter().any(|row| row.kind() == "db-loading"),
        "a refresh over a loaded list replaces nothing"
    );
}

#[test]
fn a_loading_key_value_list_shows_the_session_spinner() {
    let mut params = base_params();
    params.db_registry.insert(
        "redis".to_owned(),
        DriverMeta {
            is_key_value: true,
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::r1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("r1", "Cache", "redis")],
    }];
    params
        .active_connections
        .insert("r1".into(), connected_entry("rss"));
    params.schemas.insert(
        "rss".into(),
        SchemaStateSnapshot {
            loading: true,
            databases: vec![],
            ..Default::default()
        },
    );
    let rows = rows_of(&params);
    let NavigatorRow::DbLoading { owner_key, .. } = first_of(&rows, "db-loading") else {
        panic!()
    };
    assert_eq!(owner_key, "session:rss");
}

// ---------------------------------------------------------------------------
// the namespace-tree branch (custom tree / path-hierarchy drivers)
// ---------------------------------------------------------------------------

fn ns_tree() -> SqlNamespace {
    use std::collections::BTreeMap;
    let mut tables: BTreeMap<String, SqlNamespace> = BTreeMap::new();
    tables.insert("events".to_owned(), SqlNamespace::Leaf(vec![]));
    tables.insert("users".to_owned(), SqlNamespace::Leaf(vec![]));
    let mut analytics: BTreeMap<String, SqlNamespace> = BTreeMap::new();
    analytics.insert("daily".to_owned(), SqlNamespace::Leaf(vec![]));
    let mut root: BTreeMap<String, SqlNamespace> = BTreeMap::new();
    root.insert("analytics".to_owned(), SqlNamespace::Branch(analytics));
    root.insert("public".to_owned(), SqlNamespace::Branch(tables));
    SqlNamespace::Branch(root)
}

fn ns_params() -> BuildFlatRowsParams {
    let mut params = base_params();
    params.db_registry.insert(
        "mongoish".to_owned(),
        DriverMeta {
            has_multi_database: true,
            schema_tree_mode: Some("custom".to_owned()),
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::m1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot::new("m1", "Mongo", "mongoish")],
    }];
    params
        .active_connections
        .insert("m1".into(), connected_entry("mss"));
    params.schemas.insert(
        "mss".into(),
        SchemaStateSnapshot {
            namespace_tree: ns_tree(),
            tables: vec![tbl("events", TableType::View)], // leafKind override
            ..Default::default()
        },
    );
    params
}

#[test]
fn namespace_branches_flatten_into_rows_with_path_keys() {
    let params = ns_params();
    let rows = rows_of(&params);
    let ns: Vec<&NavigatorRow> = rows
        .iter()
        .filter(|row| row.kind() == "namespace-node")
        .collect();
    // Branches sorted, then their leaves; search off, branches collapsed.
    let names: Vec<&str> = ns
        .iter()
        .map(|row| match row {
            NavigatorRow::NamespaceNode { name, .. } => name.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(names, vec!["analytics", "public"]);
    assert!(ns.iter().all(|row| match row {
        NavigatorRow::NamespaceNode { depth_pair, .. } => *depth_pair == (2, 2),
        _ => false,
    }));
    let NavigatorRow::NamespaceNode {
        key,
        segments,
        expanded,
        is_leaf,
        ..
    } = ns[0]
    else {
        panic!()
    };
    assert_eq!(key, "m1::ns::analytics");
    assert_eq!(segments, &["analytics".to_owned()]);
    assert!(!expanded);
    assert!(!is_leaf);
    assert_eq!(navigator_row_key(ns[0]), "ns:m1::ns::analytics");
}

#[test]
fn an_expanded_empty_namespace_branch_shows_the_path_spinner_unless_loaded() {
    use std::collections::BTreeMap;
    let mut params = ns_params();
    let mut empty: BTreeMap<String, SqlNamespace> = BTreeMap::new();
    let mut root: BTreeMap<String, SqlNamespace> = BTreeMap::new();
    root.insert(
        "empty".to_owned(),
        SqlNamespace::Branch(std::mem::take(&mut empty)),
    );
    params.schemas.get_mut("mss").unwrap().namespace_tree = SqlNamespace::Branch(root);
    params.expanded_dbs.insert("m1::ns::empty".to_owned());
    let rows = rows_of(&params);
    let NavigatorRow::DbLoading { owner_key, .. } = first_of(&rows, "db-loading") else {
        panic!()
    };
    assert_eq!(owner_key, "ns:empty");

    // Loaded paths replace the spinner with (still no) children.
    params
        .schemas
        .get_mut("mss")
        .unwrap()
        .loaded_paths
        .insert("empty".to_owned());
    let rows = rows_of(&params);
    assert!(!rows.iter().any(|row| row.kind() == "db-loading"));
}

#[test]
fn a_namespace_leaf_takes_its_kind_from_the_tables_metadata() {
    let mut params = ns_params();
    params.expanded_dbs.insert("m1::ns::public".to_owned());
    let rows = rows_of(&params);
    let leaf = rows.iter().find(|row| matches!(row, NavigatorRow::NamespaceNode { name, is_leaf, .. } if name == "events" && *is_leaf))
        .expect("events leaf");
    let NavigatorRow::NamespaceNode {
        leaf_kind,
        segments,
        depth_pair,
        ..
    } = leaf
    else {
        panic!()
    };
    assert_eq!(
        *leaf_kind,
        Some(TableType::View),
        "the tables metadata overrides the default"
    );
    assert_eq!(segments, &["public".to_owned(), "events".to_owned()]);
    assert_eq!(*depth_pair, (3, 3));
    // users has no metadata: default kind is table.
    let users = rows
        .iter()
        .find(|row| matches!(row, NavigatorRow::NamespaceNode { name, .. } if name == "users"))
        .unwrap();
    let NavigatorRow::NamespaceNode { leaf_kind, .. } = users else {
        panic!()
    };
    assert_eq!(*leaf_kind, Some(TableType::Table));
}

#[test]
fn an_empty_namespace_tree_shows_the_session_spinner_only_while_working() {
    let mut params = ns_params();
    params.schemas.get_mut("mss").unwrap().namespace_tree = SqlNamespace::Empty;
    let rows = rows_of(&params);
    assert!(
        !rows.iter().any(|row| row.kind() == "db-loading"),
        "idle and empty: no spinner"
    );

    params.schemas.get_mut("mss").unwrap().ensuring_count = 1;
    let rows = rows_of(&params);
    let NavigatorRow::DbLoading { owner_key, .. } = first_of(&rows, "db-loading") else {
        panic!()
    };
    assert_eq!(owner_key, "session:mss");
}

// ---------------------------------------------------------------------------
// the search rules
// ---------------------------------------------------------------------------

#[test]
fn a_search_suppresses_group_headers_and_rekeys_connections_to_the_flat_section() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases = vec!["orders".into()];
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![tbl("invoices", TableType::Table)],
    );
    params.query = "invoices".to_owned();
    let rows = rows_of(&params);
    // One flat section, no group header, connection at depth 0.
    assert!(!rows.iter().any(|row| row.kind() == "group"));
    let connection = first_of(&rows, "connection");
    assert_eq!(connection.depth(), 0);
    // The database row is force-expanded by the search.
    let NavigatorRow::Db {
        db_name, expanded, ..
    } = first_of(&rows, "db")
    else {
        panic!()
    };
    assert_eq!(db_name, "orders");
    assert!(expanded, "a search force-expands matches");
    // The matching table is visible, its category too, non-matching tables not.
    let tables: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Table { item, .. } => Some(item.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tables, vec!["invoices"]);
}

#[test]
fn a_category_with_no_matches_under_a_search_disappears() {
    let mut params = db_error_params();
    params.db_tables_map.insert(
        "sess-1::orders".into(),
        vec![
            tbl("invoices", TableType::Table),
            tbl("daily", TableType::View),
        ],
    );
    params.expanded_dbs = HashSet::new();
    params.query = "daily".to_owned();
    let rows = rows_of(&params);
    let categories: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Category { cat_id, .. } => Some(*cat_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        categories,
        vec!["views"],
        "tables has no match: the whole category drops"
    );
    let NavigatorRow::Category { count, .. } = first_of(&rows, "category") else {
        panic!()
    };
    assert_eq!(*count, 1);
}

#[test]
fn a_search_matching_only_a_database_keeps_it_via_its_tables() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases = vec!["orders".into(), "billing".into()];
    params.db_tables_map.insert(
        "sess-1::billing".into(),
        vec![tbl("charges", TableType::Table)],
    );
    params.query = "charges".to_owned();
    let rows = rows_of(&params);
    let dbs: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Db { db_name, .. } => Some(db_name.as_str()),
            _ => None,
        })
        .collect();
    // orders matches nothing (name and tables alike); billing survives.
    assert_eq!(dbs, vec!["billing"]);
}

#[test]
fn a_search_drops_a_leaf_that_misses_but_keeps_a_branch_whose_subtree_matches() {
    let mut params = ns_params();
    params.query = "daily".to_owned();
    let rows = rows_of(&params);
    let ns: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::NamespaceNode { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        ns.contains(&"analytics"),
        "the branch matches through its subtree"
    );
    assert!(ns.contains(&"daily"));
    assert!(
        !ns.contains(&"public"),
        "the branch whose subtree misses drops"
    );
    assert!(!ns.contains(&"events"));
}

#[test]
fn a_search_below_the_connection_subtracts_one_announced_level() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases = vec!["orders".into()];
    params.db_tables_map.insert(
        "sess-1::orders".to_owned(),
        vec![tbl("invoices", TableType::Table)],
    );
    params.query = "invoices".to_owned();
    let rows = rows_of(&params);
    // Connection paints at 0 announcing 1 (no hidden parent); the database
    // paints at 2 announcing 1 — one rung up from its painted depth.
    let NavigatorRow::Db { depth_pair, .. } = first_of(&rows, "db") else {
        panic!()
    };
    assert_eq!(*depth_pair, (2, 1));
    let table = first_of(&rows, "table");
    // Table rows sit under the database + category: painted 4, announced 3.
    assert_eq!(table.depth_pair(), (4, 3));
}

// ---------------------------------------------------------------------------
// the locator's ranking contract
// ---------------------------------------------------------------------------

#[test]
fn an_empty_query_returns_every_connection_with_null_matches() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![
            ConnectionSnapshot::new("c1", "Alpha", "mysql"),
            ConnectionSnapshot::new("c2", "Beta", "mysql"),
        ],
    }];
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    params
        .schemas
        .insert("sess-1".into(), SchemaStateSnapshot::default());
    let rows = rows_of(&params);
    let connections: Vec<&NavigatorRow> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .collect();
    assert_eq!(connections.len(), 2);
    for connection in connections {
        let NavigatorRow::Connection { match_payload, .. } = connection else {
            panic!()
        };
        assert!(match_payload.is_none(), "no query: no match payload");
    }
}

#[test]
fn a_search_ranks_exact_names_above_hosts_above_table_hits() {
    let mut params = base_params();
    let mut named = ConnectionSnapshot::new("c-named", "warehouse", "mysql");
    named.host = Some("db1.internal".into());
    let mut hosting = ConnectionSnapshot::new("c-host", "Reporting", "mysql");
    hosting.host = Some("warehouse.internal".into());
    let mut holding = ConnectionSnapshot::new("c-holding", "Archive", "mysql");
    holding.host = Some("db9".into());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![holding, hosting, named],
    }];
    params
        .active_connections
        .insert("c-holding".into(), connected_entry("s3"));
    params.db_tables_map.insert(
        "s3::app".to_owned(),
        vec![tbl("warehouse", TableType::Table)],
    );
    let mut s3 = SchemaStateSnapshot::default();
    s3.databases = vec!["app".to_owned()];
    params.schemas.insert("s3".into(), s3);
    params.query = "warehouse".to_owned();
    let rows = rows_of(&params);
    let connections: Vec<&NavigatorRow> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .collect();
    assert_eq!(connections.len(), 3);
    let ids: Vec<&str> = connections
        .iter()
        .map(|row| match row {
            NavigatorRow::Connection { id, .. } => id.as_str(),
            _ => "",
        })
        .collect();
    // Exact name (rank 0) < host prefix (10+1) < table substring (50+2).
    assert_eq!(ids, vec!["c-named", "c-host", "c-holding"]);
    let NavigatorRow::Connection { match_payload, .. } = connections[2] else {
        panic!()
    };
    let payload = match_payload.as_ref().unwrap();
    assert_eq!(
        payload.rank, 50,
        "an exact table-name match is the field offset itself"
    );
    assert_eq!(payload.context, "warehouse");
}

#[test]
fn an_exact_table_hit_outranks_a_substring_table_hit() {
    let mut params = base_params();
    // Names deliberately invert the name tiebreak: if the ranks tie (the
    // mutation), "Aaa..." sorts first and the assertion fails.
    let exact = ConnectionSnapshot::new("c-exact", "Zzz", "mysql");
    let substring = ConnectionSnapshot::new("c-sub", "Aaa", "mysql");
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![exact, substring],
    }];
    params
        .active_connections
        .insert("c-exact".into(), connected_entry("s1"));
    params
        .active_connections
        .insert("c-sub".into(), connected_entry("s2"));
    let mut s1 = SchemaStateSnapshot::default();
    s1.databases = vec!["app".to_owned()];
    let mut s2 = SchemaStateSnapshot::default();
    s2.databases = vec!["app".to_owned()];
    params.schemas.insert("s1".into(), s1);
    params.schemas.insert("s2".into(), s2);
    params.db_tables_map.insert(
        "s1::app".to_owned(),
        vec![tbl("warehouse", TableType::Table)],
    );
    params.db_tables_map.insert(
        "s2::app".to_owned(),
        vec![tbl("warehouse_log", TableType::Table)],
    );
    params.query = "warehouse".to_owned();
    let rows = rows_of(&params);
    let ids: Vec<&str> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .map(|row| match row {
            NavigatorRow::Connection { id, .. } => id.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(
        ids,
        vec!["c-exact", "c-sub"],
        "exact (50) before substring (52)"
    );
}

#[test]
fn pinned_and_recent_connections_outrank_the_rest_at_equal_match_rank() {
    let mut params = base_params();
    let mut pinned = ConnectionSnapshot::new("c-pin", "archive1", "mysql");
    pinned.pinned = Some(true);
    pinned.last_connected_at = None;
    let mut fresh = ConnectionSnapshot::new("c-fresh", "archive2", "mysql");
    fresh.last_connected_at = Some("2026-11-01T00:00:00Z".into());
    let mut stale = ConnectionSnapshot::new("c-stale", "archive3", "mysql");
    stale.last_connected_at = Some("2026-01-01T00:00:00Z".into());
    // Group order: stale's group sorts before fresh's — but recency beats it.
    stale.group = Some("a".into());
    fresh.group = Some("z".into());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![stale.clone(), fresh.clone(), pinned.clone()],
    }];
    params.query = "archive".to_owned();
    let rows = rows_of(&params);
    let ids: Vec<&str> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .map(|row| match row {
            NavigatorRow::Connection { id, .. } => id.as_str(),
            _ => "",
        })
        .collect();
    // All three match their name as a prefix (rank 1). Pinned first, then
    // most recently connected, then the group order.
    assert_eq!(ids, vec!["c-pin", "c-fresh", "c-stale"]);
}

#[test]
fn a_search_with_no_matches_yields_only_the_nothing_found_shape() {
    let mut params = base_params();
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![mysql_conn()],
    }];
    params.query = "zzz-nothing".to_owned();
    let rows = rows_of(&params);
    // The flat section exists but is empty: the loop skips it, no rows at all
    // (the host shows its own empty state).
    assert!(rows.is_empty());
}

// ---------------------------------------------------------------------------
// the multi-database decision and the single-database branch
// ---------------------------------------------------------------------------

#[test]
fn a_pinned_logical_database_locks_the_standard_tree_but_a_domain_does_not() {
    use crate::views::navigator::build_flat_rows::deps::utils::should_use_multi_database_tree;
    // name-typed field: a configured database locks the tree.
    assert!(!should_use_multi_database_tree(true, "name", Some("shop")));
    assert!(
        should_use_multi_database_tree(true, "name", Some("  ")),
        "blank locks nothing"
    );
    assert!(should_use_multi_database_tree(true, "name", None));
    // domain-typed (Kiwi): the configured value is the instance domain, not a
    // database choice — the multi-database tree stays.
    assert!(should_use_multi_database_tree(
        true,
        "domain",
        Some("kiwi.example")
    ));
    assert!(!should_use_multi_database_tree(false, "domain", None));
}

#[test]
fn a_single_database_connection_names_its_db_from_the_session_then_the_config() {
    let mut params = base_params();
    params
        .db_registry
        .insert("sqliteish".to_owned(), DriverMeta::default());
    params.expanded_connections.insert("::s1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot {
            database: Some("config.db".into()),
            ..ConnectionSnapshot::new("s1", "Local", "sqliteish")
        }],
    }];
    params
        .active_connections
        .insert("s1".into(), connected_entry("sss"));
    params.schemas.insert(
        "sss".into(),
        SchemaStateSnapshot {
            database_type: Some("sqliteish".into()),
            current_database: None, // session has not said: fall back to config
            tables: vec![tbl("rows", TableType::Table)],
            ..Default::default()
        },
    );
    params.expanded_dbs.insert("s1::config.db".to_owned());
    let rows = rows_of(&params);
    let NavigatorRow::Db { db_name, .. } = first_of(&rows, "db") else {
        panic!()
    };
    assert_eq!(db_name, "config.db");
    assert!(rows.iter().any(|row| row.kind() == "category"));

    // No database anywhere: React skips the db row entirely — a database
    // with no name is not a row, and the connection stands alone.
    params.schemas.get_mut("sss").unwrap().current_database = None;
    params.grouped[0].connections[0].database = None;
    params.expanded_dbs.clear();
    let rows = rows_of(&params);
    assert_eq!(kinds(&rows), vec!["group", "connection"]);
}

#[test]
fn a_single_database_connection_shows_the_db_spinner_while_tables_load() {
    let mut params = base_params();
    params
        .db_registry
        .insert("sqliteish".to_owned(), DriverMeta::default());
    params.expanded_connections.insert("::s1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot {
            database: Some("local".into()),
            ..ConnectionSnapshot::new("s1", "Local", "sqliteish")
        }],
    }];
    params
        .active_connections
        .insert("s1".into(), connected_entry("sss"));
    params.schemas.insert(
        "sss".into(),
        SchemaStateSnapshot {
            database_type: Some("sqliteish".into()),
            current_database: Some("local".into()),
            loading: true,
            tables: vec![],
            ..Default::default()
        },
    );
    params.expanded_dbs.insert("s1::local".to_owned());
    let rows = rows_of(&params);
    let NavigatorRow::Db { loading, .. } = first_of(&rows, "db") else {
        panic!()
    };
    assert!(loading);
    let NavigatorRow::DbLoading { owner_key, .. } = first_of(&rows, "db-loading") else {
        panic!()
    };
    assert_eq!(owner_key, "db:s1::local");
}

#[test]
fn the_single_database_branch_reads_schema_names_for_its_ladder() {
    let mut params = base_params();
    params
        .db_registry
        .insert("sqliteish".to_owned(), DriverMeta::default());
    params.expanded_connections.insert("::s1".to_owned());
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![ConnectionSnapshot {
            database: Some("local".into()),
            ..ConnectionSnapshot::new("s1", "Local", "sqliteish")
        }],
    }];
    params
        .active_connections
        .insert("s1".into(), connected_entry("sss"));
    params.schemas.insert(
        "sss".into(),
        SchemaStateSnapshot {
            database_type: Some("sqliteish".into()),
            current_database: Some("local".into()),
            tables: vec![tbl_in("main", "rows", TableType::Table)],
            schema_names: vec!["empty".to_owned(), "main".to_owned()],
            ..Default::default()
        },
    );
    params.expanded_dbs.insert("s1::local".to_owned());
    let rows = rows_of(&params);
    // The session's schema list seeds a ladder even for schemas with no
    // tables — single-db has no same-name folding.
    let schemas: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Schema { schema_name, .. } => Some(schema_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(schemas, vec!["empty", "main"]);
}

// ---------------------------------------------------------------------------
// stable keys, end to end
// ---------------------------------------------------------------------------

#[test]
fn every_row_kind_has_a_unique_stable_key() {
    let mut params = db_error_params();
    params.schemas.get_mut("sess-1").unwrap().databases = vec!["orders".into()];
    params.expanded_cats.insert("c1::orders::tables".to_owned());
    params.db_tables_map.insert(
        "sess-1::orders".to_owned(),
        vec![tbl("invoices", TableType::Table)],
    );
    let mut params = params;
    params.db_registry.insert(
        "postgresql".to_owned(),
        DriverMeta {
            has_multi_database: true,
            supported_object_kinds: vec!["function".to_owned()],
            ..Default::default()
        },
    );
    let rows = rows_of(&params);
    let keys: Vec<String> = rows.iter().map(navigator_row_key).collect();
    let unique: HashSet<&String> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len(), "keys collide: {keys:?}");
}

#[test]
fn identically_named_objects_in_two_connections_get_distinct_keys() {
    let mut conn = ConnectionSnapshot::new("c1", "One", "mysql");
    conn.options = None;
    let mut params = base_params();
    params
        .db_registry
        .get_mut("mysql")
        .unwrap()
        .supported_object_kinds = vec!["function".to_owned()];
    params.grouped = vec![GroupedSection {
        group: String::new(),
        connections: vec![conn, ConnectionSnapshot::new("c2", "Two", "mysql")],
    }];
    for id in ["c1", "c2"] {
        params
            .active_connections
            .insert(id.to_owned(), connected_entry("s"));
    }
    // One shared session: both connections show the same objects.
    params.schemas.insert(
        "s".into(),
        SchemaStateSnapshot {
            databases: vec!["app".into()],
            ..Default::default()
        },
    );
    params.expanded_connections.insert("::c1".to_owned());
    params.expanded_connections.insert("::c2".to_owned());
    params.expanded_dbs.insert("c1::app".to_owned());
    params.expanded_dbs.insert("c2::app".to_owned());
    params.expanded_cats.insert("c1::app::function".to_owned());
    params.expanded_cats.insert("c2::app::function".to_owned());
    params.db_objects_map.insert(
        "c1::app::function".to_owned(),
        vec![obj("function", "fn_calc")],
    );
    params.db_objects_map.insert(
        "c2::app::function".to_owned(),
        vec![obj("function", "fn_calc")],
    );
    let rows = rows_of(&params);
    let objects: Vec<&NavigatorRow> = rows.iter().filter(|row| row.kind() == "object").collect();
    assert_eq!(objects.len(), 2);
    let keys: Vec<String> = objects.iter().map(|row| navigator_row_key(row)).collect();
    assert_ne!(keys[0], keys[1], "the owner tuple must keep them apart");
    assert!(keys[0].starts_with("obj:c1:app"));
    assert!(keys[1].starts_with("obj:c2:app"));
}

#[test]
fn the_expand_key_is_scoped_per_section() {
    use crate::views::navigator::build_flat_rows::deps::locator::connection_expand_key;
    assert_eq!(connection_expand_key("g1", "c1"), "g1::c1");
    assert_eq!(connection_expand_key("", "c1"), "::c1");
    // The builder only expands the connection in the section whose key is in
    // the set: expanding in pinned must not expand the ungrouped copy.
    let mut params = base_params();
    params
        .active_connections
        .insert("c1".into(), connected_entry("sess-1"));
    params.grouped = vec![
        GroupedSection {
            group: "__pinned__".to_owned(),
            connections: vec![mysql_conn()],
        },
        GroupedSection {
            group: String::new(),
            connections: vec![mysql_conn()],
        },
    ];
    params.expanded_groups = HashSet::from(["__pinned__".to_owned(), String::new()]);
    params.expanded_connections = HashSet::from(["__pinned__::c1".to_owned()]);
    let rows = rows_of(&params);
    let connections: Vec<&NavigatorRow> = rows
        .iter()
        .filter(|row| row.kind() == "connection")
        .collect();
    let NavigatorRow::Connection {
        expanded: pinned_expanded,
        ..
    } = connections[0]
    else {
        panic!()
    };
    let NavigatorRow::Connection {
        expanded: plain_expanded,
        ..
    } = connections[1]
    else {
        panic!()
    };
    assert!(pinned_expanded);
    assert!(!plain_expanded);
}
