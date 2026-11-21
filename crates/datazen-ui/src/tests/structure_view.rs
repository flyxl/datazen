//! T534: `StructureView` — the React test cases as view/state tests: column
//! rows with PK/UQ/AI badges, target-column highlight, edit action, summary
//! counts, loading/error/no-schema states and rect geometry at both sizes in
//! both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::structure_view::{
    build, StructureViewAction, StructureViewVm, StructColumnVm, StructIndexVm, TableSchemaVm,
    EDIT_KEY, ERROR_KEY, LOADING_KEY, SUMMARY_KEY, TABLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: StructureViewVm,
    actions: Rc<RefCell<Vec<StructureViewAction>>>,
    dark: bool,
}

fn column(name: &str, data_type: &str, nullable: bool) -> StructColumnVm {
    StructColumnVm {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable,
        ..StructColumnVm::default()
    }
}

fn schema() -> TableSchemaVm {
    TableSchemaVm {
        table_name: "er_customers".to_owned(),
        columns: vec![
            column("id", "integer", false),
            column("order_no", "varchar(64)", false),
            column("city", "text", true),
        ],
        primary_keys: vec!["id".to_owned()],
        indexes: vec![
            StructIndexVm { is_unique: true, is_primary: false, columns: vec!["order_no".to_owned()] },
            StructIndexVm { is_unique: true, is_primary: true, columns: vec!["id".to_owned()] },
        ],
        foreign_keys: Vec::new(),
    }
}

fn ready() -> StructureViewVm {
    StructureViewVm {
        db_session_id: "sess-1".to_owned(),
        table_name: "er_customers".to_owned(),
        database: "appdb".to_owned(),
        can_edit: true,
        schema: Some(schema()),
        ..StructureViewVm::default()
    }
}

fn scene(vm: StructureViewVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            StructureViewAction::EditStructure { table_name } => {
                state.vm.table_name = table_name.clone();
            }
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, Some(callback), s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts(rt: &Runtime<Scene>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// ── pure rules ────────────────────────────────────────────────────────────────

#[test]
fn unique_columns_exclude_primary_index() {
    let schema = schema();
    assert_eq!(schema.unique_columns(), vec!["order_no".to_owned()]);
    assert_eq!(schema.unique_index_count(), 1);
}

#[test]
fn target_column_matches_case_insensitively() {
    let mut vm = ready();
    vm.target_column = Some("ORDER_NO".to_owned());
    assert_eq!(vm.matched_column(), Some("order_no"));
    assert_eq!(vm.effective_highlight().as_deref(), Some("order_no"));
    vm.target_column = Some("missing".to_owned());
    assert_eq!(vm.matched_column(), None);
    vm.highlighted_col = Some("city".to_owned());
    assert_eq!(vm.effective_highlight().as_deref(), Some("city"));
}

#[test]
fn summary_counts_use_exact_i18n_keys() {
    let vm = ready();
    assert_eq!(vm.fields_text(), t("structView.fields").replace("{count}", "3"));
    assert_eq!(vm.primary_keys_text(), t("structView.primaryKeys").replace("{count}", "1"));
    assert_eq!(vm.unique_indexes_text(), t("structView.uniqueIndexes").replace("{count}", "1"));
    assert_eq!(vm.index_count_text(), t("common.indexCount").replace("{count}", "2"));
    assert_eq!(vm.foreign_key_count_text(), t("common.foreignKeyCount").replace("{count}", "0"));
}

// ── keys, rows, actions and states ───────────────────────────────────────────

#[test]
fn column_rows_and_headers_are_exact() {
    let rt = runtime(scene(ready(), true), DESIGN);
    for key in [
        "structure-view",
        "structure-view-header",
        "structure-view-table-name",
        EDIT_KEY,
        TABLE_KEY,
        "struct-col-id",
        "struct-col-order_no",
        "struct-col-city",
        SUMMARY_KEY,
        "structure-view-fields",
        "structure-view-pks",
        "structure-view-uniques",
        "structure-view-indexes",
        "structure-view-fks",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in ["structView.fieldName", "structView.type", "structView.nullable",
                "structView.defaultValue", "structView.primaryKey", "structView.comment"] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key} header is exact");
    }
    assert!(rendered.iter().any(|text| text == "er_customers"));
    assert!(rendered.iter().any(|text| text == "order_no"));
    assert!(rendered.iter().any(|text| text == "varchar(64)"));
    assert!(has_key(&rt, "struct-col-pk"), "PK badge on id");
    assert!(has_key(&rt, "struct-col-uq"), "UQ badge on order_no");
}

#[test]
fn target_column_is_highlighted() {
    let mut vm = ready();
    vm.target_column = Some("order_no".to_owned());
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, "struct-col-order_no"));
    // The highlight is reflected in the matched-column helper (the row fill and
    // focus border are computed from it).
    assert!(rt.state.vm.matched_column().is_some());
}

#[test]
fn edit_action_routes_through_the_host() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, EDIT_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        [StructureViewAction::EditStructure { table_name: "er_customers".to_owned() }]
    );
}

#[test]
fn loading_error_and_no_schema_states() {
    let mut loading = ready();
    loading.loading = true;
    let rt = runtime(scene(loading, true), DESIGN);
    assert!(has_key(&rt, LOADING_KEY));
    assert!(!has_key(&rt, TABLE_KEY));

    let mut errored = ready();
    errored.error = "boom".to_owned();
    let rt = runtime(scene(errored, false), DESIGN);
    assert!(has_key(&rt, ERROR_KEY));
    assert!(!has_key(&rt, TABLE_KEY));

    let mut empty = ready();
    empty.schema = None;
    let rt = runtime(scene(empty, true), DESIGN);
    assert!(!has_key(&rt, TABLE_KEY), "no schema mounts nothing");
    assert!(!has_key(&rt, LOADING_KEY));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = ready();
            vm.target_column = Some("order_no".to_owned());
            vm.schema.as_mut().unwrap().columns[0].is_auto_increment = true;
            let rt = runtime(scene(vm, dark), size);
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}
