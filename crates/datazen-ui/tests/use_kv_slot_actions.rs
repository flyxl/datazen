//! T616: `useKvSlotActions` logic — every action branch and guard as pure
//! decisions: wired refresh/settings/selectDatabase, unwired degradation with
//! the diagnostic warning, and the flushDb confirm gate (Safe Mode vs consent).

use datazen_ui::views::use_kv_slot_actions::{
    dispatch, flush_db_gate, flush_db_result, warn_unwired_text, wired_actions, ConfirmKind,
    DangerDialog, KvSlotAction, KvSlotDecision, DRIVER_SETTINGS_SECTION, FLUSH_BLOCKED_KEY,
    FLUSH_MESSAGE_KEY, FLUSH_TITLE_KEY,
};

// ── contract mirrors ─────────────────────────────────────────────────────────

#[test]
fn wired_list_and_warning_text_name_the_action_and_wired_set() {
    assert_eq!(wired_actions(), &["refresh", "openSettings", "selectDatabase"]);
    let text = warn_unwired_text("newKey");
    assert!(text.contains("\"newKey\""), "warning names the action");
    assert!(text.contains("refresh"), "warning advertises what is wired");
    assert!(text.contains("openSettings") && text.contains("selectDatabase"));
}

#[test]
fn every_action_has_the_react_wire_type_name() {
    assert_eq!(KvSlotAction::Refresh.type_name(), "refresh");
    assert_eq!(KvSlotAction::NewKey.type_name(), "newKey");
    assert_eq!(KvSlotAction::Import.type_name(), "import");
    assert_eq!(KvSlotAction::Export.type_name(), "export");
    assert_eq!(KvSlotAction::FlushDb.type_name(), "flushDb");
    assert_eq!(KvSlotAction::OpenMonitor.type_name(), "openMonitor");
    assert_eq!(KvSlotAction::OpenSettings.type_name(), "openSettings");
    assert_eq!(KvSlotAction::SetScanBudget { value: 5.0 }.type_name(), "setScanBudget");
    assert_eq!(KvSlotAction::SelectDatabase { database: "db".to_owned() }.type_name(), "selectDatabase");
    assert_eq!(KvSlotAction::Unknown("compactKeys".to_owned()).type_name(), "compactKeys");
    assert_eq!(DRIVER_SETTINGS_SECTION, "extensions");
}

// ── wired branches ───────────────────────────────────────────────────────────

#[test]
fn refresh_and_open_settings_route_to_their_sinks() {
    assert_eq!(dispatch(&KvSlotAction::Refresh, false, false), KvSlotDecision::Refresh);
    assert_eq!(
        dispatch(&KvSlotAction::OpenSettings, false, false),
        KvSlotDecision::OpenSettings { section: "extensions".to_owned() }
    );
}

#[test]
fn select_database_is_wired_only_when_the_host_threaded_a_sink() {
    let action = KvSlotAction::SelectDatabase { database: "db3".to_owned() };
    assert_eq!(
        dispatch(&action, true, false),
        KvSlotDecision::SelectDatabase { database: "db3".to_owned() }
    );
    // No sink → degrade to the warning, never a throw.
    assert_eq!(
        dispatch(&action, false, false),
        KvSlotDecision::WarnUnwired { action_type: "selectDatabase".to_owned() }
    );
}

// ── unwired branches ─────────────────────────────────────────────────────────

#[test]
fn every_unwired_action_degrades_to_a_warning() {
    for action in [
        KvSlotAction::NewKey,
        KvSlotAction::Import,
        KvSlotAction::Export,
        KvSlotAction::OpenMonitor,
        KvSlotAction::SetScanBudget { value: 42.0 },
        KvSlotAction::Unknown("compactKeys".to_owned()),
    ] {
        let expected = KvSlotDecision::WarnUnwired {
            action_type: action.type_name().to_owned(),
        };
        assert_eq!(dispatch(&action, false, false), expected, "{} must not throw", action.type_name());
    }
}

// ── dangerous gate ───────────────────────────────────────────────────────────

#[test]
fn flush_db_asks_the_warning_confirm_when_safe_mode_is_off() {
    assert_eq!(
        flush_db_gate(false),
        DangerDialog {
            title_key: FLUSH_TITLE_KEY,
            message_key: FLUSH_MESSAGE_KEY,
            confirm_label_key: "common.confirm",
            kind: ConfirmKind::Warning,
            safe_mode: false,
        }
    );
    assert_eq!(
        dispatch(&KvSlotAction::FlushDb, false, false),
        KvSlotDecision::ConfirmDanger { dialog: flush_db_gate(false) }
    );
}

#[test]
fn safe_mode_hard_blocks_with_an_info_dismiss_dialog() {
    let dialog = flush_db_gate(true);
    assert_eq!(dialog.title_key, "settings.safeMode");
    assert_eq!(dialog.message_key, FLUSH_BLOCKED_KEY);
    assert_eq!(dialog.confirm_label_key, "common.dismiss");
    assert_eq!(dialog.kind, ConfirmKind::Info);
    assert!(dialog.safe_mode);
    assert_eq!(
        dispatch(&KvSlotAction::FlushDb, false, true),
        KvSlotDecision::ConfirmDanger { dialog }
    );
}

#[test]
fn consent_only_reaches_the_warning_and_a_decline_does_nothing() {
    let dialog = flush_db_gate(false);
    assert_eq!(
        flush_db_result(&dialog, true),
        KvSlotDecision::WarnUnwired { action_type: "flushDb".to_owned() }
    );
    assert_eq!(flush_db_result(&dialog, false), KvSlotDecision::None);

    // Safe Mode's "Dismiss" is not consent: nothing proceeds or warns.
    let blocked = flush_db_gate(true);
    assert_eq!(flush_db_result(&blocked, true), KvSlotDecision::None);
    assert_eq!(flush_db_result(&blocked, false), KvSlotDecision::None);
}

// ── T616 independent audit (appended; reuses this module's imports) ──────

/// React's `warnUnwired` text is fully pinned (action name + the wired set in
/// declaration order). The coder suite only checks `contains`; pin the exact
/// string and an unknown union member's name.
#[test]
fn audit_unwired_warning_text_is_exact() {
    assert_eq!(
        warn_unwired_text("compactKeys"),
        "[kv-slot-actions] no host handler for \"compactKeys\" (wired: refresh, openSettings, selectDatabase); ignored. A driver slot may offer it, but nothing will happen until the host wires it."
    );
}

/// React's switch handles `refresh` / `openSettings` / `selectDatabase` before
/// the dangerous branch and the unwired default; the flags for a *different*
/// branch (safe mode, select sink) must never leak into those decisions.
#[test]
fn audit_wired_branches_ignore_unrelated_flags() {
    assert_eq!(
        dispatch(&KvSlotAction::Refresh, false, true),
        KvSlotDecision::Refresh,
        "safe mode must not gate a refresh"
    );
    assert_eq!(
        dispatch(&KvSlotAction::OpenSettings, false, true),
        KvSlotDecision::OpenSettings { section: "extensions".to_owned() },
        "safe mode must not gate openSettings"
    );
    assert_eq!(
        dispatch(&KvSlotAction::SelectDatabase { database: String::new() }, true, true),
        KvSlotDecision::SelectDatabase { database: String::new() },
        "an empty db label is still forwarded verbatim, safe mode independent"
    );
}

/// Every unwired action degrades the same way regardless of the unrelated host
/// flags — the guard order in React's `request` puts them in the default branch.
#[test]
fn audit_unwired_branch_is_flag_independent() {
    let action = KvSlotAction::NewKey;
    for (has_select, safe) in [(false, false), (true, false), (false, true), (true, true)] {
        assert_eq!(
            dispatch(&action, has_select, safe),
            KvSlotDecision::WarnUnwired { action_type: "newKey".to_owned() }
        );
    }
}

/// The dangerous branch is flag-independent on the way in: `flushDb` always
/// returns the gate, whether or not a select sink exists. Only safe mode shapes
/// the gate itself.
#[test]
fn audit_flush_gate_is_independent_of_select_sink() {
    assert_eq!(
        dispatch(&KvSlotAction::FlushDb, false, false),
        dispatch(&KvSlotAction::FlushDb, true, false)
    );
    assert_eq!(
        dispatch(&KvSlotAction::FlushDb, false, true),
        dispatch(&KvSlotAction::FlushDb, true, true)
    );
    assert_ne!(
        dispatch(&KvSlotAction::FlushDb, false, false),
        dispatch(&KvSlotAction::FlushDb, false, true)
    );
}
