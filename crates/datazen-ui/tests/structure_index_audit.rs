//! Independent audit for T532: `StructureIndexTable` + `StructurePlanPreview`.
//!
//! Cover the pure helpers and capability gates the coder does not drive, and
//! pin the React contract for controls disabled by driver capabilities.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::SortDir;

use datazen_ui::i18n::t;
use datazen_ui::structure_editor::{PlanStatement, StatementRisk, StructureCapabilities, StructureChangePlan, StructureIndexDraft};
use datazen_ui::views::structure_index_table::{
    build, suggested_index_name, IndexPatch, StructureIndexAction, StructureIndexActionFn,
    StructureIndexTableVm,
};
use datazen_ui::views::structure_plan_preview::StructurePlanPreviewVm;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: StructureIndexTableVm,
    actions: Rc<RefCell<Vec<StructureIndexAction>>>,
    dark: bool,
}

fn caps(create: bool, drop: bool, index_type: bool) -> StructureCapabilities {
    StructureCapabilities { create_index: create, drop_index: drop, index_type, ..Default::default() }
}

fn draft(id: &str, name: &str, columns: &[&str], index_type: Option<&str>) -> StructureIndexDraft {
    StructureIndexDraft {
        id: id.to_owned(),
        name: name.to_owned(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        is_unique: false,
        is_primary: false,
        index_type: index_type.map(str::to_owned),
        include_columns: None,
        filter: None,
        comment: None,
    }
}

fn vm() -> StructureIndexTableVm {
    StructureIndexTableVm {
        caps: Some(caps(true, true, true)),
        index_methods: vec!["btree".to_owned(), "hash".to_owned()],
        column_names: vec!["id".to_owned(), "email".to_owned()],
        table_name: "users".to_owned(),
        indexes: vec![
            draft("a", "z_name", &["email"], None),
            draft("b", "a_name", &["id"], Some("hash")),
        ],
        ..Default::default()
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let on_action: Option<StructureIndexActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    build(&s.vm, &on_action, s.dark)
}

fn runtime(vm: StructureIndexTableVm, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("audit: {key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn suggested_name_trims_whitespace_table_name() {
    assert_eq!(suggested_index_name("   ", &[]), "idx_table", "audit: blank table name falls back to table");
    assert_eq!(suggested_index_name("  demo  ", &["a".to_owned()]), "idx_demo_a", "audit: table name is trimmed");
}

#[test]
fn row_order_sorts_every_column_in_both_directions() {
    let mut s = vm();
    s.sort = Some((0, SortDir::Ascending));
    assert_eq!(s.row_order(), vec![1, 0], "audit: name ascending");
    s.sort = Some((0, SortDir::Descending));
    assert_eq!(s.row_order(), vec![0, 1], "audit: name descending");
    s.sort = Some((1, SortDir::Ascending));
    assert_eq!(s.row_order(), vec![0, 1], "audit: columns ascending");
    s.sort = Some((1, SortDir::Descending));
    assert_eq!(s.row_order(), vec![1, 0], "audit: columns descending");
    s.sort = Some((2, SortDir::Ascending));
    assert_eq!(s.row_order(), vec![0, 1], "audit: index type ascending");
}

#[test]
fn column_patch_keeps_only_an_auto_generated_name() {
    let s = vm();
    let mut auto = s.indexes[0].clone();
    auto.name = suggested_index_name(&s.table_name, &auto.columns); // auto-generated name
    let patches = s.column_patch(&auto, vec!["id".to_owned()]);
    assert_eq!(patches[0], IndexPatch::Columns(vec!["id".to_owned()]));
    assert_eq!(patches[1], IndexPatch::Name("idx_users_id".to_owned()), "audit: an unchanged auto name is refreshed");

    let mut custom = auto.clone();
    custom.name = "my_custom".to_owned();
    let patches = s.column_patch(&custom, vec!["id".to_owned()]);
    assert_eq!(patches, vec![IndexPatch::Columns(vec!["id".to_owned()])], "audit: a custom name is preserved");

    let mut blank = auto.clone();
    blank.name.clear();
    let patches = s.column_patch(&blank, vec!["id".to_owned()]);
    assert_eq!(patches[1], IndexPatch::Name("idx_users_id".to_owned()), "audit: a blank name is refreshed");
}

#[test]
fn plan_preview_empty_warnings_and_all_risk_labels() {
    let mut p = StructurePlanPreviewVm::default();
    p.plan.warnings = Some(Vec::new());
    assert!(!p.has_warnings(), "audit: an empty warnings list is not a warnings block");
    for risk in [StatementRisk::Additive, StatementRisk::Destructive, StatementRisk::Rewrite] {
        let p = StructurePlanPreviewVm {
            plan: StructureChangePlan {
                statements: vec![PlanStatement { sql: "SELECT 1".to_owned(), summary: "s".to_owned(), risk }],
                warnings: None,
            },
            loading: false,
        };
        assert_eq!(p.count_text(), format!("1 {}", t("structEditor.statements")));
        assert!(p.plan.statements[0].risk.as_str().len() > 0, "audit: risk label is a non-empty string");
    }
}

#[test]
fn all_capabilities_off_keeps_rows_in_bounds_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut s = vm();
            s.caps = Some(caps(false, false, false));
            let rt = runtime(s, dark, (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
            }
        }
    }
}

// ----- React parity assertions for capability-gated controls -----

#[test]
fn create_index_capability_on_keeps_the_name_field_editable() {
    let rt = runtime(vm(), true, DESIGN);
    assert!(
        element(&rt, "structure-index-name-a").focusable,
        "DEFECT: React's name Input is editable when createIndex capability is on"
    );
}

#[test]
fn create_index_capability_off_blocks_index_type_action() {
    let mut s = vm();
    s.caps = Some(caps(false, true, true));
    let mut rt = runtime(s, false, MIN);
    click(&mut rt, "structure-index-method-a");
    assert!(
        !rt.state.actions.borrow().iter().any(|a| matches!(a, StructureIndexAction::ToggleMethod(_))),
        "DEFECT: React disables the index-type Select when createIndex capability is off"
    );
}
