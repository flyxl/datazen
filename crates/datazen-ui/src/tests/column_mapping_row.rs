//! T023-S1: `ColumnMappingRow` — the leaf row of the data-transfer
//! column-mapping table (`ColumnMappingEditor.tsx:442-524`).
//!
//! Probes the rendered element tree (keys, fixed widths, unmapped wash,
//! two-theme/two-size geometry), drives the real kit controls through
//! pointer/keyboard journeys (skip checkbox, target select, both text inputs
//! via paste + Enter), and checks the pure helpers (`target_options`,
//! `is_unmapped`, `ColumnMappingPatch::apply_to`) against the React source.
//! Every assertion observes the runtime tree or the probe's recorded patches,
//! not the component's own report.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::{has, t};
use crate::ui::kit::kit;
use crate::views::{
    is_unmapped, target_options, ColumnMappingPatch, ColumnMappingRow, ColumnMappingRowVm,
    TargetOption, TransferColumnMapping,
};

const ROOT: &str = "data-transfer-column-row";
const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

/// Host state: the VM snapshot, the two `TextEdit` buffers the row edits
/// into, and every patch the row commits. Callbacks write values back into
/// the VM, exactly as the editor shell (T023-S2) will.
#[derive(Clone)]
struct Probe {
    vm: ColumnMappingRowVm,
    dark: bool,
    target_column_edit: TextEdit,
    target_native_type_edit: TextEdit,
    patches: Vec<ColumnMappingPatch>,
    toggles: usize,
    closes: usize,
}

fn row(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

/// A mapped select-branch row: `id -> id`, types shown.
fn seeded() -> Probe {
    let mut vm = ColumnMappingRowVm::default();
    vm.row = row("id", "id", false);
    vm.show_target_type = true;
    vm.source_type = Some("INTEGER".to_owned());
    vm.row.target_native_type = Some("BIGINT".to_owned());
    vm.target_options = target_options(
        &["id".to_owned(), "name".to_owned(), "email".to_owned()],
        "",
        t("transfer.mapping.pickTargetColumn"),
    );
    Probe {
        vm,
        dark: true,
        target_column_edit: TextEdit::new("id"),
        target_native_type_edit: TextEdit::new("BIGINT"),
        patches: Vec::new(),
        toggles: 0,
        closes: 0,
    }
}

/// An unmapped create-new row: no target yet, so the target input is empty.
fn seeded_create_new() -> Probe {
    let mut p = seeded();
    p.vm.row = row("email", "", false);
    p.vm.create_new = true;
    p.vm.row.target_native_type = None;
    p.target_column_edit = TextEdit::new(String::new());
    p.target_native_type_edit = TextEdit::new(String::new());
    p
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let row = ColumnMappingRow::<Probe>::new(ROOT, s.vm.clone())
        .target_column_edit(&s.target_column_edit)
        .target_native_type_edit(&s.target_native_type_edit)
        .on_change(|st: &mut Probe, patch| {
            patch.apply_to(&mut st.vm.row);
            st.vm.target_select_open = false;
            st.patches.push(patch);
        })
        .on_target_column_edit(|st: &mut Probe| &mut st.target_column_edit)
        .on_target_native_type_edit(|st: &mut Probe| &mut st.target_native_type_edit)
        .on_toggle_select(|st: &mut Probe| {
            st.toggles += 1;
            st.vm.target_select_open = !st.vm.target_select_open;
        })
        .on_close_select(|st: &mut Probe| {
            st.closes += 1;
            st.vm.target_select_open = false;
        });
    col().key("page").w_full().h_full().child(row.build(s.dark))
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn the_two_i18n_keys_the_row_asks_for_resolve() {
    for key in [
        "transfer.mapping.pickTargetColumn",
        "transfer.mapping.targetColumn",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

#[test]
fn target_options_follows_the_react_order_rules() {
    let pick = "— pick —";
    // Current among the columns: pick first, then the columns in order.
    assert_eq!(
        target_options(&["a".to_owned(), "b".to_owned()], "a", pick),
        vec![
            TargetOption::new("", pick),
            TargetOption::new("a", "a"),
            TargetOption::new("b", "b"),
        ]
    );
    // Current outside the columns: pick, current, then the columns.
    assert_eq!(
        target_options(&["a".to_owned(), "b".to_owned()], "renamed", pick),
        vec![
            TargetOption::new("", pick),
            TargetOption::new("renamed", "renamed"),
            TargetOption::new("a", "a"),
            TargetOption::new("b", "b"),
        ]
    );
    // Empty current is not prepended as a duplicate pick row.
    assert_eq!(
        target_options(&["a".to_owned()], "", pick),
        vec![TargetOption::new("", pick), TargetOption::new("a", "a"),]
    );
    // No columns at all: just the pick row.
    assert_eq!(
        target_options(&[], "", pick),
        vec![TargetOption::new("", pick)]
    );
}

#[test]
fn is_unmapped_only_counts_live_rows_without_a_target() {
    assert!(is_unmapped(&row("a", "", false)));
    assert!(is_unmapped(&row("a", "   ", false)));
    assert!(!is_unmapped(&row("a", "", true)), "skipped is excluded");
    assert!(!is_unmapped(&row("a", "b", false)));
}

#[test]
fn the_patch_applies_exactly_the_fields_it_carries() {
    let mut mapping = row("id", "id", false);
    mapping.target_native_type = Some("BIGINT".to_owned());
    ColumnMappingPatch {
        target_column: Some("user_id".to_owned()),
        skip: Some(true),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert_eq!(mapping.target_column, "user_id");
    assert!(mapping.skip);
    assert_eq!(mapping.target_native_type.as_deref(), Some("BIGINT"));

    ColumnMappingPatch {
        target_native_type: Some(None),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert_eq!(mapping.target_native_type, None);

    ColumnMappingPatch::default().apply_to(&mut mapping);
    assert_eq!(mapping.target_column, "user_id");
    assert!(mapping.skip);
}

#[test]
fn the_select_branch_renders_the_react_keys_and_cells() {
    let rt = runtime(seeded(), DESIGN);
    assert!(has_key(&rt, ROOT));
    assert!(has_key(&rt, "data-transfer-target-select-id"));
    assert!(has_key(&rt, "data-transfer-target-select-id-field-trigger"));
    assert!(has_key(&rt, "data-transfer-skip-id"));
    assert!(has_key(&rt, &format!("{ROOT}-source")));
    assert!(has_key(&rt, &format!("{ROOT}-source-type")));
    assert!(has_key(&rt, &format!("{ROOT}-target")));
    assert!(has_key(&rt, &format!("{ROOT}-target-type")));
    assert_eq!(
        element(&rt, &format!("{ROOT}-source")).text.as_deref(),
        Some("id"),
        "source column text"
    );
    assert_eq!(
        element(&rt, &format!("{ROOT}-source-type")).text.as_deref(),
        Some("INTEGER"),
        "source type text"
    );
    // The trigger shows the chosen option's label, not the pick text: the
    // label lives on a child of the trigger, so match by trigger path.
    let trigger = element(&rt, "data-transfer-target-select-id-field-trigger");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("id") && e.path.starts_with(&trigger.path)),
        "trigger shows the current target label"
    );
}

#[test]
fn the_create_new_branch_renders_inputs_not_a_select() {
    let rt = runtime(seeded_create_new(), DESIGN);
    assert!(has_key(&rt, "data-transfer-target-col-email"));
    assert!(has_key(&rt, "data-transfer-target-type-email"));
    assert!(
        !has_key(&rt, "data-transfer-target-select-email"),
        "create-new edits the name instead of picking it"
    );
    // The native-type input carries React's literal placeholder — as the
    // inner input's accessible label, never as element text (kit contract
    // field_kit::placeholder_visible_when_empty_and_hidden_when_filled).
    let elements = rt.elements();
    let field = element(&rt, "data-transfer-target-type-email");
    let input = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("input") && e.path.starts_with(&field.path))
        .expect("inner input of the native-type field");
    assert_eq!(input.label.as_deref(), Some("VARCHAR(255)"));
    assert_eq!(input.text.as_deref(), Some(""));
    assert!(
        !elements
            .iter()
            .any(|e| e.text.as_deref() == Some("VARCHAR(255)")),
        "the placeholder is a label, not rendered text"
    );
}

#[test]
fn the_fixed_cells_keep_the_react_widths_and_gaps() {
    for dark in [true, false] {
        let mut probe = seeded();
        probe.dark = dark;
        let rt = runtime(probe, DESIGN);
        let root = element(&rt, ROOT);
        let source = element(&rt, &format!("{ROOT}-source"));
        let source_type = element(&rt, &format!("{ROOT}-source-type"));
        let target = element(&rt, &format!("{ROOT}-target"));
        let target_type = element(&rt, &format!("{ROOT}-target-type"));
        let skip = element(&rt, &format!("{ROOT}-skip-cell"));

        // `px-3 py-1.5`: the row starts at the page edge, children 12pt in.
        assert_eq!(root.rect[0], 0.0);
        assert!(
            (source.rect[0] - 12.0).abs() <= 0.5,
            "px-3: {:?}",
            source.rect
        );
        // `w-28` / `w-36` / `w-16`.
        assert_eq!(source_type.rect[2], 112.0, "w-28 source type");
        assert_eq!(target_type.rect[2], 144.0, "w-36 native type");
        assert_eq!(skip.rect[2], 64.0, "w-16 skip");
        // `gap-3` (12pt) between every pair of siblings, in React order.
        for (left, right) in [
            (&source, &source_type),
            (&source_type, &target),
            (&target, &target_type),
            (&target_type, &skip),
        ] {
            assert!(
                ((right.rect[0] - (left.rect[0] + left.rect[2])) - 12.0).abs() <= 0.5,
                "gap-3 between {:?} and {:?}",
                left.rect,
                right.rect
            );
        }
        // The skip cell ends 12pt from the row's right edge (`px-3`).
        assert!(
            ((root.rect[0] + root.rect[2] - (skip.rect[0] + skip.rect[2])) - 12.0).abs() <= 0.5,
            "px-3 on the right: root {:?} skip {:?}",
            root.rect,
            skip.rect
        );
        // `py-1.5` cannot be asserted as a 6pt offset in this probe:
        // `build` ends in `rune_kit::root`, which forces the row to the full
        // window height (816 here, the same shape as T010's bar), so the
        // row's `items-center` centres the 40pt input instead of leaving
        // 6pt above it. Assert the observable relative geometry instead:
        // the input is vertically centred in the row, its centre aligns
        // with the source text's centre, and it fills its cell's width.
        let input = element(&rt, "data-transfer-target-type-id");
        let input_cy = input.rect[1] + input.rect[3] / 2.0;
        let root_cy = root.rect[1] + root.rect[3] / 2.0;
        assert!(
            (input_cy - root_cy).abs() <= 0.5,
            "input vertically centred in the row: root {:?} input {:?}",
            root.rect,
            input.rect
        );
        let source_cy = source.rect[1] + source.rect[3] / 2.0;
        assert!(
            (input_cy - source_cy).abs() <= 0.5,
            "input centre aligns with the source text centre: source {:?} input {:?}",
            source.rect,
            input.rect
        );
        assert_eq!(
            input.rect[2], target_type.rect[2],
            "input fills its w-36 cell"
        );
        assert_eq!(input.rect[3], 40.0, "React Input h-10");
    }
}

#[test]
fn the_unmapped_wash_appears_only_on_live_rows_without_a_target() {
    for dark in [true, false] {
        let k = kit(dark);
        let wash = k.alpha(k.c.warning.fg, 0.05);

        let mut unmapped = seeded_create_new();
        unmapped.dark = dark;
        let rt = runtime(unmapped, DESIGN);
        assert_eq!(
            element(&rt, ROOT).fill,
            wash,
            "unmapped row carries the amber wash"
        );

        let mut mapped = seeded();
        mapped.dark = dark;
        let rt = runtime(mapped, DESIGN);
        assert_ne!(element(&rt, ROOT).fill, wash, "mapped row has no wash");

        let mut skipped = seeded_create_new();
        skipped.dark = dark;
        skipped.vm.row.skip = true;
        let rt = runtime(skipped, DESIGN);
        assert_ne!(
            element(&rt, ROOT).fill,
            wash,
            "skipped row is excluded, not unmapped"
        );
    }
}

#[test]
fn choosing_a_target_commits_the_column_and_clears_skip() {
    let mut rt = runtime(seeded(), DESIGN);
    // Options are pick, id, name, email — option2 is "name".
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(has_key(&rt, "option2"), "the listbox opened");
    click(&mut rt, "option2");
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some("name".to_owned()),
            skip: Some(false),
            ..ColumnMappingPatch::default()
        }],
        "React onChange({{ targetColumn, skip: !value.trim() }})"
    );
    assert_eq!(rt.state.vm.row.target_column, "name");
    assert!(!rt.state.vm.row.skip);
    assert!(
        !rt.state.vm.target_select_open,
        "choosing closes the listbox"
    );
}

#[test]
fn choosing_the_pick_row_commits_an_empty_target_and_skip() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(has_key(&rt, "option0"), "the pick row is option0");
    click(&mut rt, "option0");
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some(String::new()),
            skip: Some(true),
            ..ColumnMappingPatch::default()
        }]
    );
    assert_eq!(rt.state.vm.row.target_column, "");
    assert!(rt.state.vm.row.skip);
}

#[test]
fn toggling_skip_commits_only_the_skip_patch() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-skip-id");
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            skip: Some(true),
            ..ColumnMappingPatch::default()
        }]
    );
    assert!(rt.state.vm.row.skip);
    assert_eq!(rt.state.vm.row.target_column, "id", "target untouched");
}

#[test]
fn typing_a_target_name_and_pressing_enter_commits_it() {
    let mut rt = runtime(seeded_create_new(), DESIGN);
    click(&mut rt, "data-transfer-target-col-email");
    rt.paste("user_email");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some("user_email".to_owned()),
            skip: Some(false),
            ..ColumnMappingPatch::default()
        }],
        "submit commits the buffer with skip derived from blankness"
    );
    assert_eq!(rt.state.vm.row.target_column, "user_email");
}

#[test]
fn typing_a_native_type_and_pressing_enter_commits_it() {
    let mut rt = runtime(seeded_create_new(), DESIGN);
    click(&mut rt, "data-transfer-target-type-email");
    rt.paste("VARCHAR(100)");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_native_type: Some(Some("VARCHAR(100)".to_owned())),
            ..ColumnMappingPatch::default()
        }]
    );
    assert_eq!(
        rt.state.vm.row.target_native_type.as_deref(),
        Some("VARCHAR(100)")
    );
}

#[test]
fn submitting_a_blank_native_type_clears_the_override() {
    let mut probe = seeded();
    probe.target_native_type_edit = TextEdit::new("   ");
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-type-id");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_native_type: Some(None),
            ..ColumnMappingPatch::default()
        }],
        "React clears the override on a blank edit"
    );
    assert_eq!(rt.state.vm.row.target_native_type, None);
}

#[test]
fn a_disabled_row_commits_nothing() {
    // Select branch: the skip checkbox is inert and the select never opens.
    let mut probe = seeded();
    probe.vm.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-skip-id");
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(
        !has_key(&rt, "option0"),
        "a disabled select opens no listbox"
    );
    assert!(
        rt.state.patches.is_empty(),
        "disabled commits nothing: {:?}",
        rt.state.patches
    );

    // Create-new branch: the text inputs take no commit either.
    let mut probe = seeded_create_new();
    probe.vm.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-col-email");
    rt.paste("should-not-commit");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(
        rt.state.patches.is_empty(),
        "disabled commits nothing: {:?}",
        rt.state.patches
    );
    assert_eq!(rt.state.vm.row.target_column, "");
    assert!(!rt.state.vm.row.skip);
}

#[test]
fn the_row_stays_inside_both_window_sizes_in_both_themes() {
    for dark in [true, false] {
        for size in [MIN, DESIGN] {
            let mut probe = seeded();
            probe.dark = dark;
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "select branch");
            let mut probe = seeded_create_new();
            probe.dark = dark;
            let rt = runtime(probe, size);
            assert_in_window(&rt, size.0, size.1, "create-new branch");
        }
    }
}
