//! The shared row/control builders of
//! `packages/frontend/src/windows/settings/ai/ModelProfileForm.tsx` —
//! `SettingRow`/`ToggleRow` stand-ins, the `inputClass` field, the local-CLI
//! path fields, the single-open select, and the slider+readout row — split
//! out of [`super::model_profile_form`] for the file-size rule.


// The form core (`model_profile_form.rs`) owns the wiring story; until the
// dialog/section consumers land, this crate's tests are the only callers.
#![allow(dead_code)]

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::Kit;

use crate::ui::field::TextField;
use crate::ui::select::Select;
use crate::ui::slider::Slider;
use crate::ui::toggle::Switch;
use crate::ui::tooltip::Tooltip;
use crate::views::settings::ai::model_profile_form::{
    ModelFormSelect, ModelProfileForm, INPUT_H, LABEL_MIN_H, LABEL_W, MONO_XS,
};

/// React `text-xs` (the "?" hint mark, the `ai_context_settings.rs` size).
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);

/// The CLI field a keystroke edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalCliField {
    Path,
    Workspace,
}

impl<'a, S: 'static> ModelProfileForm<'a, S> {
    /// React's `SettingRow` (settingsUi.tsx, the T397 stand-in): a 160pt
    /// label column beside a growing control column.
    pub(super) fn setting_row(&self, label: &str, control: El, k: &Kit) -> El {
        row()
            .key(format!("{}-row-{}", self.key, row_key_of(label)))
            .w_full()
            .gap(16.0)
            .items_start()
            .child(
                col()
                    .key(format!("{}-row-{}-label", self.key, row_key_of(label)))
                    // React's `w-40 shrink-0`: a fixed 160pt column that a
                    // wide control cannot squeeze (`.shrink()` here once let
                    // the `flex-1` control starve it to 0).
                    .w(LABEL_W)
                    .min_h(LABEL_MIN_H)
                    .center_y()
                    .child(text(label.to_owned()).font(k.ty.body).color(k.c.text_muted)),
            )
            .child(
                col()
                    .key(format!("{}-row-{}-control", self.key, row_key_of(label)))
                    // React's `min-w-0 flex-1`: grow into the leftover, not a
                    // full-width basis.
                    .grow(1.0)
                    .min_w(0.0)
                    .child(control),
            )
    }

    /// React's `ToggleRow` (the T397 stand-in): label and its "?" hint
    /// trigger left, the hand-drawn switch's kit stand-in right.
    pub(super) fn toggle_row(
        &self,
        key: String,
        label: &str,
        hint: &str,
        checked: bool,
        on_toggle: fn(&mut S),
        k: &Kit,
        layer: &mut Vec<El>,
    ) -> El {
        let mut switch = Switch::new(key.clone(), checked)
            .label(label.to_owned())
            .hide_label();
        switch = switch.on_toggle(move |state: &mut S| (on_toggle)(state));
        // The SettingHint "?" trigger (the `ai_context_settings.rs`
        // stand-in while settingsUi.tsx is unported).
        let mark = row()
            .key(format!("{key}-hint-mark"))
            .size(16.0, 16.0)
            .center()
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(text("?").font(TEXT_11).color(k.c.text_muted));
        let tooltip = Tooltip::new(format!("{key}-hint"))
            .trigger(mark)
            .text(hint.to_owned())
            .build(k, layer);
        row()
            .key(format!("{key}-row"))
            .w_full()
            .gap(16.0)
            .center_y()
            .justify_between()
            .child(
                row()
                    .key(format!("{key}-label"))
                    .gap(4.0)
                    .center_y()
                    .child(text(label.to_owned()).font(k.ty.body).color(k.c.text_muted))
                    .child(tooltip),
            )
            .child(switch.build(k))
    }

    /// React's `inputClass` text field.
    pub(super) fn text_field(
        &self,
        key: impl Into<String>,
        edit: &TextEdit,
        placeholder: &str,
        access: fn(&mut S) -> &mut TextEdit,
        k: &Kit,
    ) -> El {
        let field = TextField::new(key, edit)
            .height(INPUT_H)
            .w_full()
            .border(1.0, k.c.border_base)
            .fill(k.c.bg_base)
            .placeholder(placeholder);
        self.bind_edit(field, access).build(k.dark)
    }

    /// A local-CLI path field: the edit also clears the validate badge
    /// (`handleLocalCliPathChange`'s `setValidateOk(false)` half).
    pub(super) fn cli_field(
        &self,
        key: &str,
        edit: &TextEdit,
        placeholder: &str,
        field: LocalCliField,
        k: &Kit,
    ) -> El {
        let draft_mut = self.slots.draft_mut;
        let path_access = self.slots.cli_path_edit;
        let workspace_access = self.slots.cli_workspace_edit;
        let field_el = TextField::new(key.to_owned(), edit)
            .height(INPUT_H)
            .w_full()
            .border(1.0, k.c.border_base)
            .fill(k.c.bg_base)
            .placeholder(placeholder);
        let field_el = match field {
            LocalCliField::Path => field_el.on_edit(move |state: &mut S| {
                (draft_mut)(state).note_cli_path_edit();
                (path_access)(state)
            }),
            LocalCliField::Workspace => field_el.on_edit(move |state: &mut S| {
                (draft_mut)(state).note_cli_path_edit();
                (workspace_access)(state)
            }),
        };
        field_el.build(k.dark)
    }

    /// The select builder every picker funnels through: one open listbox,
    /// Escape / an outside press closes, a choice closes and reports the
    /// value behind the chosen label.
    pub(super) fn select_field(
        &self,
        key: impl Into<String>,
        labels: Vec<String>,
        values: Vec<String>,
        current: Option<String>,
        label: &str,
        which: ModelFormSelect,
        s: &S,
        k: &Kit,
        layer: &mut Vec<El>,
        on_choose: impl Fn(&mut S, &str) + Clone + 'static,
    ) -> El {
        let slots = self.slots;
        let mut select = Select::new(key, &labels)
            .values(&values)
            .current(current.as_deref())
            .label(label)
            .open((slots.open)(s) == Some(which));
        select = select
            .on_toggle(move |state: &mut S| {
                let next = if (slots.open)(state) == Some(which) {
                    None
                } else {
                    Some(which)
                };
                (slots.set_open)(state, next);
            })
            .on_close(move |state: &mut S| (slots.set_open)(state, None))
            .on_choose(move |state: &mut S, value: &str, _tx| {
                on_choose(state, value);
                (slots.set_open)(state, None);
            });
        select.build(k, layer).h(INPUT_H)
    }

    /// A slider with its mono value readout (React's `flex items-center
    /// gap-3` + the `w-*` span). The slider grows; the readout is fixed.
    pub(super) fn slider_row(
        &self,
        key: impl Into<String>,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        value_w: f32,
        k: &Kit,
        on_change: impl Fn(&mut S, f64) + 'static,
    ) -> El {
        let key: String = key.into();
        let slider = Slider::new(key.clone(), value)
            .min(min)
            .max(max)
            .step(step)
            .on_change(on_change)
            .build(k.dark);
        row()
            .key(format!("{}-slider-row", key))
            .w_full()
            .gap(12.0)
            .center_y()
            .child(
                col()
                    .key(format!("{}-track-col", key))
                    .grow(1.0)
                    .min_w(0.0)
                    .child(slider),
            )
            .child(
                text(format!("{}", value as i64))
                    .key(format!("{}-value", key))
                    .font(MONO_XS.mono())
                    .color(k.c.text_base)
                    .w(value_w),
            )
    }

    /// Bind a text field's edit to one of the draft's buffers.
    pub(super) fn bind_edit<'b>(
        &self,
        field: TextField<'b>,
        access: fn(&mut S) -> &mut TextEdit,
    ) -> TextField<'b> {
        field.on_edit(move |state: &mut S| (access)(state))
    }
}

/// A stable lowercase key fragment for a row label (labels carry spaces and
/// parenthesised ranges).
fn row_key_of(label: &str) -> String {
    label
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '-' })
        .collect()
}
