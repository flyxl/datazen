//! Independent fresh-tester re-audit of the deferred-notify fix (D1/D2).
//!
//! Every probe installs a `subscribe` listener that calls `get_snapshot()`
//! *from inside* the notify callback — the exact React `useSyncExternalStore`
//! pattern that used to panic with "RefCell already borrowed". Each probe
//! drives a different public mutation path and asserts the callback observed
//! the committed (post-mutation) epoch. Test-only: no product source is touched.

use std::cell::RefCell;
use std::rc::Rc;

use crate::relation_metadata::*;

fn bare(name: &str) -> EditorRelationRequest {
    EditorRelationRequest {
        identity: QualifiedRelationId::bare(name),
        kind: Some(SqlRelationSourceKind::Table),
    }
}

fn ctx(database: &str, dialect: &str) -> EditorMetadataContext {
    EditorMetadataContext {
        database: database.to_owned(),
        schema: None,
        dialect_id: dialect.to_owned(),
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

struct H {
    cache: MetadataCache,
    clock: Rc<RefCell<i64>>,
    config: Rc<RefCell<EditorSessionConfig>>,
    bus: Rc<RefCell<SchemaInvalidationBus>>,
}

fn h_with(debounce_ms: i64) -> H {
    let clock = Rc::new(RefCell::new(1_000i64));
    let config = Rc::new(RefCell::new(EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 1,
    }));
    let bus = Rc::new(RefCell::new(SchemaInvalidationBus::new()));

    let clock_for_now = Rc::clone(&clock);
    let config_for_resolver = Rc::clone(&config);
    let bus_for_cache = Rc::clone(&bus);
    let cache = MetadataCache::new(MetadataCacheDeps {
        load_table_schema: Some(Box::new(move |_sid, table, _db, _schema| {
            Ok(make_schema(table))
        })),
        config_resolver: Some(Box::new(move |_sid| Some(config_for_resolver.borrow().clone()))),
        invalidation_bus: Some(bus_for_cache),
        debounce_ms,
        error_ttl_ms: 100,
        allow_qualified: true,
        now: Some(Box::new(move || *clock_for_now.borrow())),
    });
    H {
        cache,
        clock,
        config,
        bus,
    }
}

/// Records the epoch a subscriber saw each time it was notified.
#[derive(Clone)]
struct Epochs(Rc<RefCell<Vec<u64>>>);

impl Epochs {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Vec::new())))
    }

    fn subscriber(&self, cache: &MetadataCache, sid: &'static str) -> Box<dyn Fn()> {
        let cache = cache.clone();
        let epochs = Rc::clone(&self.0);
        Box::new(move || {
            // This is the call that panicked under D2 (re-entrant borrow).
            let snapshot = cache.get_snapshot(sid);
            epochs.borrow_mut().push(snapshot.epoch);
        })
    }

    fn seen(&self) -> Vec<u64> {
        self.0.borrow().clone()
    }
}

#[test]
fn retest_subscriber_reads_snapshot_on_tick_flush_due_at() {
    let h = h_with(40);
    let ctx = ctx("demo", "standard");
    let epochs = Epochs::new();
    h.cache
        .subscribe(epochs.subscriber(&h.cache, "s1"));

    h.cache.ensure_relations("s1", &[bare("users")], &ctx);
    assert!(epochs.seen().is_empty(), "ensure only queues");

    *h.clock.borrow_mut() = 1_040;
    h.cache.tick();
    assert_eq!(epochs.seen(), vec![1], "tick: callback saw post-flush epoch");
}

#[test]
fn retest_subscriber_reads_snapshot_on_flush_now() {
    let h = h_with(40);
    let ctx = ctx("demo", "standard");
    let epochs = Epochs::new();
    h.cache
        .subscribe(epochs.subscriber(&h.cache, "s1"));

    h.cache.ensure_relations("s1", &[bare("users")], &ctx);
    h.cache.flush_now();
    assert_eq!(epochs.seen(), vec![1], "flush_now: callback saw post-flush epoch");
}

#[test]
fn retest_subscriber_reads_snapshot_on_invalidate_paths() {
    let h = h_with(0);
    let ctx = ctx("demo", "standard");
    let epochs = Epochs::new();
    h.cache
        .subscribe(epochs.subscriber(&h.cache, "s1"));

    h.cache.ensure_relations("s1", &[bare("users")], &ctx);
    h.cache.flush_now(); // epoch 1 -> one notification

    h.cache.invalidate_relation("s1", &QualifiedRelationId::bare("users"), Some("standard"));
    assert_eq!(epochs.seen(), vec![1, 2], "invalidate_relation: post epoch 2");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);

    // invalidate_session drops the partition: the post-mutation state is a
    // fresh empty snapshot (epoch 0), which is exactly what the callback saw.
    h.cache.invalidate_session("s1");
    assert_eq!(epochs.seen().len(), 3, "invalidate_session notified once");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);

    h.cache.invalidate_all();
    assert_eq!(
        epochs.seen().len(),
        4,
        "invalidate_all: no lost notifications across four mutations"
    );
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);
}

#[test]
fn retest_subscriber_reads_snapshot_on_config_rebind_and_switch_context() {
    let h = h_with(0);
    let base = ctx("demo", "standard");
    let epochs = Epochs::new();
    h.cache
        .subscribe(epochs.subscriber(&h.cache, "s1"));

    h.cache.ensure_relations("s1", &[bare("users")], &base);
    h.cache.flush_now(); // one notification

    // Config rebind: the old partition is dropped (parity with the TS source:
    // the drop itself is silent); the reload of the new partition notifies.
    *h.config.borrow_mut() = EditorSessionConfig {
        connection_id: "conn-1".to_owned(),
        config_revision: 2,
    };
    h.cache.ensure_relations("s1", &[bare("users")], &base);
    h.cache.flush_now();
    assert_eq!(epochs.seen().len(), 2, "rebind + reload notified once");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);

    // switch_context explicitly refreshes and notifies subscribers.
    h.cache.switch_context("s1", &ctx("other", "postgresql"));
    assert_eq!(epochs.seen().len(), 3, "switch_context notified once");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);
    assert_eq!(h.cache.get_snapshot("s1").database, "other");
}

#[test]
fn retest_subscriber_reads_snapshot_on_external_invalidation_bus() {
    let h = h_with(0);
    let ctx = ctx("demo", "standard");
    let epochs = Epochs::new();
    h.cache
        .subscribe(epochs.subscriber(&h.cache, "s1"));

    h.cache.ensure_relations("s1", &[bare("users")], &ctx);
    h.cache.flush_now(); // epoch 1

    // Partial (table-name) invalidation via the external bus.
    h.bus.borrow().notify("s1", Some("users"));
    assert_eq!(epochs.seen().len(), 2, "bus partial notified once");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);

    // Whole-session (None) invalidation drops the partition -> fresh empty
    // snapshot (epoch 0); the callback must still see that post-mutation state.
    h.bus.borrow().notify("s1", None);
    assert_eq!(epochs.seen().len(), 3, "bus whole-session notified once");
    assert_eq!(*epochs.seen().last().unwrap(), h.cache.get_snapshot("s1").epoch);
}

#[test]
fn retest_unsubscribing_mid_delivery_is_copy_then_call_and_does_not_panic() {
    let h = h_with(0);
    // Listener A unsubscribes listener B from *inside* a notify callback.
    let b_token: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    let b_token_for_a = Rc::clone(&b_token);
    let cache_for_a = h.cache.clone();
    h.cache.subscribe(Box::new(move || {
        let token = {
            let mut slot = b_token_for_a.borrow_mut();
            slot.take()
        };
        if let Some(token) = token {
            // Must not panic: no inner.borrow_mut() is held during delivery.
            cache_for_a.unsubscribe(token);
        }
    }));

    let b_calls = Rc::new(RefCell::new(0usize));
    let b_calls_local = Rc::clone(&b_calls);
    let b_id = h.cache.subscribe(Box::new(move || *b_calls_local.borrow_mut() += 1));
    *b_token.borrow_mut() = Some(b_id);

    h.cache.invalidate_all();
    assert_eq!(
        *b_calls.borrow(),
        1,
        "copy-then-call: B still fires in the batch that removed it"
    );

    h.cache.invalidate_all();
    assert_eq!(
        *b_calls.borrow(),
        1,
        "B is silent in the next batch"
    );

    // Independently confirm an observing subscriber never panicked and counted both.
    let observer = Epochs::new();
    h.cache.subscribe(observer.subscriber(&h.cache, "s1"));
    h.cache.invalidate_all();
    assert_eq!(observer.seen().len(), 1);
}
