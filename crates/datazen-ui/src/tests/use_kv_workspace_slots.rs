//! T618 audit for the pure KV workspace slot resolver: both gates, the shared
//! panel props, atom identity, the landing home, the overview jump mapping and
//! the state relay lifecycle. No driver component or IPC is faked.

use std::collections::{BTreeMap, BTreeSet};

use crate::views::use_kv_workspace_slots as slots;
use crate::views::use_kv_workspace_slots::{
    commit_slot_patch, has_slot_capability, has_slot_component, panel_state_key,
    plan_overview_jump, prune_slot_states, resolve_kv_database_index, resolve_slots,
    shared_panel_props, KvConnectionContext, KvDriverMeta, KvJumpAction, KvJumpKind, KvJumpTarget,
    KvPanelRef, KvPendingAction, KvSlotName, KvSlotPatch, KvSlotRegistry, KvSlotSnapshot,
    KvSlotsInput,
};

fn meta() -> KvDriverMeta {
    KvDriverMeta {
        database_field_type: "index".to_owned(),
        context_bar: true,
        status_bar: true,
        key_props_sidebar: true,
        home: true,
    }
}

fn input() -> KvSlotsInput {
    KvSlotsInput {
        active_panel: Some(KvPanelRef {
            id: "panel-7".to_owned(),
            database_type: Some("kvfull".to_owned()),
            database: Some("db7".to_owned()),
        }),
        is_kv_panel: true,
        database_type: Some("kvfull".to_owned()),
        database: Some("db7".to_owned()),
        db_session_id: "sess-1".to_owned(),
        connection_id: "cfg-1".to_owned(),
        connection_name: "KV Local".to_owned(),
        connection_context: Some(KvConnectionContext {
            connection_id: "cfg-1".to_owned(),
            db_session_id: "sess-1".to_owned(),
            connection_name: "KV Local".to_owned(),
            database_type: "kvfull".to_owned(),
        }),
        initial_database: Some("db3".to_owned()),
        has_slot_action: true,
        has_select_kv_db: true,
        meta: Some(meta()),
        registry: KvSlotRegistry {
            contributed: KvSlotName::ALL.iter().map(|slot| ("kvfull".to_owned(), *slot)).collect(),
        },
    }
}

// ── dbIndex / capability / component gates ────────────────────────────────

#[test]
fn db_index_resolves_only_index_addressed_trailing_digits() {
    let indexed = Some(meta());
    let named = Some(KvDriverMeta { database_field_type: "name".to_owned(), ..meta() });
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("db7")), Some(7));
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("db15")), Some(15));
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("db007")), Some(7));
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("db7-")), None);
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("keyspace")), None);
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("")), None);
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), None), None);
    assert_eq!(resolve_kv_database_index(named.as_ref(), Some("db7")), None);
    assert_eq!(resolve_kv_database_index(None, Some("db7")), None);
    assert_eq!(resolve_kv_database_index(indexed.as_ref(), Some("db99999999999999999999")), None);
}

#[test]
fn capability_reader_maps_connection_home_to_the_home_flag() {
    let full = Some(meta());
    assert!(has_slot_capability(full.as_ref(), KvSlotName::ContextBar));
    assert!(has_slot_capability(full.as_ref(), KvSlotName::StatusBar));
    assert!(has_slot_capability(full.as_ref(), KvSlotName::KeyPropsSidebar));
    assert!(has_slot_capability(full.as_ref(), KvSlotName::ConnectionHome));
    assert_eq!(KvSlotName::ConnectionHome.capability_key(), "home");
    assert_eq!(KvSlotName::ContextBar.capability_key(), "contextBar");

    let partial = Some(KvDriverMeta { home: false, ..meta() });
    assert!(has_slot_capability(partial.as_ref(), KvSlotName::ContextBar));
    assert!(!has_slot_capability(partial.as_ref(), KvSlotName::ConnectionHome));
    assert!(!has_slot_capability(None, KvSlotName::ContextBar));
}

#[test]
fn a_slot_component_needs_capability_and_contribution() {
    let registry = input().registry;
    assert!(has_slot_component(
        Some(&meta()), &registry, Some("kvfull"), KvSlotName::ContextBar
    ));
    let no_cap = KvDriverMeta { context_bar: false, ..meta() };
    assert!(!has_slot_component(Some(&no_cap), &registry, Some("kvfull"), KvSlotName::ContextBar));
    let no_contrib = KvSlotRegistry::default();
    assert!(!has_slot_component(Some(&meta()), &no_contrib, Some("kvfull"), KvSlotName::ContextBar));
    assert!(!has_slot_component(Some(&meta()), &registry, None, KvSlotName::ContextBar));
    assert!(!has_slot_component(Some(&meta()), &registry, Some("other"), KvSlotName::ContextBar));
}

// ── panel identity / shared props ─────────────────────────────────────────

#[test]
fn panel_state_key_requires_a_kv_identified_panel() {
    assert_eq!(panel_state_key(&input()), Some("panel-7".to_owned()));
    let mut not_kv = input();
    not_kv.is_kv_panel = false;
    assert_eq!(panel_state_key(&not_kv), None);
    let mut no_id = input();
    no_id.active_panel = Some(KvPanelRef { id: String::new(), ..no_id.active_panel.expect("panel") });
    assert_eq!(panel_state_key(&no_id), None);
    let mut no_panel = input();
    no_panel.active_panel = None;
    assert_eq!(panel_state_key(&no_panel), None);
}

#[test]
fn shared_props_require_an_atom_and_a_database_type() {
    let props = shared_panel_props(&input(), Some("panel-7")).expect("shared props");
    assert_eq!(props.connection_id, "cfg-1");
    assert_eq!(props.db_session_id, "sess-1");
    assert_eq!(props.connection_name, "KV Local");
    assert_eq!(props.database_type, "kvfull");
    assert_eq!(props.database.as_deref(), Some("db7"));
    assert_eq!(props.db_index, Some(7));
    assert_eq!(props.state_key, "panel-7");
    assert!(shared_panel_props(&input(), None).is_none());
    let mut no_type = input();
    no_type.database_type = None;
    assert!(shared_panel_props(&no_type, Some("panel-7")).is_none());
}

#[test]
fn in_panel_context_follows_panel_kv_flag_and_props() {
    let props = shared_panel_props(&input(), Some("panel-7"));
    assert!(slots::in_panel_context(&input(), props.as_ref()));
    let mut not_kv = input();
    not_kv.is_kv_panel = false;
    assert!(!slots::in_panel_context(&not_kv, props.as_ref()));
    assert!(!slots::in_panel_context(&input(), None));
}

// ── full resolution ───────────────────────────────────────────────────────

#[test]
fn resolve_slots_binds_every_in_panel_slot_to_the_same_props() {
    let result = resolve_slots(&input());
    assert_eq!(result.panel_state_key.as_deref(), Some("panel-7"));
    assert!(result.connection_home.is_none(), "a panel suppresses the landing home");
    let bar = result.context_bar.expect("context bar");
    let status = result.status_bar.expect("status bar");
    let sidebar = result.key_props_sidebar.expect("key props sidebar");
    assert_eq!(bar.slot, KvSlotName::ContextBar);
    assert_eq!(status.slot, KvSlotName::StatusBar);
    assert_eq!(sidebar.slot, KvSlotName::KeyPropsSidebar);
    assert_eq!(bar.props, status.props);
    assert_eq!(status.props, sidebar.props);
    assert!(bar.has_request, "only the context bar carries the action channel");
    assert!(!status.has_request && !sidebar.has_request);
}

#[test]
fn resolve_slots_gates_each_slot_independently() {
    // Capability without contribution — the atom still exists, nothing renders.
    let capable_only = KvSlotsInput { registry: KvSlotRegistry::default(), ..input() };
    let result = resolve_slots(&capable_only);
    assert_eq!(result.panel_state_key.as_deref(), Some("panel-7"));
    assert!(result.context_bar.is_none() && result.status_bar.is_none() && result.key_props_sidebar.is_none());

    // Contribution without capability — same status quo.
    let no_cap = KvSlotsInput {
        meta: Some(KvDriverMeta { context_bar: false, status_bar: false, key_props_sidebar: false, ..meta() }),
        ..input()
    };
    let result = resolve_slots(&no_cap);
    assert!(result.context_bar.is_none() && result.status_bar.is_none() && result.key_props_sidebar.is_none());

    // Not a KV panel: no atom, no bindings.
    let not_kv = KvSlotsInput { is_kv_panel: false, ..input() };
    let result = resolve_slots(&not_kv);
    assert_eq!(result, crate::views::use_kv_workspace_slots::KvWorkspaceSlots::default());

    // A panel whose type is not resolved yet publishes the atom but no bindings.
    let no_type = KvSlotsInput { database_type: None, ..input() };
    let result = resolve_slots(&no_type);
    assert_eq!(result.panel_state_key.as_deref(), Some("panel-7"));
    assert!(result.context_bar.is_none() && result.status_bar.is_none() && result.key_props_sidebar.is_none());

    // A panel without an id must not mint an atom under a shared key.
    let no_id = KvSlotsInput {
        active_panel: Some(KvPanelRef { id: String::new(), ..input().active_panel.expect("panel") }),
        ..input()
    };
    let result = resolve_slots(&no_id);
    assert!(result.panel_state_key.is_none() && result.context_bar.is_none());
}

#[test]
fn context_bar_request_presence_follows_the_host_dispatcher() {
    let result = resolve_slots(&KvSlotsInput { has_slot_action: false, ..input() });
    let bar = result.context_bar.expect("context bar");
    assert!(!bar.has_request);
    assert!(result.status_bar.expect("status bar").props == bar.props);
}

#[test]
fn home_slot_only_while_no_panel_and_capable_and_contributed() {
    let mut landing = input();
    landing.active_panel = None;
    let result = resolve_slots(&landing);
    let home = result.connection_home.expect("home slot");
    assert_eq!(home.connection_id, "cfg-1");
    assert_eq!(home.db_session_id, "sess-1");
    assert_eq!(home.connection_name, "KV Local");
    assert_eq!(home.database_type, "kvfull");
    assert_eq!(home.initial_database.as_deref(), Some("db3"));
    assert!(home.has_open_target);
    assert!(result.panel_state_key.is_none() && result.context_bar.is_none());

    let no_jump = KvSlotsInput { has_select_kv_db: false, ..landing.clone() };
    assert!(!resolve_slots(&no_jump).connection_home.expect("home").has_open_target);
    let no_context = KvSlotsInput { connection_context: None, ..landing.clone() };
    assert!(resolve_slots(&no_context).connection_home.is_none());
    let no_cap = KvSlotsInput {
        meta: Some(KvDriverMeta { home: false, ..meta() }),
        ..landing.clone()
    };
    assert!(resolve_slots(&no_cap).connection_home.is_none());
    let no_component = KvSlotsInput {
        registry: KvSlotRegistry {
            contributed: vec![("kvfull".to_owned(), KvSlotName::ContextBar)],
        },
        ..landing
    };
    assert!(resolve_slots(&no_component).connection_home.is_none());
}

// ── overview jump ─────────────────────────────────────────────────────────

fn jump(kind: KvJumpKind, index: Option<u64>) -> KvJumpTarget {
    KvJumpTarget { kind, db_index: index, key: Some("session:1".to_owned()), section: None }
}

#[test]
fn overview_jump_maps_every_target_kind() {
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::Database, Some(4)), Some("db3")),
        KvJumpAction::Open { database: "db4".to_owned(), pending: None }
    );
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::Database, None), Some("db3")),
        KvJumpAction::Open { database: "db0".to_owned(), pending: None }
    );
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::NewKey, Some(2)), None),
        KvJumpAction::Open {
            database: "db2".to_owned(),
            pending: Some(KvPendingAction { open_new_key: true, ..KvPendingAction::default() }),
        }
    );
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::Key, Some(5)), None),
        KvJumpAction::Open {
            database: "db5".to_owned(),
            pending: Some(KvPendingAction {
                select_key: Some("session:1".to_owned()),
                key_db_index: Some(5),
                ..KvPendingAction::default()
            }),
        }
    );
}

#[test]
fn overview_jump_tab_actions_use_initial_database_and_fallback() {
    for (kind, tab) in [(KvJumpKind::Console, "console"), (KvJumpKind::Pubsub, "pubsub")] {
        assert_eq!(
            plan_overview_jump(&jump(kind, Some(9)), Some("db3")),
            KvJumpAction::Open {
                database: "db3".to_owned(),
                pending: Some(KvPendingAction { tab: Some(tab.to_owned()), ..KvPendingAction::default() }),
            }
        );
    }
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::Console, None), None),
        KvJumpAction::Open {
            database: "db0".to_owned(),
            pending: Some(KvPendingAction { tab: Some("console".to_owned()), ..KvPendingAction::default() }),
        }
    );
    for (section, expected) in [("info", "info"), ("memory", "memory"), ("slowlog", "slowlog"), ("other", "info")] {
        let mut target = jump(KvJumpKind::Monitor, Some(1));
        target.section = Some(section.to_owned());
        assert_eq!(
            plan_overview_jump(&target, Some("db3")),
            KvJumpAction::Open {
                database: "db3".to_owned(),
                pending: Some(KvPendingAction {
                    tab: Some("monitor".to_owned()),
                    monitor_sub_page: expected.to_owned(),
                    ..KvPendingAction::default()
                }),
            }
        );
    }
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::ImportExport, Some(1)), Some("db3")),
        KvJumpAction::Open {
            database: "db3".to_owned(),
            pending: Some(KvPendingAction { open_import_export: true, ..KvPendingAction::default() }),
        }
    );
    assert_eq!(
        plan_overview_jump(&jump(KvJumpKind::Unknown("mystery".to_owned()), Some(1)), None),
        KvJumpAction::Unhandled
    );
}

// ── relay lifecycle ───────────────────────────────────────────────────────

#[test]
fn relay_commit_is_silent_for_noops_and_applies_real_patches() {
    let empty = KvSlotSnapshot::empty();
    assert_eq!(empty.scan_cursor, "0");
    assert!(commit_slot_patch(&empty, &KvSlotPatch::default()).is_none());
    assert!(commit_slot_patch(&empty, &KvSlotPatch { scan_cursor: Some("0".to_owned()), ..KvSlotPatch::default() }).is_none());
    assert!(commit_slot_patch(&empty, &KvSlotPatch { selected_key: Some(None), ..KvSlotPatch::default() }).is_none());
    assert!(commit_slot_patch(&empty, &KvSlotPatch { dirty: Some(false), ..KvSlotPatch::default() }).is_none());

    let next = commit_slot_patch(
        &empty,
        &KvSlotPatch {
            selected_key: Some(Some("session:1".to_owned())),
            scan_budget: Some((5, 5_000)),
            last_write: Some((Some("set".to_owned()), Some(12.5))),
            ..KvSlotPatch::default()
        },
    )
    .expect("a real patch produces a new snapshot");
    assert_eq!(next.selected_key.as_deref(), Some("session:1"));
    assert_eq!((next.scan_budget_used, next.scan_budget_total), (5, 5_000));
    assert_eq!(next.last_write_command.as_deref(), Some("set"));
    assert_eq!(next.last_write_duration_ms, Some(12.5));
    assert_eq!(empty.selected_key, None, "the published snapshot is immutable");

    // Re-applying the same values is a no-op on the new snapshot.
    let same = KvSlotPatch {
        selected_key: Some(next.selected_key.clone()),
        scan_budget: Some((next.scan_budget_used, next.scan_budget_total)),
        ..KvSlotPatch::default()
    };
    assert!(commit_slot_patch(&next, &same).is_none());
}

#[test]
fn prune_slot_states_removes_only_absent_panels() {
    let atoms: BTreeMap<String, u8> = [("a", 1), ("b", 2), ("c", 3)].into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    let live: BTreeSet<String> = ["a".to_owned(), "c".to_owned()].into_iter().collect();
    assert_eq!(prune_slot_states(&atoms, &live), vec!["b".to_owned()]);
    let all_live: BTreeSet<String> = atoms.keys().cloned().collect();
    assert!(prune_slot_states(&atoms, &all_live).is_empty());
    assert!(prune_slot_states(&atoms, &BTreeSet::new()).len() == 3);
}

#[test]
fn db_index_safe_integer_boundary_is_exact() {
    let indexed = Some(meta());
    assert_eq!(
        resolve_kv_database_index(indexed.as_ref(), Some("db9007199254740991")),
        Some(9_007_199_254_740_991),
        "MAX_SAFE_INTEGER itself is still safe"
    );
    assert_eq!(
        resolve_kv_database_index(indexed.as_ref(), Some("db9007199254740992")),
        None,
        "one past MAX_SAFE_INTEGER is refused"
    );
}

#[test]
fn commit_uses_object_is_semantics_for_nan_and_minus_zero() {
    let nan = KvSlotSnapshot {
        last_write_duration_ms: Some(f64::NAN),
        ..KvSlotSnapshot::empty()
    };
    let same_nan = KvSlotPatch {
        last_write: Some((None, Some(f64::NAN))),
        ..KvSlotPatch::default()
    };
    assert!(commit_slot_patch(&nan, &same_nan).is_none(), "identical NaN bits are a no-op");

    let minus_zero = KvSlotSnapshot {
        last_write_duration_ms: Some(-0.0),
        ..KvSlotSnapshot::empty()
    };
    let plus_zero = KvSlotPatch {
        last_write: Some((None, Some(0.0))),
        ..KvSlotPatch::default()
    };
    assert!(commit_slot_patch(&minus_zero, &plus_zero).is_some(), "Object.is(-0, 0) is false");
}
