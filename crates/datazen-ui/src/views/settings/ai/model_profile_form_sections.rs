//! The two tab bodies of
//! `packages/frontend/src/windows/settings/ai/ModelProfileForm.tsx` (the
//! basic-configuration and safety-gate panels), split out of
//! [`super::model_profile_form`] for the file-size rule. The module doc,
//! slot table and key/delta tables live there.


// The form core (`model_profile_form.rs`) owns the wiring story; until the
// dialog/section consumers land, this crate's tests are the only callers.
#![allow(dead_code)]

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use datazen_ui_api::{AiDataEgressLevel, AiToolPermissionPolicy};

use crate::components::ai::AiSafetyPreset;
use crate::i18n::t;
use crate::icons::{ICON_LAYERS, ICON_SHIELD};
use crate::ui::field::TextField;
use crate::ui::toggle::Checkbox;
use crate::ui::palette::{
    ai_preset_amber_border, ai_preset_blue_border, ai_preset_emerald_border,
    ai_shield_cloud_strict, ai_shield_enterprise_balanced, ai_shield_local_trust,
};
use crate::views::settings::ai::model_profile_form::{
    ModelFormSelect, ModelProfileForm, INPUT_H,
};
use crate::views::settings::ai::model_profile_form_controls::LocalCliField;
use crate::views::settings::ai::use_model_profile_draft::{
    normalize_ai_providers, provider_type_from_str, SafetyPresetKey,
};

/// One of the always-on gate sliders behind its cast: the slot's typed
/// signature plus the integer type React's `parseInt` lands on.
#[derive(Clone, Copy)]
enum GateSlot<S: 'static> {
    U32(fn(&mut S, u32)),
    Usize(fn(&mut S, usize)),
    U64(fn(&mut S, u64)),
}

impl<S: 'static> GateSlot<S> {
    fn call(&self, s: &mut S, v: f64) {
        match self {
            GateSlot::U32(f) => f(s, v as u32),
            GateSlot::Usize(f) => f(s, v as usize),
            GateSlot::U64(f) => f(s, v as u64),
        }
    }
}

impl<'a, S: 'static> ModelProfileForm<'a, S> {
    // ---- tab 1: basic configuration -------------------------------------------

    pub(super) fn basic_tab(&self, s: &S, k: &Kit, layer: &mut Vec<El>) -> El {
        let draft = (self.slots.draft)(s);
        let providers = normalize_ai_providers((self.slots.providers)(s));
        let mut tab = col()
            .key(format!("{}-basic", self.key))
            .w_full()
            .gap(12.0)
            .py(4.0);

        tab = tab.child(self.setting_row(
            t("settings.ai.modelName"),
            self.text_field(
                format!("{}-name", self.key),
                &draft.name,
                t("settings.ai.modelNamePlaceholder"),
                self.slots.name_edit,
                k,
            ),
            k,
        ));

        // Provider picker: labels are the normalized display names, values
        // the provider literals.
        let mut labels = Vec::new();
        let mut values = Vec::new();
        let mut current = None;
        for provider in &providers {
            labels.push(provider.display_name.clone());
            values.push(provider.provider_type.as_str().to_owned());
            if provider.provider_type == draft.provider_type {
                current = Some(provider.display_name.clone());
            }
        }
        let on_provider_change = self.slots.on_provider_change;
        tab = tab.child(self.setting_row(
            t("settings.ai.provider"),
            self.select_field(
                "ai-provider-select",
                labels,
                values,
                current,
                t("settings.ai.provider"),
                ModelFormSelect::Provider,
                s,
                k,
                layer,
                move |state, value| {
                    if let Some(provider_type) = provider_type_from_str(value) {
                        (on_provider_change)(state, provider_type);
                    }
                },
            ),
            k,
        ));

        if draft.is_custom() {
            let labels = [
                t("settings.ai.protocolOpenAiChat").to_owned(),
                t("settings.ai.protocolOpenAiResponses").to_owned(),
                t("settings.ai.protocolAnthropic").to_owned(),
            ];
            let values = [
                "open_ai_compatible".to_owned(),
                "open_ai_responses".to_owned(),
                "anthropic_compatible".to_owned(),
            ];
            let current = values
                .iter()
                .position(|v| v == &draft.custom_protocol)
                .map(|i| labels[i].clone());
            let on_protocol_change = self.slots.on_protocol_change;
            tab = tab.child(self.setting_row(
                t("settings.ai.protocol"),
                self.select_field(
                    format!("{}-protocol", self.key),
                    labels.to_vec(),
                    values.to_vec(),
                    current,
                    t("settings.ai.protocol"),
                    ModelFormSelect::Protocol,
                    s,
                    k,
                    layer,
                    move |state, value| (on_protocol_change)(state, value.to_owned()),
                ),
                k,
            ));
        }

        if draft.is_local_cli() {
            tab = tab.child(
                text(t("settings.ai.localCliHint").to_owned())
                    .key(format!("{}-local-cli-hint", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .px(12.0)
                    .py(8.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_base)
                    .fill(k.alpha(k.c.bg_base, 0.6)),
            );
            let path = self.cli_field(
                "ai-local-cli-path",
                &draft.local_cli_path,
                t("settings.ai.localCliPathPlaceholder"),
                LocalCliField::Path,
                k,
            );
            tab = tab.child(self.setting_row(t("settings.ai.localCliPath"), path, k));
            let workspace = self.cli_field(
                "ai-local-cli-workspace",
                &draft.local_cli_workspace,
                t("settings.ai.localCliWorkspacePlaceholder"),
                LocalCliField::Workspace,
                k,
            );
            tab = tab.child(self.setting_row(t("settings.ai.localCliWorkspace"), workspace, k));
        } else {
            let api_key = TextField::new(format!("{}-api-key", self.key), &draft.api_key)
                .height(INPUT_H)
                .w_full()
                .border(1.0, k.c.border_base)
                .fill(k.c.bg_base)
                .secure()
                .placeholder(t("settings.ai.apiKeyPlaceholder"));
            let api_key = self.bind_edit(api_key, self.slots.api_key_edit).build(k.dark);
            tab = tab.child(self.setting_row(t("settings.ai.apiKey"), api_key, k));

            let endpoint = self.text_field(
                format!("{}-endpoint", self.key),
                &draft.endpoint,
                t("settings.ai.endpointPlaceholder"),
                self.slots.endpoint_edit,
                k,
            );
            tab = tab.child(self.setting_row(t("settings.ai.endpoint"), endpoint, k));

            // Fetch models + the manual-input checkbox (only with options).
            let remote_models = (self.slots.remote_models)(s);
            let fetching = (self.slots.fetching_remote_models)(s);
            let mut fetch = Button::new(if fetching {
                t("settings.ai.fetchingModels")
            } else {
                t("settings.ai.fetchModels")
            })
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Large)
            .disabled(fetching);
            if !fetching {
                let on_fetch = self.slots.on_fetch_models;
                fetch = fetch.on_press(move |state: &mut S| (on_fetch)(state));
            }
            let mut fetch_row = row()
                .key(format!("{}-fetch-row", self.key))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(fetch.build(k).key(format!("{}-fetch-models", self.key)));
            if !remote_models.is_empty() {
                let mut manual = Checkbox::new(
                    format!("{}-model-manual", self.key),
                    draft.manual_model_input,
                )
                .label(t("settings.ai.modelManual"));
                let on_toggle = self.slots.on_toggle_manual_model;
                manual = manual.on_toggle(move |state: &mut S| (on_toggle)(state));
                fetch_row = fetch_row.child(manual.build(k));
            }
            tab = tab.child(fetch_row);

            // Model: the select while options exist and the manual toggle is
            // off; the free-text input otherwise.
            let model_control = if draft.manual_model_input || remote_models.is_empty() {
                self.text_field(
                    format!("{}-model", self.key),
                    &draft.model,
                    "e.g. gpt-4o, claude-3-7-sonnet, qwen2.5-coder",
                    self.slots.model_edit,
                    k,
                )
            } else {
                let mut labels = Vec::new();
                let mut values = Vec::new();
                let mut current = None;
                for model in remote_models {
                    let label = if model.display_name.is_empty() {
                        model.id.clone()
                    } else {
                        model.display_name.clone()
                    };
                    if model.id == draft.model.text {
                        current = Some(label.clone());
                    }
                    labels.push(label);
                    values.push(model.id.clone());
                }
                let on_model_change = self.slots.on_model_change;
                self.select_field(
                    format!("{}-model-select", self.key),
                    labels,
                    values,
                    current,
                    t("settings.ai.model"),
                    ModelFormSelect::Model,
                    s,
                    k,
                    layer,
                    move |state, value| (on_model_change)(state, value.to_owned()),
                )
            };
            tab = tab.child(self.setting_row(t("settings.ai.model"), model_control, k));

            let max_tokens = self.text_field(
                format!("{}-max-tokens", self.key),
                &draft.max_tokens,
                "",
                self.slots.max_tokens_edit,
                k,
            );
            tab = tab.child(self.setting_row(t("settings.ai.maxTokens"), max_tokens, k));
        }

        // The default toggle (React's raw-English hint, kept verbatim).
        tab = tab.child(self.toggle_row(
            format!("{}-default", self.key),
            t("settings.ai.setDefault"),
            "Use this profile as the default for AI features",
            draft.is_default,
            self.slots.on_toggle_default,
            k,
            layer,
        ));

        if let Some(error) = (self.slots.config_error)(s) {
            tab = tab.child(
                text(error)
                    .key(format!("{}-config-error", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            );
        }
        tab
    }

    // ---- tab 2: safety gate ---------------------------------------------------

    pub(super) fn safety_tab(&self, s: &S, k: &Kit, layer: &mut Vec<El>) -> El {
        let draft = (self.slots.draft)(s);
        let current_preset = draft.current_preset();
        let dark = k.dark;
        let mut tab = col()
            .key(format!("{}-safety", self.key))
            .w_full()
            .gap(16.0)
            .py(4.0);

        // The three preset buttons.
        let presets = row()
            .key(format!("{}-presets", self.key))
            .w_full()
            .gap(8.0)
            .child(self.preset_button(
                k,
                SafetyPresetKey::CloudStrict,
                current_preset == AiSafetyPreset::CloudStrict,
                t("settings.ai.presetCloudStrict"),
                &ICON_SHIELD,
                ai_preset_blue_border(dark),
                ai_shield_cloud_strict(dark),
            ))
            .child(self.preset_button(
                k,
                SafetyPresetKey::EnterpriseBalanced,
                current_preset == AiSafetyPreset::EnterpriseBalanced,
                t("settings.ai.presetEnterpriseBalanced"),
                &ICON_LAYERS,
                ai_preset_amber_border(dark),
                ai_shield_enterprise_balanced(dark),
            ))
            .child(self.preset_button(
                k,
                SafetyPresetKey::LocalTrust,
                current_preset == AiSafetyPreset::LocalTrust,
                t("settings.ai.presetLocalTrust"),
                &ICON_SHIELD,
                ai_preset_emerald_border(dark),
                ai_shield_local_trust(dark),
            ));
        tab = tab.child(
            col()
                .key(format!("{}-safety-preset", self.key))
                .w_full()
                .gap(8.0)
                .child(
                    text(t("settings.ai.safetyPreset").to_owned())
                        .key(format!("{}-safety-preset-label", self.key))
                        .font(k.ty.text_xsmall_medium)
                        .color(k.c.text_muted),
                )
                .child(presets),
        );

        // Egress level.
        let labels = [
            t("settings.ai.egressStrict").to_owned(),
            t("settings.ai.egressSampleMasked").to_owned(),
            t("settings.ai.egressUnrestricted").to_owned(),
        ];
        let values = [
            AiDataEgressLevel::Strict.as_str().to_owned(),
            AiDataEgressLevel::SampleMasked.as_str().to_owned(),
            AiDataEgressLevel::Unrestricted.as_str().to_owned(),
        ];
        let current = values
            .iter()
            .position(|v| v == draft.safety_gate.data_egress_level.as_str())
            .map(|i| labels[i].clone());
        let on_egress_change = self.slots.on_egress_change;
        tab = tab.child(self.setting_row(
            t("settings.ai.dataEgressLevel"),
            self.select_field(
                format!("{}-egress", self.key),
                labels.to_vec(),
                values.to_vec(),
                current,
                t("settings.ai.dataEgressLevel"),
                ModelFormSelect::Egress,
                s,
                k,
                layer,
                move |state, value| {
                    if let Some(level) = egress_from_str(value) {
                        (on_egress_change)(state, level);
                    }
                },
            ),
            k,
        ));

        // The sample-rows slider, only while sample_masked.
        if draft.safety_gate.data_egress_level == AiDataEgressLevel::SampleMasked {
            let slot = self.slots.on_gate_max_sample_rows;
            tab = tab.child(self.setting_row(
                &format!("{} (1-10)", t("settings.ai.maxSampleRows")),
                self.slider_row(
                    format!("{}-max-sample-rows", self.key),
                    draft.safety_gate.max_sample_rows as f64,
                    1.0,
                    10.0,
                    1.0,
                    24.0,
                    k,
                    move |state: &mut S, v: f64| (slot)(state, v as u32),
                ),
                k,
            ));
        }

        // The five always-on sliders: key, label, value, min, max, step, the
        // readout width, and the slot behind the cast.
        let sliders: [(&str, String, f64, f64, f64, f64, f32, GateSlot<S>); 5] = [
            (
                "max-tool-rounds",
                format!("{} (1-50)", t("settings.ai.maxToolRounds")),
                draft.safety_gate.max_tool_rounds as f64,
                1.0,
                50.0,
                1.0,
                32.0,
                GateSlot::Usize(self.slots.on_gate_max_tool_rounds),
            ),
            (
                "api-retries",
                format!("{} (0-10)", t("settings.ai.apiRetries")),
                draft.safety_gate.api_retries as f64,
                0.0,
                10.0,
                1.0,
                24.0,
                GateSlot::U32(self.slots.on_gate_api_retries),
            ),
            (
                "max-tool-tokens",
                format!("{} (1000-200000)", t("settings.ai.maxToolTokens")),
                draft.safety_gate.max_tool_tokens as f64,
                1000.0,
                200000.0,
                1000.0,
                64.0,
                GateSlot::Usize(self.slots.on_gate_max_tool_tokens),
            ),
            (
                "max-tool-result-bytes",
                format!("{} (256-65536)", t("settings.ai.maxToolResultBytes")),
                draft.safety_gate.max_tool_result_bytes as f64,
                256.0,
                65536.0,
                256.0,
                64.0,
                GateSlot::Usize(self.slots.on_gate_max_tool_result_bytes),
            ),
            (
                "max-tool-duration-secs",
                format!("{} (10-1800)", t("settings.ai.maxToolDurationSecs")),
                draft.safety_gate.max_tool_duration_secs as f64,
                10.0,
                1800.0,
                10.0,
                40.0,
                GateSlot::U64(self.slots.on_gate_max_tool_duration_secs),
            ),
        ];
        for (suffix, label, value, min, max, step, value_w, slot) in sliders {
            tab = tab.child(self.setting_row(
                &label,
                self.slider_row(
                    format!("{}-{suffix}", self.key),
                    value,
                    min,
                    max,
                    step,
                    value_w,
                    k,
                    move |state: &mut S, v: f64| slot.call(state, v),
                ),
                k,
            ));
        }

        // The two tool policies.
        let labels = [
            t("settings.ai.policyDisabled").to_owned(),
            t("settings.ai.policyReadOnly").to_owned(),
            t("settings.ai.policyRequireConfirm").to_owned(),
            t("settings.ai.policyUnrestricted").to_owned(),
        ];
        let values = [
            AiToolPermissionPolicy::Disabled.as_str().to_owned(),
            AiToolPermissionPolicy::ReadOnly.as_str().to_owned(),
            AiToolPermissionPolicy::RequireConfirm.as_str().to_owned(),
            AiToolPermissionPolicy::Unrestricted.as_str().to_owned(),
        ];
        let db_slot = self.slots.on_db_policy_change;
        let mcp_slot = self.slots.on_mcp_policy_change;
        for (suffix, label, current_value, which, on_change) in [
            (
                "db-policy",
                t("settings.ai.dbToolPolicy"),
                draft.safety_gate.db_tool_policy,
                ModelFormSelect::DbPolicy,
                db_slot,
            ),
            (
                "mcp-policy",
                t("settings.ai.mcpToolPolicy"),
                draft.safety_gate.mcp_tool_policy,
                ModelFormSelect::McpPolicy,
                mcp_slot,
            ),
        ] {
            let current = values
                .iter()
                .position(|v| v == current_value.as_str())
                .map(|i| labels[i].clone());
            let on_choose = move |state: &mut S, value: &str| {
                if let Some(policy) = policy_from_str(value) {
                    (on_change)(state, policy);
                }
            };
            tab = tab.child(self.setting_row(
                label,
                self.select_field(
                    format!("{}-{suffix}", self.key),
                    labels.to_vec(),
                    values.to_vec(),
                    current,
                    label,
                    which,
                    s,
                    k,
                    layer,
                    on_choose,
                ),
                k,
            ));
        }

        tab = tab.child(self.toggle_row(
            format!("{}-require-sql-confirm", self.key),
            t("settings.ai.requireSqlConfirm"),
            "Always show a confirmation prompt before executing AI-generated SQL",
            draft.safety_gate.require_sql_confirm,
            self.slots.on_toggle_sql_confirm,
            k,
            layer,
        ));
        tab
    }

    /// One safety-preset button: icon + label stacked in a tinted border box.
    fn preset_button(
        &self,
        k: &Kit,
        kind: SafetyPresetKey,
        active: bool,
        label: &str,
        icon: &'static rune::Icon,
        border: rune::Color,
        ink: rune::Color,
    ) -> El {
        let on_apply_preset = self.slots.on_apply_preset;
        let mut button = col()
            .key(format!("{}-preset-{}", self.key, kind.as_str()))
            .grow(1.0)
            .min_w(0.0)
            .px(10.0)
            .py(10.0)
            .gap(4.0)
            .items_center()
            .radius(k.radius.lg)
            .border(1.0, if active { border } else { k.c.border_base })
            .fill(if active {
                k.alpha(border, 0.10)
            } else {
                k.c.bg_base
            })
            .on_press(move |state: &mut S| (on_apply_preset)(state, kind))
            .focusable(true);
        button = button
            .child(rune::icon(icon, 16.0).color(if active { ink } else { k.c.text_muted }))
            .child(
                text(label.to_owned())
                    .font(k.ty.text_xsmall_medium)
                    .color(if active { ink } else { k.c.text_muted }),
            );
        button
    }
}

fn egress_from_str(value: &str) -> Option<AiDataEgressLevel> {
    [
        AiDataEgressLevel::Strict,
        AiDataEgressLevel::SampleMasked,
        AiDataEgressLevel::Unrestricted,
    ]
    .into_iter()
    .find(|level| level.as_str() == value)
}

fn policy_from_str(value: &str) -> Option<AiToolPermissionPolicy> {
    [
        AiToolPermissionPolicy::Disabled,
        AiToolPermissionPolicy::ReadOnly,
        AiToolPermissionPolicy::RequireConfirm,
        AiToolPermissionPolicy::Unrestricted,
    ]
    .into_iter()
    .find(|policy| policy.as_str() == value)
}
