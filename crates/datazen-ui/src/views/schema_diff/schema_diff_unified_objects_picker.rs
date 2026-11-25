//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffUnifiedObjectsPicker.tsx` (T290) — the unified non-table
//! object picker.
//!
//! Two side lists (source / target) grouped by object kind (view, type,
//! sequence, function, procedure, trigger — the kinds the schema diff
//! compares beyond tables), each kind's block either listing its rows
//! as checkboxes, showing its load error, or showing the empty line
//! (a kind block only renders with rows OR an error). The header
//! offers the retry when any side failed, the cross-dialect note
//! (with the clear-selections escape when something is already
//! selected), and each side's select-all/none row — select-ALL disabled
//! under cross-dialect (the note explains why; the per-row toggles of
//! already-selected rows stay live so a cross-dialect pick can be
//! unmade).
//!
//! ## What maps where
//!
//! - `SchemaDiffObjectIdentity`/`schemaDiffObjectIdentityKey` are
//!   T363's ported single sources ([`SchemaDiffObjectIdentityVm`] /
//!   [`schema_diff_object_identity_key`]).
//! - `displayObjectIdentity` is the pure label ([`display_object_identity`]):
//!   `schema.name(signature) → targetSchema.targetName`, each optional
//!   half collapsing out.
//! - React's `<details open>` collapse is per-kind static here: the
//!   `view` kind starts open (the engine keeps the group always
//!   rendered; a collapse state machine is a later polish if the UX
//!   needs it — documented, not silent).
//! - React testids stay verbatim (`schema-diff-unified-object-picker`,
//!   `-object-kind-{side}-{kind}`, `-object-error-{side}-{kind}`,
//!   `-object-row-{side}-{kind}-{i}`, `-object-retry`,
//!   `-cross-dialect-objects-note`, `-clear-object-selections`).
//!
//! Known deltas: the kind groups render open (no `<details>` folding);
//! Tailwind's grid lg:grid-cols-2 renders as a two-column row at the
//! fixed desktop width.

use std::rc::Rc;

use rune::{col, row, text, tok, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;
use crate::views::schema_diff::schema_diff_object_identity::{
    schema_diff_object_identity_key, SchemaDiffObjectIdentityVm, SchemaDiffObjectKindVm,
};

/// React `SchemaDiffObjectKind[]` — the picker's kinds, declaration
/// order.
pub const OBJECT_KINDS: [SchemaDiffObjectKindVm; 6] = [
    SchemaDiffObjectKindVm::View,
    SchemaDiffObjectKindVm::Type,
    SchemaDiffObjectKindVm::Sequence,
    SchemaDiffObjectKindVm::Function,
    SchemaDiffObjectKindVm::Procedure,
    SchemaDiffObjectKindVm::Trigger,
];

impl SchemaDiffObjectKindVm {
    /// The wire/id spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Type => "type",
            Self::Sequence => "sequence",
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
        }
    }

    /// The i18n key of the kind's label.
    fn label_key(self) -> String {
        format!("schemaDiff.objectKind.{}", self.as_str())
    }
}

/// `displayObjectIdentity` — the row's mono label: `schema.name
/// (signature) → targetSchema.targetName`, each optional half
/// collapsing out.
pub fn display_object_identity(object: &SchemaDiffObjectIdentityVm) -> String {
    let name = match &object.schema {
        Some(schema) => format!("{schema}.{}", object.name),
        None => object.name.clone(),
    };
    let signature = object
        .signature
        .as_ref()
        .map(|signature| format!("({signature})"))
        .unwrap_or_default();
    let target = object.target_name.as_ref().map(|target_name| {
        let prefix = object
            .target_schema
            .as_ref()
            .map(|schema| format!("{schema}."))
            .unwrap_or_default();
        format!(" → {prefix}{target_name}")
    }).unwrap_or_default();
    format!("{name}{signature}{target}")
}

/// One side's per-kind load errors (React `Partial<Record<kind, error>>`
/// — a per-kind lookup, not an ordered set; the kind has no Ord).
pub type ObjectLoadErrors = Vec<(SchemaDiffObjectKindVm, String)>;

/// Which side a callback addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerSide {
    Source,
    Target,
}

impl PickerSide {
    /// The side's id spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }

    /// The side's label key.
    fn label_key(self) -> &'static str {
        match self {
            Self::Source => "schemaDiff.objectSource",
            Self::Target => "schemaDiff.objectTarget",
        }
    }
}

fn error_for(errors: &ObjectLoadErrors, kind: SchemaDiffObjectKindVm) -> Option<&str> {
    errors
        .iter()
        .find(|(error_kind, _)| *error_kind == kind)
        .map(|(_, message)| message.as_str())
}

/// The unified objects picker (React `SchemaDiffUnifiedObjectsPicker`).
pub struct SchemaDiffUnifiedObjectsPicker<'a, S: 'static> {
    loading: bool,
    source_objects: &'a [SchemaDiffObjectIdentityVm],
    target_objects: &'a [SchemaDiffObjectIdentityVm],
    selected_source_keys: &'a [String],
    selected_target_keys: &'a [String],
    source_errors: &'a ObjectLoadErrors,
    target_errors: &'a ObjectLoadErrors,
    cross_dialect: bool,
    on_toggle: [Option<Rc<dyn Fn(&mut S, PickerSide, &SchemaDiffObjectIdentityVm)>>; 2],
    on_select_all: Option<Rc<dyn Fn(&mut S, PickerSide, bool)>>,
    on_clear_selections: Option<Rc<dyn Fn(&mut S)>>,
    on_retry: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> SchemaDiffUnifiedObjectsPicker<'a, S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        loading: bool,
        source_objects: &'a [SchemaDiffObjectIdentityVm],
        target_objects: &'a [SchemaDiffObjectIdentityVm],
        selected_source_keys: &'a [String],
        selected_target_keys: &'a [String],
        source_errors: &'a ObjectLoadErrors,
        target_errors: &'a ObjectLoadErrors,
        cross_dialect: bool,
    ) -> Self {
        Self {
            loading,
            source_objects,
            target_objects,
            selected_source_keys,
            selected_target_keys,
            source_errors,
            target_errors,
            cross_dialect,
            on_toggle: [None, None],
            on_select_all: None,
            on_clear_selections: None,
            on_retry: None,
        }
    }

    /// `onToggleSource(object)`.
    pub fn on_toggle_source(
        mut self,
        f: impl Fn(&mut S, &SchemaDiffObjectIdentityVm) + 'static,
    ) -> Self {
        self.on_toggle[0] = Some(Rc::new(move |s, _, object| f(s, object)));
        self
    }

    /// `onToggleTarget(object)`.
    pub fn on_toggle_target(
        mut self,
        f: impl Fn(&mut S, &SchemaDiffObjectIdentityVm) + 'static,
    ) -> Self {
        self.on_toggle[1] = Some(Rc::new(move |s, _, object| f(s, object)));
        self
    }

    /// `onSelectAll(side, selected)`.
    pub fn on_select_all(
        mut self,
        f: impl Fn(&mut S, PickerSide, bool) + 'static,
    ) -> Self {
        self.on_select_all = Some(Rc::new(f));
        self
    }

    /// `onClearSelections`.
    pub fn on_clear_selections(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_selections = Some(Rc::new(f));
        self
    }

    /// `onRetry`.
    pub fn on_retry(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_retry = Some(Rc::new(f));
        self
    }

    /// Build the picker.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let source_errors_present = !self.source_errors.is_empty();
        let target_errors_present = !self.target_errors.is_empty();
        let any_errors = source_errors_present || target_errors_present;
        let has_selected =
            !self.selected_source_keys.is_empty() || !self.selected_target_keys.is_empty();

        let mut section = col()
            .key("schema-diff-unified-object-picker")
            .w_full()
            .gap(12.0)
            .px(12.0)
            .py(12.0)
            .radius(8.0)
            .border(1.0, k.c.border_base);

        // The header: the title + hint, the retry when any side failed.
        let mut header = row().key("schema-diff-unified-object-picker-head")
            .w_full()
            .justify_between()
            .gap(8.0)
            .flex_wrap();
        header = header.child(
            col().gap(4.0).child(
                text(t("schemaDiff.schemaObjects").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted),
            ),
        );
        if any_errors {
            let mut retry = Button::new(t("schemaDiff.objectRetry"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .disabled(self.loading);
            if let Some(on_retry) = &self.on_retry {
                let on_retry = Rc::clone(on_retry);
                retry = retry.on_press(move |s: &mut S| on_retry(s));
            }
            header = header.child(retry.build(&k).key("schema-diff-object-retry"));
        }
        section = section.child(header);

        // The cross-dialect note (with the clear-selections escape).
        if self.cross_dialect {
            let mut note = col()
                .key("schema-diff-cross-dialect-objects-note")
                .w_full()
                .gap(6.0)
                .px(8.0)
                .py(8.0)
                .radius(6.0)
                .border(1.0, k.c.warning.fg)
                .child(text(t("schemaDiff.crossDialectNote").to_owned()).font(tok::type_::LABEL).color(k.c.text_muted))
                .child(text(t("schemaDiff.crossDialectObjectNote").to_owned()).font(tok::type_::LABEL).color(k.c.text_muted));
            if has_selected {
                let mut clear = Button::new(t("schemaDiff.clearObjectSelections"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small);
                if let Some(on_clear) = &self.on_clear_selections {
                    let on_clear = Rc::clone(on_clear);
                    clear = clear.on_press(move |s: &mut S| on_clear(s));
                }
                note = note.child(clear.build(&k).key("schema-diff-clear-object-selections"));
            }
            section = section.child(note);
        }

        // The partial-load warning line.
        if any_errors {
            section = section.child(
                text(t("schemaDiff.objectPartialLoad").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.warning.fg)
                    .key("schema-diff-object-partial-load"),
            );
        }

        // The body: the loading line or the two side lists.
        if self.loading {
            return section.child(
                text(t("schemaDiff.schemaObjectsLoading").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key("schema-diff-unified-object-picker-loading"),
            );
        }

        let mut grid = row().key("schema-diff-unified-object-picker-grid")
            .w_full()
            .gap(12.0);
        grid = grid.child(self.build_side(&k, PickerSide::Source));
        grid = grid.child(self.build_side(&k, PickerSide::Target));
        section.child(grid)
    }

    fn build_side(&self, k: &Kit, side: PickerSide) -> El {
        let (objects, selected, errors) = match side {
            PickerSide::Source => (self.source_objects, self.selected_source_keys, self.source_errors),
            PickerSide::Target => (self.target_objects, self.selected_target_keys, self.target_errors),
        };
        let selected_set: std::collections::BTreeSet<&str> =
            selected.iter().map(String::as_str).collect();
        let side_index = match side { PickerSide::Source => 0, PickerSide::Target => 1 };

        let mut column = col().key(format!("schema-diff-object-side-{}", side.as_str())).grow(1.0).gap(6.0);

        // The side's selection-count + select-all/none row.
        let count_text = t("schemaDiff.objectSideSelectionCount")
            .replace("{count}", &selected.len().to_string())
            .replace("{total}", &objects.len().to_string());
        let mut controls = row().gap(8.0);
        let (all_key, all_label) = match side {
            PickerSide::Source => ("schema-diff-object-select-all-source", t("schemaDiff.objectSelectAllSource")),
            PickerSide::Target => ("schema-diff-object-select-all-target", t("schemaDiff.objectSelectAllTarget")),
        };
        let mut select_all = Button::new(all_label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.cross_dialect);
        if let Some(on_select_all) = &self.on_select_all {
            let on_select_all = Rc::clone(on_select_all);
            let side_value = side;
            select_all = select_all.on_press(move |s: &mut S| on_select_all(s, side_value, true));
        }
        controls = controls.child(select_all.build(k).key(all_key));
        let (none_key, none_label) = match side {
            PickerSide::Source => ("schema-diff-object-select-none-source", t("schemaDiff.objectSelectNoneSource")),
            PickerSide::Target => ("schema-diff-object-select-none-target", t("schemaDiff.objectSelectNoneTarget")),
        };
        let mut select_none = Button::new(none_label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(on_select_all) = &self.on_select_all {
            let on_select_all = Rc::clone(on_select_all);
            let side_value = side;
            select_none = select_none.on_press(move |s: &mut S| on_select_all(s, side_value, false));
        }
        controls = controls.child(select_none.build(k).key(none_key));

        column = column.child(
            row().key(format!("schema-diff-object-side-head-{}", side.as_str()))
                .w_full()
                .justify_between()
                .center_y()
                .child(text(count_text).font(tok::type_::LABEL).color(k.c.text_muted))
                .child(controls),
        );

        // The side's label.
        column = column.child(
            text(t(side.label_key()).to_owned())
                .font(tok::type_::LABEL)
                .color(k.c.text_muted)
                .key(format!("schema-diff-object-side-label-{}", side.as_str())),
        );

        // The kind blocks.
        let mut list = col()
            .key(format!("schema-diff-object-list-{}", side.as_str()))
            .w_full()
            .gap(4.0);
        for kind in OBJECT_KINDS {
            let rows: Vec<&SchemaDiffObjectIdentityVm> =
                objects.iter().filter(|object| object.kind == kind).collect();
            let error = error_for(errors, kind);
            if rows.is_empty() && error.is_none() {
                continue;
            }
            let kind_id = kind.as_str();
            let mut block = col()
                .key(format!("schema-diff-object-kind-{}-{kind_id}", side.as_str()))
                .w_full()
                .gap(4.0)
                .px(8.0)
                .py(6.0)
                .radius(6.0)
                .border(1.0, k.c.border_base)
                .child(
                    row().gap(8.0).center_y().child(
                        text(t(&kind.label_key()).to_owned())
                            .font(tok::type_::LABEL)
                            .color(k.c.text_muted),
                    )
                    .child(
                        text(rows.len().to_string())
                            .font(tok::type_::LABEL)
                            .color(k.c.text_muted),
                    ),
                );
            if let Some(error) = error {
                block = block.child(
                    text(t("schemaDiff.objectLoadFailed").replace("{error}", error))
                        .font(tok::type_::LABEL)
                        .color(k.c.danger.fg)
                        .key(format!("schema-diff-object-error-{}-{kind_id}", side.as_str())),
                );
            } else if rows.is_empty() {
                block = block.child(
                    text(t("schemaDiff.objectKindEmpty").to_owned())
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                );
            } else {
                let mut rows_list = col().gap(2.0);
                for (index, object) in rows.iter().enumerate() {
                    let key = schema_diff_object_identity_key(object);
                    let label = display_object_identity(object);
                    let checked = selected_set.contains(key.as_str());
                    let mut checkbox = Checkbox::new(
                        format!("schema-diff-object-row-{}-{kind_id}-{index}", side.as_str()),
                        checked,
                    )
                    // Cross-dialect locks UNselected rows (a pick made
                    // before the switch can still be unmade).
                    .disabled(self.cross_dialect && !checked);
                    if let Some(on_toggle) = &self.on_toggle[side_index] {
                        let on_toggle = Rc::clone(on_toggle);
                        let owned = (*object).clone();
                        checkbox = checkbox.on_toggle(move |s: &mut S| {
                            on_toggle(s, side, &owned);
                        });
                    }
                    rows_list = rows_list.child(
                        row().gap(8.0).center_y().child(checkbox.build(k)).child(
                            text(label).font(tok::type_::LABEL).color(k.c.text_muted),
                        ),
                    );
                }
                block = block.child(rows_list);
            }
            list = list.child(block);
        }
        column.child(list)
    }
}
