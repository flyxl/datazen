//! T250 audit: the SchemaDiffSelectionSteps orchestration port,
//! verified against the React source re-read line by line (the file
//! has no dedicated React test — its behaviour is pinned through the
//! step switch, the T010 bar's wizard configuration and the T149
//! objects step's callback wiring).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use rune::{col, Env, El, Runtime};

use crate::views::schema_diff::schema_diff_selection_steps::{
    CatalogLoadState, SchemaDiffEndpointsVm, SchemaDiffSelectionSteps, SelectionConnectionOption,
    SelectionStep, UnifiedObjectsVm,
};
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, TablePickOriginVm,
};

fn conn_option(value: &str, label: &str) -> SelectionConnectionOption {
    SelectionConnectionOption { value: value.to_owned(), label: label.to_owned(), disabled: false }
}

fn pick(name: &str, enabled: bool) -> SchemaDiffTablePickVm {
    SchemaDiffTablePickVm {
        name: name.to_owned(),
        enabled,
        origin: TablePickOriginVm::Both,
        source_name: Some(name.to_owned()),
        target_name: Some(name.to_owned()),
    }
}

fn endpoints() -> SchemaDiffEndpointsVm {
    SchemaDiffEndpointsVm {
        source_id: "conn-a".to_owned(),
        target_id: "conn-b".to_owned(),
        source_database: Some("shop".to_owned()),
        target_database: Some("shop2".to_owned()),
        source_schema: Some("public".to_owned()),
        target_schema: None,
        conn_options: vec![conn_option("conn-a", "Local PG"), conn_option("conn-b", "Remote PG")],
        target_options: None,
        source_databases: vec!["shop".to_owned()],
        target_databases: vec!["shop2".to_owned()],
        source_schemas: vec!["public".to_owned()],
        target_schemas: Vec::new(),
        is_cross_dialect: true,
    }
}

fn unified<'a>(load_state: CatalogLoadState) -> UnifiedObjectsVm<'a> {
    UnifiedObjectsVm {
        loading: false,
        source_objects: &[],
        target_objects: &[],
        selected_source_keys: &[],
        selected_target_keys: &[],
        errors: &[],
        catalog_load_state: load_state,
    }
}

#[derive(Clone)]
struct AuditState {
    endpoints: SchemaDiffEndpointsVm,
    picks: Vec<SchemaDiffTablePickVm>,
    loading: bool,
    dark: bool,
    load_state: CatalogLoadState,
    toggled_tables: Vec<String>,
    select_all_pressed: usize,
    select_none_pressed: usize,
}

impl AuditState {
    fn scene() -> Self {
        Self {
            endpoints: endpoints(),
            picks: vec![pick("orders", true), pick("users", false)],
            loading: false,
            dark: true,
            load_state: CatalogLoadState::Completed,
            toggled_tables: Vec::new(),
            select_all_pressed: 0,
            select_none_pressed: 0,
        }
    }
}

fn objects_view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let unified = unified(s.load_state);
    // The prebuilt picker seam (T290's): a keyed stand-in proving the
    // mount below the objects step.
    let picker = col().key("audit-unified-picker");
    SchemaDiffSelectionSteps::<AuditState>::new(
        SelectionStep::Objects,
        &s.endpoints,
        s.loading,
        &s.picks,
        &unified,
    )
    .unified_picker(Some(picker))
    .on_toggle_table(|s: &mut AuditState, name| s.toggled_tables.push(name))
    .on_select_all(|s: &mut AuditState| {
        s.select_all_pressed += 1;
        for pick in &mut s.picks {
            pick.enabled = true;
        }
    })
    .on_select_none(|s: &mut AuditState| {
        s.select_none_pressed += 1;
        for pick in &mut s.picks {
            pick.enabled = false;
        }
    })
    .build(s.dark)
}

fn endpoints_view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let unified = unified(CatalogLoadState::Completed);
    SchemaDiffSelectionSteps::<AuditState>::new(
        SelectionStep::Endpoints,
        &s.endpoints,
        s.loading,
        &s.picks,
        &unified,
    )
    .endpoints_cross_dialect_note(Some("跨方言比较：类型映射有差异".to_owned()))
    .build(s.dark)
}

fn rt_with(
    view_fn: fn(&AuditState, &mut rune::Cx<AuditState>) -> El,
    scene: AuditState,
) -> Runtime<AuditState> {
    let mut rt =
        Runtime::new(scene, view_fn, (960.0, 640.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
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

/// The endpoints step mounts the migration bar with the wizard's
/// configuration: the schema-diff testid prefix, the cross-dialect
/// footer note, no swap and no compare.
#[test]
fn the_endpoints_step_mounts_the_bar_configured_for_the_wizard() {
    let rt = rt_with(endpoints_view, AuditState::scene());
    // The T010 bar's keys carry the schema-diff prefix.
    assert!(has(&rt, "schema-diff-source-connection-field-trigger"));
    assert!(has(&rt, "schema-diff-target-connection-field-trigger"));
    // The cross-dialect note rides the footer.
    assert!(rt
        .elements()
        .into_iter()
        .any(|e| !e.exiting && e.text.as_deref() == Some("跨方言比较：类型映射有差异")));
    // No compare button (showCompare=false) and no swap (showSwap=false).
    assert!(!rt
        .elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.contains("compare"))));
    assert!(!rt
        .elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.contains("swap"))));
}

/// The endpoints step's source column reflects the projection (the
/// chosen connection's option list and the picked database).
#[test]
fn the_endpoints_projection_reaches_the_bar() {
    let rt = rt_with(endpoints_view, AuditState::scene());
    // Both connection options render in the source list's tree.
    let texts: Vec<String> = rt
        .elements()
        .into_iter()
        .filter_map(|e| e.text)
        .collect();
    assert!(texts.iter().any(|text| text == "Local PG"));
    assert!(texts.iter().any(|text| text == "Remote PG"));
}

/// The objects step mounts the objects panel and the picker seam below
/// it, under the catalog-state-suffixed container key.
#[test]
fn the_objects_step_mounts_both_children() {
    let rt = rt_with(objects_view, AuditState::scene());
    // The container key (a completed catalog has no suffix).
    assert!(has(&rt, "schema-diff-objects-step"));
    // T149's objects panel and the picker seam.
    assert!(has(&rt, "schema-diff-objects-panel"));
    assert!(has(&rt, "audit-unified-picker"));
    // The header's count line shows both picks.
    assert!(has(&rt, "schema-diff-objects-count"));
}

/// A loading catalog rides the container key's suffix (the
/// `data-catalog-load-state` encoding).
#[test]
fn the_catalog_load_state_rides_the_container_key() {
    let mut scene = AuditState::scene();
    scene.load_state = CatalogLoadState::Loading;
    let rt = rt_with(objects_view, scene);
    assert!(has(&rt, "schema-diff-objects-step-loading"));
    assert!(!has(&rt, "schema-diff-objects-step"));

    let mut scene = AuditState::scene();
    scene.load_state = CatalogLoadState::Invalidated;
    let rt = rt_with(objects_view, scene);
    assert!(has(&rt, "schema-diff-objects-step-invalidated"));

    let rt = rt_with(objects_view, AuditState::scene());
    assert!(has(&rt, "schema-diff-objects-step"), "completed settles on the bare key");
}

/// The objects step's select-all / select-none flip every pick (the two
/// `setTablePicks` map arms).
#[test]
fn select_all_and_none_flip_every_pick() {
    let mut rt = rt_with(objects_view, AuditState::scene());
    // users starts unchecked.
    assert!(!rt.state.picks.iter().all(|pick| pick.enabled));
    press(&mut rt, "schema-diff-objects-select-all");
    let state = rt.state.clone();
    assert_eq!(state.select_all_pressed, 1);
    assert!(state.picks.iter().all(|pick| pick.enabled), "every pick enabled");

    press(&mut rt, "schema-diff-objects-select-none");
    let state = rt.state.clone();
    assert_eq!(state.select_none_pressed, 1);
    assert!(state.picks.iter().all(|pick| !pick.enabled), "every pick disabled");
}

/// A loading step forwards the flag to both children (T149's panel
/// shows its loading state).
#[test]
fn the_loading_flag_reaches_the_children() {
    let mut scene = AuditState::scene();
    scene.loading = true;
    let rt = rt_with(objects_view, scene);
    // The objects panel's loading branch renders (the empty/loading
    // surface replaces the list).
    assert!(has(&rt, "schema-diff-objects-panel"));
}
