//! T176: `windows/data-transfer/MappingGateNotice.tsx` port — the message
//! chain (visible / gateLost / derived reason), the nothing-renders
//! branches, the alert role, and window-width geometry.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::{has, t};
use crate::views::{MappingGateNotice, TransferColumnMapping, TransferTableVm};

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

/// A row that clears the gate on its own: enabled, no create-new, an
/// active mapping.
fn clear_row() -> TransferTableVm {
    TransferTableVm {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: crate::views::TransferMappingStatus::Matched,
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

/// A row nobody has named: enabled create-new behind an empty name, with
/// an active mapping — the one shape that shuts the gate.
fn unnamed_new_row() -> TransferTableVm {
    TransferTableVm {
        target_table: String::new(),
        create_new: true,
        ..clear_row()
    }
}

/// A row with mappings all skipped: it neither clears the gate nor blocks
/// it, so the reason stays `None`.
fn idle_row() -> TransferTableVm {
    TransferTableVm {
        column_mappings: vec![mapping("id", "", true)],
        source_columns: Some(vec!["id".to_owned()]),
        target_columns: Some(vec![]),
        ..clear_row()
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

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn banner<S>(rt: &Runtime<S>) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-transfer-mapping-gate-error") && !e.exiting)
}

fn has_banner<S>(rt: &Runtime<S>) -> bool {
    banner(rt).is_some()
}

fn text_of<S>(rt: &Runtime<S>) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-transfer-mapping-gate-error-text") && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

// ---- the message chain ----

#[test]
fn an_invisible_notice_renders_nothing() {
    let rt = mount(
        Scene {
            visible: false,
            rows: vec![unnamed_new_row()],
            gate_lost: "prepare failed".to_owned(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(!has_banner(&rt), "visible=false renders null before any other check");
}

#[test]
fn a_prepare_receipt_wins_over_the_derived_reason() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![unnamed_new_row(), clear_row()],
            gate_lost: "target vanished".to_owned(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(has_banner(&rt));
    assert_eq!(text_of(&rt), "target vanished", "gateLost is the message, the rows are not consulted");
}

#[test]
fn the_unnamed_create_new_reason_is_the_derived_message() {
    assert!(has("transfer.mapping.targetNameRequired"));
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![unnamed_new_row()],
            gate_lost: String::new(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(has_banner(&rt));
    assert_eq!(text_of(&rt), t("transfer.mapping.targetNameRequired"));
}

#[test]
fn a_clearing_row_shuts_off_the_derived_reason() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![unnamed_new_row(), clear_row()],
            gate_lost: String::new(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(
        !has_banner(&rt),
        "some(rowClearsGate) -> reason null -> empty message -> no banner"
    );
}

#[test]
fn rows_that_neither_clear_nor_block_render_no_banner() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![idle_row()],
            gate_lost: String::new(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(!has_banner(&rt), "no blocked row -> reason null -> no banner");
}

#[test]
fn an_empty_gate_lost_is_falsy_not_a_message() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![clear_row()],
            gate_lost: String::new(),
            dark: true,
        },
        (960.0, 640.0),
    );
    assert!(!has_banner(&rt));
}

// ---- the alert role ----

#[test]
fn the_banner_carries_the_alert_role() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![unnamed_new_row()],
            gate_lost: String::new(),
            dark: true,
        },
        (960.0, 640.0),
    );
    let info = banner(&rt).expect("the banner is mounted");
    assert_eq!(info.role, "alert", "React sets role=alert");
}

// ---- geometry ----

#[test]
fn the_banner_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(
            Scene {
                visible: true,
                rows: vec![unnamed_new_row()],
                gate_lost: "prepare came back with a very long refusal reason".to_owned(),
                dark: true,
            },
            (w, h),
        );
        let [x, y, ew, eh] = banner(&rt).expect("mounted").rect;
        assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h}: banner x={x} w={ew}");
        assert!(y >= -0.5, "banner y={y}");
        assert!(eh > 0.0, "banner has height");
    }
}

#[test]
fn the_light_theme_mounts_the_same_banner() {
    let rt = mount(
        Scene {
            visible: true,
            rows: vec![unnamed_new_row()],
            gate_lost: String::new(),
            dark: false,
        },
        (960.0, 640.0),
    );
    assert!(has_banner(&rt));
    assert_eq!(text_of(&rt), t("transfer.mapping.targetNameRequired"));
}
