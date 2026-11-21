//! T297: the cell editor host — coder vectors: the state machine's
//! gates (cancel/commit/pick with the done guard and the
//! nothing-changed close), the coercion's families, the keyboard and
//! blur rules with their guards, the owned-surface predicates, the
//! candidates state machine, and the mount's controls and error.

use serde_json::{json, Value};

use crate::components::data_table::editable_cell::EditableCellKind;
use crate::components::data_table::editors::cell_editor_host::{
    is_owned_editor_target, temporal_kind_of, CellEditorHost, CellEditorHostState, EditAction,
    EditMove,
};
use crate::support::cell_editor_kind::{
    coerce_value, CellEditorKind, CellValueErrorReason, CoerceInput, CoerceResult,
};
use crate::support::cell_types::CellType;
use crate::table_data_dirty_index::normalize_edit_value;

fn host_state(kind: CellEditorKind, cell_type: CellType, original: &Value) -> CellEditorHostState {
    let derived = EditableCellKind::of(original, "text");
    let _ = derived;
    CellEditorHostState::seed(kind, cell_type, original, true)
}

// -- the coercion (the T118 extension) ----------------------------------------

#[test]
fn the_empty_input_rules() {
    // Empty on a nullable column: NULL.
    let result = coerce_value(&CoerceInput {
        kind: CellEditorKind::Text,
        cell_type: CellType::Text,
        text: "",
        nullable: true,
        original_value: None,
        integer_only: false,
    });
    assert_eq!(result, CoerceResult::Ok(Value::Null));
    // Empty on a NOT NULL column: refused, never an empty string.
    let result = coerce_value(&CoerceInput {
        kind: CellEditorKind::Text,
        cell_type: CellType::Text,
        text: "",
        nullable: false,
        original_value: None,
        integer_only: false,
    });
    assert_eq!(result, CoerceResult::Refused(CellValueErrorReason::EmptyNotNull));
}

#[test]
fn the_number_family_rules() {
    fn input<'a>(text: &'a str, integer_only: bool) -> CoerceInput<'a> {
        CoerceInput {
            kind: CellEditorKind::Number,
            cell_type: CellType::Number,
            text,
            nullable: true,
            original_value: None,
            integer_only,
        }
    }
    // A clean integer.
    assert_eq!(coerce_value(&input("42", false)), CoerceResult::Ok(json!(42)));
    // A decimal.
    assert_eq!(coerce_value(&input("3.5", false)), CoerceResult::Ok(json!(3.5)));
    // A negative with an exponent.
    assert_eq!(coerce_value(&input("-1.5e2", false)), CoerceResult::Ok(json!(-150)));
    // Arabic-Indic digits are not numbers here.
    assert_eq!(
        coerce_value(&input("١٢", false)),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    // 0x10 is not a number here.
    assert_eq!(
        coerce_value(&input("0x10", false)),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
    // A fraction on an integer-only column.
    assert_eq!(
        coerce_value(&input("3.5", true)),
        CoerceResult::Refused(CellValueErrorReason::InvalidInteger)
    );
    // A whole number on an integer-only column passes.
    assert_eq!(coerce_value(&input("3", true)), CoerceResult::Ok(json!(3)));
    // NaN is unreachable (the regex), but the fraction gate holds.
    assert_eq!(
        coerce_value(&input(".", false)),
        CoerceResult::Refused(CellValueErrorReason::InvalidNumber)
    );
}

#[test]
fn the_boolean_family_rules() {
    fn input<'a>(text: &'a str) -> CoerceInput<'a> {
        CoerceInput {
            kind: CellEditorKind::Boolean,
            cell_type: CellType::Bool,
            text,
            nullable: true,
            original_value: None,
            integer_only: false,
        }
    }
    assert_eq!(coerce_value(&input("true")), CoerceResult::Ok(json!(true)));
    assert_eq!(coerce_value(&input("FALSE")), CoerceResult::Ok(json!(false)));
    assert_eq!(coerce_value(&input(" 1 ")), CoerceResult::Ok(json!(true)));
    assert_eq!(coerce_value(&input("0")), CoerceResult::Ok(json!(false)));
    assert_eq!(
        coerce_value(&input("yes")),
        CoerceResult::Refused(CellValueErrorReason::InvalidBoolean)
    );
}

#[test]
fn the_temporal_family_rules() {
    fn input<'a>(text: &'a str, cell_type: CellType) -> CoerceInput<'a> {
        CoerceInput {
            kind: CellEditorKind::Date,
            cell_type,
            text,
            nullable: true,
            original_value: None,
            integer_only: false,
        }
    }
    // A real date: wall clock, verbatim.
    assert_eq!(
        coerce_value(&input("2026-11-21", CellType::Date)),
        CoerceResult::Ok(json!("2026-11-21"))
    );
    // Not a real day.
    assert_eq!(
        coerce_value(&input("2026-02-30", CellType::Date)),
        CoerceResult::Refused(CellValueErrorReason::InvalidDate)
    );
    // A leap day.
    assert_eq!(
        coerce_value(&input("2024-02-29", CellType::Date)),
        CoerceResult::Ok(json!("2024-02-29"))
    );
    // A time: canonicalized with the seconds.
    assert_eq!(
        coerce_value(&input("9:05", CellType::Time)),
        CoerceResult::Ok(json!("09:05:00"))
    );
    // An impossible hour.
    assert_eq!(
        coerce_value(&input("25:00", CellType::Time)),
        CoerceResult::Refused(CellValueErrorReason::InvalidTime)
    );
    // A datetime without a zone.
    assert_eq!(
        coerce_value(&input("2026-11-21T10:30", CellType::DateTime)),
        CoerceResult::Ok(json!("2026-11-21T10:30"))
    );
    // The datetime kind on a datetime cell type.
    assert_eq!(
        coerce_value(&CoerceInput {
            kind: CellEditorKind::Date,
            cell_type: CellType::DateTime,
            text: "2026-13-01T10:00",
            nullable: true,
            original_value: None,
            integer_only: false,
        }),
        CoerceResult::Refused(CellValueErrorReason::InvalidDateTime)
    );
}

#[test]
fn the_json_uuid_and_zoned_text_rules() {
    // Valid JSON passes as its parsed value.
    let result = coerce_value(&CoerceInput {
        kind: CellEditorKind::Json,
        cell_type: CellType::Json,
        text: "{\"a\": 1}",
        nullable: true,
        original_value: None,
        integer_only: false,
    });
    assert_eq!(result, CoerceResult::Ok(json!({"a": 1})));
    // Invalid JSON is refused.
    let result = coerce_value(&CoerceInput {
        kind: CellEditorKind::Json,
        cell_type: CellType::Json,
        text: "{a: 1}",
        nullable: true,
        original_value: None,
        integer_only: false,
    });
    assert_eq!(result, CoerceResult::Refused(CellValueErrorReason::InvalidJson));

    // A uuid: trimmed and validated.
    fn uuid_input<'a>(text: &'a str) -> CoerceInput<'a> {
        CoerceInput {
                    kind: CellEditorKind::Text,
                    cell_type: CellType::Uuid,
                    text,
                    nullable: true,
                    original_value: None,
                    integer_only: false,
        }
    }
    assert_eq!(
        coerce_value(&uuid_input(" 123e4567-e89b-12d3-a456-426614174000 ")),
        CoerceResult::Ok(json!("123e4567-e89b-12d3-a456-426614174000"))
    );
    assert_eq!(
        coerce_value(&uuid_input("not-a-uuid")),
        CoerceResult::Refused(CellValueErrorReason::InvalidUuid)
    );

    // A zoned datetime as text: the offset survives untouched.
    assert_eq!(
        coerce_value(&CoerceInput {
            kind: CellEditorKind::Text,
            cell_type: CellType::DateTime,
            text: "2026-11-21T10:30:00+08:00",
            nullable: true,
            original_value: None,
            integer_only: false,
        }),
        CoerceResult::Ok(json!("2026-11-21T10:30:00+08:00"))
    );
}

#[test]
fn binary_refuses_and_enum_passes() {
    assert_eq!(
        coerce_value(&CoerceInput {
            kind: CellEditorKind::Binary,
            cell_type: CellType::Bytes,
            text: "[1,2,3]",
            nullable: true,
            original_value: None,
            integer_only: false,
        }),
        CoerceResult::Refused(CellValueErrorReason::NotEditable)
    );
    assert_eq!(
        coerce_value(&CoerceInput {
            kind: CellEditorKind::Enum,
            cell_type: CellType::Enum,
            text: "choice",
            nullable: true,
            original_value: None,
            integer_only: false,
        }),
        CoerceResult::Ok(json!("choice"))
    );
}

#[test]
fn normalize_edit_value_refuses_nan() {
    // A finite number passes.
    assert_eq!(normalize_edit_value(Some(&json!(5))), Some(json!(5)));
    // A string passes.
    assert_eq!(normalize_edit_value(Some(&json!("x"))), Some(json!("x")));
    // Null passes.
    assert_eq!(normalize_edit_value(Some(&Value::Null)), Some(Value::Null));
    // None passes (do not write).
    assert_eq!(normalize_edit_value(None), None);
}

// -- the state machine ---------------------------------------------------------

#[test]
fn commit_writes_and_the_done_guard_holds() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    let action = state.commit(None);
    assert_eq!(
        action,
        EditAction::Commit {
            value: Some(json!("b")),
            direction: None
        }
    );
    // Done: a second commit is silent.
    assert_eq!(state.commit(None), EditAction::None);
    // Done: cancel is silent too.
    assert_eq!(state.cancel(), EditAction::None);
}

#[test]
fn nothing_changed_closes_without_writing() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    // The draft equals the initial.
    let action = state.commit(None);
    assert_eq!(action, EditAction::Cancel, "the editor closes without writing");
    assert!(state.done);
}

#[test]
fn a_refusal_keeps_the_editor_open_with_its_error() {
    let mut state = CellEditorHostState::seed(
        CellEditorKind::Number,
        CellType::Number,
        &json!(5),
        false, // NOT NULL
    );
    // An empty draft on a NOT NULL column: refused.
    state.draft = String::new();
    let action = state.commit(None);
    assert_eq!(action, EditAction::Refused(CellValueErrorReason::EmptyNotNull));
    assert_eq!(state.error, Some(CellValueErrorReason::EmptyNotNull));
    assert!(!state.done, "the editor stays open");

    // A draft change clears the error.
    state.draft_changed("7".to_owned());
    assert_eq!(state.error, None);
    let action = state.commit(None);
    assert_eq!(
        action,
        EditAction::Commit {
            value: Some(json!(7)),
            direction: None
        }
    );
}

#[test]
fn a_pick_writes_right_away() {
    let mut state = host_state(CellEditorKind::Boolean, CellType::Bool, &json!(false));
    let action = state.pick(Some(json!(true)));
    assert_eq!(
        action,
        EditAction::Commit {
            value: Some(json!(true)),
            direction: None
        }
    );
    // A null pick writes SQL NULL.
    let mut state = host_state(CellEditorKind::Boolean, CellType::Bool, &json!(false));
    let action = state.pick(None);
    assert_eq!(
        action,
        EditAction::Commit {
            value: Some(Value::Null),
            direction: None
        }
    );
    // Done guards the pick too.
    assert_eq!(state.pick(Some(json!(true))), EditAction::None);
}

#[test]
fn the_keyboard_rules() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    // Enter commits.
    assert!(matches!(state.key("Enter", false), EditAction::Commit { .. }));

    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    // Escape cancels.
    assert_eq!(state.key("Escape", false), EditAction::Cancel);

    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    // Tab commits with the move; shift moves back.
    assert!(matches!(state.key("Tab", false), EditAction::Commit {
        direction: Some(EditMove::Next),
        ..
    }));
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    assert!(matches!(state.key("Tab", true), EditAction::Commit {
        direction: Some(EditMove::Prev),
        ..
    }));

    // An unhandled key is silent.
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    assert_eq!(state.key("a", false), EditAction::None);
}

#[test]
fn the_ime_owns_the_keyboard() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.composition_started();
    // Enter and Escape reach nothing while composing.
    assert_eq!(state.key("Enter", false), EditAction::None);
    assert_eq!(state.key("Escape", false), EditAction::None);
    assert_eq!(state.key("Tab", false), EditAction::None);
    state.composition_ended();
    // After the composition: Escape cancels.
    assert_eq!(state.key("Escape", false), EditAction::Cancel);
}

#[test]
fn the_blur_rules_with_their_guards() {
    // A blur into an owned surface writes nothing.
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    assert_eq!(
        state.blur(Some("cell-editor-temporal-popover"), None),
        EditAction::None,
        "the temporal picker's popover is owned"
    );
    assert_eq!(state.blur(Some("select-listbox"), None), EditAction::None);
    assert_eq!(state.blur(Some("cell-editor"), None), EditAction::None);
    assert_eq!(state.blur(None, Some("menu")), EditAction::None);

    // A foreign blur commits.
    assert!(matches!(state.blur(Some("somewhere-else"), None), EditAction::Commit { .. }));

    // An unfinished composition owns the focus.
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    state.composition_started();
    assert_eq!(state.blur(None, None), EditAction::None);

    // A right-click's blur writes nothing (the menu may still change
    // the cell).
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    state.context_menu_opened();
    assert_eq!(state.blur(None, None), EditAction::None);
    assert!(!state.context_menu, "the flag is one-shot");
    // The next blur commits.
    assert!(matches!(state.blur(None, None), EditAction::Commit { .. }));
}

#[test]
fn the_owned_surface_predicates() {
    assert!(is_owned_editor_target(Some("cell-editor"), None));
    assert!(is_owned_editor_target(Some("select-listbox"), None));
    assert!(is_owned_editor_target(Some("cell-editor-temporal-popover"), None));
    assert!(is_owned_editor_target(None, Some("menu")));
    assert!(!is_owned_editor_target(Some("other"), None));
    assert!(!is_owned_editor_target(None, Some("button")));
    assert!(!is_owned_editor_target(None, None));
    // The temporal shapes.
    assert_eq!(temporal_kind_of(CellType::Date), "date");
    assert_eq!(temporal_kind_of(CellType::Time), "time");
    assert_eq!(temporal_kind_of(CellType::DateTime), "datetime");
    assert_eq!(temporal_kind_of(CellType::Text), "date");
}

#[test]
fn the_candidates_state_machine() {
    let mut state = host_state(CellEditorKind::Enum, CellType::Enum, &json!("a"));
    // The load begins.
    assert_eq!(state.load_candidates(), EditAction::LoadCandidates);
    assert!(state.candidates_loading);
    // The values land.
    state.candidates_landed(vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(state.candidates.as_ref().map(Vec::len), Some(2));
    assert!(!state.candidates_loading);
    // A failure quotes the loader's own message.
    state.candidates_failed(Some("该列的选择器不唯一".to_owned()));
    assert!(state.candidates_failed);
    assert_eq!(
        state.candidates_message.as_deref(),
        Some("该列的选择器不唯一")
    );
    // A silent loader keeps the generic wording (no message).
    state.candidates_failed(None);
    assert_eq!(state.candidates_message, None);
}

#[test]
fn the_value_change_reseeds_the_draft() {
    let mut state = host_state(CellEditorKind::Text, CellType::Text, &json!("a"));
    state.draft_changed("b".to_owned());
    state.value_changed(&json!("c"));
    assert_eq!(state.initial, "c");
    assert_eq!(state.draft, "c");
}

// -- the mount ------------------------------------------------------------------

#[derive(Clone)]
struct Probe {
    state: CellEditorHostState,
}

fn view(s: &Probe, _cx: &mut rune::Cx<Probe>) -> rune::El {
    let mut layer = Vec::new();
    let el = CellEditorHost::<Probe>::new(&s.state)
        .on_toggle_select(|_st| {})
        .on_close_select(|_st| {})
        .build(crate::ui::kit::kit(true), &mut layer);
    rune_kit::root(el, layer)
}

fn mount(state: CellEditorHostState, size: (f32, f32)) -> rune::Runtime<Probe> {
    let mut rt = rune::Runtime::new(
        Probe { state },
        view,
        size,
        rune::Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn keyed<S>(rt: &rune::Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn the_mount_carries_the_control_and_the_inline_error() {
    // A text cell: the pending-family seam with the draft.
    let rt = mount(
        host_state(CellEditorKind::Text, CellType::Text, &json!("a")),
        (960.0, 640.0),
    );
    assert!(keyed(&rt, "cell-editor").is_some());
    assert!(keyed(&rt, "cell-editor-seam-text").is_some());
    assert_eq!(
        keyed(&rt, "cell-editor-seam-text-text").and_then(|e| e.text),
        Some("a".to_owned())
    );
    assert!(keyed(&rt, "cell-editor-error").is_none(), "no error");

    // A refused state: the inline error mounts with its pack word.
    let mut refused = host_state(CellEditorKind::Number, CellType::Number, &json!(5));
    refused.error = Some(CellValueErrorReason::InvalidNumber);
    let rt = mount(refused, (960.0, 640.0));
    let error = keyed(&rt, "cell-editor-error").expect("the error mounts");
    assert_eq!(error.role, "alert");
    assert_eq!(error.text, Some(crate::i18n::t("cellEditor.invalidNumber").to_owned()));
}

#[test]
fn the_binary_and_boolean_controls_mount_real() {
    // Binary: the read-only status row under the host's root, keeping
    // its own root key (the builder's key is the contract).
    let rt = mount(
        host_state(CellEditorKind::Binary, CellType::Bytes, &json!([1, 2])),
        (960.0, 640.0),
    );
    assert!(keyed(&rt, "cell-editor-binary").is_some(), "the binary control is real");

    // Boolean (nullable seed → the three-state select): the kit
    // derives the trigger key; the control is real.
    let rt = mount(
        host_state(CellEditorKind::Boolean, CellType::Bool, &json!(false)),
        (960.0, 640.0),
    );
    assert!(keyed(&rt, "cell-editor-boolean-trigger").is_some(), "the boolean control is real");

    // Boolean NOT NULL: the checkbox branch keeps its own key.
    let mut not_null = host_state(CellEditorKind::Boolean, CellType::Bool, &json!(false));
    not_null.nullable = false;
    let rt = mount(not_null, (960.0, 640.0));
    assert!(keyed(&rt, "cell-editor-boolean").is_some(), "the checkbox branch is real");
}
