//! T290 audit: the unified objects picker, verified against the React
//! source re-read line by line (the picker's React coverage rides the
//! SchemaDiffWizard journeys; the rules below are re-derived from its
//! own body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use std::collections::BTreeSet;

use rune::{col, Env, El, Runtime};

use datazen_ui::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm;
use datazen_ui::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectKindVm;
use datazen_ui::views::schema_diff::schema_diff_unified_objects_picker::{
    display_object_identity, SchemaDiffUnifiedObjectsPicker, PickerSide, OBJECT_KINDS,
};

fn object(kind: SchemaDiffObjectKindVm, schema: Option<&str>, name: &str) -> SchemaDiffObjectIdentityVm {
    SchemaDiffObjectIdentityVm {
        kind,
        schema: schema.map(str::to_owned),
        name: name.to_owned(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

#[derive(Clone)]
struct AuditState {
    loading: bool,
    source: Vec<SchemaDiffObjectIdentityVm>,
    target: Vec<SchemaDiffObjectIdentityVm>,
    selected_source: Vec<String>,
    selected_target: Vec<String>,
    errors: Vec<(SchemaDiffObjectKindVm, String)>,
    cross_dialect: bool,
    dark: bool,
    toggles: Vec<(PickerSide, String)>,
    select_alls: Vec<(PickerSide, bool)>,
    clears: usize,
    retries: usize,
}

impl AuditState {
    fn scene() -> Self {
        Self {
            loading: false,
            source: vec![
                object(SchemaDiffObjectKindVm::View, Some("app"), "v_orders"),
                object(SchemaDiffObjectKindVm::View, None, "v_global"),
                object(SchemaDiffObjectKindVm::Function, Some("app"), "calc_total"),
            ],
            target: vec![object(SchemaDiffObjectKindVm::View, Some("app"), "v_orders")],
            selected_source: Vec::new(),
            selected_target: Vec::new(),
            errors: Vec::new(),
            cross_dialect: false,
            dark: true,
            toggles: Vec::new(),
            select_alls: Vec::new(),
            clears: 0,
            retries: 0,
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let empty: Vec<(SchemaDiffObjectKindVm, String)> = Vec::new();
    let target_errors = &empty;
    let source_errors = &s.errors;
    SchemaDiffUnifiedObjectsPicker::<AuditState>::new(
        s.loading,
        &s.source,
        &s.target,
        &s.selected_source,
        &s.selected_target,
        source_errors,
        target_errors,
        s.cross_dialect,
    )
    .on_toggle_source(|s: &mut AuditState, object| {
        s.toggles.push((PickerSide::Source, object.name.clone()))
    })
    .on_toggle_target(|s: &mut AuditState, object| {
        s.toggles.push((PickerSide::Target, object.name.clone()))
    })
    .on_select_all(|s: &mut AuditState, side, selected| {
        s.select_alls.push((side, selected))
    })
    .on_clear_selections(|s: &mut AuditState| s.clears += 1)
    .on_retry(|s: &mut AuditState| s.retries += 1)
    .build(s.dark)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt =
        Runtime::new(scene, view, (1280.0, 720.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

/// The identity label: each optional half collapses out.
#[test]
fn display_identity_collapses_optionals() {
    let mut obj = object(SchemaDiffObjectKindVm::View, Some("app"), "v_orders");
    assert_eq!(display_object_identity(&obj), "app.v_orders");
    // No schema: the bare name.
    obj.schema = None;
    assert_eq!(display_object_identity(&obj), "v_orders");
    // A signature renders parenthesised.
    obj.signature = Some("integer,integer".to_owned());
    assert_eq!(display_object_identity(&obj), "v_orders(integer,integer)");
    // A rename target renders after the arrow.
    obj.target_schema = Some("archive".to_owned());
    obj.target_name = Some("v_orders_old".to_owned());
    assert_eq!(display_object_identity(&obj), "v_orders(integer,integer) → archive.v_orders_old");
    // Target without a schema.
    obj.target_schema = None;
    assert_eq!(display_object_identity(&obj), "v_orders(integer,integer) → v_orders_old");
}

/// The kind table is the React declaration order.
#[test]
fn the_kind_table_is_the_react_order() {
    let ids: Vec<&str> = OBJECT_KINDS.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(ids, vec!["view", "type", "sequence", "function", "procedure", "trigger"]);
}

/// The mount: the section, both sides' kind blocks (only kinds with
/// rows), and the rows themselves.
#[test]
fn the_picker_mounts_its_kind_blocks_and_rows() {
    let rt = rt(AuditState::scene());
    assert!(has(&rt, "schema-diff-unified-object-picker"));
    // Source: the view kind (2 rows) and the function kind; the empty
    // kinds (type/sequence/procedure/trigger) render nothing.
    assert!(has(&rt, "schema-diff-object-kind-source-view"));
    assert!(has(&rt, "schema-diff-object-kind-source-function"));
    assert!(!has(&rt, "schema-diff-object-kind-source-type"));
    assert!(has(&rt, "schema-diff-object-row-source-view-0"));
    assert!(has(&rt, "schema-diff-object-row-source-view-1"));
    assert!(has(&rt, "schema-diff-object-row-source-function-0"));
    // Target: one view row.
    assert!(has(&rt, "schema-diff-object-kind-target-view"));
    assert!(has(&rt, "schema-diff-object-row-target-view-0"));
    // No retry without errors.
    assert!(!has(&rt, "schema-diff-object-retry"));
    // No cross-dialect note.
    assert!(!has(&rt, "schema-diff-cross-dialect-objects-note"));
}

/// A row's toggle press reaches the host with the side and the object.
#[test]
fn a_row_toggle_reaches_the_host() {
    let mut rt = rt(AuditState::scene());
    press(&mut rt, "schema-diff-object-row-source-view-0");
    let state = rt.state.clone();
    assert_eq!(
        state.toggles,
        vec![(PickerSide::Source, "v_orders".to_owned())]
    );
    press(&mut rt, "schema-diff-object-row-target-view-0");
    assert_eq!(
        rt.state.toggles.last(),
        Some(&(PickerSide::Target, "v_orders".to_owned()))
    );
}

/// The select-all/none row: both sides, both directions.
#[test]
fn the_select_all_none_row_fires() {
    let mut rt = rt(AuditState::scene());
    press(&mut rt, "schema-diff-object-select-all-source");
    press(&mut rt, "schema-diff-object-select-none-source");
    press(&mut rt, "schema-diff-object-select-all-target");
    press(&mut rt, "schema-diff-object-select-none-target");
    assert_eq!(
        rt.state.select_alls,
        vec![
            (PickerSide::Source, true),
            (PickerSide::Source, false),
            (PickerSide::Target, true),
            (PickerSide::Target, false),
        ]
    );
}

/// A kind's load error renders its line (and the retry appears).
#[test]
fn a_kind_error_renders_and_the_retry_appears() {
    let mut scene = AuditState::scene();
    scene.errors
        .push((SchemaDiffObjectKindVm::Sequence, "permission denied".to_owned()));
    let rt = rt(scene);
    assert!(has(&rt, "schema-diff-object-error-source-sequence"));
    assert!(text_of(&rt, "schema-diff-object-error-source-sequence").contains("permission denied"));
    assert!(has(&rt, "schema-diff-object-retry"));
    assert!(has(&rt, "schema-diff-object-partial-load"));
    // The errored kind block mounts even with no rows.
    assert!(has(&rt, "schema-diff-object-kind-source-sequence"));
}

/// The retry press reaches the host.
#[test]
fn the_retry_fires() {
    let mut scene = AuditState::scene();
    scene.errors
        .push((SchemaDiffObjectKindVm::Trigger, "catalog busy".to_owned()));
    let mut rt = rt(scene);
    press(&mut rt, "schema-diff-object-retry");
    assert_eq!(rt.state.retries, 1);
}

/// The loading state replaces both lists.
#[test]
fn loading_replaces_the_lists() {
    let mut scene = AuditState::scene();
    scene.loading = true;
    let rt = rt(scene);
    assert!(has(&rt, "schema-diff-unified-object-picker-loading"));
    assert!(!has(&rt, "schema-diff-object-kind-source-view"));
}

/// The cross-dialect note with the clear-selections escape (only when
/// something is selected).
#[test]
fn the_cross_dialect_note_and_clear_escape() {
    {
        let mut scene = AuditState::scene();
        scene.cross_dialect = true;
        let rt = rt(scene);
        assert!(has(&rt, "schema-diff-cross-dialect-objects-note"));
        assert!(!has(&rt, "schema-diff-clear-object-selections"), "nothing selected, no escape");
    }

    {
        let mut scene = AuditState::scene();
        scene.cross_dialect = true;
        scene.selected_source = vec!["app.v_orders".to_owned()];
        let mut rt = rt(scene);
        assert!(has(&rt, "schema-diff-clear-object-selections"));
        press(&mut rt, "schema-diff-clear-object-selections");
        assert_eq!(rt.state.clears, 1);
    }
}
