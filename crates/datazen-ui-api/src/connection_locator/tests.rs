use std::collections::BTreeMap;

use crate::connection_types::{
    ConnectionConfig, DatabaseObject, DatabaseObjectKind, SslMode, TableInfo, TableType,
};
use crate::build_editor_schema::SqlNamespace;

use super::*;

fn conn(id: &str, name: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: "postgres".to_owned(),
        host: None,
        port: None,
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Disable,
        connection_timeout: None,
        max_pool_size: None,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: None,
        pinned: None,
    }
}

fn table(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
    TableInfo {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        table_type,
        row_count: None,
    }
}

fn object(name: &str, schema: Option<&str>) -> DatabaseObject {
    DatabaseObject {
        kind: DatabaseObjectKind::Function,
        schema: schema.map(str::to_owned),
        name: name.to_owned(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

fn connected(session: &str) -> ConnectionLocatorUsageState {
    ConnectionLocatorUsageState {
        active_connections: vec![(
            "c1".to_owned(),
            ActiveConnectionSlice {
                db_session_id: session.to_owned(),
                status: ActiveStatus::Connected,
            },
        )],
        ..Default::default()
    }
}

fn ids(results: &[ConnectionLocatorResult]) -> Vec<&str> {
    results.iter().map(|result| result.connection.id.as_str()).collect()
}

#[test]
fn expand_key_roundtrip_and_first_separator() {
    assert_eq!(connection_expand_key("nav", "c1"), "nav::c1");
    assert_eq!(
        parse_connection_expand_key("nav::c1"),
        ExpandKey { section_group: "nav".to_owned(), connection_id: "c1".to_owned() }
    );
    assert_eq!(
        parse_connection_expand_key("plain"),
        ExpandKey { section_group: String::new(), connection_id: "plain".to_owned() }
    );
    assert_eq!(
        parse_connection_expand_key("a::b::c"),
        ExpandKey { section_group: "a".to_owned(), connection_id: "b::c".to_owned() }
    );
    assert_eq!(
        parse_connection_expand_key("::c1"),
        ExpandKey { section_group: String::new(), connection_id: "c1".to_owned() }
    );
}

#[test]
fn normalize_trims_and_lowercases() {
    assert_eq!(normalize(Some("  MyDB ")), "mydb");
    assert_eq!(normalize(None), "");
    assert_eq!(normalize(Some("   ")), "");
}

#[test]
fn empty_query_returns_all_connections_with_null_match_in_stable_order() {
    let mut a = conn("c3", "Zulu");
    a.group = Some("beta".to_owned());
    let mut b = conn("c1", "alpha");
    b.group = Some("alpha".to_owned());
    let mut c = conn("c2", "alpha");
    c.group = Some("alpha".to_owned());
    let connections = vec![a, b, c];

    let results = rank_connections(&connections, "   ", &ConnectionLocatorUsageState::default(), &|_| None);
    assert_eq!(ids(&results), vec!["c1", "c2", "c3"]);
    assert!(results.iter().all(|result| result.match_value.is_none()));
}

#[test]
fn duplicate_ids_keep_the_first_connection() {
    let mut first = conn("c1", "First");
    first.host = Some("first-host".to_owned());
    let mut second = conn("c1", "Second");
    second.host = Some("second-host".to_owned());

    let results = rank_connections(&[first, second], "", &ConnectionLocatorUsageState::default(), &|_| None);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].connection.name, "First");
}

#[test]
fn scalar_fields_rank_exact_prefix_and_contains() {
    let mut connection = conn("c1", "My Database");
    connection.host = Some("db.example.com".to_owned());
    connection.database = Some("analytics".to_owned());

    let label = |value: &str| if value == "postgres" { Some("PostgreSQL".to_owned()) } else { None };

    let exact = rank_connections(&[connection.clone()], "my database", &ConnectionLocatorUsageState::default(), &label);
    assert_eq!(exact[0].match_value.as_ref().expect("match").rank, 0);
    assert_eq!(exact[0].match_value.as_ref().expect("match").reason, ConnectionMatchReason::Name);

    let prefix = rank_connections(&[connection.clone()], "db", &ConnectionLocatorUsageState::default(), &label);
    let match_value = prefix[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.rank, 11);
    assert_eq!(match_value.reason, ConnectionMatchReason::Host);

    let contains = rank_connections(&[connection.clone()], "example", &ConnectionLocatorUsageState::default(), &label);
    let match_value = contains[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.rank, 12);
    assert_eq!(match_value.reason, ConnectionMatchReason::Host);

    let by_label = rank_connections(&[connection], "postgresql", &ConnectionLocatorUsageState::default(), &label);
    let match_value = by_label[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.rank, 30);
    assert_eq!(match_value.reason, ConnectionMatchReason::DatabaseType);
    assert_eq!(match_value.context, "PostgreSQL");
}

#[test]
fn non_matching_connections_are_filtered_out() {
    let connection = conn("c1", "Alpha");
    let results = rank_connections(&[connection], "zzz", &ConnectionLocatorUsageState::default(), &|_| None);
    assert!(results.is_empty());
}

#[test]
fn runtime_candidates_require_a_connected_session() {
    let mut connection = conn("c1", "Alpha");
    connection.database_type = "postgres".to_owned();
    let mut schema = SchemaSlice::default();
    schema.schema_names.push("secret_schema".to_owned());
    let mut usage = connected("s1");
    usage.schemas.push(("s1".to_owned(), schema));

    let results = rank_connections(&[connection.clone()], "secret_schema", &usage, &|_| None);
    assert_eq!(results.len(), 1, "a connected session exposes the schema");

    usage.active_connections[0].1.status = ActiveStatus::Connecting;
    let results = rank_connections(&[connection.clone()], "secret_schema", &usage, &|_| None);
    assert!(results.is_empty(), "a connecting session exposes no runtime data");

    usage.active_connections[0].1.status = ActiveStatus::Connected;
    usage.active_connections[0].1.db_session_id.clear();
    let results = rank_connections(&[connection], "secret_schema", &usage, &|_| None);
    assert!(results.is_empty(), "an empty session id exposes no runtime data");
}

#[test]
fn schema_tables_views_namespaces_and_paths_contribute_candidates() {
    let connection = conn("c1", "Alpha");
    let mut namespace = BTreeMap::new();
    namespace.insert(
        "sales".to_owned(),
        SqlNamespace::Branch(BTreeMap::from([("orders".to_owned(), SqlNamespace::table(&["id"]))])),
    );
    let mut schema = SchemaSlice {
        databases: vec!["main_db".to_owned()],
        schema_names: vec!["public".to_owned()],
        namespace_tree: SqlNamespace::Branch(namespace),
        ..Default::default()
    };
    schema.tables.push(table("users", Some("public"), TableType::Table));
    schema.views.push(table("active_users", Some("public"), TableType::View));
    schema.path_items.push(("root".to_owned(), vec![table("path_file", None, TableType::Table)]));
    let usage = connected_with_schema("s1", schema);

    let by_database = rank_connections(&[connection.clone()], "main_db", &usage, &|_| None);
    assert_eq!(by_database[0].match_value.as_ref().expect("match").rank, 20);

    let by_schema = rank_connections(&[connection.clone()], "public", &usage, &|_| None);
    let match_value = by_schema[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.rank, 40);
    assert_eq!(match_value.reason, ConnectionMatchReason::Schema);

    let by_table = rank_connections(&[connection.clone()], "users", &usage, &|_| None);
    let match_value = by_table[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Table);
    assert_eq!(match_value.context, "public.users");

    let by_view = rank_connections(&[connection.clone()], "active_users", &usage, &|_| None);
    assert_eq!(by_view[0].match_value.as_ref().expect("match").reason, ConnectionMatchReason::View);

    let by_materialized = rank_connections(&[connection.clone()], "sales", &usage, &|_| None);
    let match_value = by_materialized[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Namespace);
    assert_eq!(match_value.context, "sales");
    assert_eq!(match_value.rank, 60);

    let by_namespace_leaf = rank_connections(&[connection.clone()], "orders", &usage, &|_| None);
    let match_value = by_namespace_leaf[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Table);
    assert_eq!(match_value.context, "sales/orders");
    assert_eq!(match_value.rank, 60);

    let by_path = rank_connections(&[connection], "path_file", &usage, &|_| None);
    let match_value = by_path[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Path);
    assert_eq!(match_value.rank, 70);
    assert_eq!(match_value.context, "path_file");
}

fn connected_with_schema(session: &str, schema: SchemaSlice) -> ConnectionLocatorUsageState {
    let mut usage = connected(session);
    usage.schemas.push((session.to_owned(), schema));
    usage
}

#[test]
fn db_tables_map_is_session_scoped_and_contributes_database_and_tables() {
    let connection = conn("c1", "Alpha");
    let mut usage = connected("s1");
    usage.db_tables_map.push((
        "s1::analytics".to_owned(),
        vec![table("events", Some("public"), TableType::Table)],
    ));
    usage.db_tables_map.push((
        "other::wrong".to_owned(),
        vec![table("hidden", None, TableType::Table)],
    ));

    let by_database = rank_connections(&[connection.clone()], "analytics", &usage, &|_| None);
    assert_eq!(by_database[0].match_value.as_ref().expect("match").reason, ConnectionMatchReason::Database);

    let by_table = rank_connections(&[connection.clone()], "events", &usage, &|_| None);
    let match_value = by_table[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Table);
    assert_eq!(match_value.context, "public.events");
    assert_eq!(match_value.rank, 50);

    let hidden = rank_connections(&[connection], "hidden", &usage, &|_| None);
    assert!(hidden.is_empty(), "a different session prefix is ignored");
}

#[test]
fn db_objects_map_is_connection_scoped_and_strips_the_kind_suffix() {
    let connection = conn("c1", "Alpha");
    let mut usage = connected("s1");
    usage.db_objects_map.push((
        "c1::myschema::functions".to_owned(),
        vec![object("compute", Some("myschema"))],
    ));
    usage.db_objects_map.push((
        "c2::other".to_owned(),
        vec![object("foreign_fn", None)],
    ));

    let results = rank_connections(&[connection.clone()], "compute", &usage, &|_| None);
    let match_value = results[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.reason, ConnectionMatchReason::Object);
    assert_eq!(match_value.context, "myschema.myschema.compute");
    assert_eq!(match_value.rank, 50);

    let foreign = rank_connections(&[connection], "foreign_fn", &usage, &|_| None);
    assert!(foreign.is_empty(), "another connection's objects are ignored");
}

#[test]
fn best_candidate_breaks_ties_by_reason_then_context() {
    let connection = conn("c1", "Alpha");
    let mut usage = ConnectionLocatorUsageState::default();
    usage.active_connections = vec![(
        "c1".to_owned(),
        ActiveConnectionSlice { db_session_id: "s1".to_owned(), status: ActiveStatus::Connected },
    )];
    let mut schema = SchemaSlice::default();
    schema.schema_names.push("zeta".to_owned());
    schema.tables.push(table("zeta", Some("public"), TableType::Table));
    usage.schemas.push(("s1".to_owned(), schema));

    let results = rank_connections(&[connection], "zeta", &usage, &|_| None);
    let match_value = results[0].match_value.as_ref().expect("match");
    assert_eq!(match_value.rank, 40);
    assert_eq!(match_value.reason, ConnectionMatchReason::Schema);
    assert_eq!(match_value.context, "zeta");
}

#[test]
fn ranking_prefers_pinned_then_recent_then_group_name_id() {
    let mut plain = conn("c3", "Alpha");
    plain.group = Some("b".to_owned());
    let mut pinned = conn("c2", "Zulu");
    pinned.pinned = Some(true);
    let mut recent = conn("c1", "Beta");
    recent.last_connected_at = Some("2026-01-02T00:00:00Z".to_owned());

    let results = rank_connections(
        &[plain, pinned, recent],
        "",
        &ConnectionLocatorUsageState::default(),
        &|_| None,
    );
    assert_eq!(ids(&results), vec!["c2", "c1", "c3"]);
}

#[test]
fn invalid_timestamp_sorts_after_valid_and_group_name_id_break_ties() {
    let mut invalid = conn("c4", "Zzz");
    invalid.last_connected_at = Some("not-a-date".to_owned());
    let mut newer = conn("c2", "Zzz");
    newer.last_connected_at = Some("2026-01-02".to_owned());
    let mut older = conn("c3", "Zzz");
    older.last_connected_at = Some("2026-01-01".to_owned());
    let mut no_date = conn("c1", "Zzz");

    let results = rank_connections(
        &[invalid, newer, older, no_date],
        "",
        &ConnectionLocatorUsageState::default(),
        &|_| None,
    );
    assert_eq!(ids(&results), vec!["c2", "c3", "c1", "c4"]);
}

#[test]
fn order_for_display_keeps_pinned_first_and_preserves_the_rest() {
    let mut first = conn("c1", "First");
    first.pinned = Some(true);
    let second = conn("c2", "Second");
    let mut third = conn("c3", "Third");
    third.pinned = Some(true);
    let connections = vec![second.clone(), first.clone(), third.clone()];
    assert_eq!(
        order_connections_for_display(&connections).iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        vec!["c1", "c3", "c2"]
    );
    let plain = vec![second, conn("c4", "Fourth")];
    assert_eq!(order_connections_for_display(&plain), plain);
}

#[test]
fn grouping_without_promotions_keeps_group_order_and_empty_groups() {
    let mut work = conn("c1", "Work");
    work.group = Some("work".to_owned());
    let mut home = conn("c2", "Home");
    home.group = Some("home".to_owned());
    let mut other = conn("c3", "Other");
    other.group = Some("misc".to_owned());
    let groups = vec!["home".to_owned(), "work".to_owned(), "empty".to_owned()];

    let sections = group_connections_with_recent_sections(&[work, home, other], &groups);
    assert_eq!(sections.iter().map(|section| section.group.as_str()).collect::<Vec<_>>(), vec!["home", "work", "empty", "misc"]);
    assert_eq!(sections[0].connections[0].id, "c2");
    assert_eq!(sections[1].connections[0].id, "c1");
    assert!(sections[2].connections.is_empty());
    assert_eq!(sections[3].connections[0].id, "c3");
}

#[test]
fn grouping_promotes_pinned_and_recent_and_keeps_them_in_their_groups() {
    let mut pinned = conn("c1", "Pinned");
    pinned.group = Some("work".to_owned());
    pinned.pinned = Some(true);
    let mut recent = conn("c2", "Recent");
    recent.group = Some("work".to_owned());
    recent.last_connected_at = Some("2026-01-02T00:00:00Z".to_owned());
    let mut other = conn("c3", "Other");
    other.group = Some("home".to_owned());
    let groups = vec!["home".to_owned(), "work".to_owned()];

    let sections = group_connections_with_recent_sections(&[pinned, recent, other], &groups);
    assert_eq!(
        sections.iter().map(|section| section.group.as_str()).collect::<Vec<_>>(),
        vec![PINNED_GROUP_KEY, RECENT_GROUP_KEY, "home", "work"]
    );
    assert_eq!(sections[0].connections[0].id, "c1");
    assert_eq!(sections[1].connections[0].id, "c2");
    assert_eq!(sections[2].connections[0].id, "c3");
    assert_eq!(sections[3].connections.len(), 2, "pinned and recent stay in their persisted group");
}

#[test]
fn recent_section_is_capped_and_sorted_by_time_name_id() {
    let mut connections = Vec::new();
    for index in 0..7 {
        let mut connection = conn(&format!("c{index}"), &format!("Name{}", 6 - index));
        connection.group = Some("g".to_owned());
        connection.last_connected_at = Some(format!("2026-01-{:02}", index + 1));
        connections.push(connection);
    }
    let sections = group_connections_with_recent_sections(&connections, &["g".to_owned()]);
    let recent = sections.iter().find(|section| section.group == RECENT_GROUP_KEY).expect("recent section");
    assert_eq!(recent.connections.len(), RECENT_CONNECTION_LIMIT);
    assert_eq!(recent.connections[0].id, "c6");
    assert_eq!(recent.connections[4].id, "c2");
}

#[test]
fn recent_ties_fall_back_to_name_then_id() {
    let mut a = conn("c2", "alpha");
    a.group = Some("g".to_owned());
    a.last_connected_at = Some("2026-01-01".to_owned());
    let mut b = conn("c1", "Alpha");
    b.group = Some("g".to_owned());
    b.last_connected_at = Some("2026-01-01".to_owned());
    let sections = group_connections_with_recent_sections(&[a, b], &["g".to_owned()]);
    let recent = sections.iter().find(|section| section.group == RECENT_GROUP_KEY).expect("recent section");
    assert_eq!(recent.connections[0].id, "c1", "case-insensitive name tie, then id");
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::rank_connections;
    let _ = crate::order_connections_for_display;
    let _ = crate::group_connections_with_recent_sections;
    let _ = crate::connection_expand_key;
    let _ = crate::parse_connection_expand_key;
    let _: Option<crate::ConnectionLocatorUsageState> = None;
    let _: Option<crate::ConnectionLocatorResult> = None;
    let _: Option<crate::ConnectionMatch> = None;
    let _: Option<crate::ConnectionMatchReason> = None;
    let _: Option<crate::ConnectionSection> = None;
    let _: Option<crate::ActiveConnectionSlice> = None;
    let _: Option<crate::ActiveStatus> = None;
    let _: Option<crate::SchemaSlice> = None;
    let _: Option<crate::ExpandKey> = None;
    assert_eq!(crate::RECENT_CONNECTION_LIMIT, 5);
    assert_eq!(crate::RECENT_GROUP_KEY, "__recent__");
    assert_eq!(crate::PINNED_GROUP_KEY, "__pinned__");
}
