//! T002: the single DataTable cell (`components/data_table/cell_renderer.rs`).
//!
//! Probes mount real cells into a Rune runtime and read the accessibility tree
//! (`rt.elements()` / `rt.find()`). Decisions the tree cannot expose — mono vs
//! proportional, right alignment, italic — are pinned through the shared
//! [`plan`] projection that the element builder consumes, so they are the same
//! decisions the renderer makes, not a parallel implementation.
//!
//! Keys: each case is `<key>-value`, or `<key>-editor` while editing; two cells
//! in a row therefore never collide.

use super::*;

use rune::{col, row, Cx, El, Key, KeyEvent, Mods};
use rune_kit::Kit;
use serde_json::{json, Value};

use crate::components::data_table::cell_renderer::{
    plan, CellBranch, CellPresentationVm, CellRenderer, CellRendererVm, PresentationResultVm,
};
use crate::support::cell_editor_kind::CellEditorKind;
use crate::support::cell_types::CellType;
use crate::ui::kit::kit;

#[derive(Clone)]
struct CellCase {
    vm: CellRendererVm,
    editable: bool,
}

#[derive(Default)]
struct Scene {
    cells: Vec<CellCase>,
    dark: bool,
    edits: Vec<String>,
    commits: Vec<Option<Value>>,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let k: &Kit = kit(scene.dark);
    let mut page = col()
        .key("cell-page")
        .w_full()
        .h_full()
        .padding(8.0)
        .fill(k.c.bg_base);
    for cell in &scene.cells {
        let renderer = CellRenderer::<Scene>::new(cell.vm.clone());
        let renderer = if cell.editable {
            let key = cell.vm.key.clone();
            renderer
                .on_edit(move |state: &mut Scene| state.edits.push(key.clone()))
                .on_commit(move |state: &mut Scene, value| state.commits.push(value))
        } else {
            renderer
        };
        page = page.child(
            row()
                .key(format!("{}-row", cell.vm.key))
                .w_full()
                .h(28.0)
                .child(renderer.build(scene.dark)),
        );
    }
    page
}

fn runtime(cells: Vec<CellCase>, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        Scene {
            cells,
            dark,
            ..Default::default()
        },
        view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn case(key: &str, data_type: &str, value: Value) -> CellCase {
    CellCase {
        vm: CellRendererVm::new(key, "col", value).data_type(data_type),
        editable: false,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn press<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn escape<S>(rt: &mut Runtime<S>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
}

#[test]
fn each_family_formats_its_cell_text_in_react_order() {
    let cells = vec![
        case("num", "int", json!(42)),
        case("bool", "boolean", json!(true)),
        case("date", "timestamp", json!("2024-01-15T10:00:00Z")),
        case("json", "jsonb", json!({"b": 1, "a": 2})),
        case("text", "varchar(255)", json!("hello")),
        case("bytes", "bytea", json!("AA==")),
    ];
    let rt = runtime(cells, true, (1440.0, 816.0));

    // React's `String(value)` for bool/number, `formatTimestamp` for the
    // datetime family, `formatCell` for json, and the text fallback for the
    // rest. serde_json sorts object keys, pinned by the support layer.
    assert_eq!(text_of(&rt, "num-value"), "42");
    assert_eq!(text_of(&rt, "bool-value"), "true");
    assert_eq!(text_of(&rt, "date-value"), "2024-01-15T10:00:00.000Z");
    assert_eq!(text_of(&rt, "json-value"), r#"{"a":2,"b":1}"#);
    assert_eq!(text_of(&rt, "text-value"), "hello");
    assert_eq!(text_of(&rt, "bytes-value"), "AA==");
}

#[test]
fn null_and_undefined_are_an_italic_null_that_ignores_the_plan() {
    let mut vm = CellRendererVm::new("nil", "col", Value::Null).data_type("bigint");
    vm = vm.presentation_plan(CellPresentationVm::text("col", "should not show"));
    let p = plan(&vm, true);
    assert_eq!(p.branch, CellBranch::Null);
    assert_eq!(p.text, "NULL");
    assert!(p.italic, "React's NULL span is italic");
    assert!(p.help.is_none(), "a NULL cell has no tooltip / copy source");
    assert!(!p.mono && !p.right, "NULL carries no type style");

    let rt = runtime(
        vec![CellCase {
            vm,
            editable: false,
        }],
        true,
        (960.0, 640.0),
    );
    assert_eq!(text_of(&rt, "nil-value"), "NULL");
    assert!(
        maybe(&rt, "nil-editor").is_none(),
        "NULL never opens an editor"
    );
}

#[test]
fn the_presentation_plan_wins_over_the_family_but_falls_through_on_unchanged() {
    // A BIGINT with a Unix-milliseconds plan renders as the presented text,
    // not a number — React consults the plan after editing/NULL but before
    // the family branches.
    let planned = CellRendererVm::new("planned", "bigint", json!(1_700_000_000_000i64))
        .presentation_plan(CellPresentationVm::new("col", |_| {
            PresentationResultVm::Text("2023-11-14T22:13:20.000Z".to_owned())
        }));
    let p = plan(&planned, true);
    assert_eq!(p.branch, CellBranch::Presented);
    assert_eq!(p.text, "2023-11-14T22:13:20.000Z");
    assert!(p.mono && p.xsmall, "presented text uses the datetime style");
    assert_eq!(
        p.help.as_deref(),
        Some("1700000000000"),
        "the raw value is the tooltip / copy source, never the presented text"
    );

    let unchanged = CellRendererVm::new("unchanged", "col", json!(42))
        .data_type("bigint")
        .presentation_plan(CellPresentationVm::new("col", |_| {
            PresentationResultVm::Unchanged
        }));
    let p = plan(&unchanged, true);
    assert_eq!(p.branch, CellBranch::Number, "unchanged falls through");
    assert_eq!(p.text, "42");

    let rt = runtime(
        vec![
            CellCase {
                vm: planned,
                editable: false,
            },
            CellCase {
                vm: unchanged,
                editable: false,
            },
        ],
        true,
        (1440.0, 816.0),
    );
    assert_eq!(text_of(&rt, "planned-value"), "2023-11-14T22:13:20.000Z");
    assert_eq!(text_of(&rt, "unchanged-value"), "42");
}

#[test]
fn mono_and_right_alignment_follow_the_family() {
    let cases = [
        (CellType::Bool, CellBranch::Bool, true, false, false),
        (CellType::Number, CellBranch::Number, true, false, true),
        (CellType::DateTime, CellBranch::Datetime, true, true, false),
        (CellType::Json, CellBranch::Json, true, true, false),
        (CellType::Text, CellBranch::Text, false, false, false),
    ];
    for (cell_type, branch, mono, xsmall, right) in cases {
        let vm = CellRendererVm::new("c", "col", json!("x")).cell_type(cell_type);
        let p = plan(&vm, true);
        assert_eq!(p.branch, branch, "branch for {cell_type:?}");
        assert_eq!(p.mono, mono, "mono for {cell_type:?}");
        assert_eq!(p.xsmall, xsmall, "text-xs for {cell_type:?}");
        assert_eq!(p.right, right, "text-right for {cell_type:?}");
    }
}

#[test]
fn the_cell_colour_comes_from_the_normalised_type() {
    // cell_value_color is the port of cellValueTextClassFor: a NULL value is
    // always the null family, otherwise the colour follows the CellType. A
    // normalised type and its type name must give the same answer.
    let by_type = plan(
        &CellRendererVm::new("c", "col", json!(42)).cell_type(CellType::Number),
        true,
    )
    .color;
    let by_name = plan(
        &CellRendererVm::new("c", "col", json!(42)).data_type("bigint"),
        true,
    )
    .color;
    assert_eq!(by_type, by_name);

    let null_color = plan(
        &CellRendererVm::new("c", "col", Value::Null).data_type("bigint"),
        true,
    )
    .color;
    assert_ne!(
        null_color, by_name,
        "NULL does not borrow the column colour"
    );
    assert_eq!(
        null_color,
        plan(
            &CellRendererVm::new("c", "col", Value::Null).data_type("text"),
            true
        )
        .color,
        "every NULL paints the same null family"
    );

    // Editing resolves the editor kind from the same normalised type, even for
    // a cell that is not currently editing (React's useMemo runs unconditionally).
    let p = plan(
        &CellRendererVm::new("c", "col", json!("2024-01-15T10:00:00Z")).data_type("timestamptz"),
        true,
    );
    assert_eq!(p.editor_kind, CellEditorKind::Text, "zoned time stays text");
}

#[test]
fn an_editor_kind_override_wins_over_the_type() {
    let mut vm = CellRendererVm::new("fk", "col", json!(7)).data_type("int");
    vm.editor_kind_override = Some(CellEditorKind::Enum);
    assert_eq!(plan(&vm, true).editor_kind, CellEditorKind::Enum);

    let mut long = CellRendererVm::new("long", "col", json!("x".repeat(201))).data_type("text");
    long.editor_kind_override = None;
    assert_eq!(
        plan(&long, true).editor_kind,
        CellEditorKind::LongText,
        "long text uses the long editor (via toEditString + code-point count)"
    );
}

#[test]
fn primary_key_and_total_flags_do_not_change_the_visual_branch() {
    // React's CellRenderer has no branch for these flags: they are reserved
    // metadata. The Rune VM carries them, but the rendered text and branch are
    // identical to a plain cell.
    let plain = CellRendererVm::new("plain", "col", json!(7)).data_type("int");
    let mut marked = CellRendererVm::new("marked", "col", json!(7)).data_type("int");
    marked.is_primary_key = true;
    marked.is_total = true;
    marked.is_summary = true;
    assert_eq!(plan(&plain, true).branch, plan(&marked, true).branch);
    assert_eq!(plan(&plain, true).text, plan(&marked, true).text);

    let rt = runtime(
        vec![
            CellCase {
                vm: plain,
                editable: false,
            },
            CellCase {
                vm: marked,
                editable: false,
            },
        ],
        true,
        (1440.0, 816.0),
    );
    assert_eq!(text_of(&rt, "plain-value"), "7");
    assert_eq!(text_of(&rt, "marked-value"), "7");
}

#[test]
fn a_link_cell_exposes_the_link_affordance() {
    let mut vm = CellRendererVm::new("link", "text", json!("orders/42"));
    vm.is_link = true;
    let rt = runtime(
        vec![CellCase {
            vm,
            editable: false,
        }],
        true,
        (1440.0, 816.0),
    );
    assert_eq!(
        element(&rt, "link-value").role,
        "link",
        "the Rune link affordance is observable in the tree"
    );
    assert_eq!(text_of(&rt, "link-value"), "orders/42");

    let plain = runtime(
        vec![case("plain", "text", json!("orders/42"))],
        true,
        (1440.0, 816.0),
    );
    assert_ne!(element(&plain, "plain-value").role, "link");
}

#[test]
fn read_only_cells_withhold_the_press_to_edit_hook() {
    let mut writable = case("w", "text", json!("value"));
    writable.editable = true;
    let mut readonly = case("r", "text", json!("value"));
    readonly.editable = true;
    readonly.vm = readonly.vm.read_only(true);

    let mut rt = runtime(vec![writable, readonly], true, (1440.0, 816.0));
    press(&mut rt, "w-value");
    press(&mut rt, "r-value");
    assert_eq!(
        rt.state.edits,
        vec!["w".to_owned()],
        "only the writable cell edits"
    );
}

#[test]
fn the_editing_branch_hands_off_to_a_keyed_editor() {
    let mut vm = CellRendererVm::new("edit", "text", json!("seed"));
    vm.is_editing = true;
    let mut case = CellCase { vm, editable: true };
    case.editable = true;
    let mut rt = runtime(vec![case], true, (1440.0, 816.0));

    assert!(
        maybe(&rt, "edit-value").is_none(),
        "the value span is replaced"
    );
    let editor = element(&rt, "edit-editor");
    assert_eq!(
        editor.text.as_deref(),
        Some("seed"),
        "editor starts from raw text"
    );
    assert_eq!(editor.role, "textbox");
    assert_eq!(
        editor.label.as_deref(),
        Some("text"),
        "labelled with the editor kind"
    );

    press(&mut rt, "edit-editor");
    assert_eq!(
        rt.state.commits,
        vec![Some(json!("seed"))],
        "raw value is committed"
    );
}

#[test]
fn long_values_truncate_but_keep_the_full_copy_source() {
    let long = "x".repeat(140);
    let mut vm = CellRendererVm::new("longjson", "jsonb", json!(long.clone()));
    vm.cell_type = Some(CellType::Json);
    let p = plan(&vm, true);
    assert_eq!(
        p.text.chars().count(),
        121,
        "120 code points plus an ellipsis"
    );
    assert!(p.text.ends_with('…'));
    assert_eq!(
        p.help.as_deref(),
        Some(long.as_str()),
        "copy gets the full text"
    );

    let rt = runtime(
        vec![CellCase {
            vm,
            editable: false,
        }],
        true,
        (1440.0, 816.0),
    );
    let shown = text_of(&rt, "longjson-value");
    assert_eq!(shown.chars().count(), 121);
    assert_eq!(
        element(&rt, "longjson-value").help.as_deref(),
        Some(long.as_str())
    );
}

#[test]
fn tooltips_and_copy_sources_follow_the_react_title_branches() {
    // React puts `title` on presented / datetime / json / text cells, and no
    // title on bool / number / NULL. In Rune the title is the element's `help`
    // and doubles as the full copy source.
    let cells = vec![
        case("num", "int", json!(42)),
        case("bool", "boolean", json!(true)),
        case("date", "timestamp", json!("2024-01-15T10:00:00Z")),
        case("json", "jsonb", json!({"k": "v"})),
        case("text", "varchar(255)", json!("hi")),
        case("nil", "int", Value::Null),
    ];
    let rt = runtime(cells, true, (1440.0, 816.0));
    assert!(element(&rt, "num-value").help.is_none());
    assert!(element(&rt, "bool-value").help.is_none());
    assert!(element(&rt, "nil-value").help.is_none());
    assert_eq!(
        element(&rt, "date-value").help.as_deref(),
        Some("2024-01-15T10:00:00Z")
    );
    assert_eq!(
        element(&rt, "json-value").help.as_deref(),
        Some(r#"{"k":"v"}"#)
    );
    assert_eq!(element(&rt, "text-value").help.as_deref(), Some("hi"));
}

#[test]
fn an_editing_cell_labels_its_editor_with_the_resolved_kind() {
    // The kind is resolved from the normalised type even while editing; a long
    // text value uses the long-text editor and its keyed element carries that
    // label. This exercises toEditString + countCodePoints through the handoff.
    let vm = CellRendererVm::new("edit", "col", json!("x".repeat(201)))
        .data_type("text")
        .editing(true);
    assert_eq!(plan(&vm, true).editor_kind, CellEditorKind::LongText);
    let rt = runtime(vec![CellCase { vm, editable: true }], true, (960.0, 640.0));
    assert_eq!(
        element(&rt, "edit-editor").label.as_deref(),
        Some("longText")
    );
}

#[test]
fn two_cells_in_a_row_have_unique_keys() {
    let rt = runtime(
        vec![
            case("c0", "text", json!("a")),
            case("c1", "text", json!("b")),
        ],
        true,
        (1440.0, 816.0),
    );
    let keys: Vec<String> = rt
        .elements()
        .into_iter()
        .filter_map(|element| element.key)
        .filter(|key| key.ends_with("-value"))
        .collect();
    assert_eq!(keys, vec!["c0-value".to_owned(), "c1-value".to_owned()]);
}

#[test]
fn cells_stay_inside_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let long = "a".repeat(400);
            let cells = vec![
                case("n", "bigint", json!(1_700_000_000_000i64)),
                case("b", "boolean", json!(false)),
                case("d", "timestamp", json!("2024-01-15T10:00:00Z")),
                case("j", "jsonb", json!({"key": long.clone()})),
                case("t", "varchar(255)", json!(long.clone())),
                case("nil", "int", Value::Null),
            ];
            let rt = runtime(cells, dark, (width, height));
            let mut outside = Vec::new();
            for element in rt.elements() {
                let Some(key) = element.key else { continue };
                if !key.ends_with("-value") && !key.ends_with("-row") && key != "cell-page" {
                    continue;
                }
                let [x, y, w, h] = element.rect;
                if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
                    outside.push(format!("{key} [{x}, {y}, {w}, {h}]"));
                }
            }
            assert!(
                outside.is_empty(),
                "{width}x{height} dark={dark}: {} nodes leave the window: {outside:?}",
                outside.len()
            );
        }
    }
}

#[test]
fn escape_during_editing_does_not_commit_a_value() {
    let mut vm = CellRendererVm::new("edit", "text", json!("seed"));
    vm.is_editing = true;
    let mut rt = runtime(vec![CellCase { vm, editable: true }], true, (1440.0, 816.0));
    escape(&mut rt);
    assert!(rt.state.commits.is_empty(), "Escape never commits");
}
