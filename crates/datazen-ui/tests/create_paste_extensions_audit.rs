//! T263 audit: the paste compartment's host factory, verified against
//! the React source re-read line by line (createPasteExtensions.ts has
//! no direct React test — the paste journey tests exercise T320's
//! machine; the assembly rule below is re-derived from the factory's
//! body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use serde_json::json;

use datazen_ui::components::sql_editor::create_paste_extensions::{
    create_paste_extensions, PasteCompartmentOptions, PasteCompartmentPayload,
};

/// The base always mounts — with or without the enhanced EP, with or
/// without options.
#[test]
fn the_base_always_mounts() {
    // No EP: the community shape (multi-cursor only, no paste-as-IN).
    assert_eq!(
        create_paste_extensions(None, false),
        PasteCompartmentPayload::BaseOnly
    );
    assert_eq!(
        create_paste_extensions(
            Some(&PasteCompartmentOptions { connection_id: Some("conn-1"), pro_settings: None }),
            false,
        ),
        PasteCompartmentPayload::BaseOnly,
        "a missing EP contributes nothing, the base still mounts"
    );
}

/// The enhanced half forwards the options verbatim — the same values
/// the host was given, one source, no overrides.
#[test]
fn the_enhanced_half_forwards_the_options_verbatim() {
    // No options: the enhanced payload carries empty halves.
    assert_eq!(
        create_paste_extensions(None, true),
        PasteCompartmentPayload::Enhanced { connection_id: None, pro_settings: None }
    );
    // With options: connection id and the G3 bag, verbatim.
    let bag = json!({"pasteAsIn": true});
    let opts = PasteCompartmentOptions {
        connection_id: Some("conn-1"),
        pro_settings: Some(&bag),
    };
    assert_eq!(
        create_paste_extensions(Some(&opts), true),
        PasteCompartmentPayload::Enhanced {
            connection_id: Some("conn-1".to_owned()),
            pro_settings: Some(bag.clone()),
        }
    );
    // The bag is forwarded WHOLE (a sibling key rides along untouched).
    let rich_bag = json!({"pasteAsIn": false, "unrelated": "kept"});
    let opts = PasteCompartmentOptions {
        connection_id: None,
        pro_settings: Some(&rich_bag),
    };
    assert_eq!(
        create_paste_extensions(Some(&opts), true),
        PasteCompartmentPayload::Enhanced {
            connection_id: None,
            pro_settings: Some(rich_bag),
        }
    );
}

/// The payload's two arms are the assembly's whole vocabulary: there is
/// no gate (no setting switches the compartment off) and no third
/// shape.
#[test]
fn there_is_no_gate_and_no_third_shape() {
    // Unlike the fold compartment, paste is ungated: the base is the
    // community floor, the enhanced half is additive.
    let shapes = [
        create_paste_extensions(None, false),
        create_paste_extensions(None, true),
        create_paste_extensions(
            Some(&PasteCompartmentOptions { connection_id: Some("c"), pro_settings: None }),
            true,
        ),
    ];
    for shape in &shapes {
        assert!(
            matches!(
                shape,
                PasteCompartmentPayload::BaseOnly | PasteCompartmentPayload::Enhanced { .. }
            ),
            "no third shape exists"
        );
    }
}
