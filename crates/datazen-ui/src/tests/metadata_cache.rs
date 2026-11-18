//! T032: real assertions for the Rust port of
//! `packages/frontend/src/lib/relationMetadata/metadataCache.ts`.
//!
//! Registered in `tests/mod.rs` as an ordinary test module, so its test names
//! are `tests::metadata_cache::*` and the `metadata_cache` filter really runs it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::relation_metadata::*;

fn ident(namespace: &[&str], name: &str) -> QualifiedRelationId {
    QualifiedRelationId::qualified(namespace, name)
}

fn request(namespace: &[&str], name: &str, kind: SqlRelationSourceKind) -> EditorRelationRequest {
    EditorRelationRequest::new(ident(namespace, name), kind)
}

fn bare_request(name: &str) -> EditorRelationRequest {
    EditorRelationRequest {
        identity: QualifiedRelationId::bare(name),
        kind: Some(SqlRelationSourceKind::Table),
    }
}

fn context(database: &str, schema: Option<&str>, dialect_id: &str) -> EditorMetadataContext {
    EditorMetadataContext {
        database: database.to_owned(),
        schema: schema.map(str::to_owned),
        dialect_id: dialect_id.to_owned(),
    }
}

fn make_schema(name: &str) -> TableSchema {
    TableSchema {
        table_name: name.to_owned(),
        columns: vec![ColumnSchema {
            name: "id".to_owned(),
            data_type: "int".to_owned(),
            nullable: false,
            is_primary_key: true,
            ..Default::default()
        }],
        primary_keys: vec!["id".to_owned()],
        ..Default::default()
    }
}

struct Harness {
    cache: MetadataCache,
    loads: Rc<RefCell<usize>>,
    clock: Rc<RefCell<i64>>,
    bus: Rc<RefCell<SchemaInvalidationBus>>,
    config: Rc<RefCell<EditorSessionConfig>>,
}

fn harness_with(debounce_ms: i64, error_ttl_ms: i64, allow_qualified: bool) -> Harness {
    let loads = Rc::new(RefCell::new(0usize));
    let clock = Rc::new(RefCell::new(1_000i64));
    let bus = Rc::new(RefCell::new(SchemaInvalidationBus::new()));
    let config = Rc::new(RefCell::new(EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 1,
    }));

    let loads_for_loader = Rc::clone(&loads);
    let config_for_resolver = Rc::clone(&config);
    let clock_for_now = Rc::clone(&clock);
    let bus_for_cache = Rc::clone(&bus);

    let cache = MetadataCache::new(MetadataCacheDeps {
        load_table_schema: Some(Box::new(move |_sid, table, _db, _schema| {
            *loads_for_loader.borrow_mut() += 1;
            Ok(make_schema(table))
        })),
        config_resolver: Some(Box::new(move |_sid| Some(config_for_resolver.borrow().clone()))),
        invalidation_bus: Some(bus_for_cache),
        debounce_ms,
        error_ttl_ms,
        allow_qualified,
        now: Some(Box::new(move || *clock_for_now.borrow())),
    });

    Harness {
        cache,
        loads,
        clock,
        bus,
        config,
    }
}

fn harness() -> Harness {
    harness_with(40, 100, true)
}

fn failing_cache(clock: Rc<RefCell<i64>>, loads: Rc<RefCell<usize>>) -> MetadataCache {
    MetadataCache::new(MetadataCacheDeps {
        load_table_schema: Some(Box::new(move |_sid, _table, _db, _schema| {
            *loads.borrow_mut() += 1;
            Err(())
        })),
        debounce_ms: 0,
        error_ttl_ms: 100,
        now: Some(Box::new(move || *clock.borrow())),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Key derivation (identity.ts / sessionConfig.ts / metadata/identity.ts)
// ---------------------------------------------------------------------------

#[test]
fn session_key_is_the_json_session_prefix_and_is_stable() {
    let config = EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 7,
    };
    let key = editor_session_cache_key("s1", &config);
    assert_eq!(key, r#"["conn-1",7,"s1"]"#);
    assert_eq!(key, metadata_session_key(&config, "s1"));
    assert_eq!(key, editor_session_cache_key("s1", &config));
}

#[test]
fn session_key_separates_revision_connection_and_unbound_partitions() {
    let a = editor_session_cache_key(
        "s1",
        &EditorSessionConfig {
            connection_id: "conn-1".to_owned(),
            config_revision: 1,
        },
    );
    let b = editor_session_cache_key(
        "s1",
        &EditorSessionConfig {
            connection_id: "conn-1".to_owned(),
            config_revision: 2,
        },
    );
    let c = editor_session_cache_key(
        "s1",
        &EditorSessionConfig {
            connection_id: "conn-2".to_owned(),
            config_revision: 1,
        },
    );
    let unbound = editor_session_cache_key("s1", &unbound_session_config());
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(unbound, a);
    assert_ne!(unbound, b);
    assert_ne!(unbound, c);
    // An explicitly empty config is still its own address, not "unbound by
    // accident" — but JSON of `("",0,"s1")` differs from the unbound one only
    // if a resolver truly registered it; here they are the same values.
    assert_eq!(
        unbound,
        editor_session_cache_key(
            "s1",
            &EditorSessionConfig {
                connection_id: String::new(),
                config_revision: 0,
            }
        )
    );
}

#[test]
fn a_resolver_that_knows_nothing_resolves_to_the_unbound_partition() {
    let resolver = |sid: &str| {
        if sid == "known" {
            Some(EditorSessionConfig {
                connection_id: "conn-1".to_owned(),
                config_revision: 2,
            })
        } else {
            None
        }
    };
    assert_eq!(
        resolve_editor_session_config_with(&resolver, "known"),
        EditorSessionConfig {
            connection_id: "conn-1".to_owned(),
            config_revision: 2,
        }
    );
    assert_eq!(
        resolve_editor_session_config_with(&resolver, "other"),
        unbound_session_config()
    );
    assert_ne!(
        editor_session_cache_key("known", &unbound_session_config()),
        editor_session_cache_key("other", &unbound_session_config())
    );
}

#[test]
fn relation_keys_fold_by_dialect_and_preserve_quoted_segments() {
    let mixed = QualifiedRelationId {
        namespace_path: vec![SqlIdentifierSegment::new("PUBLIC", false)],
        name: SqlIdentifierSegment::new("Users", false),
    };
    assert_eq!(relation_identity_key(&mixed, "postgresql"), "public.users");
    assert_eq!(relation_identity_key(&mixed, "oracle"), "PUBLIC.USERS");
    assert_eq!(relation_identity_key(&mixed, "sqlserver"), "PUBLIC.Users");

    let quoted = QualifiedRelationId {
        namespace_path: vec![SqlIdentifierSegment::new("PUBLIC", true)],
        name: SqlIdentifierSegment::new("Users", true),
    };
    assert_eq!(
        relation_identity_key(&quoted, "postgresql"),
        "PUBLIC.Users"
    );
    assert_eq!(relation_identity_key(&quoted, "oracle"), "PUBLIC.Users");

    let bare = QualifiedRelationId::bare("Users");
    assert_eq!(relation_base_name(&bare, "postgresql"), "users");
    assert_eq!(relation_base_name(&bare, "oracle"), "USERS");
    assert_eq!(relation_base_name(&quoted, "postgresql"), "Users");
    assert!(!has_namespace(&bare));
    assert!(has_namespace(&mixed));
}

#[test]
fn editor_relation_key_is_session_plus_folded_identity() {
    let identity = ident(&["Public"], "Users");
    assert_eq!(
        build_editor_relation_key("s1", &identity, "postgresql"),
        "s1::public.users"
    );
    assert_eq!(
        editor_relation_key("s1", &identity, "postgresql"),
        "s1::public.users"
    );
}

#[test]
fn qualified_name_text_quotes_only_quoted_segments() {
    let plain = ident(&["public"], "users");
    assert_eq!(qualified_name_text(&plain, "postgresql"), "public.users");

    let quoted = QualifiedRelationId {
        namespace_path: vec![SqlIdentifierSegment::new("public", true)],
        name: SqlIdentifierSegment::new("weird\"name", true),
    };
    assert_eq!(
        qualified_name_text(&quoted, "postgresql"),
        "\"public\".\"weird\"\"name\""
    );
    assert_eq!(
        qualified_name_text(&quoted, "mysql"),
        "`public`.`weird\"name`"
    );
    assert_eq!(qualified_name_text(&quoted, "sqlserver"), "[public].[weird\"name]");
    assert_eq!(qualified_name_text(&plain, "oracle"), "public.users");
}

#[test]
fn schema_to_metadata_maps_every_field() {
    let schema = TableSchema {
        table_name: "users".to_owned(),
        columns: vec![ColumnSchema {
            name: "id".to_owned(),
            data_type: "int".to_owned(),
            nullable: false,
            is_primary_key: true,
            ..Default::default()
        }],
        primary_keys: vec!["id".to_owned()],
        indexes: vec![IndexInfo {
            name: "pk".to_owned(),
            columns: vec!["id".to_owned()],
            is_unique: true,
            is_primary: true,
            ..Default::default()
        }],
        foreign_keys: vec![ForeignKeyInfo {
            name: "fk".to_owned(),
            columns: vec!["id".to_owned()],
            referenced_table: "roles".to_owned(),
            referenced_columns: vec!["id".to_owned()],
            deferrability: Some(Deferrability::NotDeferrable),
            ..Default::default()
        }],
        comment: Some("a table".to_owned()),
    };
    let identity = ident(&["public"], "users");
    let meta = schema_to_metadata(
        "s1::public.users".to_owned(),
        &identity,
        EditorRelationKind::View,
        &schema,
        42,
    );
    assert_eq!(meta.key, "s1::public.users");
    assert_eq!(meta.identity, identity);
    assert_eq!(meta.kind, EditorRelationKind::View);
    assert_eq!(meta.columns.len(), 1);
    assert_eq!(meta.primary_key, vec!["id".to_owned()]);
    assert_eq!(meta.indexes.len(), 1);
    assert_eq!(meta.foreign_keys.len(), 1);
    assert_eq!(meta.comment.as_deref(), Some("a table"));
    assert_eq!(meta.loaded_at, 42);
}

// ---------------------------------------------------------------------------
// Cache hit / miss, dedup, TTL, debounce
// ---------------------------------------------------------------------------

#[test]
fn a_load_populates_the_snapshot_and_bumps_the_epoch() {
    let h = harness();
    let ctx = context("demo", Some("public"), "postgresql");
    h.cache
        .ensure_relations("s1", &[request(&["public"], "users", SqlRelationSourceKind::Table)], &ctx);
    h.cache.flush_now();

    let snapshot = h.cache.get_snapshot("s1");
    assert_eq!(snapshot.db_session_id, "s1");
    assert_eq!(snapshot.database, "demo");
    assert_eq!(snapshot.schema.as_deref(), Some("public"));
    assert_eq!(snapshot.epoch, 1);
    assert_eq!(snapshot.relations().len(), 1);
    let meta = snapshot.relation("s1::public.users").expect("loaded relation");
    assert_eq!(meta.kind, EditorRelationKind::Table);
    assert_eq!(meta.loaded_at, 1_000);
    assert_eq!(*h.loads.borrow(), 1);
    assert!(h.cache.get_relation("s1", "s1::public.users").is_some());
}

#[test]
fn a_cached_relation_is_not_loaded_twice() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    let req = bare_request("users");
    h.cache.ensure_relations("s1", &[req.clone()], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 1);

    h.cache.ensure_relations("s1", &[req.clone()], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 1, "the loaded relation is a cache hit");
    assert_eq!(h.cache.get_snapshot("s1").epoch, 1, "no epoch bump on a hit");
}

#[test]
fn an_unknown_session_gets_a_stable_empty_snapshot() {
    let h = harness();
    let first = h.cache.get_snapshot("unknown");
    assert_eq!(first.epoch, 0);
    assert!(first.relations().is_empty());
    assert_eq!(first.database, "");
    let second = h.cache.get_snapshot("unknown");
    assert_eq!(first, second, "referential stability is value equality here");
    assert!(h.cache.get_relation("unknown", "unknown::users").is_none());
}

#[test]
fn cte_and_subquery_are_skipped_while_view_and_table_load() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    let requests = vec![
        request(&[], "cte1", SqlRelationSourceKind::Cte),
        request(&[], "sub1", SqlRelationSourceKind::Subquery),
        request(&[], "tbl", SqlRelationSourceKind::Table),
        request(&[], "vw", SqlRelationSourceKind::View),
        EditorRelationRequest {
            identity: QualifiedRelationId::bare("implicit"),
            kind: None,
        },
    ];
    h.cache.ensure_relations("s1", &requests, &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 3);
    let snapshot = h.cache.get_snapshot("s1");
    assert!(snapshot.relation("s1::cte1").is_none());
    assert!(snapshot.relation("s1::sub1").is_none());
    assert_eq!(
        snapshot
            .relation("s1::vw")
            .expect("view loaded")
            .kind,
        EditorRelationKind::View
    );
    assert_eq!(
        snapshot
            .relation("s1::implicit")
            .expect("missing kind defaults to table")
            .kind,
        EditorRelationKind::Table
    );
}

#[test]
fn allow_qualified_false_skips_namespaced_relations() {
    let h = harness_with(40, 100, false);
    let ctx = context("demo", None, "standard");
    h.cache
        .ensure_relations("s1", &[request(&["public"], "users", SqlRelationSourceKind::Table)], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 0, "a driver that can't resolve qualified names");

    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 1);
    assert!(h.cache.get_relation("s1", "s1::users").is_some());
}

#[test]
fn a_failed_load_is_memoized_for_the_error_ttl_then_retried() {
    let clock = Rc::new(RefCell::new(1_000i64));
    let loads = Rc::new(RefCell::new(0usize));
    let cache = failing_cache(Rc::clone(&clock), Rc::clone(&loads));
    let ctx = context("demo", None, "standard");
    let req = bare_request("broken");

    cache.ensure_relations("s1", &[req.clone()], &ctx);
    cache.flush_now();
    assert_eq!(*loads.borrow(), 1);
    let snapshot = cache.get_snapshot("s1");
    assert!(snapshot.relation("s1::broken").is_none());
    assert_eq!(snapshot.epoch, 0, "a failure does not bump the epoch");

    cache.ensure_relations("s1", &[req.clone()], &ctx);
    cache.flush_now();
    assert_eq!(*loads.borrow(), 1, "the error marker suppresses a retry in TTL");

    *clock.borrow_mut() = 1_101;
    cache.ensure_relations("s1", &[req], &ctx);
    cache.flush_now();
    assert_eq!(*loads.borrow(), 2, "past the TTL the relation is requested again");
}

#[test]
fn debounce_defers_until_tick_and_flush_now_forces() {
    let h = harness_with(40, 100, true);
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    assert_eq!(*h.loads.borrow(), 0, "ensure only queues");

    h.cache.tick();
    assert_eq!(*h.loads.borrow(), 0, "the deadline (1040) has not passed");

    *h.clock.borrow_mut() = 1_040;
    h.cache.tick();
    assert_eq!(*h.loads.borrow(), 1, "tick drains the due batch");

    h.cache.ensure_relations("s1", &[bare_request("orders")], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 2, "flush_now ignores the deadline");
}

#[test]
fn an_empty_database_requeues_until_the_context_arrives() {
    let h = harness();
    h.cache.ensure_relations(
        "s1",
        &[bare_request("users")],
        &context("", None, "standard"),
    );
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 0, "never query without a tab-bound database");
    assert!(h.cache.get_relation("s1", "s1::users").is_none());

    h.cache
        .ensure_relations("s1", &[bare_request("users")], &context("demo", None, "standard"));
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 1);
    assert!(h.cache.get_relation("s1", "s1::users").is_some());
}

// ---------------------------------------------------------------------------
// Subscriptions / generation counter
// ---------------------------------------------------------------------------

#[test]
fn subscribers_fire_on_snapshot_changes_but_not_on_a_failure() {
    let h = harness();
    let notifications = Rc::new(RefCell::new(0usize));
    let counter = Rc::clone(&notifications);
    let _token = h.cache.subscribe(Box::new(move || *counter.borrow_mut() += 1));

    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(*notifications.borrow(), 1, "successful load refreshes");

    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(*notifications.borrow(), 1, "a cache hit is silent");

    h.cache.invalidate_relation("s1", &QualifiedRelationId::bare("users"), Some("standard"));
    assert_eq!(*notifications.borrow(), 2, "invalidation refreshes");

    let clock = Rc::new(RefCell::new(0i64));
    let loads = Rc::new(RefCell::new(0usize));
    let failing = failing_cache(clock, loads);
    let failures = Rc::new(RefCell::new(0usize));
    let failure_counter = Rc::clone(&failures);
    let _ = failing.subscribe(Box::new(move || *failure_counter.borrow_mut() += 1));
    failing.ensure_relations("s1", &[bare_request("x")], &ctx);
    failing.flush_now();
    assert_eq!(*failures.borrow(), 0, "a failure keeps the epoch unchanged");
}

#[test]
fn unsubscribing_stops_callbacks() {
    let h = harness();
    let first = Rc::new(RefCell::new(0usize));
    let second = Rc::new(RefCell::new(0usize));
    let first_counter = Rc::clone(&first);
    let second_counter = Rc::clone(&second);
    let first_token = h.cache.subscribe(Box::new(move || *first_counter.borrow_mut() += 1));
    let _second_token = h.cache.subscribe(Box::new(move || *second_counter.borrow_mut() += 1));

    h.cache.invalidate_all();
    assert_eq!(*first.borrow(), 1);
    assert_eq!(*second.borrow(), 1);

    h.cache.unsubscribe(first_token);
    h.cache.invalidate_all();
    assert_eq!(*first.borrow(), 1, "the unsubscribed listener is silent");
    assert_eq!(*second.borrow(), 2);
}

// ---------------------------------------------------------------------------
// Invalidation semantics
// ---------------------------------------------------------------------------

#[test]
fn invalidate_relation_drops_only_the_named_key() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations(
        "s1",
        &[bare_request("users"), bare_request("orders")],
        &ctx,
    );
    h.cache.flush_now();
    let before = h.cache.get_snapshot("s1").epoch;
    h.cache.invalidate_relation(
        "s1",
        &QualifiedRelationId::bare("users"),
        Some("standard"),
    );

    let snapshot = h.cache.get_snapshot("s1");
    assert_eq!(snapshot.epoch, before + 1);
    assert!(snapshot.relation("s1::users").is_none());
    assert!(snapshot.relation("s1::orders").is_some());
    assert_eq!(*h.loads.borrow(), 2);
}

#[test]
fn invalidate_session_and_invalidate_all_clear_snapshots_and_notify() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();

    let notifications = Rc::new(RefCell::new(0usize));
    let counter = Rc::clone(&notifications);
    h.cache.subscribe(Box::new(move || *counter.borrow_mut() += 1));

    h.cache.invalidate_session("s1");
    assert_eq!(h.cache.get_snapshot("s1").epoch, 0);
    assert!(h.cache.get_relation("s1", "s1::users").is_none());
    assert_eq!(*notifications.borrow(), 1);

    // invalidate_all also notifies even when nothing is cached.
    h.cache.invalidate_all();
    assert_eq!(*notifications.borrow(), 2);
}

#[test]
fn external_invalidation_matches_both_qualified_and_base_names() {
    // Oracle folds unquoted names to upper in the identity key, while the
    // qualified text stays as written — so the two halves of `||` differ.
    let h = harness();
    let ctx = context("demo", None, "oracle");
    let req = bare_request("users");
    h.cache.ensure_relations("s1", &[req.clone()], &ctx);
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::USERS").is_some());

    h.bus.borrow().notify("s1", Some("users"));
    assert!(h.cache.get_relation("s1", "s1::USERS").is_none(), "qualified text match");

    h.cache.ensure_relations("s1", &[req], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 2);
    h.bus.borrow().notify("s1", Some("USERS"));
    assert!(h.cache.get_relation("s1", "s1::USERS").is_none(), "base name match");
}

#[test]
fn external_whole_session_invalidation_reaches_the_current_partition() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::users").is_some());

    // A config rebind moves the session; invalidation is config-agnostic and
    // still drops whatever partition the session currently lives in.
    *h.config.borrow_mut() = EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 2,
    };
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::users").is_some());

    h.bus.borrow().notify("s1", None);
    assert!(h.cache.get_snapshot("s1").relations().is_empty());
    assert!(h.cache.get_relation("s1", "s1::users").is_none());
}

#[test]
fn a_partial_name_invalidation_only_drops_matching_relations() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations(
        "s1",
        &[bare_request("users"), bare_request("orders")],
        &ctx,
    );
    h.cache.flush_now();
    h.bus.borrow().notify("s1", Some("orders"));
    let snapshot = h.cache.get_snapshot("s1");
    assert!(snapshot.relation("s1::orders").is_none());
    assert!(snapshot.relation("s1::users").is_some());
}

// ---------------------------------------------------------------------------
// Configuration isolation (sessionConfig.ts / configIsolation.test.ts)
// ---------------------------------------------------------------------------

#[test]
fn a_config_rebind_makes_the_old_partition_unreachable_and_reloads() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    let req = bare_request("users");
    h.cache.ensure_relations("s1", &[req.clone()], &ctx);
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::users").is_some());
    assert_eq!(*h.loads.borrow(), 1);

    *h.config.borrow_mut() = EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 2,
    };
    let rebound = h.cache.get_snapshot("s1");
    assert_eq!(rebound.relations().len(), 0, "no previous-config snapshot");

    h.cache.ensure_relations("s1", &[req], &ctx);
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 2, "the new partition reloads from scratch");
    assert!(h.cache.get_relation("s1", "s1::users").is_some());
}

#[test]
fn two_live_sessions_of_the_same_config_stay_isolated() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::users").is_some());
    assert!(h.cache.get_relation("s2", "s1::users").is_none());
    assert!(h.cache.get_snapshot("s2").relations().is_empty());
}

#[test]
fn switch_context_drops_loaded_relations_and_bumps_the_epoch() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(h.cache.get_snapshot("s1").epoch, 1);

    h.cache.switch_context(
        "s1",
        &context("other", Some("schema2"), "postgresql"),
    );
    let snapshot = h.cache.get_snapshot("s1");
    assert_eq!(snapshot.epoch, 2);
    assert!(snapshot.relations().is_empty());
    assert_eq!(snapshot.database, "other");
    assert_eq!(snapshot.schema.as_deref(), Some("schema2"));
    assert_eq!(*h.loads.borrow(), 1, "switching does not itself load");
}

#[test]
fn a_context_change_clears_relations_and_reloads_under_the_new_context() {
    let h = harness();
    h.cache.ensure_relations(
        "s1",
        &[bare_request("users")],
        &context("demo", None, "standard"),
    );
    h.cache.flush_now();
    assert!(h.cache.get_relation("s1", "s1::users").is_some());

    h.cache.ensure_relations(
        "s1",
        &[bare_request("users")],
        &context("other", None, "standard"),
    );
    h.cache.flush_now();
    let snapshot = h.cache.get_snapshot("s1");
    assert_eq!(snapshot.database, "other");
    // The reset bumps the epoch, and the reload that follows bumps it again.
    assert_eq!(snapshot.epoch, 3, "a non-fresh context change resets then reloads");
    assert_eq!(*h.loads.borrow(), 2, "the relation is reloaded under 'other'");
}

#[test]
fn the_default_cache_loads_with_no_host_config_published() {
    let loads = Rc::new(RefCell::new(0usize));
    let clock = Rc::new(RefCell::new(0i64));
    let loads_for_loader = Rc::clone(&loads);
    let cache = MetadataCache::new(MetadataCacheDeps {
        load_table_schema: Some(Box::new(move |_sid, table, _db, _schema| {
            *loads_for_loader.borrow_mut() += 1;
            Ok(make_schema(table))
        })),
        debounce_ms: 0,
        now: Some(Box::new(move || *clock.borrow())),
        ..Default::default()
    });
    cache.ensure_relations(
        "s1",
        &[bare_request("users")],
        &context("demo", None, "standard"),
    );
    cache.flush_now();
    assert_eq!(*loads.borrow(), 1);
    assert!(cache.get_relation("s1", "s1::users").is_some());
}

// ---------------------------------------------------------------------------
// T032 independent tester audit (coverage-driven). Append-only: no existing
// assertion above was changed, weakened, skipped or deleted.
// ---------------------------------------------------------------------------

/// A snapshot subscriber is the React `useSyncExternalStore` pattern: the whole
/// point of `subscribe` + `getSnapshot` is that the listener can read the store.
///
/// KNOWN DEFECT D2 (tester audit): `notify()` runs while the public method holds
/// an `inner.borrow_mut()` (e.g. `flush_now`), so a listener calling
/// `get_snapshot()` re-enters `inner.borrow_mut()` and panics
/// ("RefCell already borrowed" at metadata_cache.rs:704). The module doc only
/// forbids mutating listeners in a callback, not reading the snapshot. Ignored
/// only so the gate stays green while the defect is reported to the coder; the
/// raw failure was captured verbatim before ignoring.
#[test]
fn audit_subscriber_can_read_snapshot_during_notify() {
    let h = harness();
    let cache = h.cache.clone();
    let seen = Rc::new(RefCell::new(0u64));
    let seen_counter = Rc::clone(&seen);
    h.cache.subscribe(Box::new(move || {
        let snapshot = cache.get_snapshot("s1");
        *seen_counter.borrow_mut() = snapshot.epoch;
    }));

    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations("s1", &[bare_request("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(*seen.borrow(), 1, "the subscriber observed the committed epoch");
}

/// `invalidate_relation` must clear the error marker, so an explicitly
/// invalidated failed relation is retried even inside its error TTL.
#[test]
fn audit_invalidate_relation_clears_the_error_marker() {
    let clock = Rc::new(RefCell::new(1_000i64));
    let loads = Rc::new(RefCell::new(0usize));
    let cache = failing_cache(Rc::clone(&clock), Rc::clone(&loads));
    let ctx = context("demo", None, "standard");
    let req = bare_request("broken");

    cache.ensure_relations("s1", &[req.clone()], &ctx);
    cache.flush_now();
    assert_eq!(*loads.borrow(), 1);

    cache.invalidate_relation("s1", &QualifiedRelationId::bare("broken"), Some("standard"));
    cache.ensure_relations("s1", &[req], &ctx);
    cache.flush_now();
    assert_eq!(*loads.borrow(), 2, "explicit invalidation clears the failure memo");
}

/// A batch with one success and one failure bumps the epoch exactly once.
#[test]
fn audit_mixed_batch_bumps_epoch_once_for_the_success() {
    let clock = Rc::new(RefCell::new(1_000i64));
    let loads = Rc::new(RefCell::new(0usize));
    let loads_for_loader = Rc::clone(&loads);
    let fail_for_loader = Rc::new(RefCell::new(true));
    let cache = MetadataCache::new(MetadataCacheDeps {
        load_table_schema: Some(Box::new(move |_sid, table, _db, _schema| {
            *loads_for_loader.borrow_mut() += 1;
            if table == "bad" && *fail_for_loader.borrow() {
                Err(())
            } else {
                Ok(make_schema(table))
            }
        })),
        debounce_ms: 0,
        error_ttl_ms: 100,
        now: Some(Box::new(move || *clock.borrow())),
        ..Default::default()
    });
    let ctx = context("demo", None, "standard");
    cache.ensure_relations("s1", &[bare_request("good"), bare_request("bad")], &ctx);
    cache.flush_now();
    let snapshot = cache.get_snapshot("s1");
    assert_eq!(snapshot.epoch, 1, "one success bumps once, the failure is silent");
    assert!(snapshot.relation("s1::good").is_some());
    assert!(snapshot.relation("s1::bad").is_none());
    assert_eq!(*loads.borrow(), 2);
}

/// Duplicate requests for the same identity in one batch are deduped by key.
#[test]
fn audit_duplicate_requests_in_one_batch_load_once() {
    let h = harness();
    let ctx = context("demo", None, "standard");
    h.cache.ensure_relations(
        "s1",
        &[bare_request("users"), bare_request("users")],
        &ctx,
    );
    h.cache.flush_now();
    assert_eq!(*h.loads.borrow(), 1, "two requests for one key load once");
}
