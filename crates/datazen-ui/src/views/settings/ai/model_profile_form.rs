//! Port of `packages/frontend/src/windows/settings/ai/ModelProfileForm.tsx`.
//!
//! The AI model profile form (basic + safety gate tabs) used by the settings
//! `ModelProfileDialog` and the first-run journey's "Set up AI assistance"
//! step. One implementation, so the journey can never drift from
//! Settings → AI. All draft/validate/save behaviour lives in
//! [`super::use_model_profile_draft::ModelProfileDraft`]; this module is the
//! render + event router over it, split for the file-size rule into
//! [`super::model_profile_form_sections`] (the two tab bodies) and
//! [`super::model_profile_form_controls`] (the shared row/control builders).
//!
//! ## What maps where
//!
//! - React's `draft` prop (the hook's return value) is the `S`-side
//!   [`ModelProfileDraft`] reached through [`ModelProfileFormSlots`], the
//!   function-pointer slot pattern of `views/workflow/workflow_form.rs`.
//!   The `useAiStore` halves the hook read (`validating`, `configError`,
//!   `remoteModels`, `fetchingRemoteModels`, `providers`) are slots too;
//!   every action (`handleProviderChange`, `handleProtocolChange`,
//!   `handleFetchModels`, `handleValidate`, `applyPreset`, `updateGate`,
//!   `requestSave`, `confirmHighRisk`, `setTab`, `setManualModelInput`) is a
//!   slot the host wires to the state machine's methods. `onSave` /
//!   `onSaved` / `onCancel` stay the host's (the dialog port wires them).
//! - React's `ui/Select` keeps its open state internally; the port keeps ONE
//!   open listbox at a time ([`ModelFormSelect`]) in `S`, the
//!   `workflow_form.rs` ruling — every `on_choose` closes it, as each of
//!   React's handlers does.
//! - The hook's clock-driven bit surfaces as the caller-computed
//!   `validate_ok` slot (the 3s deadline read as a bool); the preset
//!   detection is the draft's `current_preset`.
//! - The CLI path fields' edit handlers call
//!   [`ModelProfileDraft::note_cli_path_edit`] before returning the buffer,
//!   reproducing `handleLocalCliPathChange`'s `setValidateOk(false)` on the
//!   first keystroke.
//! - `SettingRow` / `ToggleRow` (settingsUi.tsx) are the later task T397;
//!   until it lands they render here in React's style — a 160pt label
//!   column beside a growing control column, and a label + kit `Switch`
//!   row (the hint riding the `ui/tooltip.rs` "?" trigger exactly how
//!   `ai_context_settings.rs` carried its later-task dependencies). When
//!   T397 lands these locals move there.
//! - The high-risk unrestricted-egress confirmation is the nested React
//!   `Dialog`; the port pushes the same panel through `ui/dialog.rs`'s
//!   [`Dialog`] into the caller's `layer`.
//! - `ModelProfileDialog` (T065) is this form's dialog consumer;
//!   `AiSettingsSection` (T172) and the first-run journey's AI step mount
//!   the dialog / the inline variant later — until then this crate's audit
//!   is the only caller.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; unlabeled parts derive from
//! the component key (`key`: `"ai-profile-form"` in the dialog variant,
//! `"onboarding-ai-form"` inline):
//!
//! | part | key |
//!|------|-----|
//! | root / tab strip / strip border | `<key>`, `<key>-tabs`, `<key>-tabs-border` |
//! | tab buttons / active underline | `<key>-tab-basic`, `<key>-tab-safety`, `<key>-tab-<tab>-active` |
//! | name / api key / endpoint / model / max tokens fields | `<key>-name`, `<key>-api-key`, `<key>-endpoint`, `<key>-model`, `<key>-max-tokens` |
//! | provider / protocol / model selects | `ai-provider-select`, `<key>-protocol`, `<key>-model-select` |
//! | CLI hint / path / workspace | `<key>-local-cli-hint`, `ai-local-cli-path`, `ai-local-cli-workspace` |
//! | fetch row / fetch button / manual checkbox | `<key>-fetch-row`, `<key>-fetch-models`, `<key>-model-manual` |
//! | preset label / preset buttons | `<key>-safety-preset-label`, `<key>-preset-cloud_strict` (+ `enterprise_balanced`, `local_trust`) |
//! | egress / db policy / mcp policy selects | `<key>-egress`, `<key>-db-policy`, `<key>-mcp-policy` |
//! | the six sliders / their readouts | `<key>-max-sample-rows`, `<key>-max-tool-rounds`, `<key>-api-retries`, `<key>-max-tool-tokens`, `<key>-max-tool-result-bytes`, `<key>-max-tool-duration-secs` (+ `-value`) |
//! | default / sql-confirm switches | `<key>-default`, `<key>-require-sql-confirm` |
//! | config error line | `<key>-config-error` |
//! | actions row / validate / cancel / save | `<key>-actions-row`, `ai-profile-validate`, `ai-profile-cancel`, `ai-profile-save` |
//! | high-risk dialog parts | `ai-profile-high-risk` (+ the base's `-title`/`-close`/`-body`), `ai-profile-high-risk-warning`, `ai-profile-high-risk-message`, `ai-profile-high-risk-cancel`, `ai-profile-confirm-risk` |
//!
//! ## Known deltas (documented, not silent)
//!
//! - Kit button sizes stand in for Tailwind's pixel classes: React's
//!   `size="sm"` (h-8) buttons render `ControlSize::Large` (the
//!   `ai_context_settings.rs` mapping); the tab buttons are Ghost `Regular`.
//! - No single-side borders in Rune (the `ai_chat_header.rs` ruling): the
//!   tab strip's `border-b` is a 1pt divider element and the active tab's
//!   2pt accent underline is a line element under the button; the actions
//!   row's `border-t` is a 1pt divider above its content.
//! - React's `type="range"` sliders ride `ui/slider.rs` (stepped snapping,
//!   drag + arrow keys); each `onChange`'s `parseInt(value) || fallback`
//!   never fires on a snapped in-range value, so the slot receives the raw
//!   snapped integer cast to the gate field's type.
//! - `type="number"`'s spinner and `min=1000` attribute have no Rune
//!   counterpart (React's attribute never blocked typing either); the
//!   `parseInt || 200000` rule lives in the state machine.
//! - `type="password"` is `TextField::secure`. The preset buttons keep
//!   React's exact Tailwind hues (`ui/palette.rs`'s `ai_preset_*_border`
//!   borders + `ai_shield_*` inks over 10% `kit.alpha` fills); the
//!   high-risk box uses the `ai_amber_*` helpers and its confirm button is
//!   kit `ButtonVariant::Danger` for React's `bg-red-600`.
//! - React's validated button inlines the emerald `CheckCircle2` inside the
//!   label; a kit button label is a single ink, so the glyph rides beside
//!   the button in an `ai-profile-validate-group` row.
//! - `ToggleRow`'s hand-drawn 36×20 switch rides the kit switch track, and
//!   its `SettingHint` "?" tooltip is the `ui/tooltip.rs` stand-in (the
//!   `ai_context_settings.rs` pattern) rather than inline hint text.
//! - The two ToggleRow hints and the model placeholder are React's raw
//!   English literals (no i18n key exists — kept verbatim per the rules).
//! - `transition-colors` / `transition-all` / `hover:` tints have no Rune
//!   counterpart; the preset buttons' hover tint is dropped.


// `ModelProfileDialog` (T065) and the first-run journey's AI step are the
// production consumers; until they land this crate's tests are the only
// callers.
#![allow(dead_code)]

use rune::{col, el, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use datazen_ui_api::{
    AiDataEgressLevel, AiProviderType, AiToolPermissionPolicy, ModelInfo, ProviderListItem,
};

use crate::i18n::t;
use crate::icons::{ICON_ALERT_TRIANGLE, ICON_CIRCLE_CHECK, ICON_SETTINGS_2, ICON_SHIELD};
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::palette::{
    ai_amber_border, ai_amber_fill, ai_shield_enterprise_balanced, ai_shield_local_trust,
};
use crate::views::settings::ai::use_model_profile_draft::{
    DraftTab, ModelProfileDraft, SafetyPresetKey,
};

/// React `inputClass` fields' height (the `ai_context_settings.rs` mapping
/// of `px-3 py-1.5 text-sm`).
pub(super) const INPUT_H: f32 = 24.0;
/// React `w-40`: the SettingRow label column.
pub(super) const LABEL_W: f32 = 160.0;
/// React `min-h-9`: the SettingRow label column's minimum height.
pub(super) const LABEL_MIN_H: f32 = 36.0;
/// React `text-xs` in the mono family (the slider readouts).
pub(super) const MONO_XS: TextStyle = TextStyle::new(12.0, 400.0);

/// Which of the form's listboxes is open (one at a time — React's `ui/Select`
/// keeps its own open state; closing on a choice is every handler's job,
/// reproduced by each `on_choose` here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelFormSelect {
    Provider,
    Protocol,
    Model,
    Egress,
    DbPolicy,
    McpPolicy,
}

/// React's `variant` prop: `dialog` renders the Cancel/Save action row
/// (settings dialog); `inline` leaves the primary action to the host
/// (first-run journey footer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormVariant {
    Dialog,
    Inline,
}

/// Where the form's state lives in the host's `S` (function pointers, the
/// `workflow_form.rs` slot pattern — every field is a `fn` pointer, so the
/// struct is `Copy`).
pub(crate) struct ModelProfileFormSlots<S: 'static> {
    /// The controlled draft (React's `draft` prop).
    pub(crate) draft: fn(&S) -> &ModelProfileDraft,
    pub(crate) draft_mut: fn(&mut S) -> &mut ModelProfileDraft,
    /// The one open listbox.
    pub(crate) open: fn(&S) -> Option<ModelFormSelect>,
    pub(crate) set_open: fn(&mut S, Option<ModelFormSelect>),
    // ---- the useAiStore halves the hook reads ----
    pub(crate) validating: fn(&S) -> bool,
    pub(crate) config_error: fn(&S) -> Option<String>,
    pub(crate) fetching_remote_models: fn(&S) -> bool,
    pub(crate) remote_models: fn(&S) -> &[ModelInfo],
    /// The store's raw provider list (normalized per frame, like the hook's
    /// `providerDefaults` memo).
    pub(crate) providers: fn(&S) -> &[ProviderListItem],
    /// The validate badge, caller-computed from the draft's 3s deadline.
    pub(crate) validate_ok: fn(&S) -> bool,
    // ---- actions (the hook's callbacks) ----
    pub(crate) on_set_tab: fn(&mut S, DraftTab),
    /// `handleProviderChange` (the literal converts at the select edge).
    pub(crate) on_provider_change: fn(&mut S, AiProviderType),
    pub(crate) on_protocol_change: fn(&mut S, String),
    pub(crate) on_fetch_models: fn(&mut S),
    /// The manual-model checkbox (`setManualModelInput(!checked)`).
    pub(crate) on_toggle_manual_model: fn(&mut S),
    pub(crate) on_model_change: fn(&mut S, String),
    pub(crate) on_validate: fn(&mut S),
    pub(crate) on_apply_preset: fn(&mut S, SafetyPresetKey),
    pub(crate) on_egress_change: fn(&mut S, AiDataEgressLevel),
    pub(crate) on_gate_max_sample_rows: fn(&mut S, u32),
    pub(crate) on_gate_max_tool_rounds: fn(&mut S, usize),
    pub(crate) on_gate_api_retries: fn(&mut S, u32),
    pub(crate) on_gate_max_tool_tokens: fn(&mut S, usize),
    pub(crate) on_gate_max_tool_result_bytes: fn(&mut S, usize),
    pub(crate) on_gate_max_tool_duration_secs: fn(&mut S, u64),
    pub(crate) on_db_policy_change: fn(&mut S, AiToolPermissionPolicy),
    pub(crate) on_mcp_policy_change: fn(&mut S, AiToolPermissionPolicy),
    /// The default switch (`updateDraft({ isDefault: !checked })`).
    pub(crate) on_toggle_default: fn(&mut S),
    /// The SQL-confirm switch (`updateGate('requireSqlConfirm', !checked)`).
    pub(crate) on_toggle_sql_confirm: fn(&mut S),
    /// React's `onCancel` prop (the dialog variant's cancel button).
    pub(crate) on_cancel: fn(&mut S),
    /// The save press: the host runs `request_save` (and on the confirmation
    /// `confirm_high_risk`) against the draft.
    pub(crate) on_save: fn(&mut S),
    /// The high-risk dialog's dismissal (`setShowHighRiskModal(false)`).
    pub(crate) on_cancel_high_risk: fn(&mut S),
    /// The high-risk dialog's confirm (`confirmHighRisk`).
    pub(crate) on_confirm_high_risk: fn(&mut S),
    // ---- the controlled inputs' TextEdits ----
    pub(crate) name_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) api_key_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) endpoint_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) model_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) max_tokens_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) cli_path_edit: fn(&mut S) -> &mut TextEdit,
    pub(crate) cli_workspace_edit: fn(&mut S) -> &mut TextEdit,
}

impl<S: 'static> Clone for ModelProfileFormSlots<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: 'static> Copy for ModelProfileFormSlots<S> {}

/// The model profile form, built per frame over the host's `S`.
pub(crate) struct ModelProfileForm<'a, S: 'static> {
    pub(super) key: &'a str,
    pub(super) variant: FormVariant,
    pub(super) slots: ModelProfileFormSlots<S>,
}

impl<'a, S: 'static> ModelProfileForm<'a, S> {
    /// The form over the slots (React's default `variant: 'dialog'`).
    pub(crate) fn new(slots: ModelProfileFormSlots<S>) -> Self {
        Self {
            key: "ai-profile-form",
            variant: FormVariant::Dialog,
            slots,
        }
    }

    /// React's `variant` prop (also switches the root testid to
    /// `onboarding-ai-form`).
    pub(crate) fn variant(mut self, variant: FormVariant) -> Self {
        self.key = match variant {
            FormVariant::Dialog => "ai-profile-form",
            FormVariant::Inline => "onboarding-ai-form",
        };
        self.variant = variant;
        self
    }

    /// One appearance of the form. The high-risk confirmation (while the
    /// draft's `show_high_risk_modal` is set) pushes into `layer`.
    pub(crate) fn build(&self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let draft = (self.slots.draft)(s);
        let tabs = self.tab_strip(s, &k);
        let body = match draft.tab {
            DraftTab::Basic => self.basic_tab(s, &k, layer),
            DraftTab::Safety => self.safety_tab(s, &k, layer),
        };
        let actions = self.actions_row(s, &k);
        if draft.show_high_risk_modal {
            self.high_risk_dialog(dark, layer);
        }
        col()
            .key(self.key)
            .w_full()
            .gap(16.0)
            .child(tabs)
            .child(body)
            .child(actions)
    }

    // ---- the tab strip -------------------------------------------------------

    /// React's tab switcher: two icon+label buttons on a bottom border, the
    /// active one accent-inked with a 2pt underline.
    fn tab_strip(&self, s: &S, k: &Kit) -> El {
        let basic =
            self.tab_button(s, k, DraftTab::Basic, t("settings.ai.tabBasic"), &ICON_SETTINGS_2);
        let safety =
            self.tab_button(s, k, DraftTab::Safety, t("settings.ai.tabSafety"), &ICON_SHIELD);
        // React's `border-b border-edge` sits on the STRIP CONTAINER: the
        // buttons row and a full-width hairline under it (a w_full border
        // CHILD of the row would push the row past its container — the
        // onboarding inline variant caught it escaping the window).
        col()
            .key(format!("{}-tabs", self.key))
            .w_full()
            .child(row().w_full().child(basic).child(safety))
            .child(
                el()
                    .key(format!("{}-tabs-border", self.key))
                    .w_full()
                    .h(1.0)
                    .fill(k.c.border_base),
            )
    }

    fn tab_button(
        &self,
        s: &S,
        k: &Kit,
        tab: DraftTab,
        label: &str,
        icon: &'static rune::Icon,
    ) -> El {
        let active = (self.slots.draft)(s).tab == tab;
        let accent = k.c.text_accent;
        let ink = if active { accent } else { k.c.text_muted };
        let name = match tab {
            DraftTab::Basic => "basic",
            DraftTab::Safety => "safety",
        };
        let label_text = label.to_owned();
        let on_set_tab = self.slots.on_set_tab;
        let button = Button::new(label.to_owned())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .icon(icon)
            .label_with(move |font, _ink| text(label_text.clone()).font(font).color(ink))
            .on_press(move |s: &mut S| (on_set_tab)(s, tab))
            .build(k)
            .key(format!("{}-tab-{name}", self.key));
        // The active tab's 2pt accent underline (React's `border-b-2`).
        col()
            .key(format!("{}-tab-{name}-group", self.key))
            .child(button)
            .child(
                el()
                    .key(format!("{}-tab-{name}-active", self.key))
                    .h(2.0)
                    .fill(if active {
                        accent
                    } else {
                        rune::Color::TRANSPARENT
                    }),
            )
    }

    // ---- validate + actions ----------------------------------------------------

    fn actions_row(&self, s: &S, k: &Kit) -> El {
        let draft = (self.slots.draft)(s);
        let validating = (self.slots.validating)(s);
        let validate_ok = (self.slots.validate_ok)(s);

        // React's four validate labels, in order.
        let validate_label = if validating {
            t("settings.ai.validating")
        } else if validate_ok {
            t("settings.ai.validated")
        } else if draft.is_local_cli() {
            t("settings.ai.localCliDetect")
        } else {
            t("settings.ai.validate")
        };
        let mut validate = Button::new(validate_label)
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Large)
            .disabled(validating);
        if !validating {
            let on_validate = self.slots.on_validate;
            validate = validate.on_press(move |state: &mut S| (on_validate)(state));
        }
        let validate = validate.build(k).key("ai-profile-validate");
        // React's validated state prefixes the emerald check glyph (the
        // glyph rides beside the button — see the deltas above).
        let validate_group = if validate_ok && !validating {
            row()
                .key("ai-profile-validate-group")
                .gap(4.0)
                .center_y()
                .child(
                    rune::icon(&ICON_CIRCLE_CHECK, 14.0)
                        .key("ai-profile-validate-check")
                        .color(ai_shield_local_trust(k.dark)),
                )
                .child(validate)
        } else {
            row().key("ai-profile-validate-group").child(validate)
        };

        let mut buttons = None;
        if self.variant == FormVariant::Dialog {
            let mut cancel = Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Large);
            let on_cancel = self.slots.on_cancel;
            cancel = cancel.on_press(move |state: &mut S| (on_cancel)(state));
            let mut save = Button::new(if draft.saving {
                t("settings.ai.saving")
            } else {
                t("common.save")
            })
            .variant(ButtonVariant::Submit)
            .control_size(ControlSize::Large)
            .disabled(draft.saving);
            if !draft.saving {
                let on_save = self.slots.on_save;
                save = save.on_press(move |state: &mut S| (on_save)(state));
            }
            buttons = Some(
                row()
                    .key(format!("{}-actions-buttons", self.key))
                    .gap(8.0)
                    .center_y()
                    .child(cancel.build(k).key("ai-profile-cancel"))
                    .child(save.build(k).key("ai-profile-save")),
            );
        }

        // React: `mt-4 flex items-center gap-3 border-t border-edge pt-4`,
        // `justify-between` only in the dialog variant.
        let content = match buttons {
            Some(buttons) => row()
                .key(format!("{}-actions-row", self.key))
                .w_full()
                .justify_between()
                .center_y()
                .child(validate_group)
                .child(buttons),
            None => row()
                .key(format!("{}-actions-row", self.key))
                .w_full()
                .gap(12.0)
                .center_y()
                .child(validate_group),
        };
        col()
            .key(format!("{}-actions", self.key))
            .w_full()
            .mt(16.0)
            .gap(16.0)
            // The `border-t` divider (no single-side borders in Rune).
            .child(
                el()
                    .key(format!("{}-actions-border", self.key))
                    .w_full()
                    .h(1.0)
                    .fill(k.c.border_base),
            )
            .child(content)
    }

    // ---- the high-risk confirmation --------------------------------------------

    /// React's nested unrestricted-egress `Dialog` (max-w-md), pushed into
    /// the caller's layer.
    fn high_risk_dialog(&self, dark: bool, layer: &mut Vec<El>) {
        let k = kit(dark);
        let on_cancel = self.slots.on_cancel_high_risk;
        let on_confirm = self.slots.on_confirm_high_risk;

        let warning = row()
            .key("ai-profile-high-risk-warning")
            .gap(12.0)
            .px(12.0)
            .py(12.0)
            .radius(k.radius.lg)
            .border(1.0, ai_amber_border(dark))
            .fill(ai_amber_fill(dark))
            .child(
                rune::icon(&ICON_ALERT_TRIANGLE, 20.0)
                    .color(ai_shield_enterprise_balanced(dark)),
            )
            .child(
                text(t("settings.ai.unrestrictedWarningMessage").to_owned())
                    .key("ai-profile-high-risk-message")
                    .font(k.ty.text_xsmall)
                    .color(ai_shield_enterprise_balanced(dark)),
            );

        let cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Large)
            .on_press(move |state: &mut S| (on_cancel)(state));
        let confirm = Button::new(t("common.confirm"))
            .variant(ButtonVariant::Danger)
            .control_size(ControlSize::Large)
            .on_press(move |state: &mut S| (on_confirm)(state));

        let dialog = Dialog::new(
            "ai-profile-high-risk",
            t("settings.ai.unrestrictedWarningTitle"),
        )
        .open(true)
        .width(448.0) // React's max-w-md
        .fit()
        .child(
            col()
                .key("ai-profile-high-risk-body")
                .w_full()
                .gap(16.0)
                .child(warning)
                .child(
                    row()
                        .key("ai-profile-high-risk-buttons")
                        .w_full()
                        .gap(8.0)
                        .justify_end()
                        .child(cancel.build(&k).key("ai-profile-high-risk-cancel"))
                        .child(confirm.build(&k).key("ai-profile-confirm-risk")),
                ),
        )
        .on_close(move |state: &mut S| (on_cancel)(state));
        dialog.build(&k, layer);
    }
}
