use serde_json::json;

use super::*;
use crate::{
    build_enum_view, build_json_view, build_long_text_view, enum_key_decision, json_indent_of,
    reformat_json, EnumEditorInput, EnumEditorMode, JsonAction, KeyDecision, LongTextEditorInput,
    CELL_EDITORS_GAP, CELL_EDITORS_SURFACE,
};

fn enum_input() -> EnumEditorInput {
    EnumEditorInput {
        draft: "a".to_owned(),
        candidates: None,
        has_load: true,
        loading: false,
        failed: false,
        message: None,
        load_label: "Load".to_owned(),
        loading_label: "Loading".to_owned(),
        failure_label: "Could not load".to_owned(),
        has_pick: true,
    }
}

fn as_prompt(view: &EnumEditorMode) -> (String, Option<LoadButtonView>, Option<String>) {
    match view {
        EnumEditorMode::Prompt {
            draft,
            load_button,
            failure,
            ..
        } => (draft.clone(), load_button.clone(), failure.clone()),
        EnumEditorMode::Select { .. } => panic!("expected the prompt mode"),
    }
}

fn as_select(view: &EnumEditorMode) -> (Vec<SelectOption>, String, bool) {
    match view {
        EnumEditorMode::Select {
            options,
            selected,
            pick_enabled,
            ..
        } => (options.clone(), selected.clone(), *pick_enabled),
        EnumEditorMode::Prompt { .. } => panic!("expected the select mode"),
    }
}

#[test]
fn no_candidates_shows_a_plain_input_and_an_on_demand_load_button() {
    let view = build_enum_view(&enum_input());
    let (draft, button, failure) = as_prompt(&view);
    assert_eq!(draft, "a");
    let button = button.expect_load();
    assert_eq!(button.label, "Load");
    assert!(!button.disabled);
    assert_eq!(button.test_id, "cell-editor-load-candidates");
    assert_eq!(failure, None);
}

#[test]
fn loading_disables_the_button_and_uses_the_loading_label() {
    let input = EnumEditorInput {
        loading: true,
        ..enum_input()
    };
    let (_, button, _) = as_prompt(&build_enum_view(&input));
    let button = button.expect_load();
    assert_eq!(button.label, "Loading");
    assert!(button.disabled);
}

trait ExpectLoad {
    fn expect_load(self) -> LoadButtonView;
}

impl ExpectLoad for Option<LoadButtonView> {
    fn expect_load(self) -> LoadButtonView {
        match self {
            Some(button) => button,
            None => panic!("expected a load button"),
        }
    }
}

#[test]
fn without_a_load_callback_no_button_is_shown() {
    let input = EnumEditorInput {
        has_load: false,
        ..enum_input()
    };
    let (_, button, _) = as_prompt(&build_enum_view(&input));
    assert!(button.is_none());
}

#[test]
fn a_refused_load_uses_its_own_sentence_or_the_generic_one() {
    let explained = EnumEditorInput {
        failed: true,
        message: Some("uniqueness refusal".to_owned()),
        ..enum_input()
    };
    let (_, _, failure) = as_prompt(&build_enum_view(&explained));
    assert_eq!(failure.as_deref(), Some("uniqueness refusal"));

    let spaced = EnumEditorInput {
        failed: true,
        message: Some("   ".to_owned()),
        ..enum_input()
    };
    let (_, _, failure) = as_prompt(&build_enum_view(&spaced));
    assert_eq!(failure.as_deref(), Some("   "));

    for message in [None, Some(String::new())] {
        let generic = EnumEditorInput {
            failed: true,
            message,
            ..enum_input()
        };
        let (_, _, failure) = as_prompt(&build_enum_view(&generic));
        assert_eq!(failure.as_deref(), Some("Could not load"));
    }

    let quiet = EnumEditorInput {
        message: Some("ignored".to_owned()),
        ..enum_input()
    };
    let (_, _, failure) = as_prompt(&build_enum_view(&quiet));
    assert_eq!(failure, None);
}

#[test]
fn candidates_become_a_searchable_select_with_a_guarded_selection() {
    let input = EnumEditorInput {
        draft: "b".to_owned(),
        candidates: Some(vec!["a".to_owned(), "b".to_owned()]),
        ..enum_input()
    };
    let view = build_enum_view(&input);
    match &view {
        EnumEditorMode::Select {
            test_id,
            searchable,
            ..
        } => {
            assert_eq!(*test_id, "cell-editor-enum");
            assert!(*searchable);
        }
        EnumEditorMode::Prompt { .. } => panic!("expected the select mode"),
    }
    let (options, selected, pick) = as_select(&view);
    assert_eq!(options.len(), 2);
    assert_eq!(
        options[0],
        SelectOption {
            value: "a".to_owned(),
            label: "a".to_owned()
        }
    );
    assert_eq!(selected, "b");
    assert!(pick);
}

#[test]
fn a_draft_outside_the_candidates_selects_nothing() {
    let input = EnumEditorInput {
        draft: "z".to_owned(),
        candidates: Some(vec!["a".to_owned()]),
        has_pick: false,
        ..enum_input()
    };
    let (options, selected, pick) = as_select(&build_enum_view(&input));
    assert_eq!(options.len(), 1);
    assert_eq!(selected, "");
    assert!(!pick);
}

#[test]
fn enter_is_swallowed_and_other_keys_are_forwarded() {
    assert_eq!(enum_key_decision("Enter"), KeyDecision::Swallow);
    assert_eq!(enum_key_decision("Escape"), KeyDecision::Forward);
    assert_eq!(enum_key_decision("Tab"), KeyDecision::Forward);
    assert_eq!(enum_key_decision("enter"), KeyDecision::Forward);
}

#[test]
fn the_json_view_owns_a_multiline_textarea_and_two_labelled_actions() {
    let view = build_json_view(&JsonEditorInput {
        auto_focus: true,
        invalid: true,
        format_label: "Format".to_owned(),
        minify_label: "Minify".to_owned(),
    });
    assert_eq!(view.textarea.rows, 3);
    assert!(view.textarea.auto_focus);
    assert_eq!(view.textarea.aria_invalid, Some(true));
    assert!(!view.textarea.spell_check);
    assert_eq!(view.textarea.test_id, "table-edit-textarea");
    assert_eq!(
        view.actions,
        vec![
            (
                JsonAction::Format,
                "Format".to_owned(),
                "cell-editor-format-json"
            ),
            (
                JsonAction::Minify,
                "Minify".to_owned(),
                "cell-editor-minify-json"
            ),
        ]
    );
    assert_eq!(json_indent_of(JsonAction::Format), 2);
    assert_eq!(json_indent_of(JsonAction::Minify), 0);
}

#[test]
fn a_valid_draft_is_formatted_with_two_space_indentation() {
    let formatted = reformat_json(
        r#"{"a":1,"b":[true,null]}"#,
        json_indent_of(JsonAction::Format),
    );
    let formatted = match formatted {
        Some(text) => text,
        None => panic!("expected a formatted draft"),
    };
    assert_eq!(
        formatted,
        "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null\n  ]\n}"
    );
    let original: serde_json::Value =
        serde_json::from_str(r#"{"a":1,"b":[true,null]}"#).unwrap_or(json!(null));
    let reparsed: serde_json::Value = serde_json::from_str(&formatted).unwrap_or(json!(null));
    assert_eq!(original, reparsed);
}

#[test]
fn minify_produces_the_compact_form_and_keeps_the_meaning() {
    let text = reformat_json(
        "{ \"a\": 1, \"b\": [1, 2] }",
        json_indent_of(JsonAction::Minify),
    );
    assert_eq!(text.as_deref(), Some(r#"{"a":1,"b":[1,2]}"#));
}

#[test]
fn an_unparseable_draft_is_left_untouched() {
    assert_eq!(reformat_json("{", 2), None);
    assert_eq!(reformat_json("not json", 0), None);
}

#[test]
fn a_json_string_is_valid_and_stays_a_quoted_string() {
    assert_eq!(reformat_json("\"abc\"", 0).as_deref(), Some("\"abc\""));
}

#[test]
fn long_text_uses_four_rows_and_the_shared_textarea_contract() {
    let view = build_long_text_view(&LongTextEditorInput {
        auto_focus: false,
        invalid: false,
    });
    assert_eq!(view.rows, 4);
    assert!(!view.auto_focus);
    assert_eq!(view.aria_invalid, None);
    assert!(!view.spell_check);
    assert_eq!(view.test_id, "table-edit-textarea");

    let invalid = build_long_text_view(&LongTextEditorInput {
        auto_focus: true,
        invalid: true,
    });
    assert_eq!(invalid.aria_invalid, Some(true));
    assert!(invalid.auto_focus);
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(CELL_EDITORS_SURFACE.len(), 8);
    assert!(CELL_EDITORS_SURFACE.contains(&"JsonCellEditor"));
    assert_eq!(CELL_EDITORS_GAP.len(), 6);
    assert_eq!(crate::CELL_EDITORS_SURFACE.len(), 8);
    assert_eq!(crate::CELL_EDITORS_GAP.len(), 6);
    let _: Option<crate::TextAreaView> = Some(text_area(3, true, false));
}
