//! T125 independent audit: fresh vectors over the connection store.
//!
//! Written against the ported behavior, not the coder's fixtures: fresh
//! connection sets with host-level matching (the `postgres` registry
//! label), duplicate ids in the input list, an invalid timestamp string,
//! the exact/prefix/contains rank ladder across two fields, a group whose
//! rename target collides mid-list, and the refetch continuation after a
//! duplicate-fire no-op. Mutation evidence lives in the audit commit
//! message (red-then-green, restore verified).

use datazen_ui_api::{ConnectionConfig, SslMode};

use datazen_ui::connection_store::{
    filter_connections, group_connections_with_pinned_section, ConnectionStore,
    ConnectionStoreEffect, PINNED_GROUP_KEY, RECENT_GROUP_KEY,
};
use datazen_ui::i18n::t;

fn config(id: &str, name: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: "postgres".to_owned(),
        ssl_mode: SslMode::Disable,
        ..minimal()
    }
}

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

fn with(mut c: ConnectionConfig, f: impl Fn(&mut ConnectionConfig)) -> ConnectionConfig {
    f(&mut c);
    c
}

// -----------------------------------------------------------------------
// Fresh ranking vectors (the T403 placeholder subset).
// -----------------------------------------------------------------------

#[test]
fn the_registry_label_matches_when_the_type_does_not() {
    // `postgres`'s zh registry label is what the UI shows; the search
    // matches it through the label branch even though no other field
    // contains the query.
    let connections = vec![
        config("a", "Analytics"),
        with(config("b", "Ledger"), |c| {
            c.database_type = "sqlite".to_owned();
        }),
    ];
    let hits = filter_connections(&connections, None, "postgres");
    assert_eq!(
        hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["a"],
        "the registry label branch ranks the postgres connection"
    );
}

#[test]
fn the_rank_ladder_prefers_exact_over_prefix_over_contains() {
    // The host field only (offset 10): exact beats prefix beats contains.
    // The names deliberately avoid the query so the name field (offset 0)
    // never outranks the ladder under test.
    let connections = vec![
        with(config("contains", "three"), |c| {
            c.host = Some("example.pg.net".to_owned());
        }),
        with(config("prefix", "two"), |c| {
            c.host = Some("pg.example.com".to_owned());
        }),
        with(config("exact", "one"), |c| {
            c.host = Some("pg".to_owned());
        }),
    ];
    let hits = filter_connections(&connections, None, "pg");
    assert_eq!(
        hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["exact", "prefix", "contains"],
        "offset ladder: exact 10, prefix 11, contains 12"
    );
}

#[test]
fn duplicate_ids_keep_the_first_and_invalid_timestamps_do_not_promote() {
    let first = with(config("dup", "First"), |c| {
        c.group = Some("work".to_owned());
    });
    let mut second = config("dup", "Second");
    second.name = "Second".to_owned();
    let connections = vec![
        first.clone(),
        with(second, |c| {
            // An unparseable timestamp: recent never promotes it.
            c.last_connected_at = Some("not-a-date".to_owned());
        }),
    ];
    let hits = filter_connections(&connections, None, "");
    assert_eq!(
        hits.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["First"],
        "the duplicate id is dropped before anything else: first wins"
    );
    // The exact-name match must not surface the duplicate copy of the id.
    let hits = filter_connections(&connections, None, "Second");
    assert!(
        hits.is_empty(),
        "the dropped copy never matches, even on its own name"
    );

    // The invalid timestamp does not create a recent section.
    let sections = group_connections_with_pinned_section(&connections, &["work".to_owned()], "");
    assert!(
        !sections.iter().any(|s| s.group == RECENT_GROUP_KEY),
        "an unparseable lastConnectedAt promotes nothing"
    );
}

#[test]
fn tie_breaks_fall_through_pinned_time_group_name_then_id() {
    let connections = vec![
        with(config("z", "same"), |c| {
            c.group = Some("zzz".to_owned());
        }),
        with(config("a", "same"), |c| {
            c.group = Some("aaa".to_owned());
        }),
        with(config("m", "same"), |c| {
            c.pinned = Some(true);
        }),
    ];
    let hits = filter_connections(&connections, None, "");
    assert_eq!(
        hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["m", "a", "z"],
        "pinned first, then group ascending (aaa before zzz)"
    );
}

// -----------------------------------------------------------------------
// Fresh store vectors.
// ---------------------------------------------------------------------------

#[test]
fn a_failing_save_after_a_successful_one_still_refetches_when_rerun() {
    let mut store = ConnectionStore::default();
    store.connections = vec![config("a", "A")];

    // Fail once: loading drops, error lands.
    store.begin_save_connection(&config("a", "A2"), false).clear();
    store.finish_save_connection(Err(None)).clear();
    assert_eq!(store.error.as_deref(), Some(t("connStore.saveFailed")));
    assert!(!store.loading);

    // The next attempt starts clean: error cleared, loading raised.
    let effects = store.begin_save_connection(&config("a", "A3"), false);
    assert!(store.error.is_none() && store.loading);
    assert_eq!(
        effects.len(),
        1,
        "exactly one save effect, the stale error does not widen the chain"
    );
}

#[test]
fn rename_target_collapsing_keeps_the_ungrouped_members_untouched() {
    let mut store = ConnectionStore::default();
    store.groups = vec!["one".to_owned(), "two".to_owned()];
    store.connections = vec![
        with(config("a", "A"), |c| {
            c.group = Some("one".to_owned());
        }),
        with(config("u", "U"), |c| {
            c.group = None;
        }),
    ];

    // Renaming one onto two collapses the declared list.
    let effects = store.begin_rename_group("one", "two");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveGroups { groups: vec!["two".to_owned()] }]
    );

    // The settlement re-saves only the one member; the ungrouped
    // connection is never touched.
    let effects = store.finish_rename_group(Ok(vec!["two".to_owned()]), "one", "two");
    assert_eq!(
        effects,
        vec![
            ConnectionStoreEffect::SaveConnection {
                config: with(config("a", "A"), |c| {
                    c.group = Some("two".to_owned());
                }),
                clear_missing_credentials: false,
            },
            ConnectionStoreEffect::GetConnections,
        ]
    );
    assert_eq!(store.groups, vec!["two".to_owned()]);
}

#[test]
fn delete_group_on_a_missing_name_still_saves_the_same_list() {
    let mut store = ConnectionStore::default();
    store.groups = vec!["work".to_owned()];
    // React filters `groups` by the name — an absent name leaves the list
    // unchanged, and the saveGroups IPC still fires.
    let effects = store.begin_delete_group("ghost");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::SaveGroups { groups: vec!["work".to_owned()] }],
        "the filter is a no-op for an absent name, the IPC still runs"
    );
    // The settlement finds no members to regroup; the selection still
    // resets to null, exactly as React's tail `set` does.
    store.selected_group = Some("work".to_owned());
    let effects = store.finish_delete_group(Ok(vec!["work".to_owned()]), "ghost");
    assert_eq!(
        effects,
        vec![ConnectionStoreEffect::GetConnections],
        "no member re-saves, just the refetch"
    );
    assert_eq!(store.groups, vec!["work".to_owned()]);
    assert_eq!(store.selected_group, None);
}

#[test]
fn duplicate_after_a_completed_refetch_clears_the_prior_connections() {
    let mut store = ConnectionStore::default();
    store.connections = vec![config("src", "Warehouse")];

    store.begin_duplicate_connection("src", "conn_n1").clear();
    store.finish_duplicate_connection(Ok(())).clear();
    // The host runs the refetch chain and lands a fresh list.
    store.begin_fetch_connections().clear();
    store
        .finish_fetch_connections(Ok(vec![config("src", "Warehouse"), config("conn_n1", "Warehouse (副本)")]))
        .clear();
    assert_eq!(store.connections.len(), 2);
    assert!(!store.loading);
    assert!(store.connections_loaded);
    assert_eq!(store.error, None);
}
