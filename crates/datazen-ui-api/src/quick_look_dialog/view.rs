use super::types::{
    QuickLookBody, QuickLookInput, QuickLookModel, QuickLookView, BINARY_TEST_ID, DIALOG_CLASS,
    JSON_TEST_ID, PRE_CLASS,
};
use super::value::{
    binary_value_bytes, is_blob_type, is_json_value, pretty_json, raw_json_text, to_edit_string,
};

pub fn build_quick_look(input: &QuickLookInput, t: &dyn Fn(&str) -> String) -> QuickLookModel {
    let pretty = pretty_json(&input.value);
    let initial_text = pretty
        .clone()
        .unwrap_or_else(|| to_edit_string(&input.value));
    let type_lower = input.column_type.to_lowercase();
    let is_json = type_lower.contains("json") || is_json_value(&input.value);
    let blob_bytes = if is_blob_type(&type_lower) {
        binary_value_bytes(&input.value)
    } else {
        None
    };
    let can_edit = input.editable && input.can_commit;
    let quick_label = t("detail.quickLook");
    let description = if input.column_type.is_empty() {
        quick_label
    } else {
        format!("{quick_label} · {}", input.column_type)
    };
    let body = if let Some(bytes) = &blob_bytes {
        QuickLookBody::Binary {
            bytes: bytes.clone(),
            file_name: input.column_name.clone(),
            test_id: BINARY_TEST_ID,
        }
    } else if can_edit {
        QuickLookBody::Editor {
            text: initial_text.clone(),
        }
    } else if is_json {
        QuickLookBody::Json {
            text: raw_json_text(&input.value),
            test_id: JSON_TEST_ID,
        }
    } else {
        QuickLookBody::Pre {
            text: if initial_text.is_empty() {
                "NULL".to_owned()
            } else {
                initial_text.clone()
            },
            class: PRE_CLASS.to_owned(),
        }
    };
    QuickLookModel {
        view: QuickLookView {
            open: input.open,
            title: input.column_name.clone(),
            description,
            class: DIALOG_CLASS.to_owned(),
            body,
            show_cancel: true,
            show_save: can_edit && blob_bytes.is_none(),
        },
        initial_text,
        pretty,
        type_lower,
        can_commit: input.can_commit,
        blob_bytes,
    }
}
