use super::*;
use serde_json::json;

fn t(key: &str) -> String {
    format!("T:{key}")
}

fn input() -> QuickLookInput {
    QuickLookInput {
        open: true,
        column_name: "payload".to_owned(),
        column_type: "text".to_owned(),
        value: json!("hello"),
        editable: false,
        can_commit: false,
    }
}

fn model(input: &QuickLookInput) -> QuickLookModel {
    build_quick_look(input, &t)
}

#[test]
fn blob_detection_is_a_case_sensitive_substring_test() {
    assert_eq!(BLOB_TYPE_KEYWORDS, ["blob", "binary", "bytea", "image", "raw"]);
    for name in ["blob", "ximagex", "raw-bytes", "binary_data"] {
        assert!(is_blob_type(name), "{name}");
    }
    for name in ["Blob", "BYTEA", "text", "json", ""] {
        assert!(!is_blob_type(name), "{name}");
    }
    assert!(is_blob_type(&"BYTEA".to_lowercase()));
}

#[test]
fn binary_bytes_only_come_from_all_number_arrays() {
    assert_eq!(
        binary_value_bytes(&json!([0, 1, 255, 256, -1])),
        Some(vec![0, 1, 255, 0, 255])
    );
    assert_eq!(binary_value_bytes(&json!([1, "2"])), None);
    assert_eq!(binary_value_bytes(&json!("bytes")), None);
    assert_eq!(binary_value_bytes(&json!(null)), None);
    assert_eq!(binary_value_bytes(&json!([])), Some(Vec::new()));
}

#[test]
fn json_detection_and_pretty_printing_are_pinned() {
    assert!(is_json_value(&json!({"a": 1})));
    assert!(is_json_value(&json!([1, 2])));
    assert!(is_json_value(&json!("  {\"a\":1}  ")));
    assert!(!is_json_value(&json!(null)));
    assert!(!is_json_value(&json!("[oops")));
    assert!(!is_json_value(&json!("plain text")));
    assert!(!is_json_value(&json!(42)));
    assert!(!is_json_value(&json!(true)));

    let pretty = match pretty_json(&json!("{\"a\":1}")) {
        Some(value) => value,
        None => panic!("structured string must pretty print"),
    };
    assert!(pretty.contains('\n') && pretty.contains("\"a\": 1"));
    assert_eq!(pretty_json(&json!(null)), None);
    assert_eq!(pretty_json(&json!("text")), None);
    assert_eq!(pretty_json(&json!("[broken")), None);
    assert_eq!(pretty_json(&json!(42)), None);
}

#[test]
fn raw_json_and_edit_text_keep_the_wire_shape() {
    assert_eq!(raw_json_text(&json!("{\"a\":1}")), "{\"a\":1}");
    assert_eq!(raw_json_text(&json!(null)), "");
    assert_eq!(raw_json_text(&json!({"a": 1})), "{\"a\":1}");
    assert_eq!(to_edit_string(&json!(null)), "");
    assert_eq!(to_edit_string(&json!("hi")), "hi");
    assert_eq!(to_edit_string(&json!(42)), "42");
    assert_eq!(to_edit_string(&json!(3.0)), "3");
    assert_eq!(to_edit_string(&json!(3.5)), "3.5");
    assert_eq!(to_edit_string(&json!(true)), "true");
    assert_eq!(to_edit_string(&json!({"b": 1, "a": 2})), "{\"a\":2,\"b\":1}");
}

#[test]
fn quick_lookable_values_cover_binary_json_and_long_text() {
    assert!(!is_quick_lookable_value(&json!(null), None));
    assert!(is_quick_lookable_value(&json!("short"), Some("BYTEA")));
    assert!(is_quick_lookable_value(&json!({"a": 1}), None));
    assert!(is_quick_lookable_value(&json!("a".repeat(121)), None));
    assert!(!is_quick_lookable_value(&json!("a".repeat(120)), None));
    assert!(!is_quick_lookable_value(&json!(42), None));
}

#[test]
fn base64_uses_standard_padding() {
    assert_eq!(bytes_to_base64(b""), "");
    assert_eq!(bytes_to_base64(b"f"), "Zg==");
    assert_eq!(bytes_to_base64(b"fo"), "Zm8=");
    assert_eq!(bytes_to_base64(b"foo"), "Zm9v");
    assert_eq!(bytes_to_base64(&[0, 1, 2, 3]), "AAECAw==");
}

#[test]
fn the_dialog_keeps_its_title_description_and_class() {
    let dialog = model(&input()).view;
    assert!(dialog.open);
    assert_eq!(dialog.title, "payload");
    assert_eq!(dialog.description, "T:detail.quickLook · text");
    assert_eq!(dialog.class, DIALOG_CLASS);
    assert!(dialog.show_cancel);
    assert!(!dialog.show_save);

    let untyped = QuickLookInput {
        column_type: String::new(),
        ..input()
    };
    assert_eq!(model(&untyped).view.description, "T:detail.quickLook");
}

#[test]
fn body_precedence_is_binary_then_editor_then_json_then_pre() {
    let mut binary = input();
    binary.column_type = "bytea".to_owned();
    binary.value = json!([1, 2, 3]);
    binary.editable = true;
    binary.can_commit = true;
    let binary_view = model(&binary).view;
    assert!(!binary_view.show_save);
    match binary_view.body {
        QuickLookBody::Binary { bytes, file_name, test_id } => {
            assert_eq!(bytes, vec![1, 2, 3]);
            assert_eq!(file_name, "payload");
            assert_eq!(test_id, BINARY_TEST_ID);
        }
        other => panic!("expected a binary viewer, got {other:?}"),
    }

    let mut json_input = input();
    json_input.value = json!({"a": 1});
    json_input.can_commit = true;
    json_input.editable = false;
    match model(&json_input).view.body {
        QuickLookBody::Json { text, test_id } => {
            assert_eq!(text, "{\"a\":1}");
            assert_eq!(test_id, JSON_TEST_ID);
        }
        other => panic!("expected a json viewer, got {other:?}"),
    }

    let mut typed_json = input();
    typed_json.column_type = "jsonb".to_owned();
    typed_json.value = json!("{\"a\":1}");
    match model(&typed_json).view.body {
        QuickLookBody::Json { text, .. } => assert_eq!(text, "{\"a\":1}"),
        other => panic!("expected a json viewer, got {other:?}"),
    }

    match model(&input()).view.body {
        QuickLookBody::Pre { text, class } => {
            assert_eq!(text, "hello");
            assert_eq!(class, PRE_CLASS);
        }
        other => panic!("expected a pre block, got {other:?}"),
    }
}

#[test]
fn editor_initial_text_and_save_button_follow_editability() {
    let mut editor = input();
    editor.value = json!({"a": 1});
    editor.editable = true;
    editor.can_commit = true;
    let built = model(&editor);
    assert!(built.view.show_save);
    match built.view.body {
        QuickLookBody::Editor { text } => assert!(text.contains("\"a\": 1")),
        other => panic!("expected an editor, got {other:?}"),
    }

    let mut nullable = input();
    nullable.value = json!(null);
    let built = model(&nullable);
    assert_eq!(built.initial_text, "");
    match built.view.body {
        QuickLookBody::Pre { text, .. } => assert_eq!(text, "NULL"),
        other => panic!("expected a pre block, got {other:?}"),
    }
}

#[test]
fn save_routes_by_type_and_json_prettiness() {
    let mut json_input = input();
    json_input.value = json!({"a": 1});
    json_input.can_commit = true;
    let json_model = model(&json_input);
    assert_eq!(
        resolve_save(&json_model, "{\"a\":2}").commit,
        Some(CommitValue::Json(json!({"a": 2})))
    );
    assert_eq!(
        resolve_save(&json_model, "oops").commit,
        Some(CommitValue::Text("oops".to_owned()))
    );

    let mut int = input();
    int.column_type = "bigint".to_owned();
    int.can_commit = true;
    let int_model = model(&int);
    assert_eq!(resolve_save(&int_model, "42").commit, Some(CommitValue::Number(42.0)));
    assert!(matches!(resolve_save(&int_model, "x").commit, Some(CommitValue::Number(v)) if v.is_nan()));

    for name in ["serial", "int4", "bigserial"] {
        let mut typed = input();
        typed.column_type = name.to_owned();
        typed.can_commit = true;
        let built = model(&typed);
        assert!(matches!(resolve_save(&built, "7").commit, Some(CommitValue::Number(_))), "{name}");
    }

    let mut boolean = input();
    boolean.column_type = "boolean".to_owned();
    boolean.can_commit = true;
    let bool_model = model(&boolean);
    assert_eq!(resolve_save(&bool_model, "true").commit, Some(CommitValue::Bool(true)));
    assert_eq!(resolve_save(&bool_model, "yes").commit, Some(CommitValue::Bool(false)));

    let mut float = input();
    float.column_type = "numeric".to_owned();
    float.can_commit = true;
    let float_model = model(&float);
    assert_eq!(resolve_save(&float_model, "3.5").commit, Some(CommitValue::Number(3.5)));

    assert_eq!(resolve_save(&int_model, "").commit, Some(CommitValue::Null));
}

#[test]
fn save_without_a_commit_handler_does_nothing() {
    let built = model(&input());
    let outcome = resolve_save(&built, "changed");
    assert_eq!(outcome.commit, None);
    assert!(!outcome.close);
    assert_eq!(resolve_event(QuickLookEvent::Save, &built, "changed"), Vec::new());
}

#[test]
fn blob_save_requests_carry_extension_and_filters() {
    let request = save_blob_request(b"foo", "avatar.png");
    assert_eq!(request.extension, "png");
    assert_eq!(request.upper_extension, "PNG");
    assert_eq!(request.filters, vec!["png".to_owned()]);
    assert_eq!(request.base64, "Zm9v");
    assert_eq!(request.suggested_name, "avatar.png");
    let no_dot = save_blob_request(b"f", "avatar");
    assert_eq!(no_dot.extension, "bin");
    let trailing = save_blob_request(b"f", "avatar.");
    assert_eq!(trailing.extension, "");
}

#[test]
fn events_produce_observable_effects() {
    let built = model(&input());
    assert_eq!(
        resolve_event(QuickLookEvent::Close, &built, "hello"),
        vec![QuickLookEffect::Close]
    );

    let mut commit_input = input();
    commit_input.can_commit = true;
    let commit_model = model(&commit_input);
    assert_eq!(
        resolve_event(QuickLookEvent::Save, &commit_model, "changed"),
        vec![
            QuickLookEffect::Commit(CommitValue::Text("changed".to_owned())),
            QuickLookEffect::Close,
        ]
    );

    let mut blob = input();
    blob.column_type = "blob".to_owned();
    blob.value = json!([102, 111, 111]);
    let blob_model = model(&blob);
    let effects = resolve_event(
        QuickLookEvent::BlobSaveAs {
            suggested_name: "data.bin".to_owned(),
        },
        &blob_model,
        "",
    );
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        QuickLookEffect::BlobSave(request) => assert_eq!(request.base64, "Zm9v"),
        other => panic!("expected a blob save, got {other:?}"),
    }
}

#[test]
fn crate_root_reexports_the_quick_look_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::QuickLookInput>();
    resolves::<crate::QuickLookModel>();
    resolves::<crate::QuickLookView>();
    resolves::<crate::CommitValue>();
    resolves::<crate::QuickLookEvent>();
    resolves::<crate::QuickLookEffect>();
    assert!(crate::is_blob_type("bytea"));
    assert_eq!(crate::bytes_to_base64(b"foo"), "Zm9v");
    assert_eq!(crate::BINARY_TEST_ID, "detail-quick-look-binary");
}
