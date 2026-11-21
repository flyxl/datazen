use super::*;

fn t(key: &str, params: &[(&str, String)]) -> String {
    if key == KEY_SELECTED_ROWS {
        let count = params
            .iter()
            .find(|(name, _)| *name == "count")
            .map(|(_, value)| value.as_str())
            .unwrap_or("?");
        return format!("T:{key}:{count}");
    }
    format!("T:{key}")
}

fn input() -> ExecuteBarInput {
    ExecuteBarInput {
        selected_rows: 3,
        has_deletes: false,
        target_read_only: false,
        executing: false,
        can_execute: true,
    }
}

#[test]
fn endpoints_literals_are_pinned() {
    assert_eq!(ENDPOINTS_TEST_ID_PREFIX, "data-sync");
    assert_eq!(ENDPOINTS_I18N_PREFIX, "sync");
    assert_eq!(COMPARE_LABEL_KEY, "sync.compare");
    assert_eq!(PATH_TEST_ID, "data-sync-path");
    assert_eq!(EXPERIMENTAL_HINT_KEY, "common.experimentalPairHint");
    assert_eq!(EXPERIMENTAL_PAIR_KEY, "common.experimentalPair");
    assert_eq!(PATH_DIRECT_KEY, "sync.pathDirect");
    assert_eq!(PATH_IR_KEY, "sync.pathIr");
    assert_eq!(USE_TRANSFER_HINT_KEY, "sync.useTransferHint");
    assert_eq!(SyncPairingPath::Direct.as_str(), "direct");
    assert_eq!(SyncPairingPath::Ir.as_str(), "ir");
    assert_eq!(SyncPairingPath::Unsupported.as_str(), "unsupported");
    assert_eq!(EndpointsLayout::Bar.as_str(), "bar");
    assert_eq!(EndpointsLayout::Grid.as_str(), "grid");
}

#[test]
fn no_active_pairing_produces_no_note_and_defaults_to_bar() {
    let config = build_endpoints_bar_config(None, None, false, &t);
    assert_eq!(config.test_id_prefix, "data-sync");
    assert_eq!(config.i18n_prefix, "sync");
    assert_eq!(config.layout, EndpointsLayout::Bar);
    assert_eq!(config.compare_label, "T:sync.compare");
    assert_eq!(config.action_note, None);
    assert_eq!(resolve_action_note(None, true, &t), None);
}

#[test]
fn supported_pairings_choose_direct_or_ir_labels() {
    let direct = ActivePairing {
        supported: true,
        path: SyncPairingPath::Direct,
    };
    let note = match resolve_action_note(Some(&direct), false, &t) {
        Some(value) => value,
        None => panic!("direct pairing must produce a note"),
    };
    assert_eq!(note.test_id, "data-sync-path");
    assert_eq!(note.title, None);
    assert_eq!(note.text, "T:sync.pathDirect");
    assert_eq!(note.class, SUPPORTED_CLASS);

    let ir = ActivePairing {
        supported: true,
        path: SyncPairingPath::Ir,
    };
    let note = match resolve_action_note(Some(&ir), false, &t) {
        Some(value) => value,
        None => panic!("ir pairing must produce a note"),
    };
    assert_eq!(note.text, "T:sync.pathIr");
}

#[test]
fn the_experimental_flag_overrides_the_note_and_adds_a_title() {
    let direct = ActivePairing {
        supported: true,
        path: SyncPairingPath::Direct,
    };
    let note = match resolve_action_note(Some(&direct), true, &t) {
        Some(value) => value,
        None => panic!("experimental pairing must produce a note"),
    };
    assert_eq!(note.text, "T:common.experimentalPair");
    assert_eq!(note.title.as_deref(), Some("T:common.experimentalPairHint"));
    assert_eq!(note.class, SUPPORTED_CLASS);

    let unsupported = ActivePairing {
        supported: false,
        path: SyncPairingPath::Ir,
    };
    let note = match resolve_action_note(Some(&unsupported), true, &t) {
        Some(value) => value,
        None => panic!("unsupported pairing must produce a note"),
    };
    assert_eq!(note.text, "T:sync.useTransferHint");
    assert_eq!(note.title.as_deref(), Some("T:common.experimentalPairHint"));
    assert_eq!(note.class, UNSUPPORTED_CLASS);
}

#[test]
fn an_explicit_grid_layout_is_kept() {
    let config = build_endpoints_bar_config(Some(EndpointsLayout::Grid), None, false, &t);
    assert_eq!(config.layout, EndpointsLayout::Grid);
    assert_eq!(config.action_note, None);
}

#[test]
fn execute_literals_are_pinned() {
    assert_eq!(EXECUTE_TEST_ID, "data-sync-execute");
    assert_eq!(CANCEL_TEST_ID, "data-sync-cancel");
    assert_eq!(START_TEST_ID, "data-sync-start");
    assert_eq!(START_DISABLED_TEST_ID, "data-sync-start-disabled");
    assert_eq!(KEY_SELECTED_ROWS, "sync.selectedRows");
    assert_eq!(KEY_TARGET_READ_ONLY, "sync.targetReadOnly");
    assert_eq!(KEY_DELETE_WILL_APPLY, "sync.deleteWillApply");
    assert_eq!(KEY_CANCEL, "common.cancel");
    assert_eq!(KEY_EXECUTE_UNAVAILABLE, "sync.executeUnavailable");
    assert_eq!(KEY_EXECUTING, "sync.executing");
    assert_eq!(KEY_EXECUTE, "sync.execute");
    assert_eq!(CancelVariant::Ghost.as_str(), "ghost");
    assert_eq!(RunVariant::Run.as_str(), "run");
}

#[test]
fn an_enabled_bar_shows_only_the_start_action() {
    let view = build_execute_bar_view(input(), &t);
    assert!(view.enabled);
    assert_eq!(view.container_test_id, Some(EXECUTE_TEST_ID));
    assert_eq!(view.selected_rows_text, "T:sync.selectedRows:3");
    assert_eq!(view.container_class, CONTAINER_CLASS);
    assert!(!view.show_read_only_warning);
    assert!(!view.show_delete_warning);
    assert!(!view.show_cancel);
    assert_eq!(view.start_test_id, START_TEST_ID);
    assert!(!view.start_disabled);
    assert_eq!(view.start_title, None);
    assert_eq!(view.start_text, "T:sync.execute");
    assert_eq!(view.run_variant, RunVariant::Run);
}

#[test]
fn a_read_only_target_blocks_execution_and_suppresses_delete_warning() {
    let view = build_execute_bar_view(
        ExecuteBarInput {
            has_deletes: true,
            target_read_only: true,
            ..input()
        },
        &t,
    );
    assert!(!view.enabled);
    assert_eq!(view.container_test_id, None);
    assert!(view.show_read_only_warning);
    assert!(!view.show_delete_warning);
    assert_eq!(view.start_test_id, START_DISABLED_TEST_ID);
    assert!(view.start_disabled);
    assert_eq!(view.start_title.as_deref(), Some("T:sync.targetReadOnly"));
}

#[test]
fn no_rows_or_no_capability_uses_the_unavailable_title() {
    for changed in [
        ExecuteBarInput {
            selected_rows: 0,
            ..input()
        },
        ExecuteBarInput {
            can_execute: false,
            ..input()
        },
    ] {
        let view = build_execute_bar_view(changed, &t);
        assert!(!view.enabled);
        assert_eq!(view.selected_rows_text, format!("T:sync.selectedRows:{}", changed.selected_rows));
        assert_eq!(view.start_title.as_deref(), Some("T:sync.executeUnavailable"));
    }
    let deleting = build_execute_bar_view(
        ExecuteBarInput {
            has_deletes: true,
            can_execute: false,
            ..input()
        },
        &t,
    );
    assert!(deleting.show_delete_warning);
    assert_eq!(deleting.delete_text, "T:sync.deleteWillApply");
}

#[test]
fn an_executing_bar_offers_cancel_and_switches_the_start_label() {
    let view = build_execute_bar_view(
        ExecuteBarInput {
            executing: true,
            ..input()
        },
        &t,
    );
    assert!(view.enabled);
    assert_eq!(view.container_test_id, Some(EXECUTE_TEST_ID));
    assert!(view.show_cancel);
    assert_eq!(view.cancel_text, "T:common.cancel");
    assert_eq!(view.cancel_variant, CancelVariant::Ghost);
    assert_eq!(view.start_text, "T:sync.executing");
    assert!(view.start_disabled);
    assert_eq!(view.start_test_id, START_TEST_ID);
    assert_eq!(view.start_title, None);
    assert_eq!(view.read_only_text, "T:sync.targetReadOnly");
    assert_eq!(view.delete_text, "T:sync.deleteWillApply");
}

#[test]
fn events_obey_the_render_guards() {
    let view = build_execute_bar_view(input(), &t);
    assert!(can_fire(ExecuteBarEvent::Execute, &view));
    assert!(!can_fire(ExecuteBarEvent::Cancel, &view));

    let blocked = build_execute_bar_view(
        ExecuteBarInput {
            can_execute: false,
            ..input()
        },
        &t,
    );
    assert!(!can_fire(ExecuteBarEvent::Execute, &blocked));

    let executing = build_execute_bar_view(
        ExecuteBarInput {
            executing: true,
            ..input()
        },
        &t,
    );
    assert!(can_fire(ExecuteBarEvent::Cancel, &executing));
    assert!(!can_fire(ExecuteBarEvent::Execute, &executing));
}

#[test]
fn crate_root_reexports_the_bar_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ActionNote>();
    resolves::<crate::ActivePairing>();
    resolves::<crate::EndpointsBarConfig>();
    resolves::<crate::ExecuteBarView>();
    resolves::<crate::ExecuteBarInput>();
    resolves::<crate::ExecuteBarEvent>();
    assert_eq!(crate::ENDPOINTS_TEST_ID_PREFIX, "data-sync");
    assert_eq!(crate::START_TEST_ID, "data-sync-start");
    assert!(crate::can_fire(
        crate::ExecuteBarEvent::Execute,
        &crate::build_execute_bar_view(input(), &t)
    ));
}
