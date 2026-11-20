//! T176 independent audit (tester): fresh vectors — a whitespace-only
//! `gateLost` stays truthy, a whitespace target name still blocks, an empty
//! row list clears, a disabled create-new never blocks, the reason and
//! receipt coexistence order, and the banner/text nesting.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{
    MappingGateNotice, TransferColumnMapping, TransferMappingStatus, TransferTableVm,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

fn base_row() -> TransferTableVm {
    TransferTableVm {
        source_table: "orders".to_owned(),
        target_table: "orders".to_owned(),
        status: TransferMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: vec![mapping("id", "id", false)],
        source_columns: Some(vec!["id".to_owned()]),
        target_columns: Some(vec!["id".to_owned()]),
        source_column_types: vec![("id".to_owned(), "INTEGER".to_owned())],
        source_primary_keys: None,
        recordset: None,
    }
}

/// Enabled create-new whose name is only whitespace: `rowClearsGate`
/// trims, so this is the unnamed shape exactly like an empty string.
fn whitespace_named_new_row() -> TransferTableVm {
    TransferTableVm {
        target_table: "   ".to_owned(),
        create_new: true,
        ..base_row()
    }
}

#[derive(Default)]
struct Scene {
    visible: bool,
    rows: Vec<TransferTableVm>,
    gate_lost: String,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        MappingGateNotice::new(
            scene.visible,
            scene.rows.clone(),
            scene.gate_lost.clone(),
        )
        .build(scene.dark),
    )
}

fn mount(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, (1440.0, 816.0), env(dark));
    rt.frame(0.0);
    rt
}

fn find<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn message_of<S>(rt: &Runtime<S>) -> String {
    find(rt, "data-transfer-mapping-gate-error-text")
        .and_then(|e| e.text)
        .unwrap_or_default()
}

#[test]
fn a_whitespace_only_receipt_is_truthy_and_renders_as_is() {
    let rt = mount(Scene {
        visible: true,
        rows: vec![whitespace_named_new_row()],
        gate_lost: " ".to_owned(),
        dark: true,
    });
    // React `gateLost || …`: " " is truthy, so it is the message.
    assert_eq!(message_of(&rt), " ");
}

#[test]
fn a_whitespace_target_name_still_blocks() {
    let rt = mount(Scene {
        visible: true,
        rows: vec![whitespace_named_new_row()],
        gate_lost: String::new(),
        dark: true,
    });
    assert_eq!(message_of(&rt), t("transfer.mapping.targetNameRequired"));
}

#[test]
fn an_empty_row_list_renders_no_banner() {
    let rt = mount(Scene {
        visible: true,
        rows: Vec::new(),
        gate_lost: String::new(),
        dark: true,
    });
    assert!(find(&rt, "data-transfer-mapping-gate-error").is_none());
}

#[test]
fn a_disabled_create_new_row_never_blocks() {
    let rt = mount(Scene {
        visible: true,
        rows: vec![TransferTableVm {
            enabled: false,
            ..whitespace_named_new_row()
        }],
        gate_lost: String::new(),
        dark: true,
    });
    assert!(
        find(&rt, "data-transfer-mapping-gate-error").is_none(),
        "the blocked predicate requires row.enabled"
    );
}

#[test]
fn the_receipt_wins_when_both_a_reason_and_a_receipt_exist() {
    let rt = mount(Scene {
        visible: true,
        rows: vec![whitespace_named_new_row()],
        gate_lost: "prepare admitted rows that no longer clear the gate".to_owned(),
        dark: true,
    });
    assert_eq!(
        message_of(&rt),
        "prepare admitted rows that no longer clear the gate"
    );
}

#[test]
fn the_text_is_a_child_of_the_banner() {
    let rt = mount(Scene {
        visible: true,
        rows: vec![whitespace_named_new_row()],
        gate_lost: String::new(),
        dark: true,
    });
    let banner = find(&rt, "data-transfer-mapping-gate-error").expect("banner mounted");
    let message = find(&rt, "data-transfer-mapping-gate-error-text").expect("text mounted");
    // The banner's rectangle encloses the message's: the text is inside.
    let [bx, by, bw, bh] = banner.rect;
    let [tx, ty, tw, th] = message.rect;
    assert!(tx >= bx - 0.5 && tx + tw <= bx + bw + 0.5, "x containment");
    assert!(ty >= by - 0.5 && ty + th <= by + bh + 0.5, "y containment");
    // The padding (16/12pt) leaves the message strictly inside the border.
    assert!(tx >= bx + 15.0 && ty >= by + 11.0, "px-4/py-3 insets");
}
