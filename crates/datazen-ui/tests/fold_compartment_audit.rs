//! T196 audit: the fold compartment's host gate, verified against the
//! React source re-read line by line
//! (`components/sql-editor/fold/foldCompartment.ts`) and the React test
//! `__tests__/foldCompartment.test.ts` — the setting-gate group in full,
//! plus the shared-bag primitives that group pins. The registry groups
//! (base ids, atomic batch reconfiguration, multi-instance isolation,
//! overflow degradation, the extra factory) are the T335 split half and
//! land with that task.
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use serde_json::json;

use datazen_ui::components::sql_editor::fold::{
    create_fold_compartment_extensions, fold_setting_enabled, pro_setting_flag,
    FoldCompartmentPayload,
};

/// React `proSettingFlag`: absent key → fallback; boolean → value;
/// non-boolean → fallback (never silently false).
#[test]
fn pro_setting_flag_only_a_real_boolean_switches() {
    let bag = json!({ "on": true, "off": false });
    assert!(pro_setting_flag(Some(&bag), "on", false));
    assert!(!pro_setting_flag(Some(&bag), "off", true));
    // Missing key: the fallback.
    assert!(pro_setting_flag(Some(&bag), "missing", true));
    assert!(!pro_setting_flag(Some(&bag), "missing", false));
    // No bag at all: the fallback.
    assert!(pro_setting_flag(None, "on", true));
    assert!(!pro_setting_flag(None, "on", false));
    // Non-boolean values are corruption, not instructions.
    let corrupt = json!({ "a": "no", "b": 0, "c": null, "d": [] });
    assert!(pro_setting_flag(Some(&corrupt), "a", true));
    assert!(pro_setting_flag(Some(&corrupt), "b", true));
    assert!(pro_setting_flag(Some(&corrupt), "c", true));
    assert!(pro_setting_flag(Some(&corrupt), "d", true));
    assert!(!pro_setting_flag(Some(&corrupt), "a", false));
    // A non-object bag reads as every-key-missing (JS `bag?.[key]`).
    assert!(pro_setting_flag(Some(&json!("nope")), "a", true));
}

/// React `foldSettingEnabled`: the fallback is true — folding is the
/// common case, and a corrupted bag should not remove it.
#[test]
fn fold_setting_falls_back_to_on() {
    assert!(fold_setting_enabled(None), "no bag installs folding");
    assert!(fold_setting_enabled(Some(&json!({}))), "an empty bag installs folding");
    assert!(fold_setting_enabled(Some(&json!({ "codeFolding": true }))));
    assert!(!fold_setting_enabled(Some(&json!({ "codeFolding": false }))));
    // Non-boolean values ignore rather than treat as false.
    assert!(fold_setting_enabled(Some(&json!({ "codeFolding": "nope" }))));
    assert!(fold_setting_enabled(Some(&json!({ "codeFolding": 0 }))));
}

/// The React setting-gate group, case for case: default install,
/// explicit true, the false flip to the empty payload, the two-state
/// journey back, and the corrupted-bag tolerance.
#[test]
fn the_gate_installs_by_default_and_flips_both_ways() {
    // No bag present: installed.
    assert!(matches!(
        create_fold_compartment_extensions(None),
        FoldCompartmentPayload::Extension { pro_settings: None }
    ));
    // Explicitly on: installed, the bag forwarded verbatim.
    let on = json!({ "codeFolding": true });
    assert_eq!(
        create_fold_compartment_extensions(Some(&on)),
        FoldCompartmentPayload::Extension { pro_settings: Some(on.clone()) }
    );
    // Flipped off: the empty payload ([] on the wire — the slot stays
    // mounted, so a remount cannot lose scroll and selection).
    assert_eq!(
        create_fold_compartment_extensions(Some(&json!({ "codeFolding": false }))),
        FoldCompartmentPayload::Empty
    );
    // The journey in reverse: a setting is a two-state machine, not a
    // one-way door.
    assert!(matches!(
        create_fold_compartment_extensions(Some(&json!({ "codeFolding": true }))),
        FoldCompartmentPayload::Extension { .. }
    ));
    // Corrupted values keep the feature on.
    assert!(matches!(
        create_fold_compartment_extensions(Some(&json!({ "codeFolding": "nope" }))),
        FoldCompartmentPayload::Extension { .. }
    ));
    assert!(matches!(
        create_fold_compartment_extensions(Some(&json!({ "codeFolding": 0 }))),
        FoldCompartmentPayload::Extension { .. }
    ));
}

/// React's "never forwards a contradictory instruction": when the gate
/// says off, the extension sees nothing at all — there is no combination
/// of host options and bag that can install the compartment while
/// telling the extension to stay off.
#[test]
fn the_off_gate_forwards_nothing() {
    let off = json!({ "codeFolding": false });
    match create_fold_compartment_extensions(Some(&off)) {
        FoldCompartmentPayload::Empty => {}
        FoldCompartmentPayload::Extension { .. } => {
            panic!("the off gate must not build an extension payload")
        }
    }
    // Structurally: Empty carries no bag, so no forwarding exists.
    assert!(!matches!(FoldCompartmentPayload::Empty, FoldCompartmentPayload::Extension { .. }));

    // When on, the SAME key on the SAME object is what the extension
    // reads (the forwarded bag is the gate's bag, verbatim).
    let bag = json!({ "codeFolding": true, "unrelated": "kept" });
    let FoldCompartmentPayload::Extension { pro_settings } =
        create_fold_compartment_extensions(Some(&bag))
    else {
        panic!("on gate forwards");
    };
    assert_eq!(pro_settings.as_ref(), Some(&bag));
    assert_eq!(pro_settings.as_ref().unwrap()["unrelated"], "kept", "the bag travels wholesale");
}
