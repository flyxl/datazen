//! T107 audit: the ModelProfileForm Rune port.
//!
//! Tester-written: every assertion below comes from re-reading
//! `packages/frontend/src/windows/settings/ai/ModelProfileForm.tsx` — the
//! tab switcher, the basic tab's provider/CLI/custom branches, the
//! fetch-models row and the manual-model toggle, the safety tab's preset
//! buttons / egress-conditional sample-rows slider / six sliders / two
//! policy selects, the validate label's four states, the dialog-variant
//! action row, and the high-risk unrestricted-egress confirmation.
//! Element evidence comes from `Runtime::elements()` (runtime layout),
//! never from a value the component reports about itself; presses go
//! through real pointer events. The draft state machine itself is audited
//! by `use_model_profile_draft` (T479); this file pins the view wiring.

use datazen_ui_api::{
    AiDataEgressLevel, AiModelProfile, AiProviderType, AiToolPermissionPolicy, ModelInfo,
    ProviderListItem,
};
use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::ai::model_profile_form::{
    FormVariant, ModelFormSelect, ModelProfileForm, ModelProfileFormSlots,
};
use datazen_ui::views::ai::use_model_profile_draft::{
    default_safety_gate, provider_preset, DraftTab, ModelProfileDraft, SafetyPresetKey,
    SaveRequest, PROVIDER_ORDER,
};

#[derive(Clone)]
struct AuditState {
    draft: ModelProfileDraft,
    dark: bool,
    inline: bool,
    open: Option<ModelFormSelect>,
    validating: bool,
    config_error: Option<String>,
    fetching: bool,
    remote_models: Vec<ModelInfo>,
    providers: Vec<ProviderListItem>,
    /// The clock the caller reads the 3s badge deadline against.
    now: f64,
    saves: Vec<&'static str>,
    cancels: usize,
    fetches: usize,
    validates: usize,
}

fn providers() -> Vec<ProviderListItem> {
    PROVIDER_ORDER.iter().map(|t| provider_preset(*t)).collect()
}

fn model(id: &str, display: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_owned(),
        display_name: display.to_owned(),
        context_window: 128000,
        supports_streaming: true,
        supports_tools: true,
    }
}

fn profile(id: &str, provider_type: AiProviderType) -> AiModelProfile {
    AiModelProfile {
        id: id.to_owned(),
        name: format!("Profile {id}"),
        provider_type,
        api_key: Some("sk-1".to_owned()),
        endpoint: Some("https://api.remote.com".to_owned()),
        model: "gpt-4o".to_owned(),
        max_tokens: Some(8192),
        extra: None,
        safety_gate: default_safety_gate(SafetyPresetKey::CloudStrict),
        is_default: Some(false),
    }
}

impl AuditState {
    /// The settings dialog's fresh profile: the OpenAi preset, no remote
    /// models fetched yet.
    fn fresh() -> Self {
        let providers = providers();
        let mut draft = ModelProfileDraft::default();
        draft.reseed(None, "new-1", &providers);
        Self {
            draft,
            dark: true,
            inline: false,
            open: None,
            validating: false,
            config_error: None,
            fetching: false,
            remote_models: Vec::new(),
            providers,
            now: 100.0,
            saves: Vec::new(),
            cancels: 0,
            fetches: 0,
            validates: 0,
        }
    }

    fn with_profile(profile: &AiModelProfile) -> Self {
        let providers = providers();
        let mut draft = ModelProfileDraft::default();
        draft.reseed(Some(profile), profile.id.as_str(), &providers);
        let mut state = Self::fresh();
        state.draft = draft;
        state
    }

    fn cli() -> Self {
        Self::with_profile(&profile("cli-1", AiProviderType::ClaudeCode))
    }
}

fn slots() -> ModelProfileFormSlots<AuditState> {
    ModelProfileFormSlots {
        draft: |s: &AuditState| &s.draft,
        draft_mut: |s: &mut AuditState| &mut s.draft,
        open: |s: &AuditState| s.open,
        set_open: |s: &mut AuditState, which| s.open = which,
        validating: |s: &AuditState| s.validating,
        config_error: |s: &AuditState| s.config_error.clone(),
        fetching_remote_models: |s: &AuditState| s.fetching,
        remote_models: |s: &AuditState| &s.remote_models,
        providers: |s: &AuditState| &s.providers,
        validate_ok: |s: &AuditState| s.draft.validate_ok(s.now),
        on_set_tab: |s: &mut AuditState, tab| s.draft.tab = tab,
        on_provider_change: |s: &mut AuditState, provider_type| {
            s.draft.handle_provider_change(provider_type, &s.providers)
        },
        on_protocol_change: |s: &mut AuditState, value| {
            s.draft.handle_protocol_change(&value);
        },
        on_fetch_models: |s: &mut AuditState| s.fetches += 1,
        on_toggle_manual_model: |s: &mut AuditState| {
            s.draft.manual_model_input = !s.draft.manual_model_input;
        },
        on_model_change: |s: &mut AuditState, value| {
            s.draft.model = TextEdit::new(value);
        },
        on_validate: |s: &mut AuditState| s.validates += 1,
        on_apply_preset: |s: &mut AuditState, kind| s.draft.apply_preset(kind),
        on_egress_change: |s: &mut AuditState, level| s.draft.set_gate_data_egress(level),
        on_gate_max_sample_rows: |s: &mut AuditState, rows| {
            s.draft.set_gate_max_sample_rows(rows)
        },
        on_gate_max_tool_rounds: |s: &mut AuditState, rounds| {
            s.draft.set_gate_max_tool_rounds(rounds)
        },
        on_gate_api_retries: |s: &mut AuditState, retries| {
            s.draft.set_gate_api_retries(retries)
        },
        on_gate_max_tool_tokens: |s: &mut AuditState, tokens| {
            s.draft.set_gate_max_tool_tokens(tokens)
        },
        on_gate_max_tool_result_bytes: |s: &mut AuditState, bytes| {
            s.draft.set_gate_max_tool_result_bytes(bytes)
        },
        on_gate_max_tool_duration_secs: |s: &mut AuditState, secs| {
            s.draft.set_gate_max_tool_duration_secs(secs)
        },
        on_db_policy_change: |s: &mut AuditState, policy| {
            s.draft.set_gate_db_tool_policy(policy)
        },
        on_mcp_policy_change: |s: &mut AuditState, policy| {
            s.draft.set_gate_mcp_tool_policy(policy)
        },
        on_toggle_default: |s: &mut AuditState| s.draft.is_default = !s.draft.is_default,
        on_toggle_sql_confirm: |s: &mut AuditState| {
            let next = !s.draft.safety_gate.require_sql_confirm;
            s.draft.set_gate_require_sql_confirm(next);
        },
        on_cancel: |s: &mut AuditState| s.cancels += 1,
        on_save: |s: &mut AuditState| match s.draft.request_save(&s.providers) {
            SaveRequest::ConfirmHighRisk => {}
            SaveRequest::Save(_) => s.saves.push("save"),
        },
        on_cancel_high_risk: |s: &mut AuditState| s.draft.show_high_risk_modal = false,
        on_confirm_high_risk: |s: &mut AuditState| {
            if let SaveRequest::Save(_) = s.draft.confirm_high_risk(&s.providers) {
                s.saves.push("confirm");
            }
        },
        name_edit: |s: &mut AuditState| &mut s.draft.name,
        api_key_edit: |s: &mut AuditState| &mut s.draft.api_key,
        endpoint_edit: |s: &mut AuditState| &mut s.draft.endpoint,
        model_edit: |s: &mut AuditState| &mut s.draft.model,
        max_tokens_edit: |s: &mut AuditState| &mut s.draft.max_tokens,
        cli_path_edit: |s: &mut AuditState| &mut s.draft.local_cli_path,
        cli_workspace_edit: |s: &mut AuditState| &mut s.draft.local_cli_workspace,
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let mut form = ModelProfileForm::new(slots());
    if s.inline {
        form = form.variant(FormVariant::Inline);
    }
    let form = form.build(s, s.dark, &mut layer);
    col()
        .key("audit-page")
        .max_w(640.0)
        .h_full()
        .child(form)
        .children(layer)
}

fn new_rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 1400.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// A kit Button's label is the keyed `label` child inside the button's
/// subtree, so the button's own key reads as None for text: resolve its
/// label by path prefix (the `ai_context_settings_audit.rs` helper). A
/// `label_with` button (the tab buttons color their label ink) has no
/// `label`-keyed child; fall back to the first text node in the subtree.
fn button_label(rt: &Runtime<AuditState>, key: &str) -> String {
    let Some(button) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = button.path;
    let mut fallback = None;
    for e in rt.elements() {
        if e.exiting || !e.path.starts_with(&prefix) {
            continue;
        }
        if e.key.as_deref() == Some("label") && e.text.is_some() {
            return e.text.unwrap_or_default();
        }
        if fallback.is_none() && e.role == "text" && e.text.is_some() {
            fallback = e.text;
        }
    }
    fallback.unwrap_or_default()
}

/// The first text node under the element keyed `key` (the preset buttons'
/// unkeyed labels, a select trigger's current value).
fn text_under(rt: &Runtime<AuditState>, key: &str) -> String {
    let Some(el) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = el.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "text" && e.path.starts_with(&prefix))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted controls are pressable (the sibling audit's helper).
fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn type_into(rt: &mut Runtime<AuditState>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    // React tests fire `change` with the full value: Cmd+A selects the
    // field's text first so the paste replaces it instead of appending at
    // the caret (the endpoint field mounts pre-filled with the preset URL).
    rt.key(rune::KeyEvent {
        key: rune::Key::Command('a'),
        mods: rune::Mods { cmd: true, ..rune::Mods::default() },
    });
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(0.0);
}

const K: &str = "ai-profile-form";

// ---- mount: the dialog variant's basic tab --------------------------------

#[test]
fn mount_renders_the_basic_tab_tree() {
    let mut rt = new_rt(AuditState::fresh());

    assert!(has(&rt, K), "the dialog variant's root testid");
    assert!(has(&rt, "ai-provider-select-trigger"));
    // The tab strip: both tabs. React's buttons always carry `border-b-2`
    // (only the color toggles), so both underlines exist and the inactive
    // one paints TRANSPARENT.
    assert!(has(&rt, "ai-profile-form-tab-basic"));
    assert!(has(&rt, "ai-profile-form-tab-safety"));
    assert!(maybe(&rt, "ai-profile-form-tab-basic-active").is_some_and(|e| e.fill.a > 0.0));
    assert!(maybe(&rt, "ai-profile-form-tab-safety-active").is_some_and(|e| e.fill.a == 0.0));
    assert_eq!(button_label(&rt, "ai-profile-form-tab-basic"), t("settings.ai.tabBasic"));
    assert_eq!(button_label(&rt, "ai-profile-form-tab-safety"), t("settings.ai.tabSafety"));

    // The basic tab's fields (a remote provider, no models fetched yet).
    for key in [
        "ai-profile-form-name",
        "ai-profile-form-api-key",
        "ai-profile-form-endpoint",
        "ai-profile-form-model",
        "ai-profile-form-max-tokens",
        "ai-profile-form-fetch-models",
        "ai-profile-form-default",
        "ai-profile-validate",
        "ai-profile-cancel",
        "ai-profile-save",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    // No models yet: the manual checkbox is hidden, the model is free text.
    assert!(!has(&rt, "ai-profile-form-model-manual"));
    assert!(!has(&rt, "ai-profile-form-model-select-trigger"));
    // The safety tab is not mounted.
    assert!(!has(&rt, "ai-profile-form-safety"));
    // React's `value={d.draft.maxTokens ?? 200000}` seeds the buffer.
    assert_eq!(rt.state.draft.max_tokens.text, "200000");

    // The label column is React's `w-40` (160pt) — geometry, not self-report.
    // Advance past the enter motion first: `frame(0.0)` freezes a growing
    // width at its start (0), where the column has not reached 160pt yet.
    settle(&mut rt);
    let label_key = format!("{K}-row-{}-label", t("settings.ai.modelName"));
    let label = maybe(&rt, &label_key).expect("the name row's label column");
    assert!((label.rect[2] - 160.0).abs() < 0.5, "label column width {}", label.rect[2]);
}

#[test]
fn the_inline_variant_swaps_the_root_and_hides_the_dialog_actions() {
    let mut scene = AuditState::fresh();
    scene.inline = true;
    let rt = new_rt(scene);

    assert!(has(&rt, "onboarding-ai-form"), "the inline variant's testid");
    assert!(!has(&rt, K));
    assert!(has(&rt, "onboarding-ai-form-name"));
    assert!(has(&rt, "ai-profile-validate"), "the validate action stays");
    assert!(!has(&rt, "ai-profile-cancel"), "the host owns the primary action");
    assert!(!has(&rt, "ai-profile-save"));
}

// ---- the tab switcher ------------------------------------------------------

#[test]
fn tab_buttons_switch_the_panels() {
    let mut rt = new_rt(AuditState::fresh());
    press(&mut rt, "ai-profile-form-tab-safety");

    assert!(has(&rt, "ai-profile-form-safety"));
    assert!(!has(&rt, "ai-profile-form-basic"));
    // The underline fill cross-fades on the switch; two settles (0.8s) put
    // it past the kit's color motion.
    settle(&mut rt);
    settle(&mut rt);
    assert!(maybe(&rt, "ai-profile-form-tab-safety-active").is_some_and(|e| e.fill.a > 0.0));
    assert!(maybe(&rt, "ai-profile-form-tab-basic-active").is_some_and(|e| e.fill.a == 0.0));
    assert_eq!(rt.state.draft.tab, DraftTab::Safety);
    for key in [
        "ai-profile-form-safety-preset-label",
        "ai-profile-form-preset-cloud_strict",
        "ai-profile-form-preset-enterprise_balanced",
        "ai-profile-form-preset-local_trust",
        "ai-profile-form-egress-trigger",
        "ai-profile-form-max-tool-rounds",
        "ai-profile-form-api-retries",
        "ai-profile-form-max-tool-tokens",
        "ai-profile-form-max-tool-result-bytes",
        "ai-profile-form-max-tool-duration-secs",
        "ai-profile-form-db-policy-trigger",
        "ai-profile-form-mcp-policy-trigger",
        "ai-profile-form-require-sql-confirm",
    ] {
        assert!(has(&rt, key), "{key} is mounted on the safety tab");
    }
    // No sample-rows slider while the egress level is strict.
    assert!(!has(&rt, "ai-profile-form-max-sample-rows"));

    press(&mut rt, "ai-profile-form-tab-basic");
    assert!(has(&rt, "ai-profile-form-basic"));
    assert!(!has(&rt, "ai-profile-form-safety"));
    assert_eq!(rt.state.draft.tab, DraftTab::Basic);
}

// ---- the basic tab's provider branches --------------------------------------

#[test]
fn the_provider_select_wires_the_change_handler() {
    let mut rt = new_rt(AuditState::fresh());
    // The normalized list follows PROVIDER_ORDER; option0 (Anthropic) is in
    // view — the wiring proof, the branch mounts below cover the rest.
    press(&mut rt, "ai-provider-select-trigger");
    assert!(has(&rt, "option0"), "the provider listbox opened");
    press(&mut rt, "option0");
    assert_eq!(rt.state.draft.provider_type, AiProviderType::Anthropic);
    assert!(!rt.state.draft.is_local_cli());
    // Choosing closed the listbox (one open listbox at a time).
    assert!(!has(&rt, "option0"));
}

#[test]
fn a_local_cli_provider_swaps_the_credentials_for_the_cli_fields() {
    let mut rt = new_rt(AuditState::cli());
    assert!(rt.state.draft.is_local_cli());
    assert!(has(&rt, "ai-local-cli-path"));
    assert!(has(&rt, "ai-local-cli-workspace"));
    assert_eq!(text_of(&rt, "ai-profile-form-local-cli-hint"), t("settings.ai.localCliHint"));
    assert!(!has(&rt, "ai-profile-form-api-key"), "a CLI agent needs no key");
    assert!(!has(&rt, "ai-profile-form-endpoint"));
    assert!(!has(&rt, "ai-profile-form-fetch-models"));
    assert!(!has(&rt, "ai-profile-form-max-tokens"));

    // Typing a CLI path clears the validate badge (`setValidateOk(false)`).
    // `update` marks the view dirty — a bare `rt.state` write would leave
    // the committed view reading the old state.
    rt.update(|s, _| s.draft.validate_ok_until = Some(103.0));
    settle(&mut rt);
    assert!(has(&rt, "ai-profile-validate-check"));
    type_into(&mut rt, "ai-local-cli-path", "/usr/local/bin/claude");
    assert_eq!(rt.state.draft.local_cli_path.text, "/usr/local/bin/claude");
    assert_eq!(rt.state.draft.validate_ok_until, None, "the badge died on the keystroke");
    assert!(!has(&rt, "ai-profile-validate-check"));

    // Back to an API provider: the credentials return.
    press(&mut rt, "ai-provider-select-trigger");
    press(&mut rt, "option0"); // Anthropic
    assert!(!rt.state.draft.is_local_cli());
    assert!(has(&rt, "ai-profile-form-api-key"));
    assert!(has(&rt, "ai-profile-form-endpoint"));
    assert!(!has(&rt, "ai-local-cli-path"));
}

#[test]
fn a_custom_provider_shows_the_protocol_select() {
    let mut rt = new_rt(AuditState::with_profile(&profile("custom-1", AiProviderType::Custom)));
    assert!(rt.state.draft.is_custom());

    assert!(has(&rt, "ai-profile-form-protocol-trigger"));
    assert_eq!(
        text_under(&rt, "ai-profile-form-protocol-trigger"),
        t("settings.ai.protocolOpenAiChat")
    );
    press(&mut rt, "ai-profile-form-protocol-trigger");
    assert!(has(&rt, "option1"));
    press(&mut rt, "option1"); // open_ai_responses
    assert_eq!(rt.state.draft.custom_protocol, "open_ai_responses");

    // A preset provider has no protocol picker at all.
    press(&mut rt, "ai-provider-select-trigger");
    press(&mut rt, "option0"); // Anthropic
    assert!(!rt.state.draft.is_custom());
    assert!(!has(&rt, "ai-profile-form-protocol-trigger"));
}

#[test]
fn remote_models_add_the_manual_checkbox_and_the_model_select() {
    let mut scene = AuditState::fresh();
    scene.remote_models = vec![model("gpt-4o", "GPT-4o"), model("m2", "")];
    let mut rt = new_rt(scene);

    assert!(has(&rt, "ai-profile-form-model-manual"));
    assert!(has(&rt, "ai-profile-form-model-select-trigger"));
    assert!(!has(&rt, "ai-profile-form-model"), "the free-text input is replaced");

    // The select reports the model id behind the chosen label (an empty
    // display name falls back to the id).
    press(&mut rt, "ai-profile-form-model-select-trigger");
    press(&mut rt, "option1");
    assert_eq!(rt.state.draft.model.text, "m2");

    // The manual checkbox swaps the select for the free-text input.
    press(&mut rt, "ai-profile-form-model-manual");
    assert!(rt.state.draft.manual_model_input);
    assert!(has(&rt, "ai-profile-form-model"));
    assert!(!has(&rt, "ai-profile-form-model-select-trigger"));
}

#[test]
fn the_fetch_button_fires_and_is_disabled_while_fetching() {
    let mut rt = new_rt(AuditState::fresh());
    assert_eq!(button_label(&rt, "ai-profile-form-fetch-models"), t("settings.ai.fetchModels"));
    press(&mut rt, "ai-profile-form-fetch-models");
    assert_eq!(rt.state.fetches, 1);

    rt.update(|s, _| s.fetching = true);
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-form-fetch-models"), t("settings.ai.fetchingModels"));
    press(&mut rt, "ai-profile-form-fetch-models");
    assert_eq!(rt.state.fetches, 1, "a fetching button does not fire again");
}

#[test]
fn the_fields_edit_the_draft_and_the_error_line_shows() {
    let mut scene = AuditState::fresh();
    scene.config_error = Some("endpoint unreachable".to_owned());
    let mut rt = new_rt(scene);

    type_into(&mut rt, "ai-profile-form-name", "Nightly runs");
    assert_eq!(rt.state.draft.name.text, "Nightly runs");
    type_into(&mut rt, "ai-profile-form-api-key", "sk-secret");
    assert_eq!(rt.state.draft.api_key.text, "sk-secret");
    // A secure field never shows its text.
    assert_eq!(text_of(&rt, "ai-profile-form-api-key"), "");
    type_into(&mut rt, "ai-profile-form-endpoint", "https://api.remote.com/v1");
    assert_eq!(rt.state.draft.endpoint.text, "https://api.remote.com/v1");
    type_into(&mut rt, "ai-profile-form-max-tokens", "500");
    assert_eq!(rt.state.draft.max_tokens.text, "500");
    assert_eq!(text_of(&rt, "ai-profile-form-config-error"), "endpoint unreachable");

    // The default toggle flips `isDefault`.
    let before = rt.state.draft.is_default;
    press(&mut rt, "ai-profile-form-default");
    assert_eq!(rt.state.draft.is_default, !before);
}

// ---- the safety tab --------------------------------------------------------

#[test]
fn the_preset_buttons_apply_the_preset_gates() {
    let mut rt = new_rt(AuditState::fresh());
    press(&mut rt, "ai-profile-form-tab-safety");
    assert_eq!(text_of(&rt, "ai-profile-form-safety-preset-label"), t("settings.ai.safetyPreset"));
    assert_eq!(text_under(&rt, "ai-profile-form-preset-local_trust"), t("settings.ai.presetLocalTrust"));

    // Enterprise balanced: sample_masked egress, so the sample-rows slider
    // appears with the preset's 3 rows, and the db policy tightens.
    press(&mut rt, "ai-profile-form-preset-enterprise_balanced");
    assert_eq!(rt.state.draft.safety_gate.data_egress_level, AiDataEgressLevel::SampleMasked);
    assert!(has(&rt, "ai-profile-form-max-sample-rows"));
    assert_eq!(text_of(&rt, "ai-profile-form-max-sample-rows-value"), "3");
    assert_eq!(rt.state.draft.safety_gate.db_tool_policy, AiToolPermissionPolicy::RequireConfirm);

    // Local trust: unrestricted egress and policy, no sample-rows slider.
    press(&mut rt, "ai-profile-form-preset-local_trust");
    assert_eq!(rt.state.draft.safety_gate.data_egress_level, AiDataEgressLevel::Unrestricted);
    assert!(!has(&rt, "ai-profile-form-max-sample-rows"));
    assert_eq!(rt.state.draft.safety_gate.mcp_tool_policy, AiToolPermissionPolicy::Unrestricted);
    assert_eq!(rt.state.draft.safety_gate.require_sql_confirm, false);
}

#[test]
fn the_egress_select_drives_the_sample_rows_slider() {
    let mut rt = new_rt(AuditState::fresh());
    press(&mut rt, "ai-profile-form-tab-safety");
    assert!(!has(&rt, "ai-profile-form-max-sample-rows"));

    press(&mut rt, "ai-profile-form-egress-trigger");
    press(&mut rt, "option1"); // sample_masked
    assert_eq!(rt.state.draft.safety_gate.data_egress_level, AiDataEgressLevel::SampleMasked);
    assert!(has(&rt, "ai-profile-form-max-sample-rows"));
    assert_eq!(text_of(&rt, "ai-profile-form-max-sample-rows-value"), "0", "cloud_strict's rows");

    press(&mut rt, "ai-profile-form-egress-trigger");
    press(&mut rt, "option2"); // unrestricted
    assert!(!has(&rt, "ai-profile-form-max-sample-rows"));
}

#[test]
fn the_sliders_show_the_gate_values_and_the_policies_choose() {
    let mut rt = new_rt(AuditState::fresh());
    press(&mut rt, "ai-profile-form-tab-safety");

    assert_eq!(text_of(&rt, "ai-profile-form-max-tool-rounds-value"), "30");
    assert_eq!(text_of(&rt, "ai-profile-form-api-retries-value"), "2");
    assert_eq!(text_of(&rt, "ai-profile-form-max-tool-tokens-value"), "48000");
    assert_eq!(text_of(&rt, "ai-profile-form-max-tool-result-bytes-value"), "8192");
    assert_eq!(text_of(&rt, "ai-profile-form-max-tool-duration-secs-value"), "300");

    press(&mut rt, "ai-profile-form-db-policy-trigger");
    press(&mut rt, "option3"); // unrestricted
    assert_eq!(rt.state.draft.safety_gate.db_tool_policy, AiToolPermissionPolicy::Unrestricted);
    press(&mut rt, "ai-profile-form-mcp-policy-trigger");
    press(&mut rt, "option0"); // disabled
    assert_eq!(rt.state.draft.safety_gate.mcp_tool_policy, AiToolPermissionPolicy::Disabled);

    // The SQL-confirm switch flips `requireSqlConfirm`.
    press(&mut rt, "ai-profile-form-require-sql-confirm");
    assert_eq!(rt.state.draft.safety_gate.require_sql_confirm, false);
}

// ---- validate + actions ------------------------------------------------------

#[test]
fn the_validate_label_walks_reacts_four_states() {
    let mut rt = new_rt(AuditState::fresh());
    assert_eq!(button_label(&rt, "ai-profile-validate"), t("settings.ai.validate"));
    press(&mut rt, "ai-profile-validate");
    assert_eq!(rt.state.validates, 1);

    // The 3s badge: validated + the emerald check glyph, then gone.
    rt.update(|s, _| s.draft.validate_ok_until = Some(103.0));
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-validate"), t("settings.ai.validated"));
    assert!(has(&rt, "ai-profile-validate-check"));
    rt.update(|s, _| s.draft.validate_ok_until = Some(99.0)); // past
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-validate"), t("settings.ai.validate"));
    assert!(!has(&rt, "ai-profile-validate-check"));

    // Validating: the label swaps and the button refuses to fire.
    rt.update(|s, _| s.validating = true);
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-validate"), t("settings.ai.validating"));
    press(&mut rt, "ai-profile-validate");
    assert_eq!(rt.state.validates, 1, "disabled while validating");

    // A local CLI provider offers the detect wording instead.
    let mut rt_cli = new_rt(AuditState::cli());
    assert_eq!(button_label(&rt_cli, "ai-profile-validate"), t("settings.ai.localCliDetect"));
}

#[test]
fn cancel_and_save_fire_in_the_dialog_variant() {
    let mut rt = new_rt(AuditState::fresh());
    press(&mut rt, "ai-profile-cancel");
    assert_eq!(rt.state.cancels, 1);
    // A strict gate on a remote endpoint saves without any confirmation.
    press(&mut rt, "ai-profile-save");
    assert_eq!(rt.state.saves, vec!["save"]);
    assert!(!has(&rt, "ai-profile-high-risk"));
    // The saving label swaps while the host's save is in flight.
    rt.update(|s, _| s.draft.saving = true);
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-save"), t("settings.ai.saving"));
}

#[test]
fn unrestricted_remote_egress_demands_the_high_risk_confirmation() {
    let mut scene = AuditState::with_profile(&profile("risk-1", AiProviderType::OpenAi));
    scene.draft.safety_gate.data_egress_level = AiDataEgressLevel::Unrestricted;
    scene.draft.endpoint = TextEdit::new("https://api.remote.com".to_owned());
    let mut rt = new_rt(scene);

    press(&mut rt, "ai-profile-save");
    assert!(rt.state.draft.show_high_risk_modal);
    assert!(has(&rt, "ai-profile-high-risk-title"));
    assert_eq!(
        text_of(&rt, "ai-profile-high-risk-title"),
        t("settings.ai.unrestrictedWarningTitle")
    );
    assert_eq!(
        text_of(&rt, "ai-profile-high-risk-message"),
        t("settings.ai.unrestrictedWarningMessage")
    );
    assert!(has(&rt, "ai-profile-high-risk-warning"));
    assert!(has(&rt, "ai-profile-high-risk-cancel"));
    assert!(has(&rt, "ai-profile-confirm-risk"));
    assert_eq!(rt.state.saves, Vec::<&str>::new(), "nothing saved yet");

    // Cancel just closes the confirmation.
    press(&mut rt, "ai-profile-high-risk-cancel");
    assert!(!rt.state.draft.show_high_risk_modal);
    assert!(!has(&rt, "ai-profile-high-risk-title"));
    assert_eq!(rt.state.saves, Vec::<&'static str>::new());

    // Again, and this time confirm: the payload reaches the host's save.
    press(&mut rt, "ai-profile-save");
    press(&mut rt, "ai-profile-confirm-risk");
    assert!(!rt.state.draft.show_high_risk_modal);
    assert_eq!(rt.state.saves, vec!["confirm"]);
}
