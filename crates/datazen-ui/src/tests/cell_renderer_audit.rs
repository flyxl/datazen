//! Independent audit of T002 (the single DataTable cell).
//!
//! The coder's `tests/cell_renderer.rs` pins the `plan()` projection. This audit
//! deliberately observes the **mounted element** instead wherever the runtime
//! can expose a fact: `ElementInfo.text_color` comes from the paint track (not
//! from the cell's own projection), `role`/`label`/`text`/`help` come from the
//! accessibility tree, and geometry comes from layout rects. Support-layer
//! functions are used as the reference for colours so a local colour literal
//! cannot pass by matching a duplicated constant.
//!
//! Covers the branch order the coder left untested (editing > NULL > plan), the
//! raw-value handoff for non-string values, the "no invented visual branch"
//! rule for reserved flags, unique/stable keys across many cells, source-level
//! support-layer reuse, and rect-based geometry at both window sizes in both
//! themes.

use super::*;

use rune::{col, row, Cx, El};
use rune_kit::Kit;
use serde_json::{json, Value};

use crate::components::data_table::cell_renderer::{
    CellPresentationVm, CellRenderer, CellRendererVm,
};
use crate::support::cell_types::resolve_cell_type;
use crate::support::data_type_colors::{cell_value_color, family_color, DataTypeFamily};
use crate::ui::kit::kit;

#[derive(Clone)]
struct AuditCell {
    vm: CellRendererVm,
    editable: bool,
}

#[derive(Default)]
struct Scene {
    cells: Vec<AuditCell>,
    dark: bool,
    edits: Vec<String>,
    commits: Vec<Option<Value>>,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let k: &Kit = kit(scene.dark);
    let mut page = col()
        .key("audit-page")
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

fn runtime(cells: Vec<AuditCell>, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
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

fn plain(key: &str, data_type: &str, value: Value) -> AuditCell {
    AuditCell {
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

fn press<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn the_painted_text_colour_comes_from_the_support_layer_for_every_family() {
    // `text_color` is read from the runtime's paint track, so this compares the
    // actually painted ink against the support projection — not against the
    // cell's own `plan()` (which is what the coder's test does).
    let specs: [(&str, &str, Value, DataTypeFamily); 7] = [
        ("b", "boolean", json!(true), DataTypeFamily::Bool),
        ("n", "bigint", json!(42), DataTypeFamily::Number),
        (
            "d",
            "timestamp",
            json!("2024-01-15T10:00:00Z"),
            DataTypeFamily::Datetime,
        ),
        ("j", "jsonb", json!({"a": 1}), DataTypeFamily::Json),
        ("t", "varchar(255)", json!("hi"), DataTypeFamily::Text),
        ("x", "bytea", json!("AA=="), DataTypeFamily::Binary),
        ("nil", "int", Value::Null, DataTypeFamily::Null),
    ];
    let cells = specs
        .iter()
        .map(|(key, dt, value, _)| plain(key, dt, value.clone()))
        .collect();
    let rt = runtime(cells, true, (1440.0, 816.0));

    for (key, dt, value, family) in specs {
        let expected = cell_value_color(resolve_cell_type(Some(dt), None), &value, true);
        let painted = element(&rt, &format!("{key}-value")).text_color;
        assert_eq!(
            painted,
            Some(expected),
            "{key} painted ink must be the support-layer colour"
        );
        // A non-NULL value borrows its column family token; only NULL uses the
        // dedicated null family.
        if !value.is_null() {
            assert_eq!(expected, family_color(family, true), "{key} family token");
        } else {
            assert_eq!(expected, family_color(DataTypeFamily::Null, true));
        }
    }
}

#[test]
fn editing_precedes_the_null_branch_and_shows_an_empty_editor() {
    // React: `if (isEditing)` returns before `if (value == null)`. A NULL cell
    // that is being edited must therefore mount the editor, not a NULL span.
    let mut vm = CellRendererVm::new("enull", "int", Value::Null);
    vm.is_editing = true;
    let rt = runtime(
        vec![AuditCell {
            vm,
            editable: true,
        }],
        true,
        (1440.0, 816.0),
    );
    let editor = element(&rt, "enull-editor");
    assert_eq!(editor.role, "textbox");
    assert_eq!(editor.text.as_deref(), Some(""), "toEditString(null) is empty");
    assert!(maybe(&rt, "enull-value").is_none(), "no NULL span while editing");
    assert_ne!(editor.text.as_deref(), Some("NULL"));
}

#[test]
fn editing_precedes_the_presentation_plan() {
    // React consults the plan only after the editing early return.
    let vm = CellRendererVm::new("eplan", "bigint", json!(1_700_000_000_000i64))
        .editing(true)
        .presentation_plan(CellPresentationVm::text("col", "PRESENTED"));
    let rt = runtime(
        vec![AuditCell {
            vm,
            editable: true,
        }],
        true,
        (1440.0, 816.0),
    );
    let editor = element(&rt, "eplan-editor");
    assert_eq!(
        editor.text.as_deref(),
        Some("1700000000000"),
        "editor seeds from the raw value, never the presented text"
    );
    assert!(maybe(&rt, "eplan-value").is_none());
}

#[test]
fn the_editor_commits_the_raw_value_not_its_string_projection() {
    let cells = vec![
        AuditCell {
            vm: CellRendererVm::new("num", "int", json!(42)).editing(true),
            editable: true,
        },
        AuditCell {
            vm: CellRendererVm::new("boo", "boolean", json!(true)).editing(true),
            editable: true,
        },
        AuditCell {
            vm: CellRendererVm::new("obj", "jsonb", json!({"b": 1, "a": 2})).editing(true),
            editable: true,
        },
    ];
    let mut rt = runtime(cells, true, (1440.0, 816.0));
    press(&mut rt, "num-editor");
    press(&mut rt, "boo-editor");
    press(&mut rt, "obj-editor");
    assert_eq!(
        rt.state.commits,
        vec![Some(json!(42)), Some(json!(true)), Some(json!({"a": 2, "b": 1}))],
        "on_commit receives the raw Value (React's originalValue), not the seed text"
    );
}

#[test]
fn reserved_flags_do_not_invent_any_visual_branch() {
    // PK / total / summary are carried on the VM but React has no branch for
    // them. Compare two otherwise identical cells in separate runtimes so even
    // the rect must match (same row slot), catching any newly added styling.
    let plain_vm = CellRendererVm::new("c", "int", json!(7)).data_type("int");
    let mut marked = CellRendererVm::new("c", "int", json!(7)).data_type("int");
    marked.is_primary_key = true;
    marked.is_total = true;
    marked.is_summary = true;

    let a = runtime(
        vec![AuditCell {
            vm: plain_vm,
            editable: false,
        }],
        true,
        (1440.0, 816.0),
    );
    let b = runtime(
        vec![AuditCell {
            vm: marked,
            editable: false,
        }],
        true,
        (1440.0, 816.0),
    );
    let pa = element(&a, "c-value");
    let pb = element(&b, "c-value");
    assert_eq!(pa.role, pb.role, "no link/textbox role invented");
    assert_eq!(pa.label, pb.label);
    assert_eq!(pa.text, pb.text);
    assert_eq!(pa.help, pb.help);
    assert_eq!(pa.text_color, pb.text_color, "no colour branch invented");
    assert_eq!(pa.rect, pb.rect, "no geometry branch invented");
    assert_eq!(pa.focusable, pb.focusable);
}

#[test]
fn link_cells_are_observable_as_links_and_withhold_nothing_else() {
    // Same cell rendered twice in separate runtimes; the only difference is the
    // `is_link` flag, so the role is the only observable change.
    let plain_rt = runtime(
        vec![plain("l", "text", json!("orders/42"))],
        true,
        (1440.0, 816.0),
    );
    let mut linked = CellRendererVm::new("l", "text", json!("orders/42"));
    linked = linked.link(true);
    let link_rt = runtime(
        vec![AuditCell {
            vm: linked,
            editable: false,
        }],
        true,
        (1440.0, 816.0),
    );
    assert_eq!(element(&plain_rt, "l-value").role, "text");
    assert_eq!(element(&link_rt, "l-value").role, "link");
    assert_eq!(element(&link_rt, "l-value").text.as_deref(), Some("orders/42"));
    assert_eq!(
        element(&link_rt, "l-value").help,
        element(&plain_rt, "l-value").help,
        "the link affordance does not change the copy source"
    );
}

#[test]
fn many_mixed_cells_have_unique_stable_keys() {
    let mut cells = Vec::new();
    for i in 0..12 {
        let mut vm = CellRendererVm::new(format!("r{i}c{i}"), "int", json!(i));
        if i % 3 == 0 {
            vm.is_editing = true;
        }
        cells.push(AuditCell {
            vm,
            editable: true,
        });
    }
    let mut rt = runtime(cells, true, (1440.0, 816.0));
    let mounted = |rt: &Runtime<Scene>| -> Vec<String> {
        let mut keys: Vec<String> = rt
            .elements()
            .into_iter()
            .filter_map(|e| e.key)
            .filter(|k| k.ends_with("-value") || k.ends_with("-editor"))
            .collect();
        keys.sort();
        keys
    };
    let first = mounted(&rt);
    let mut expected: Vec<String> = (0..12)
        .map(|i| {
            if i % 3 == 0 {
                format!("r{i}c{i}-editor")
            } else {
                format!("r{i}c{i}-value")
            }
        })
        .collect();
    expected.sort();
    assert_eq!(first, expected, "each cell owns exactly one keyed element");

    let mut deduped = first.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), first.len(), "no two cells share an element key");

    rt.frame(0.0);
    assert_eq!(mounted(&rt), first, "keys are stable across frames");
}

#[test]
fn editing_link_and_long_cells_fit_both_windows_in_both_themes() {
    // Rect-based observation straight from layout (the runtime reports the
    // geometry, not the cell). Includes an editing cell and a link cell, which
    // the coder's geometry case did not cover.
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let long = "m".repeat(500);
            let cells = vec![
                AuditCell {
                    vm: CellRendererVm::new("e", "text", json!("seed")).editing(true),
                    editable: true,
                },
                {
                    let mut vm = CellRendererVm::new("l", "text", json!("orders/42"));
                    vm = vm.link(true);
                    AuditCell {
                        vm,
                        editable: false,
                    }
                },
                plain("j", "jsonb", json!({"k": long.clone()})),
                plain("t", "varchar(255)", json!(long)),
                plain("nil", "int", Value::Null),
            ];
            let rt = runtime(cells, dark, (width, height));
            let mut outside = Vec::new();
            for e in rt.elements() {
                let Some(key) = e.key else { continue };
                if !(key.ends_with("-value")
                    || key.ends_with("-editor")
                    || key.ends_with("-row")
                    || key == "audit-page")
                {
                    continue;
                }
                let [x, y, w, h] = e.rect;
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
fn the_port_reuses_the_support_layer_and_has_no_raw_colour_literals() {
    let src = include_str!("../components/data_table/cell_renderer.rs");
    for literal in [
        "oklch(",
        "oklcha(",
        "Color::rgb",
        "Color::rgba",
        "Color::from",
        "#fff",
        "#000",
    ] {
        assert!(
            !src.contains(literal),
            "raw colour literal `{literal}` leaked into the port"
        );
    }
    for helper in [
        "format_timestamp(",
        "format_cell(",
        "to_edit_string(",
        "count_code_points(",
        "resolve_cell_editor_kind(",
        "cell_value_color(",
        "cell_type_to_data_type_family(",
    ] {
        assert!(src.contains(helper), "support helper `{helper}` is not reused");
    }
    // The presentation VM is a two-result boundary, not a half-implemented
    // valuePresentation port: its only result kinds are text / unchanged.
    assert!(src.contains("PresentationResultVm::Text"));
    assert!(src.contains("    Unchanged,"), "the VM exposes the unchanged kind");
    assert!(
        src.contains("rune_kit::Kit") && src.contains("crate::ui::kit::kit"),
        "colours and type styles must come from the kit token source"
    );
}
