//! T064: `components/admin/AdminSchemaFields.tsx` ported to Rune.
//!
//! React renders one labelled text input per string-typed property of a
//! driver command's input schema (`AdminSchemaFields.tsx:20–37`), in
//! schema order, skipping whatever `exclude` names — the one caller,
//! `AdminCreateDialog`, passes `['name']` — and skipping non-string
//! fields (`renderField`'s `field.schema.type !== 'string' &&
//! field.schema.type != null` guard). The label is the caller's
//! override, else `fieldLabel` (`title`, else `description`, else the
//! field name); the placeholder is `fieldPlaceholder` (the first
//! `examples` entry, else `example`).
//!
//! The field model is shared with T013: [`AdminSchemaField`] lives in
//! `super::admin_create_dialog` and already carries the guard
//! ([`AdminSchemaField::renders_input`]), the label
//! ([`AdminSchemaField::label`]) and the placeholder
//! ([`AdminSchemaField::placeholder`]). This module adds the two
//! `commandSchema.ts` helpers those fields come from —
//! [`schema_properties`] (the `properties` → fields parse) and the
//! `adminSchemaHasField` / `adminSchemaField` lookups — plus the
//! `exclude` + string-only visibility rule and the component itself.
//! The `required` flag `schemaProperties` computes
//! (`commandSchema.ts:14–23`) is deliberately not carried: no consumer
//! of these two components reads it, and the T013 model has no slot for
//! it.
//!
//! The component is stateless, like React: the host owns the definition
//! (parsed with [`schema_properties`]) and the values. Values arrive
//! as the host's `Vec<(String, TextEdit)>` — the Rune shape of React's
//! `values: Record<string, string>` — and edits flow back through the
//! `on_edit` accessor (`fn(&mut S, &str) -> &mut TextEdit`), the same
//! shape `FilterBetweenInput` uses. The production consumer is the
//! `CreateDatabaseDialog` / `CreateSchemaDialog` wiring task: it owns
//! an `AdminCreateDialogState` and will swap that dialog's local
//! schema-field rendering for this component. Until then the component
//! tests are the only callers, so reusable API items carry per-item
//! `#[allow(dead_code)]` (not a module-wide blanket).
//!
//! ## Keys (prefix defaults to `admin-schema-fields`)
//!
//! | part | key |
//! |---|---|
//! | root | `<prefix>` |
//! | field `F` row | `<prefix>-field-{F}` |
//! | field `F` label | `<prefix>-field-{F}-label` |
//! | field `F` input | `<prefix>-field-{F}-input` |
//!
//! ## Deliberate deviations
//!
//! * **No `definition?: DriverCommandDefinition` prop.** React takes
//!   the whole definition and renders nothing when it is absent; here
//!   the host parses it first (with [`schema_properties`]) and passes
//!   the fields. An empty field list renders the empty root — the same
//!   nothing React renders for a definition with no visible fields.
//! * **The input key differs from T013's local rendering.** The dialog
//!   view names its field input `admin-create-field-{F}`; this
//!   component names it `{prefix}-field-{F}-input` (the row takes the
//!   bare `{prefix}-field-{F}` key). The wiring task must update the
//!   dialog tests when it swaps the rendering in.
//! * **No inter-field gap by default.** React renders a fragment, so
//!   fields stack with no gap; the component's `.gap()` defaults to
//!   `0.0`. `AdminCreateDialog`'s body col spaces its children by
//!   16pt, so the wiring task should pass `.gap(16.0)` to reproduce
//!   that dialog's spacing.

use rune::{El, TextEdit, col, text};
use rune_kit::Kit;
use serde_json::Value;

use super::admin_create_dialog::AdminSchemaField;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// The React `Input` height (T013's `FIELD_HEIGHT`).
const FIELD_HEIGHT: f32 = 36.0;

/// `schemaProperties` (`lib/commandSchema.ts:14–23`): the definition's
/// `inputSchema.properties` in schema order. `properties` must be an
/// object (an array or anything else yields no fields); a property whose
/// value is not an object parses as an empty schema, exactly as React's
/// `(raw && typeof raw === 'object' ? raw : {})` does. Only the keys
/// the field model carries are read — `type`, `title`, `description`,
/// `example`, `examples` — so `required` and every other JSON-schema
/// key are dropped here (see the module docs).
///
/// Ordering: React's `Object.entries` keeps the driver's schema order,
/// but this workspace's `serde_json` has no `preserve_order`, so the
/// fields come back in key-sorted order. A host that needs the driver's
/// order must feed the fields from an order-preserving reader instead
/// of this helper.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub fn schema_properties(input_schema: &Value) -> Vec<AdminSchemaField> {
    let Some(properties) = input_schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    properties
        .iter()
        .map(|(name, raw)| {
            let raw = raw.as_object();
            let get_str = |key: &str| raw.and_then(|o| o.get(key)).and_then(Value::as_str);
            // `fieldPlaceholder` reads only `examples[0]` when it is a
            // string, else `example` — so when the first entry is not a
            // string the list is empty and `placeholder()` falls through
            // to `example`, exactly as React does.
            let examples = raw
                .and_then(|o| o.get("examples"))
                .and_then(Value::as_array)
                .filter(|array| array.first().is_some_and(Value::is_string))
                .map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            AdminSchemaField {
                name: name.clone(),
                schema_type: get_str("type").map(str::to_string),
                title: get_str("title").map(str::to_string),
                description: get_str("description").map(str::to_string),
                example: get_str("example").map(str::to_string),
                examples,
            }
        })
        .collect()
}

/// `adminSchemaHasField` (`AdminSchemaFields.tsx:64–70`):
/// `hasSchemaField` over the parsed fields; no definition means no field.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub fn admin_schema_has_field(definition: Option<&[AdminSchemaField]>, name: &str) -> bool {
    definition.map_or(false, |fields| {
        fields.iter().any(|field| field.name == name)
    })
}

/// `adminSchemaField` (`AdminSchemaFields.tsx:72–78`): `getSchemaField`
/// over the parsed fields; no definition means no field.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub fn admin_schema_field<'a>(
    definition: Option<&'a [AdminSchemaField]>,
    name: &str,
) -> Option<&'a AdminSchemaField> {
    definition.and_then(|fields| fields.iter().find(|field| field.name == name))
}

/// The fields the component renders: the definition minus `exclude`,
/// minus non-string fields — `AdminSchemaFields.tsx:43–44` plus
/// `renderField`'s guard (`:22`), in definition order. This is the
/// same rule T013's `AdminCreateDialogState::visible_fields` applies
/// for its own `exclude={['name']}`; the dialog keeps its own copy
/// until the wiring task swaps its rendering for this component.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub fn visible_schema_fields<'a>(
    fields: &'a [AdminSchemaField],
    exclude: &[String],
) -> Vec<&'a AdminSchemaField> {
    fields
        .iter()
        .filter(|field| !excluded(exclude, &field.name) && field.renders_input())
        .collect()
}

fn excluded(exclude: &[String], name: &str) -> bool {
    exclude.iter().any(|excluded| excluded == name)
}

/// The label a field renders with: the caller's override, else
/// `fieldLabel` (`AdminSchemaFields.tsx:27`). An override of `""` wins
/// over the computed label, as React's `labelOverride ?? fieldLabel`
/// does — `""` is not nullish.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub fn schema_field_label<'a>(
    field: &'a AdminSchemaField,
    labels: &'a [(String, String)],
) -> &'a str {
    labels
        .iter()
        .find(|(name, _)| name == &field.name)
        .map(|(_, label)| label.as_str())
        .unwrap_or_else(|| field.label())
}

/// React's `onChange(name, value)` on the app's `S`, as the `TextEdit`
/// accessor `TextField::on_edit` needs: the edit the field writes into.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub type SchemaFieldEditFn<S> = for<'a, 'b> fn(&'a mut S, &'b str) -> &'a mut TextEdit;

/// The schema fields: one labelled text input per visible field, in
/// definition order. The host owns the fields and the values; this
/// component only renders them.
#[allow(dead_code)] // reusable component; wired by the create dialogs
pub struct AdminSchemaFields<S: 'static> {
    key: String,
    fields: Vec<AdminSchemaField>,
    exclude: Vec<String>,
    labels: Vec<(String, String)>,
    gap: f32,
    on_edit: SchemaFieldEditFn<S>,
}

#[allow(dead_code)] // reusable component; wired by the create dialogs
impl<S: 'static> AdminSchemaFields<S> {
    /// React `<AdminSchemaFields definition={…} onChange={…} />` with
    /// the definition already parsed (see [`schema_properties`]); the
    /// values arrive in [`AdminSchemaFields::build`].
    pub fn new(fields: Vec<AdminSchemaField>, on_edit: SchemaFieldEditFn<S>) -> Self {
        Self {
            key: "admin-schema-fields".to_string(),
            fields,
            exclude: Vec::new(),
            labels: Vec::new(),
            gap: 0.0,
            on_edit,
        }
    }

    /// Override the key prefix (two field lists under one parent need
    /// distinct keys; the default is one fixed string).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// React's `exclude` prop: field names that never render
    /// (`AdminCreateDialog` passes `['name']`).
    pub fn exclude(mut self, exclude: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.exclude = exclude.into_iter().map(Into::into).collect();
        self
    }

    /// React's `labels` prop: one override per field name, in the
    /// order given (a `Vec` keeps the render order deterministic).
    pub fn labels(
        mut self,
        labels: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.labels = labels
            .into_iter()
            .map(|(name, label)| (name.into(), label.into()))
            .collect();
        self
    }

    /// The gap between field rows. React renders a fragment (no gap);
    /// `AdminCreateDialog`'s body col spaces its children by 16pt, so
    /// the wiring task should pass `16.0` there.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Builds the field list: the visible fields in definition order,
    /// each a label over its input (`AdminSchemaFields.tsx:20–37`).
    /// `values` is the host's current edits by field name — the Rune
    /// shape of React's `values` prop. An empty field list renders the
    /// empty root, as React renders nothing for a definition with no
    /// visible fields.
    pub fn build(self, values: &[(String, TextEdit)], dark: bool) -> El {
        let k = kit(dark);
        let mut root = col().key(self.key.clone()).w_full().gap(self.gap);
        for field in visible_schema_fields(&self.fields, &self.exclude) {
            root = root.child(self.field_row(&k, values, field, dark));
        }
        root
    }

    /// One field (`AdminSchemaFields.tsx:20–37` as rendered through
    /// the dialog in T013): the label — the caller's override, else
    /// `fieldLabel` — over the input with the `fieldPlaceholder`
    /// example, if the driver sent one. A field with no edit in
    /// `values` renders an empty input, as T013's view does when the
    /// host never set the definition.
    fn field_row(
        &self,
        k: &Kit,
        values: &[(String, TextEdit)],
        field: &AdminSchemaField,
        dark: bool,
    ) -> El {
        let empty = TextEdit::new(String::new());
        let edit = values
            .iter()
            .find(|(name, _)| name == &field.name)
            .map(|(_, edit)| edit)
            .unwrap_or(&empty);
        let edit_fn = self.on_edit;
        let field_name = field.name.clone();
        let mut input = TextField::new(format!("{}-field-{}-input", self.key, field.name), edit)
            .height(FIELD_HEIGHT)
            .on_edit(move |state: &mut S| edit_fn(state, &field_name));
        if let Some(placeholder) = field.placeholder() {
            input = input.placeholder(placeholder.to_string());
        }
        col()
            .key(format!("{}-field-{}", self.key, field.name))
            .w_full()
            .gap(4.0)
            .child(
                text(schema_field_label(field, &self.labels).to_string())
                    .key(format!("{}-field-{}-label", self.key, field.name))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(input.build(dark))
    }
}
