//! T125: the persisted-connections store — the display helpers and the
//! host-driven state machine, probed as pure transitions and effects.
//!
//! Every assertion observes the store's state and the returned effect
//! lists (the host's run steps), never a value the store reports about
//! itself: the ranking/grouping contracts (the T403 placeholder subset),
//! the load/refetch chains with their exact continuation order, the
//! failure arms with their localized messages, and the silent no-ops
//! React performs on missing sources and blank names.

use datazen_ui_api::{ConnectionConfig, SslMode};

use datazen_ui::connection_store::{
    filter_connections, group_connections, group_connections_with_pinned_section,
    sort_connections_in_group, ConnectionStore, ConnectionStoreEffect, EVENT_CONNECTIONS_CHANGED,
    PINNED_GROUP_KEY, RECENT_CONNECTION_LIMIT, RECENT_GROUP_KEY,
};
use datazen_ui::i18n::t;

/// One config with the fields the store reads: `id`, `name`, the group,
/// the pin, the timestamp, and the searchable text fields.
fn config(id: &str, name: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: "postgres".to_owned(),
        ssl_mode: SslMode::Disable,
        ..minimal()
    }
}

/// The remaining fields, all empty: `host`, `database`, and the other
/// options are `Option::None` exactly as a fresh form leaves them.
fn minimal() -> ConnectionConfig {
    ConnectionConfig {
        id: String::new(),
        name: String::new(),
        database_type: "postgres".to_owned(),
        ssl_mode: SslMode::Disable,
        host: None,
        port: None,
        database: None,
        schema: None,
        username: None,
        password: None,
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

fn pinned(mut c: ConnectionConfig) -> ConnectionConfig {
    c.pinned = Some(true);
    c
}

fn with_group(mut c: ConnectionConfig, group: &str) -> ConnectionConfig {
    c.group = Some(group.to_owned());
    c
}

fn with_last_connected(mut c: ConnectionConfig, at: &str) -> ConnectionConfig {
    c.last_connected_at = Some(at.to_owned());
    c
}

// -----------------------------------------------------------------------
// The constants.
// -----------------------------------------------------------------------

#[test]
fn the_module_constants_match_the_react_literals() {
    assert_eq!(EVENT_CONNECTIONS_CHANGED, "datazen:connections-changed");
    assert_eq!(RECENT_GROUP_KEY, "__recent__");
    assert_eq!(PINNED_GROUP_KEY, "__pinned__");
    assert_eq!(RECENT_CONNECTION_LIMIT, 5);
}

// -----------------------------------------------------------------------
// The display helpers (the T403 placeholder subset).
// -----------------------------------------------------------------------

fn fixture() -> Vec<ConnectionConfig> {
    vec![
        config("a", "Analytics"),
        pinned(with_group(config("b", "Billing"), "work")),
        with_group(config("c", "Cache Mirror"), "work"),
        with_last_connected(config("d", "Data lake"), "2026-11-19T08:00:00Z"),
        with_last_connected(config("e", "ETL"), "2026-11-18T08:00:00Z"),
        with_last_connected(config("f", "Flat file"), "2026-11-01T00:00:00Z"),
        config("g", "Growth"),
    ]
}

#[test]
fn filter_connections_ranks_matches_and_keeps_the_rest_when_blank() {
    let connections = fixture();
    let all = filter_connections(&connections, None, "  ");
    assert_eq!(all.len(), 7, "a blank query keeps every connection");

    let matches = filter_connections(&connections, None, "ANALYTICS");
    assert_eq!(
        matches.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a"],
        "a case-insensitive name match, exact word"
    );

    // The selected group filters before the ranking.
    let work = filter_connections(&connections, Some("work"), "cache");
    assert_eq!(
        work.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["c"],
        "the group filter precedes the search"
    );
}

#[test]
fn sort_connections_in_group_puts_pinned_first_and_keeps_the_rest_stable() {
    let connections = fixture();
    let ordered = sort_connections_in_group(&connections);
    assert_eq!(
        ordered.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["b", "a", "c", "d", "e", "f", "g"],
        "pinned first, the persisted order otherwise"
    );
}

#[test]
fn group_connections_orders_declared_extra_then_ungrouped() {
    let connections = fixture();
    let groups = vec!["work".to_owned(), "team".to_owned()];
    let sections = group_connections(&connections, &groups, "");
    let keys: Vec<&str> = sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(keys, vec!["work", "team", ""], "declared order, ungrouped last");

    let work = &sections[0];
    assert_eq!(
        work.connections.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["b", "c"],
        "b pinned first within the group"
    );
    let team = &sections[1];
    assert!(team.connections.is_empty(), "an empty declared group still lists");
    let ungrouped = sections.last().expect("the ungrouped section");
    assert_eq!(
        ungrouped.connections.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "d", "e", "f", "g"]
    );
}

#[test]
fn group_connections_searching_keeps_the_ranked_order() {
    let connections = fixture();
    let groups = vec!["work".to_owned()];
    let sections = group_connections(&connections, &groups, "cache");
    // Only the matching connection survives; its group section lists it
    // in ranked order without the pinned-first re-sort.
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].group, "work");
    assert_eq!(sections[0].connections.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
}

#[test]
fn pinned_and_recent_sections_promote_over_the_plain_groups() {
    let connections = fixture();
    let groups = vec!["work".to_owned()];
    let sections = group_connections_with_pinned_section(&connections, &groups, "");
    let keys: Vec<&str> = sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(
        keys,
        vec![PINNED_GROUP_KEY, RECENT_GROUP_KEY, "work", ""],
        "pinned and recent hoist above the declared groups"
    );
    assert_eq!(sections[0].connections.len(), 1, "the one pinned connection");
    // The recent section: the three timestamped connections, newest first.
    assert_eq!(
        sections[1]
            .connections
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["d", "e", "f"],
        "lastConnectedAt descending, capped at 5"
    );
    // The pinned connection still appears in its persisted group.
    assert_eq!(
        sections[2].connections.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["b", "c"],
        "the promoted shortcuts stay in their group too"
    );
}

#[test]
fn without_promoted_connections_the_plain_grouping_applies() {
    let connections = vec![config("a", "Analytics"), with_group(config("c", "Cache Mirror"), "work")];
    let groups = vec!["work".to_owned()];
    let sections = group_connections_with_pinned_section(&connections, &groups, "");
    let keys: Vec<&str> = sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(keys, vec!["work", ""], "no pinned, no timestamped: no promoted sections");
}

#[test]
fn searching_bypasses_the_promoted_sections() {
    let connections = fixture();
    let groups = vec!["work".to_owned()];
    let sections = group_connections_with_pinned_section(&connections, &groups, "cache");
    let keys: Vec<&str> = sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(keys, vec!["work"], "a search query routes to the plain grouping");
}

#[test]
fn the_recent_cap_is_five_even_with_more_timestamped_connections() {
    let mut connections: Vec<ConnectionConfig> = (0..7)
        .map(|i| with_last_connected(config(&format!("c{i}"), &format!("conn {i}")), "2026-11-01T00:00:00Z"))
        .collect();
    connections.sort_by(|a, b| a.id.cmp(&b.id));
    let sections = group_connections_with_pinned_section(&connections, &[], "");
    let recent = sections
        .iter()
        .find(|s| s.group == RECENT_GROUP_KEY)
        .expect("the recent section");
    assert_eq!(recent.connections.len(), RECENT_CONNECTION_LIMIT, "capped at 5");
}

// -----------------------------------------------------------------------
// The load chain.
// -----------------------------------------------------------------------

#[test]
fn fetch_connections_loads_clears_and_marks_the_first_attempt() {
    let mut store = ConnectionStore::default();
    store.error = Some("stale".to_owned());

    let effects = store.begin_fetch_connections();
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::GetConnections],
        "one IPC effect"
    );
    assert!(store.loading && store.error.is_none(), "loading raised, error cleared");

    let effects = store.finish_fetch_connections(Ok(vec![config("a", "A")]));
    assert!(effects.is_empty(), "the chain ends here");
    assert_eq!(store.connections.len(), 1);
    assert!(!store.loading && store.connections_loaded, "the attempt is recorded");

    // A failing fetch keeps the old list, marks the attempt, and records
    // the host's message verbatim.
    store.begin_fetch_connections();
    store
        .finish_fetch_connections(Err(Some("backend down".to_owned())))
        .clear();
    assert_eq!(store.connections.len(), 1, "the list survives the failure");
    assert_eq!(store.error.as_deref(), Some("backend down"));
    assert!(store.connections_loaded);

    // Without a host message, the localized fallback.
    store.begin_fetch_connections();
    store.finish_fetch_connections(Err(None)).clear();
    assert_eq!(store.error.as_deref(), Some(t("connStore.loadFailed")));
}

#[test]
fn fetch_groups_touches_no_state_until_the_answer() {
    let mut store = ConnectionStore::default();
    let effects = store.begin_fetch_groups();
    assert_eq!(effects, vec![ConnectionStoreEffect::GetGroups]);
    assert_eq!(store, ConnectionStore::default(), "no state moves first");

    store.finish_fetch_groups(Ok(vec!["work".to_owned()])).clear();
    assert_eq!(store.groups, vec!["work".to_owned()]);
    store.finish_fetch_groups(Err(Some("x".to_owned()))).clear();
    assert_eq!(store.groups, vec!["work".to_owned()], "a failure changes nothing");
}

// -----------------------------------------------------------------------
// The save / duplicate / delete chains.
// -----------------------------------------------------------------------

#[test]
fn a_successful_save_returns_the_exact_refetch_continuation() {
    let mut store = ConnectionStore::default();
    let config = config("a", "A");
    let effects = store.begin_save_connection(&config, true);
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveConnection {
            config: config.clone(),
            clear_missing_credentials: true,
        }]
    );
    assert!(store.loading && store.error.is_none());

    // React's order: fetchConnections, fetchGroups, then the void emit.
    let effects = store.finish_save_connection(Ok(()));
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::GetConnections,
            ConnectionStoreEffect::GetGroups,
            ConnectionStoreEffect::BroadcastConnectionsChanged,
        ]
    );
    assert!(store.loading, "the refetch chain ends the loading flag");

    // The refetch landing clears it.
    store.finish_fetch_connections(Ok(vec![config.clone()])).clear();
    assert!(!store.loading);
}

#[test]
fn a_failed_save_clears_loading_with_the_localized_message() {
    let mut store = ConnectionStore::default();
    store.begin_save_connection(&config("a", "A"), false).clear();
    store.finish_save_connection(Err(Some("boom".to_owned()))).clear();
    assert!(!store.loading);
    assert_eq!(store.error.as_deref(), Some("boom"));

    let mut store = ConnectionStore::default();
    store.begin_save_connection(&config("a", "A"), false).clear();
    store.finish_save_connection(Err(None)).clear();
    assert_eq!(store.error.as_deref(), Some(t("connStore.saveFailed")));
}

#[test]
fn duplicate_derives_the_copy_name_and_takes_the_hosts_id() {
    let mut store = ConnectionStore::default();
    store.connections = vec![config("src", "Warehouse")];

    // A missing source is a silent no-op.
    let effects = store.begin_duplicate_connection("nope", "conn_x");
    assert!(effects.is_empty() && !store.loading);

    let effects = store.begin_duplicate_connection("src", "conn_new1");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::DuplicateConnection {
            id: "src".to_owned(),
            new_id: "conn_new1".to_owned(),
            new_name: format!("Warehouse ({})", t("conn.copyName")),
        }],
        "the copy name is the React formula"
    );
    assert!(store.loading);

    // The continuation is the save-shaped one.
    let effects = store.finish_duplicate_connection(Ok(()));
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::GetConnections,
            ConnectionStoreEffect::GetGroups,
            ConnectionStoreEffect::BroadcastConnectionsChanged,
        ]
    );
}

#[test]
fn delete_carries_its_own_failure_message() {
    let mut store = ConnectionStore::default();
    let effects = store.begin_delete_connection("a");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::DeleteConnection { id: "a".to_owned() }]
    );
    store.finish_delete_connection(Err(None)).clear();
    assert_eq!(store.error.as_deref(), Some(t("connStore.deleteFailed")));
    assert!(!store.loading);
}

#[test]
fn test_connection_is_a_pure_pass_through() {
    let mut store = ConnectionStore::default();
    let config = config("a", "A");
    let before = store.clone();
    let effects = store.test_connection(&config);
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::TestConnection { config: config.clone() }]
    );
    assert_eq!(store, before, "no state moves");
}

// -----------------------------------------------------------------------
// The group-list edits.
// -----------------------------------------------------------------------

#[test]
fn add_group_appends_after_trim_and_ignores_blanks_and_duplicates() {
    let mut store = ConnectionStore::default();
    store.groups = vec!["work".to_owned()];

    assert!(store.begin_add_group("   ").is_empty(), "blank is a no-op");
    assert!(store.begin_add_group("work").is_empty(), "duplicate is a no-op");

    let effects = store.begin_add_group("  prod ");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveGroups { groups: vec!["work".to_owned(), "prod".to_owned()] }],
        "trimmed and appended"
    );

    store.finish_add_group(Ok(vec!["work".to_owned(), "prod".to_owned()])).clear();
    assert_eq!(store.groups, vec!["work".to_owned(), "prod".to_owned()]);
    store.finish_add_group(Err(Some("x".to_owned()))).clear();
    assert_eq!(store.groups, vec!["work".to_owned(), "prod".to_owned()], "a failure changes nothing");
}

#[test]
fn rename_group_regroups_the_members_and_lands_the_list() {
    let mut store = ConnectionStore::default();
    store.groups = vec!["work".to_owned(), "team".to_owned()];
    store.connections = vec![
        with_group(config("a", "A"), "work"),
        with_group(config("b", "B"), "team"),
    ];

    assert!(store.begin_rename_group("work", "  ").is_empty(), "blank is a no-op");
    assert!(store.begin_rename_group("work", "work").is_empty(), "same name is a no-op");

    // The rename dedupes: renaming work onto team collapses the list.
    let effects = store.begin_rename_group("work", "team");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveGroups { groups: vec!["team".to_owned()] }],
        "Set-dedupe collapses the renamed list"
    );

    // The settlement regroups the members and refetches.
    let effects = store.finish_rename_group(
        Ok(vec!["team".to_owned()]),
        "work",
        "team",
    );
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::SaveConnection {
                config: with_group(config("a", "A"), "team"),
                clear_missing_credentials: false,
            },
            ConnectionStoreEffect::GetConnections,
        ],
        "the work member is re-saved into team, then the refetch"
    );
    assert_eq!(store.groups, vec!["team".to_owned()]);
}

#[test]
fn delete_group_ungroups_the_members_and_clears_the_selection() {
    let mut store = ConnectionStore::default();
    store.groups = vec!["work".to_owned()];
    store.selected_group = Some("work".to_owned());
    store.connections = vec![
        with_group(config("a", "A"), "work"),
        with_group(config("b", "B"), "other"),
    ];

    let effects = store.begin_delete_group("work");
    assert_eq!(effects, vec![ConnectionStoreEffect::SaveGroups { groups: vec![] }]);

    let effects = store.finish_delete_group(Ok(vec![]), "work");
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::SaveConnection {
                config: {
                    let mut ungrouped = with_group(config("a", "A"), "work");
                    ungrouped.group = None;
                    ungrouped
                },
                clear_missing_credentials: false,
            },
            ConnectionStoreEffect::GetConnections,
        ],
        "the work member is re-saved with no group, then the refetch"
    );
    assert!(store.groups.is_empty());
    assert_eq!(store.selected_group, None, "React resets the selection");
}

// -----------------------------------------------------------------------
// The per-connection moves.
// -----------------------------------------------------------------------

#[test]
fn move_connection_saves_the_regrouped_config_and_refetches_without_groups() {
    let mut store = ConnectionStore::default();
    store.connections = vec![with_group(config("a", "A"), "work")];

    assert!(store.begin_move_connection_to_group("nope", Some("team")).is_empty());

    let effects = store.begin_move_connection_to_group("a", Some("team"));
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveConnection {
            config: with_group(config("a", "A"), "team"),
            clear_missing_credentials: false,
        }]
    );
    assert!(store.loading);

    // React's move chain: fetchConnections then emit — NO fetchGroups.
    let effects = store.finish_move_connection_to_group(Ok(()));
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::GetConnections,
            ConnectionStoreEffect::BroadcastConnectionsChanged,
        ]
    );
    store.finish_move_connection_to_group(Err(None)).clear();
    assert_eq!(store.error.as_deref(), Some(t("connStore.saveFailed")));
}

#[test]
fn toggle_pinned_flips_the_flag_and_reuses_the_move_chain() {
    let mut store = ConnectionStore::default();
    store.connections = vec![pinned(config("a", "A"))];

    assert!(store.begin_toggle_connection_pinned("nope").is_empty());

    let effects = store.begin_toggle_connection_pinned("a");
    let mut expected = config("a", "A");
    expected.pinned = Some(false);
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveConnection {
            config: expected,
            clear_missing_credentials: false,
        }],
        "the pin flips from true to false"
    );

    let effects = store.finish_toggle_connection_pinned(Ok(()));
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::GetConnections,
            ConnectionStoreEffect::BroadcastConnectionsChanged,
        ]
    );
}

#[test]
fn the_selection_setters_write_the_plain_fields() {
    let mut store = ConnectionStore::default();
    store.set_selected_group(Some("work"));
    assert_eq!(store.selected_group.as_deref(), Some("work"));
    store.set_selected_group(None);
    assert_eq!(store.selected_group, None);
    store.set_search_query("cache ");
    assert_eq!(store.search_query, "cache ");
}
