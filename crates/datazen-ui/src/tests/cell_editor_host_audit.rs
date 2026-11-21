//! T297 independent audit: fresh vectors — the boundary numbers (the
//! number regex's exponent and sign edges, the leap century), the
//! nothing-changed close on a typed family, the context menu's
//! one-shot blur guard across two menus, the seed re-seed clearing
//! the error, and the refusal's message keys verbatim.

use serde_json::{json, Value};

use crate::components::data_table::editable_cell::EditableCellKind;
use crate::components::data_table::editors::cell_editor_host::{
    CellEditorHostState, EditAction,
};
use crate::support::cell_editor_kind::{
    coerce_value, CellEditorKind, CellValueErrorReason, CoerceInput, CoerceResult,
};
use crate::support::cell_types::CellType;

fn host_state(kind: CellEditorKind, cell_type: CellType, original: &Value) -> CellEditorHostState {
    CellEditorHostState::seed(kind, cell_type, original, true)
}

/// The number regex's boundary edges the coder vectors do not reach:
/// the bare sign, the exponent without digits, the trailing dot.
#[test]
fn the_number_regex_boundaries() {
    fn input<'a>(text: &'a str) -> CoerceInput<'a> {
        CoerceInput {
            kind: CellEditorKind::Number,
            cell_type: CellType::Number,
            text,
            nullable: true,
            original_value: None,
            integer_only: false,
        }
    }
    // A bare sign is not a number.
    assert_eq!(
        coerce_value(&input("+")),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    // An exponent without digits is not.
    assert_eq!(
        coerce_value(&input("1e")),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    assert_eq!(
        coerce_value(&input("1e+")),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    // A trailing dot with digits is.
    assert_eq!(coerce_value(&input("1.")), CoerceResult::Ok(json!(1)));
    // A leading dot with digits is.
    assert_eq!(coerce_value(&input(".5")), CoerceResult::Ok(json!(0.5)));
    // A dot alone is not.
    assert_eq!(
        coerce_value(&input(".")),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    // The uppercase exponent is.
    assert_eq!(coerce_value(&input("1E2")), CoerceResult::Ok(json!(100)));
}

/// The calendar's leap-century rule: 1900 is NOT a leap year, 2000 is.
#[test]
fn the_leap_century_rule() {
    fn date_input<'a>(text: &'a str) -> CoerceInput<'a> {
        CoerceInput {
            kind: CellEditorKind::Date,
            cell_type: CellType::Date,
            text,
            nullable: true,
            original_value: None,
            integer_only: false,
        }
    }
    // 2000 (divisible by 400): a leap day is real.
    assert_eq!(
        coerce_value(&date_input("2000-02-29")),
        CoerceResult::Ok(json!("2000-02-29"))
    );
    // 1900 (divisible by 100, not 400): NOT a leap year.
    assert_eq!(
        coerce_value(&date_input("1900-02-29")),
        CoerceResult::Refused(CellValueErrorReason::InvalidDate)
    );
    // 2026 (a common year): the 29th of February is not real.
    assert_eq!(
        coerce_value(&date_input("2026-02-29")),
        CoerceResult::Refused(CellValueErrorReason::InvalidDate)
    );
}

/// The nothing-changed close on a typed family: a boolean draft that
/// equals its seed closes without writing — even though a boolean
/// write path exists (the pick).
#[test]
fn nothing_changed_closes_on_the_typed_family() {
    // The seed text of true is "true"; the draft stays "true".
    let mut boolean = host_state(CellEditorKind::Boolean, CellType::Bool, &json!(true));
    let action = boolean.commit(None);
    assert_eq!(action, EditAction::Cancel, "the unchanged boolean closes");
    // But a pick still writes right away (a different channel).
    let mut boolean = host_state(CellEditorKind::Boolean, CellType::Bool, &json!(true));
    assert!(matches!(
        boolean.pick(Some(json!(false))),
        EditAction::Commit { .. }
    ));
}

/// The context menu's one-shot guard across two menus: the first
/// right-click's blur is swallowed, the flag resets, the second
/// right-click arms it again.
#[test]
fn the_context_menu_guard_is_oneshot_across_menus() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    // First menu: the blur is swallowed, the flag resets.
    state.context_menu_opened();
    assert_eq!(state.blur(None, None), EditAction::None);
    // A plain blur between menus commits.
    assert!(matches!(state.blur(None, None), EditAction::Commit { .. }));

    // A fresh editor: the second menu behaves identically.
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("c".to_owned());
    state.context_menu_opened();
    assert_eq!(state.blur(None, None), EditAction::None);
    assert!(matches!(state.blur(None, None), EditAction::Commit { .. }));
}

/// The seed's re-seed keeps the error (React's value-change effect
/// only re-seeds the initial and the draft — the error is a `useState`
/// cell only a draft change or a landed commit clears), and the draft
/// change is what clears it.
#[test]
fn the_reseed_keeps_the_error_the_draft_clears_it() {
    let mut state = CellEditorHostState::seed(
        CellEditorKind::Number,
        CellType::Number,
        &json!(5),
        false,
    );
    state.draft = String::new();
    let _ = state.commit(None);
    assert!(state.error.is_some());
    // The value changes underneath (a refresh): the draft re-seeds,
    // the error survives (React's own effect writes only the text).
    state.value_changed(&json!(7));
    assert_eq!(state.draft, "7");
    assert_eq!(state.initial, "7");
    assert!(state.error.is_some(), "the re-seed keeps the error (React's effect touches only the text)");
    // The draft change clears it.
    state.draft_changed("8".to_owned());
    assert_eq!(state.error, None);
}

/// Every refusal's message key resolves to the pack's exact word.
#[test]
fn the_refusal_message_words_are_exact() {
    let words = [
        (CellValueErrorReason::EmptyNotNull, "cellEditor.notNull", "此列不允许 NULL"),
        (CellValueErrorReason::InvalidNumber, "cellEditor.invalidNumber", "请输入有效的数字"),
        (CellValueErrorReason::InvalidInteger, "cellEditor.invalidInteger", "请输入整数"),
        (CellValueErrorReason::InvalidBoolean, "cellEditor.invalidBoolean", "请输入 true 或 false"),
        (CellValueErrorReason::InvalidJson, "cellEditor.invalidJson", "无效的 JSON — 请在应用前修正"),
        (CellValueErrorReason::InvalidUuid, "cellEditor.invalidUuid", "请输入有效的 UUID"),
        (CellValueErrorReason::NotEditable, "cellEditor.binaryNotEditable", "二进制值无法在表格中编辑"),
    ];
    for (reason, key, word) in words {
        assert_eq!(reason.message_key(), key, "the reason maps to its key");
        assert_eq!(crate::i18n::t(key), word, "the key resolves to the pack word");
    }
}

/// The kind derivation agrees with the host's coercion contract: the
/// uuid text kind coerces through the uuid branch (the EditableCell
/// shell's derivation feeding the same CellType the host reads).
#[test]
fn the_kind_derivation_feeds_the_coercion() {
    let derived = EditableCellKind::of(&json!("x"), "uuid");
    assert_eq!(derived.kind, CellEditorKind::Text);
    assert_eq!(derived.cell_type, CellType::Uuid);
    // The same coercion the host would run.
    let result = coerce_value(&CoerceInput {
        kind: derived.kind,
        cell_type: derived.cell_type,
        text: "123e4567-e89b-12d3-a456-426614174000",
        nullable: true,
        original_value: None,
        integer_only: false,
    });
    assert_eq!(
        result,
        CoerceResult::Ok(json!("123e4567-e89b-12d3-a456-426614174000"))
    );
}
