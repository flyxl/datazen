//! `SchemaDiffPlanPanel`'s type-suggestions notice (T178, the React file's
//! `TypeSuggestionsNotice` internal component).
//!
//! The yellow advisory card for cross-dialect type mismatches: title and
//! description, a four-column table (column / source type / target type /
//! reason) with one editable target-type input per suggestion, and the
//! Apply button that submits the overrides for a plan regeneration.
//!
//! ## What maps where
//!
//! - The per-suggestion input's value is DERIVED — React's
//!   `overrides.find(…)?.targetType ?? sug.currentType ?? sug.suggestedType`
//!   — recomputed every render. The port reads it from the host-mirrored
//!   [`override_edits`](TypeSuggestionsNotice::override_edits) map (keyed
//!   `{table}.{column}`), which the host resyncs whenever the plan or the
//!   overrides change (the `useEffect`-resync convention); React's
//!   `onTypeOverrideChange(table, column, value)` is where an edit lands —
//!   the port's [`on_override_edit`](TypeSuggestionsNotice::on_override_edit)
//!   accessor (entry-or-insert with the derived initial) is that landing.
//! - Without the edit map the row renders the derived value as a frozen
//!   input-styled text — React's controlled input with no `onChange`.
//! - The `<datalist>` of [`COMMON_MYSQL_TEXT_TYPES`] offers editor
//!   suggestions only (free text stays valid); the kit has no datalist
//!   atom, so the input is a plain text field — the seven literals stay
//!   exported for the host to offer through its own completion.
//! - `isKeyOrIndexed` badges the reason in the yellow wash
//!   (`bg-yellow-500/20 text-yellow-300`), else the muted surface chip.
//! - The Apply button (`schema-diff-apply-suggestions`) is the yellow chip
//!   button, disabled while `regenerating`.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: the card
//! `schema-diff-type-suggestions`, each input `type-suggestion-input-{column}`
//! and the Apply button `schema-diff-apply-suggestions`. Table rows key by
//! `{table}.{column}` (React's `key`).
//!
//! Deltas: the ⚠ emoji becomes [`ICON_ALERT_TRIANGLE`] (the error-boundary
//! precedent); the kit table atoms are heavier than this 4-column list, so
//! the rows are hand-laid-out `row`s in a `col` (same borders/dividers);
//! pixel classes → points (×0.75).

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, TextStyle};

use crate::commands::schema_diff::wire::{ColumnTypeOverrideVm, TypeSuggestionVm};
use crate::i18n::t;
use crate::icons::ICON_ALERT_TRIANGLE;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React `COMMON_MYSQL_TEXT_TYPES`: the datalist's suggestions.
pub(crate) const COMMON_MYSQL_TEXT_TYPES: [&str; 7] = [
    "VARCHAR(255)",
    "VARCHAR(128)",
    "VARCHAR(64)",
    "VARCHAR(500)",
    "TEXT",
    "MEDIUMTEXT",
    "LONGTEXT",
];

/// The host's per-suggestion edit slot: `(table, column, initial)` → the
/// mutable [`TextEdit`] the input binds to (entry-or-insert).
pub(super) type OverrideEditFn<S> = Rc<
    dyn for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit,
>;

/// The notice builder.
pub(super) struct TypeSuggestionsNotice<'a, S: 'static> {
    suggestions: &'a [TypeSuggestionVm],
    overrides: &'a [ColumnTypeOverrideVm],
    override_edits: Option<&'a HashMap<String, TextEdit>>,
    on_override_edit: Option<OverrideEditFn<S>>,
    on_apply: Option<Rc<dyn Fn(&mut S) + 'static>>,
    regenerating: bool,
}

impl<'a, S: 'static> TypeSuggestionsNotice<'a, S> {
    pub(super) fn new(
        suggestions: &'a [TypeSuggestionVm],
        overrides: &'a [ColumnTypeOverrideVm],
    ) -> Self {
        Self {
            suggestions,
            overrides,
            override_edits: None,
            on_override_edit: None,
            on_apply: None,
            regenerating: false,
        }
    }

    /// The host-mirrored edit slots (keyed `{table}.{column}`), carrying
    /// the derived `override ?? current ?? suggested` value.
    pub(super) fn override_edits(
        mut self,
        edits: Option<&'a HashMap<String, TextEdit>>,
    ) -> Self {
        self.override_edits = edits;
        self
    }

    /// React's `onOverrideChange` landing: the host's per-suggestion edit
    /// slot accessor (entry-or-insert with the derived initial).
    pub(super) fn on_override_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_override_edit = Some(Rc::new(f));
        self
    }

    /// React's `onApply`.
    pub(super) fn on_apply(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply = Some(Rc::new(f));
        self
    }

    /// React's `regenerating` (the Apply button's disabled state).
    pub(super) fn regenerating(mut self, regenerating: bool) -> Self {
        self.regenerating = regenerating;
        self
    }

    /// The derived input value:
    /// `overrides.find(…)?.targetType ?? currentType ?? suggestedType`.
    fn current_override(&self, sug: &TypeSuggestionVm) -> String {
        self.overrides
            .iter()
            .find(|o| o.table == sug.table && o.column == sug.column)
            .map(|o| o.target_type.clone())
            .or_else(|| Some(sug.current_type.clone()))
            .unwrap_or_else(|| sug.suggested_type.clone())
    }

    /// Paint the card (`None` when there are no suggestions, like React).
    pub(super) fn build(self, dark: bool) -> Option<El> {
        if self.suggestions.is_empty() {
            return None;
        }
        let k = kit(dark);
        let mut card = col()
            .key("schema-diff-type-suggestions")
            .w_full()
            .radius(4.5) // React rounded-lg
            // border-yellow-500/30 + bg-yellow-500/10
            .border(0.75, k.alpha(k.c.warning.fg, 0.30))
            .fill(k.alpha(k.c.warning.fg, 0.10))
            .padding(9.0) // React p-3
            .child(
                row()
                    .key("schema-diff-type-suggestions-title")
                    .gap(4.5) // React gap-1.5
                    .center_y()
                    .child(icon(&ICON_ALERT_TRIANGLE, 9.0).color(k.c.warning.icon))
                    .child(
                        text(t("schemaDiff.typeSuggestions.title").to_owned())
                            .key("schema-diff-type-suggestions-title-text")
                            .font(type_xs_medium())
                            .color(k.c.warning.icon),
                    ),
            )
            .child(
                text(t("schemaDiff.typeSuggestions.desc").to_owned())
                    .key("schema-diff-type-suggestions-desc")
                    .font(type_xs())
                    .color(k.c.text_muted)
                    .mt(3.0), // React mt-1
            );

        // --- The table: header row + one row per suggestion. ---
        let mut table = col()
            .key("schema-diff-type-suggestions-table")
            .w_full()
            .mt(7.5); // React mt-2.5
        let mut header = row()
            .key("schema-diff-type-suggestions-header")
            .w_full()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.alpha(k.c.border_base, 0.60))
            .pb(4.5); // React pb-1.5
        for (key, label) in [
            ("column", "schemaDiff.typeSuggestions.column"),
            ("source", "schemaDiff.typeSuggestions.sourceType"),
            ("target", "schemaDiff.typeSuggestions.targetType"),
        ] {
            header = header.child(
                text(t(label).to_owned())
                    .key(format!("schema-diff-type-suggestions-header-{key}"))
                    .font(header_type()) // React text-[11px] font-normal
                    .color(k.c.text_muted)
                    .pr(9.0), // React pr-3
            );
        }
        header = header.child(
            text(t("schemaDiff.typeSuggestions.reason").to_owned())
                .key("schema-diff-type-suggestions-header-reason")
                .font(header_type())
                .color(k.c.text_muted),
        );
        table = table.child(header);

        for sug in self.suggestions {
            table = table.child(self.suggestion_row(sug, dark));
        }
        card = card.child(table);

        // --- The Apply button, right-aligned. ---
        let mut apply = row()
            .key("schema-diff-apply-suggestions")
            .role("button")
            .label(t("schemaDiff.typeSuggestions.apply").to_owned())
            .radius(3.0) // React rounded
            // bg-yellow-500/20 + text-yellow-300
            .fill(k.alpha(k.c.warning.fg, 0.20))
            .px(7.5) // React px-2.5
            .py(3.0) // React py-1
            .disabled(self.regenerating)
            .child(
                text(t("schemaDiff.typeSuggestions.apply").to_owned())
                    .font(type_xs_medium())
                    .color(k.c.warning.icon),
            );
        if !self.regenerating {
            if let Some(on_apply) = &self.on_apply {
                let on_apply = Rc::clone(on_apply);
                apply = apply.on_press(move |s: &mut S| on_apply(s));
            }
            apply = apply.hover(k.alpha(k.c.warning.fg, 0.30)); // hover:bg-yellow-500/30
        }
        Some(card.child(
            col()
                .key("schema-diff-type-suggestions-actions")
                .w_full()
                .mt(7.5) // React mt-2.5
                .center_y() // justify-end on a column's cross axis → right
                .child(apply),
        ))
    }

    /// One suggestion row: mono column, mono source, the target input, the
    /// reason chip.
    fn suggestion_row(&self, sug: &TypeSuggestionVm, dark: bool) -> El {
        let k = kit(dark);
        let row_key = format!("{}.{}", sug.table, sug.column);
        let mut row_el = row()
            .key(format!("schema-diff-type-suggestion-{row_key}"))
            .w_full()
            .center_y()
            .pt(4.5)
            .pb(4.5) // React py-1.5
            .border_sides([1.0, 0.0, 0.0, 0.0], k.alpha(k.c.border_base, 0.30)); // divide-edge/30
        row_el = row_el
            .child(
                text(row_key.clone())
                    .key(format!("schema-diff-type-suggestion-{row_key}-name"))
                    .font(TextStyle::new(9.0, 500.0).mono()) // font-mono font-medium
                    .color(k.c.text_base)
                    .pr(9.0) // React pr-3
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
            .child(
                text(sug.source_type.clone())
                    .key(format!("schema-diff-type-suggestion-{row_key}-source"))
                    .font(type_xs().mono())
                    .color(k.c.text_muted)
                    .pr(9.0),
            );
        // The target cell: the editable override input when the host wires
        // the edit slots, else the frozen derived value.
        let target_cell = match (
            self.override_edits.and_then(|edits| edits.get(&row_key)),
            &self.on_override_edit,
        ) {
            (Some(edit), Some(accessor)) => {
                // Clone the Rc before the move closure (the match arm's
                // reference borrows self).
                let accessor = Rc::clone(accessor);
                let mut field = TextField::new(
                    format!("type-suggestion-input-{}", sug.column),
                    edit,
                )
                .height(21.0) // React h-7
                .width(96.0) // React w-32
                .pad_x(6.0) // React px-2
                .text_style(type_xs());
                let table = sug.table.clone();
                let column = sug.column.clone();
                let initial = self.current_override(sug);
                field = field.on_edit(move |s: &mut S| accessor(s, &table, &column, &initial));
                field.build(dark)
            }
            _ => {
                // No edit slot: the controlled input without onChange — the
                // derived value frozen in the input's chrome (a TextField
                // needs a live TextEdit reference, so this paints the
                // input-styled box directly).
                let k_frozen = kit(dark);
                text(self.current_override(sug))
                    .key(format!("type-suggestion-input-{}", sug.column))
                    .font(type_xs().mono())
                    .color(k_frozen.c.text_base)
                    .fill(k_frozen.c.layer_01)
                    .radius(k_frozen.radius.sm)
                    .border(0.75, k_frozen.c.border_base)
                    .h(21.0)
                    .w(96.0)
                    .center_y()
                    .pl(6.0)
            }
        };
        row_el = row_el
            .child(
                row()
                    .key(format!("schema-diff-type-suggestion-{row_key}-target"))
                    .pr(9.0) // React pr-3
                    .child(target_cell),
            )
            .child(self.reason_chip(sug, dark));
        row_el
    }

    /// The reason chip: yellow wash when key/indexed, muted otherwise.
    fn reason_chip(&self, sug: &TypeSuggestionVm, dark: bool) -> El {
        let k = kit(dark);
        let (fill, color, font) = if sug.is_key_or_indexed {
            // bg-yellow-500/20 text-yellow-300 font-medium
            (k.alpha(k.c.warning.fg, 0.20), k.c.warning.icon, type_ten_medium())
        } else {
            // bg-surface-alt text-fg-muted (text-[10px] default weight)
            (k.c.layer_01, k.c.text_muted, type_ten())
        };
        text(sug.reason.clone())
            .key(format!(
                "schema-diff-type-suggestion-{}-{}-reason",
                sug.table, sug.column
            ))
            .font(font)
            .color(color)
            .fill(fill)
            .radius(3.0) // React rounded
            .pl(4.5)
            .pr(4.5)
            .pt(1.5)
            .pb(1.5) // React px-1.5 py-0.5
    }
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}

/// React `text-xs font-medium` (9pt, weight 500).
fn type_xs_medium() -> TextStyle {
    TextStyle::new(9.0, 500.0)
}

/// React `text-[10px]` (10px → 7.5pt).
fn type_ten() -> TextStyle {
    TextStyle::new(7.5, 400.0)
}

/// React `text-[10px] font-medium` (7.5pt, weight 500).
fn type_ten_medium() -> TextStyle {
    TextStyle::new(7.5, 500.0)
}

/// React `text-[11px] font-normal` (11px → 8.25pt).
fn header_type() -> TextStyle {
    TextStyle::new(8.25, 400.0)
}
