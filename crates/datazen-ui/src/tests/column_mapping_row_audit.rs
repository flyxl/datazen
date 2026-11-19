//! T023-S1 independent audit: `ColumnMappingRow` re-probes.
//!
//! Fresh vectors the coder suite does not cover: `target_options` boundary
//! cases, `is_unmapped` combinations, `ColumnMappingPatch::apply_to`
//! combinations, skip/input interaction, disabled zero-submit (including
//! whether the edit buffer itself moves), no-edit fallback contract,
//! show_target_type off, key uniqueness, and React-parity submit shapes
//! (whitespace values, empty submit, raw-value preservation).

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::t;
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

// ---- target_options boundaries (React 96-108) ----

#[test]
fn target_options_with_an_empty_column_keeps_react_parity() {
    let pick = "— pick —";
    // React: no empty value exists check is value-based, so a "" column
    // suppresses the pick row and the "" option keeps its empty label.
    assert_eq!(
        target_options(&["".to_owned(), "a".to_owned()], "b", pick),
        vec![
            TargetOption::new("b", "b"),
            TargetOption::new("", ""),
            TargetOption::new("a", "a"),
        ]
    );
    assert_eq!(
        target_options(&["".to_owned()], "", pick),
        vec![TargetOption::new("", "")]
    );
}

#[test]
fn target_options_keeps_duplicates_and_case_sensitive_current() {
    let pick = "— pick —";
    assert_eq!(
        target_options(&["a".to_owned(), "a".to_owned()], "a", pick),
        vec![
            TargetOption::new("", pick),
            TargetOption::new("a", "a"),
            TargetOption::new("a", "a"),
        ]
    );
    // "A" is not among the columns (case-sensitive `includes`), so it is
    // prepended even though "a" is present.
    assert_eq!(
        target_options(&["a".to_owned()], "A", pick),
        vec![
            TargetOption::new("", pick),
            TargetOption::new("A", "A"),
            TargetOption::new("a", "a"),
        ]
    );
}

#[test]
fn target_options_prepends_a_whitespace_current_verbatim() {
    let pick = "— pick —";
    assert_eq!(
        target_options(&["a".to_owned()], " ", pick),
        vec![
            TargetOption::new("", pick),
            TargetOption::new(" ", " "),
            TargetOption::new("a", "a"),
        ]
    );
}

#[test]
fn target_options_with_empty_pick_label_still_adds_the_pick_row() {
    assert_eq!(
        target_options(&["a".to_owned()], "", ""),
        vec![TargetOption::new("", ""), TargetOption::new("a", "a")]
    );
}

// ---- is_unmapped combinations ----

#[test]
fn is_unmapped_covers_whitespace_and_skip_combinations() {
    assert!(is_unmapped(&row("a", "\t\n", false)));
    assert!(!is_unmapped(&row("a", "\t\n", true)));
    assert!(!is_unmapped(&row("a", "b", true)));
    assert!(!is_unmapped(&row("a", " b ", false)));
    // The source column is irrelevant to the predicate.
    assert!(is_unmapped(&row("", "", false)));
}

// ---- patch apply combinations ----

#[test]
fn patch_apply_keeps_an_empty_string_override_and_clears_skip() {
    let mut mapping = row("id", "id", true);
    mapping.target_native_type = Some("BIGINT".to_owned());
    // `apply_to` is a spread: Some("") is a carried value, not a clear.
    ColumnMappingPatch {
        target_native_type: Some(Some(String::new())),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert_eq!(mapping.target_native_type.as_deref(), Some(""));
    ColumnMappingPatch {
        skip: Some(false),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert!(!mapping.skip);
    assert_eq!(mapping.target_column, "id");
}

#[test]
fn patch_apply_sequences_override_only_carried_fields() {
    let mut mapping = row("a", "a", false);
    ColumnMappingPatch {
        target_column: Some("b".to_owned()),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    ColumnMappingPatch {
        target_column: Some("c".to_owned()),
        skip: Some(true),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert_eq!(mapping.target_column, "c");
    assert!(mapping.skip);
    // A later empty patch changes nothing.
    ColumnMappingPatch::default().apply_to(&mut mapping);
    assert_eq!(mapping.target_column, "c");
    assert!(mapping.skip);
}

#[test]
fn patch_apply_pick_row_shape_clears_target_and_skips() {
    let mut mapping = row("id", "id", false);
    ColumnMappingPatch {
        target_column: Some(String::new()),
        skip: Some(true),
        ..ColumnMappingPatch::default()
    }
    .apply_to(&mut mapping);
    assert_eq!(mapping.target_column, "");
    assert!(mapping.skip);
    assert!(is_unmapped(&mapping) == false, "skipped is not unmapped");
}

// ---- skip / input interaction ----

#[test]
fn toggling_skip_twice_restores_the_row_and_touches_nothing_else() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-skip-id");
    click(&mut rt, "data-transfer-skip-id");
    assert_eq!(
        rt.state.patches,
        vec![
            ColumnMappingPatch {
                skip: Some(true),
                ..ColumnMappingPatch::default()
            },
            ColumnMappingPatch {
                skip: Some(false),
                ..ColumnMappingPatch::default()
            },
        ]
    );
    assert!(!rt.state.vm.row.skip);
    assert_eq!(rt.state.vm.row.target_column, "id");
}

#[test]
fn choosing_a_target_on_a_skipped_row_clears_skip() {
    let mut probe = seeded();
    probe.vm.row.skip = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    click(&mut rt, "option2"); // "name"
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some("name".to_owned()),
            skip: Some(false),
            ..ColumnMappingPatch::default()
        }]
    );
    assert!(!rt.state.vm.row.skip);
    assert_eq!(rt.state.vm.row.target_column, "name");
}

#[test]
fn choosing_the_pick_row_on_a_skipped_row_keeps_skip() {
    let mut probe = seeded();
    probe.vm.row.skip = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    click(&mut rt, "option0"); // pick
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some(String::new()),
            skip: Some(true),
            ..ColumnMappingPatch::default()
        }]
    );
    assert!(rt.state.vm.row.skip);
}

// ---- submit shapes (React per-keystroke vs Rune Enter commit) ----

#[test]
fn submitting_an_empty_target_column_skips_the_row() {
    let mut rt = runtime(seeded_create_new(), DESIGN);
    click(&mut rt, "data-transfer-target-col-email");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some(String::new()),
            skip: Some(true),
            ..ColumnMappingPatch::default()
        }]
    );
    assert!(rt.state.vm.row.skip);
}

#[test]
fn submitting_a_whitespace_target_column_skips_but_keeps_the_raw_value() {
    let mut rt = runtime(seeded_create_new(), DESIGN);
    click(&mut rt, "data-transfer-target-col-email");
    rt.paste("   ");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some("   ".to_owned()),
            skip: Some(true),
            ..ColumnMappingPatch::default()
        }],
        "React keeps the raw value and derives skip from trim()"
    );
}

#[test]
fn submitting_a_native_type_keeps_surrounding_whitespace_when_non_blank() {
    let mut rt = runtime(seeded_create_new(), DESIGN);
    click(&mut rt, "data-transfer-target-type-email");
    rt.paste("  VARCHAR(10)  ");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_native_type: Some(Some("  VARCHAR(10)  ".to_owned())),
            ..ColumnMappingPatch::default()
        }],
        "React passes the raw value when trim() is non-empty"
    );
}

#[test]
fn choosing_the_current_target_again_still_commits() {
    let mut rt = runtime(seeded(), DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    click(&mut rt, "option1"); // "id", the current value
    assert_eq!(
        rt.state.patches,
        vec![ColumnMappingPatch {
            target_column: Some("id".to_owned()),
            skip: Some(false),
            ..ColumnMappingPatch::default()
        }]
    );
}

// ---- disabled: zero-submit, and whether the buffer itself moves ----

#[test]
fn a_disabled_create_new_row_takes_no_commit_and_no_buffer_change() {
    let mut probe = seeded_create_new();
    probe.vm.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-col-email");
    rt.paste("should-not-commit");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(rt.state.patches.is_empty());
    assert_eq!(rt.state.target_column_edit.display().0, "");
    click(&mut rt, "data-transfer-target-type-email");
    rt.paste("VARCHAR(9)");
    rt.settle_all();
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert!(rt.state.patches.is_empty());
    assert_eq!(rt.state.target_native_type_edit.display().0, "");
}

#[test]
fn a_disabled_row_never_opens_the_select_or_counts_toggles() {
    let mut probe = seeded();
    probe.vm.disabled = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    click(&mut rt, "data-transfer-target-select-id-field-trigger");
    assert!(!has_key(&rt, "option0"));
    assert_eq!(rt.state.toggles, 0);
    assert_eq!(rt.state.closes, 0);
    assert!(rt.state.patches.is_empty());
    click(&mut rt, "data-transfer-skip-id");
    click(&mut rt, "data-transfer-skip-id");
    assert!(rt.state.patches.is_empty());
}

// ---- show_target_type off, fallback source type, key uniqueness ----

#[test]
fn without_show_target_type_the_type_cells_are_absent() {
    let mut probe = seeded();
    probe.vm.show_target_type = false;
    let rt = runtime(probe, DESIGN);
    assert!(!has_key(&rt, &format!("{ROOT}-source-type")));
    assert!(!has_key(&rt, &format!("{ROOT}-target-type")));
    assert!(has_key(&rt, &format!("{ROOT}-target")));
    assert!(has_key(&rt, &format!("{ROOT}-skip-cell")));
}

#[test]
fn a_missing_source_type_renders_the_dash_fallback() {
    let mut probe = seeded();
    probe.vm.source_type = None;
    let rt = runtime(probe, DESIGN);
    assert_eq!(
        element(&rt, &format!("{ROOT}-source-type")).text.as_deref(),
        Some("—"),
        "React renders sourceType ?? '—'"
    );
}

#[test]
fn every_key_is_unique_among_siblings() {
    // The kit reuses "value"/"input" keys in different subtrees (one per
    // control), so the contract is uniqueness among siblings, not global.
    // React's per-row testids are unique per row; the shell must pass a
    // distinct root key per row (see the module docs).
    use std::collections::HashMap;
    for probe in [seeded(), seeded_create_new()] {
        let rt = runtime(probe, DESIGN);
        let mut by_parent: HashMap<String, Vec<String>> = HashMap::new();
        for e in rt.elements() {
            if let Some(key) = e.key {
                let parent = match e.path.rfind('/') {
                    Some(i) => e.path[..i].to_owned(),
                    None => String::new(),
                };
                by_parent.entry(parent).or_default().push(key);
            }
        }
        for (parent, keys) in &by_parent {
            let mut sorted = keys.clone();
            sorted.sort();
            let before = sorted.len();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                before,
                "duplicate keys under parent {parent}: {keys:?}"
            );
        }
    }
}

#[test]
fn the_native_type_placeholder_is_the_fields_accessible_name() {
    // Kit contract (field_kit::placeholder_visible_when_empty_and_hidden_
    // when_filled): an empty field carries its placeholder as the inner
    // input's label, never as element text.
    let rt = runtime(seeded_create_new(), DESIGN);
    let elements = rt.elements();
    let field = element(&rt, "data-transfer-target-type-email");
    let input = elements
        .iter()
        .find(|e| {
            e.key.as_deref() == Some("input") && e.path.starts_with(&field.path)
        })
        .expect("inner input of the native-type field");
    assert_eq!(input.label.as_deref(), Some("VARCHAR(255)"));
    assert_eq!(input.text.as_deref(), Some(""));
    assert!(
        !elements
            .iter()
            .any(|e| e.text.as_deref() == Some("VARCHAR(255)")),
        "the placeholder is a label, not rendered text"
    );
    // The target-column input has no placeholder, so its inner input is
    // unnamed — the two fields are distinguishable.
    let col_field = element(&rt, "data-transfer-target-col-email");
    let col_input = elements
        .iter()
        .find(|e| {
            e.key.as_deref() == Some("input") && e.path.starts_with(&col_field.path)
        })
        .expect("inner input of the target-column field");
    assert_eq!(col_input.label, None);
}

#[test]
fn an_empty_option_list_shows_the_pick_text_on_the_trigger() {
    let mut probe = seeded();
    probe.vm.target_options = Vec::new();
    probe.vm.row.target_column = String::new();
    let rt = runtime(probe, DESIGN);
    let trigger = element(&rt, "data-transfer-target-select-id-field-trigger");
    assert!(
        rt.elements().iter().any(|e| e.text.as_deref()
            == Some(t("transfer.mapping.pickTargetColumn"))
            && e.path.starts_with(&trigger.path)),
        "nothing chosen: the trigger shows the pick text"
    );
}

// ---- no-edit fallback contract ----

#[test]
#[should_panic(expected = "target_column_edit")]
fn a_create_new_row_without_its_edit_buffer_fails_loudly_in_debug() {
    let probe = seeded_create_new();
    let view = |s: &Probe, _cx: &mut Cx<Probe>| {
        ColumnMappingRow::<Probe>::new(ROOT, s.vm.clone()).build(s.dark)
    };
    let mut rt = Runtime::new(probe, view, DESIGN, env(true));
    rt.frame(0.0);
}

#[test]
#[should_panic(expected = "target_native_type_edit")]
fn a_typed_row_without_its_type_edit_buffer_fails_loudly_in_debug() {
    let probe = seeded();
    let view = |s: &Probe, _cx: &mut Cx<Probe>| {
        ColumnMappingRow::<Probe>::new(ROOT, s.vm.clone())
            .target_column_edit(&s.target_column_edit)
            .build(s.dark)
    };
    let mut rt = Runtime::new(probe, view, DESIGN, env(true));
    rt.frame(0.0);
}

// ---- geometry spot checks in the create-new branch at MIN ----

#[test]
fn the_create_new_branch_keeps_fixed_widths_at_min_size() {
    for dark in [true, false] {
        let mut probe = seeded_create_new();
        probe.dark = dark;
        let rt = runtime(probe, MIN);
        assert_eq!(
            element(&rt, &format!("{ROOT}-source-type")).rect[2],
            112.0,
            "w-28 source type at MIN"
        );
        assert_eq!(
            element(&rt, &format!("{ROOT}-target-type")).rect[2],
            144.0,
            "w-36 native type at MIN"
        );
        assert_eq!(
            element(&rt, &format!("{ROOT}-skip-cell")).rect[2],
            64.0,
            "w-16 skip at MIN"
        );
        let mut outside = Vec::new();
        for e in rt.elements() {
            let r = e.rect;
            if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > MIN.0 + 0.5 || r[1] + r[3] > MIN.1 + 0.5
            {
                outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
            }
        }
        assert!(outside.is_empty(), "MIN: {outside:?}");
    }
}
