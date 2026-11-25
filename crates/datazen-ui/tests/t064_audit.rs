//! T064 audit: independent checks on the AdminSchemaFields /
//! PrivilegeSelector port — the seams the coder tests do not pin:
//! the key-sorted parse order, the non-string `type` conflation,
//! the array-rejection arms of the `x-datazen` parse, the mounted
//! placeholder, the labelled-group spacing, and the same-label key
//! collision at runtime.
//!
//! Pure-logic tests call the helpers directly; runtime tests render in
//! a probe `Runtime` and read back the laid-out element tree.

use rune::{Cx, El, Env, Runtime, TextEdit, col};
use serde_json::json;

use datazen_ui::components::admin::{
    AdminSchemaField, AdminSchemaFields, PrivilegeGroup, PrivilegeSelector, admin_schema_field,
    privilege_groups, schema_properties,
};

const SCHEMA_PREFIX: &str = "admin-schema-fields";
const PRIV_PREFIX: &str = "privilege-selector";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// The accessible label of the inner `input` child of the field keyed
/// `key` — the kit names an input from its placeholder when the field
/// has no label, so the placeholder is observable here, not as text
/// (same shape as the T013 audit's `input_label`).
fn input_label<S>(rt: &Runtime<S>, key: &str) -> Option<String> {
    let parent = element(rt, key).path;
    let prefix = format!("{parent}/value/input");
    rt.elements()
        .into_iter()
        .find(|e| e.path == prefix)
        .and_then(|e| e.label)
}

// ---- AdminSchemaFields: parse order, type conflation, labels ----------

#[test]
fn audit_schema_properties_returns_key_sorted_order() {
    // React's `Object.entries` keeps the driver's schema order; this
    // workspace's `serde_json` has no `preserve_order`, so the parse
    // comes back key-sorted (see `schema_properties`). Pin the sorted
    // order so a future preserve_order change shows up here, not in a
    // silent render-order change. For the known `create_database`
    // schema the orders coincide once `name` is excluded
    // (`encoding`, `owner` either way).
    let fields = schema_properties(&json!({
        "properties": {
            "zebra": {"type": "string"},
            "apple": {"type": "string"},
            "mango": {"type": "string"}
        }
    }));
    assert_eq!(
        fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        vec!["apple", "mango", "zebra"]
    );
}

#[test]
fn audit_non_string_type_parses_as_untyped_and_renders() {
    // A non-string `type` is dropped by the parse, so the field looks
    // untyped — and untyped fields render. React's `renderField`
    // guard (`type !== 'string' && type != null`) would NOT render
    // either of these: a numeric `type` and an array `type`
    // (`["string", "null"]` is valid JSON Schema) are both non-null
    // and non-`"string"`. No driver in this repo sends such a type
    // (the `commandSchema` fixture uses plain strings), so this pins
    // the current behaviour rather than demanding a fix — but the
    // divergence is real and lives in the shared T013 model
    // (`renders_input` treats `None` as "renders").
    for raw_type in [json!(7), json!(["string", "null"])] {
        let fields = schema_properties(&json!({
            "properties": { "odd": {"type": raw_type} }
        }));
        let field = admin_schema_field(Some(&fields), "odd").expect("field is parsed");
        assert_eq!(field.schema_type, None);
        assert!(
            field.renders_input(),
            "a {raw_type} type renders in Rune; React would skip it"
        );
    }
}

#[test]
fn audit_null_property_parses_as_empty_schema_and_renders() {
    // React: `(null && typeof null === 'object' ? null : {})` is `{}`,
    // so a null property renders with the name as its label.
    let fields = schema_properties(&json!({
        "properties": { "a": null }
    }));
    let field = admin_schema_field(Some(&fields), "a").expect("field is parsed");
    assert_eq!(field.schema_type, None);
    assert!(field.renders_input());
    assert_eq!(field.label(), "a");
}

#[test]
fn audit_empty_title_and_description_fall_through() {
    // React's `fieldLabel` uses truthiness, so an empty title falls
    // through to the description and an empty description to the name.
    let fields = schema_properties(&json!({
        "properties": {
            "a": {"type": "string", "title": "", "description": "Desc"},
            "b": {"type": "string", "title": "", "description": ""}
        }
    }));
    assert_eq!(
        admin_schema_field(Some(&fields), "a")
            .expect("field is parsed")
            .label(),
        "Desc"
    );
    assert_eq!(
        admin_schema_field(Some(&fields), "b")
            .expect("field is parsed")
            .label(),
        "b"
    );
}

#[test]
fn audit_examples_empty_array_and_mixed_tail() {
    // An empty `examples` list falls back to `example` (React:
    // `examples[0]` is `undefined`, not a string); a string first
    // entry wins even when a later entry is not a string.
    let fields = schema_properties(&json!({
        "properties": {
            "a": {"type": "string", "examples": [], "example": "fallback"},
            "b": {"type": "string", "examples": ["first", 9], "example": "fallback"}
        }
    }));
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
}

// ---- AdminSchemaFields: mounted placeholder ----------------------------

#[derive(Default)]
struct SchemaScene {
    fields: Vec<AdminSchemaField>,
    values: Vec<(String, TextEdit)>,
}

fn schema_field_edit<'a>(scene: &'a mut SchemaScene, name: &str) -> &'a mut TextEdit {
    if let Some(index) = scene.values.iter().position(|(n, _)| n == name) {
        return &mut scene.values[index].1;
    }
    scene
        .values
        .push((name.to_string(), TextEdit::new(String::new())));
    let index = scene.values.len() - 1;
    &mut scene.values[index].1
}

fn schema_view(scene: &SchemaScene, _cx: &mut Cx<SchemaScene>) -> El {
    col().w_full().h_full().child(
        AdminSchemaFields::<SchemaScene>::new(scene.fields.clone(), schema_field_edit)
            .build(&scene.values, true),
    )
}

#[test]
fn audit_placeholder_reaches_the_mounted_input() {
    let mut encoding = AdminSchemaField::string_field("encoding");
    encoding.examples = vec!["UTF8".to_string()];
    let scene = SchemaScene {
        fields: vec![encoding],
        values: vec![],
    };
    let mut rt = Runtime::new(scene, schema_view, (800.0, 600.0), env(true));
    rt.frame(0.0);
    assert_eq!(
        input_label(&rt, &format!("{SCHEMA_PREFIX}-field-encoding-input")),
        Some("UTF8".to_string()),
        "the first example is the mounted input's placeholder"
    );
}

// ---- PrivilegeSelector: parse rejection arms ---------------------------

#[test]
fn audit_privilege_groups_rejects_arrays_and_non_array_privileges() {
    // React's `readDatazenExtension` rejects an array `x-datazen`
    // (`typeof [] === 'object'` but `Array.isArray` is true), and
    // `privilegeGroups` skips array entries and drops a group whose
    // `privileges` is not an array.
    assert!(privilege_groups(&json!({"x-datazen": []})).is_empty());
    let groups = privilege_groups(&json!({"x-datazen": {"privilegeGroups": [
        ["SELECT"],
        {"label": "X", "privileges": "SELECT"},
        {"label": "Y", "privileges": ["USAGE"]}
    ]}}));
    assert_eq!(groups.len(), 1, "only the well-formed group survives");
    assert_eq!(groups[0].label, "Y");
    let groups = privilege_groups(&json!({"x-datazen": {"privilegeGroups": [
        {"label": null, "privileges": ["USAGE"]}
    ]}}));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].label, "", "a null label becomes \"\"");
}

#[test]
fn audit_same_label_groups_share_a_key() {
    // React's `key={group.label || 'default'}` collides for two
    // groups with the same label; the port's `PrivilegeGroup::key`
    // collides the same way.
    let a = PrivilegeGroup::new("Tables", ["SELECT"]);
    let b = PrivilegeGroup::new("Tables", ["INSERT"]);
    assert_eq!(a.key(), b.key());
    assert_eq!(a.key(), "Tables");
}

// ---- PrivilegeSelector: mounted collision + spacing --------------------

#[derive(Default)]
struct PrivScene {
    groups: Vec<PrivilegeGroup>,
    selected: Vec<String>,
}

fn priv_view(scene: &PrivScene, _cx: &mut Cx<PrivScene>) -> El {
    col().w_full().h_full().child(
        PrivilegeSelector::<PrivScene>::new(
            scene.groups.clone(),
            |s: &PrivScene, privilege: &str| s.selected.iter().any(|n| n == privilege),
            |_s: &mut PrivScene, _privilege: String| {},
        )
        .build(scene, true),
    )
}

fn mount_priv(scene: PrivScene) -> Runtime<PrivScene> {
    let mut rt = Runtime::new(scene, priv_view, (800.0, 600.0), env(true));
    rt.frame(0.0);
    rt
}

#[test]
fn audit_same_label_groups_both_render_their_items() {
    // Two groups with the same label collide on the section key
    // (see the test above); React still renders both sections, so
    // both groups' items must be laid out here too.
    let rt = mount_priv(PrivScene {
        groups: vec![
            PrivilegeGroup::new("Tables", ["SELECT"]),
            PrivilegeGroup::new("Tables", ["INSERT"]),
        ],
        selected: vec![],
    });
    assert_eq!(
        text_of(&rt, &format!("{PRIV_PREFIX}-group-Tables-item-SELECT-name")),
        "SELECT"
    );
    assert_eq!(
        text_of(&rt, &format!("{PRIV_PREFIX}-group-Tables-item-INSERT-name")),
        "INSERT"
    );
}

#[test]
fn audit_labelled_group_spacing_is_twelve_below_and_six_above_grid() {
    // React: a labelled group has `mb-3` (12pt) below the section and
    // the label has `mb-1.5` (6pt) above the grid.
    let rt = mount_priv(PrivScene {
        groups: vec![
            PrivilegeGroup::new("Tables", ["SELECT"]),
            PrivilegeGroup::new("", ["USAGE"]),
        ],
        selected: vec![],
    });
    let section = element(&rt, &format!("{PRIV_PREFIX}-group-Tables"));
    let label = element(&rt, &format!("{PRIV_PREFIX}-group-Tables-label"));
    let grid = element(&rt, &format!("{PRIV_PREFIX}-group-Tables-grid"));
    let next = element(&rt, &format!("{PRIV_PREFIX}-group-default"));
    let grid_above = grid.rect[1] - (label.rect[1] + label.rect[3]);
    assert!(
        (grid_above - 6.0).abs() < 0.6,
        "label-to-grid gap is 6pt, got {grid_above}"
    );
    let below = next.rect[1] - (section.rect[1] + section.rect[3]);
    assert!(
        (below - 12.0).abs() < 0.6,
        "labelled-section bottom margin is 12pt, got {below}"
    );
    assert!(
        has_key(&rt, &format!("{PRIV_PREFIX}-group-default-item-USAGE-name")),
        "the unlabelled group still renders its items"
    );
}
