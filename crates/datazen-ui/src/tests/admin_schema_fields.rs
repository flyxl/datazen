//! T064: `components/admin/AdminSchemaFields.tsx` port — the
//! `commandSchema` parse, the visibility rule, labels and placeholders,
//! and the mounted field list.
//!
//! Pure-logic tests call the helpers directly; runtime tests render the
//! component in a probe `Runtime` and read back the laid-out element
//! tree — never the component's own report of itself. Values are
//! asserted on the host state, as T013 does: the same `TextEdit` the
//! field renders is what `on_edit` writes into.

use rune::{Cx, El, Env, Runtime, TextEdit, col};
use serde_json::json;

use crate::components::admin::{
    AdminSchemaField, AdminSchemaFields, admin_schema_field, admin_schema_has_field,
    schema_field_label, schema_properties, visible_schema_fields,
};

const PREFIX: &str = "admin-schema-fields";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn definition_fields() -> Vec<AdminSchemaField> {
    let mut encoding = AdminSchemaField::string_field("encoding");
    encoding.title = Some("Encoding".to_string());
    encoding.example = Some("UTF8".to_string());
    let mut owner = AdminSchemaField::string_field("owner");
    owner.description = Some("Owner".to_string());
    owner.examples = vec!["postgres".to_string()];
    vec![
        AdminSchemaField::string_field("name"),
        encoding,
        owner,
        AdminSchemaField {
            name: "template".to_string(),
            schema_type: Some("boolean".to_string()),
            ..Default::default()
        },
        AdminSchemaField {
            name: "untyped".to_string(),
            schema_type: None,
            ..Default::default()
        },
    ]
}

#[derive(Default)]
struct Scene {
    fields: Vec<AdminSchemaField>,
    values: Vec<(String, TextEdit)>,
    exclude: Vec<String>,
    labels: Vec<(String, String)>,
    gap: f32,
    dark: bool,
}

impl Scene {
    fn with(fields: Vec<AdminSchemaField>) -> Self {
        Self {
            fields,
            exclude: vec!["name".to_string()],
            dark: true,
            ..Self::default()
        }
    }

    fn value(&self, name: &str) -> &str {
        self.values
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, edit)| edit.text.as_str())
            .unwrap_or("")
    }
}

fn field_edit<'a>(scene: &'a mut Scene, name: &str) -> &'a mut TextEdit {
    if let Some(index) = scene.values.iter().position(|(n, _)| n == name) {
        return &mut scene.values[index].1;
    }
    scene
        .values
        .push((name.to_string(), TextEdit::new(String::new())));
    let index = scene.values.len() - 1;
    &mut scene.values[index].1
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        AdminSchemaFields::<Scene>::new(scene.fields.clone(), field_edit)
            .exclude(scene.exclude.iter().cloned())
            .labels(scene.labels.iter().cloned())
            .gap(scene.gap)
            .build(&scene.values, scene.dark),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        AdminSchemaFields::<Scene>::new(scene.fields.clone(), field_edit)
            .key("custom-fields")
            .exclude(scene.exclude.iter().cloned())
            .build(&scene.values, scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    mount_with(scene, size, view)
}

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn type_into<S>(rt: &mut Runtime<S>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.settle_all();
    rt.frame(0.0);
}

fn row_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(&format!("{PREFIX}-field-")))
        .collect()
}

// ---- pure model: the commandSchema parse ----

#[test]
fn schema_properties_parses_the_typed_keys_and_ignores_the_rest() {
    // `serde_json` without `preserve_order` sorts object keys, so the
    // parse is asserted by lookup, not order — order is the host Vec's
    // job (see `schema_properties`).
    let fields = schema_properties(&json!({
        "properties": {
            "name": {},
            "encoding": {"type": "string", "title": "Encoding", "example": "UTF8", "examples": ["LATIN1", "UTF8"]},
            "owner": {"type": "string", "description": "Owner"},
            "template": {"type": "boolean"},
            "weird": 42,
            "typed": {"type": 7}
        },
        "required": ["name", "encoding"]
    }));
    assert_eq!(fields.len(), 6);
    let field = admin_schema_field(Some(&fields), "encoding").expect("field is parsed");
    assert_eq!(field.schema_type.as_deref(), Some("string"));
    assert_eq!(field.title.as_deref(), Some("Encoding"));
    assert_eq!(field.placeholder(), Some("LATIN1"));
    let field = admin_schema_field(Some(&fields), "owner").expect("field is parsed");
    assert_eq!(field.label(), "Owner");
    // No `type` at all renders an input (the guard's `!= null` arm).
    let field = admin_schema_field(Some(&fields), "name").expect("field is parsed");
    assert_eq!(field.schema_type, None);
    assert!(field.renders_input());
    // A non-object property parses as an empty schema.
    let field = admin_schema_field(Some(&fields), "weird").expect("field is parsed");
    assert_eq!(field.schema_type, None);
    assert_eq!(field.label(), "weird");
    // A non-string `type` is dropped, not carried as a string.
    let field = admin_schema_field(Some(&fields), "typed").expect("field is parsed");
    assert_eq!(field.schema_type, None);
}

#[test]
fn schema_properties_needs_an_object_of_properties() {
    assert!(schema_properties(&json!({})).is_empty());
    assert!(schema_properties(&json!({"properties": [1, 2]})).is_empty());
    assert!(schema_properties(&json!({"properties": "nope"})).is_empty());
    assert!(schema_properties(&json!({"properties": null})).is_empty());
}

#[test]
fn examples_fall_back_to_example_when_the_first_entry_is_not_a_string() {
    let fields = schema_properties(&json!({
        "properties": {
            "a": {"type": "string", "examples": [123, "x"], "example": "fallback"},
            "b": {"type": "string", "examples": ["first", "second"]},
            "c": {"type": "string", "example": "only"}
        }
    }));
    // React's `fieldPlaceholder` reads `examples[0]` only when it is a
    // string, else `example` — the non-string first entry must not
    // promote the later string into the placeholder.
    assert_eq!(
        admin_schema_field(Some(&fields), "a")
            .expect("field is parsed")
            .placeholder(),
        Some("fallback")
    );
    assert_eq!(
        admin_schema_field(Some(&fields), "b")
            .expect("field is parsed")
            .placeholder(),
        Some("first")
    );
    assert_eq!(
        admin_schema_field(Some(&fields), "c")
            .expect("field is parsed")
            .placeholder(),
        Some("only")
    );
    assert_eq!(
        AdminSchemaField::string_field("x").placeholder(),
        None,
        "no example at all means no placeholder"
    );
}

#[test]
fn schema_field_label_prefers_the_override_then_title_description_name() {
    let mut titled = AdminSchemaField::string_field("encoding");
    titled.title = Some("Encoding".to_string());
    titled.description = Some("Enc".to_string());
    let mut described = AdminSchemaField::string_field("owner");
    described.description = Some("Owner".to_string());
    let plain = AdminSchemaField::string_field("name");

    assert_eq!(schema_field_label(&titled, &[]), "Encoding");
    assert_eq!(schema_field_label(&described, &[]), "Owner");
    assert_eq!(schema_field_label(&plain, &[]), "name");
    assert_eq!(
        schema_field_label(&titled, &[("encoding".to_string(), "字符编码".to_string())]),
        "字符编码",
        "the caller's override wins over the title"
    );
    assert_eq!(
        schema_field_label(&titled, &[("encoding".to_string(), String::new())]),
        "",
        "an empty override still wins: React's `??` only skips nullish"
    );
    assert_eq!(
        schema_field_label(&titled, &[("owner".to_string(), "Other".to_string())]),
        "Encoding",
        "an override for another field does not apply"
    );
}

#[test]
fn visible_schema_fields_drops_excluded_and_non_string_fields_in_order() {
    let fields = definition_fields();
    let visible = visible_schema_fields(&fields, &["name".to_string()]);
    assert_eq!(
        visible.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        vec!["encoding", "owner", "untyped"],
        "the boolean field drops, the untyped field stays, order is kept"
    );
    let visible = visible_schema_fields(&fields, &[]);
    assert_eq!(
        visible.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        vec!["name", "encoding", "owner", "untyped"]
    );
    assert!(visible_schema_fields(&[], &["name".to_string()]).is_empty());
}

#[test]
fn admin_schema_lookups_handle_no_definition() {
    assert!(!admin_schema_has_field(None, "encoding"));
    assert_eq!(admin_schema_field(None, "encoding"), None);
    let fields = definition_fields();
    assert!(admin_schema_has_field(Some(&fields), "encoding"));
    assert!(!admin_schema_has_field(Some(&fields), "missing"));
    assert_eq!(
        admin_schema_field(Some(&fields), "owner")
            .expect("field is parsed")
            .name,
        "owner"
    );
    assert_eq!(admin_schema_field(Some(&fields), "missing"), None);
}

// ---- mounted field list: order, labels, exclusion ----

#[test]
fn the_visible_fields_render_in_schema_order_with_labels() {
    let rt = mount(Scene::with(definition_fields()), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    assert_eq!(
        row_keys(&rt),
        vec![
            format!("{PREFIX}-field-encoding"),
            format!("{PREFIX}-field-encoding-label"),
            format!("{PREFIX}-field-encoding-input"),
            format!("{PREFIX}-field-owner"),
            format!("{PREFIX}-field-owner-label"),
            format!("{PREFIX}-field-owner-input"),
            format!("{PREFIX}-field-untyped"),
            format!("{PREFIX}-field-untyped-label"),
            format!("{PREFIX}-field-untyped-input"),
        ]
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-field-encoding-label")),
        "Encoding",
        "the title is the label"
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-field-owner-label")),
        "Owner",
        "the description is the label when there is no title"
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-field-untyped-label")),
        "untyped",
        "no title or description falls back to the name"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-field-name")));
    assert!(!has_key(&rt, &format!("{PREFIX}-field-template")));
}

#[test]
fn a_label_override_replaces_the_computed_label() {
    let mut scene = Scene::with(definition_fields());
    scene.labels = vec![("encoding".to_string(), "字符编码".to_string())];
    let rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-field-encoding-label")),
        "字符编码"
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-field-owner-label")),
        "Owner",
        "fields without an override keep their computed label"
    );
}

#[test]
fn typing_in_a_field_reaches_the_host_state() {
    let mut scene = Scene::with(definition_fields());
    scene.values = vec![("encoding".to_string(), TextEdit::new("UTF8".to_string()))];
    let mut rt = mount(scene, (800.0, 600.0));
    assert_eq!(rt.state.value("encoding"), "UTF8");
    // `paste` inserts at the caret (the end after the click) — it
    // appends, it does not replace the draft.
    type_into(&mut rt, &format!("{PREFIX}-field-encoding-input"), "LATIN1");
    assert_eq!(rt.state.value("encoding"), "UTF8LATIN1");
    // Typing into one field leaves the others alone.
    type_into(&mut rt, &format!("{PREFIX}-field-owner-input"), "alice");
    assert_eq!(rt.state.value("owner"), "alice");
    assert_eq!(rt.state.value("encoding"), "UTF8LATIN1");
}

#[test]
fn a_field_with_no_edit_renders_an_empty_input() {
    let rt = mount(Scene::with(definition_fields()), (800.0, 600.0));
    assert!(has_key(&rt, &format!("{PREFIX}-field-encoding-input")));
    assert_eq!(rt.state.value("encoding"), "");
    // …and typing into it creates the edit instead of failing.
    let mut rt = rt;
    type_into(&mut rt, &format!("{PREFIX}-field-encoding-input"), "UTF8");
    assert_eq!(rt.state.value("encoding"), "UTF8");
}

#[test]
fn an_empty_definition_renders_the_empty_root() {
    let rt = mount(Scene::with(vec![]), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    assert!(row_keys(&rt).is_empty());
}

#[test]
fn a_custom_key_prefix_moves_every_key_under_it() {
    let rt = mount_with(
        Scene::with(definition_fields()),
        (800.0, 600.0),
        view_custom_key,
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-fields"));
    assert!(has_key(&rt, "custom-fields-field-encoding-label"));
    assert!(has_key(&rt, "custom-fields-field-encoding-input"));
}

#[test]
fn the_gap_builder_spaces_the_rows() {
    let flat = mount(Scene::with(definition_fields()), (800.0, 600.0));
    let mut spaced = Scene::with(definition_fields());
    spaced.gap = 16.0;
    let spaced = mount(spaced, (800.0, 600.0));
    let y = |rt: &Runtime<Scene>, key: &str| element(rt, key).rect[1];
    let flat_gap =
        y(&flat, &format!("{PREFIX}-field-owner")) - y(&flat, &format!("{PREFIX}-field-encoding"));
    let spaced_gap = y(&spaced, &format!("{PREFIX}-field-owner"))
        - y(&spaced, &format!("{PREFIX}-field-encoding"));
    assert!(
        (spaced_gap - flat_gap - 16.0).abs() < 0.5,
        "gap(16) adds exactly 16pt between rows: flat {flat_gap}, spaced {spaced_gap}"
    );
}

#[test]
fn the_fields_fit_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::with(definition_fields());
        scene.dark = dark;
        let rt = mount(scene, size);
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else {
                continue;
            };
            if !key.starts_with(PREFIX) {
                continue;
            }
            let [x, _y, w, _h] = e.rect;
            if x < -0.5 || x + w > size.0 + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{label}: {outside:?} overflow");
    }
}
