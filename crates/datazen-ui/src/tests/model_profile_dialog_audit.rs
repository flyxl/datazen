//! T065 audit: the ModelProfileDialog Rune port.
//!
//! Tester-written: every assertion below comes from re-reading
//! `packages/frontend/src/windows/settings/ai/ModelProfileDialog.tsx` and
//! the `useModelProfileDraft` save path it configures — the open/close
//! gate, the add-vs-edit title, the form mounted in the dialog variant,
//! Esc / header-close / cancel closing, the Save press running React's
//! `performSave` wiring (the host's save, closing on success, staying open
//! on failure), and the unrestricted-egress confirmation's confirm path
//! saving + closing while its cancel only clears the flag. Element
//! evidence comes from `Runtime::elements()` (runtime layout), never from
//! a value the component reports about itself; presses go through real
//! pointer events. The form itself is audited by `model_profile_form_audit`
//! (T107) and the state machine by `use_model_profile_draft` (T479); this
//! file pins the shell and the save wiring the dialog owns.

use datazen_ui_api::{
    AiDataEgressLevel, AiModelProfile, AiProviderType, ModelInfo, ProviderListItem,
};
use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::ai::model_profile_dialog::{
    ModelProfileDialog, ModelProfileDialogHost, ModelProfileDialogSlots,
};
use crate::views::ai::model_profile_form::{ModelFormSelect, ModelProfileFormSlots};
use crate::views::ai::use_model_profile_draft::{
    default_safety_gate, provider_preset, ModelProfileDraft, SafetyPresetKey, PROVIDER_ORDER,
};

#[derive(Clone)]
struct AuditState {
    draft: ModelProfileDraft,
    dark: bool,
    open: bool,
    epoch: u64,
    /// React's `profile` prop: `Some` edits (the edit title), `None` adds.
    profile: Option<AiModelProfile>,
    open_select: Option<ModelFormSelect>,
    validating: bool,
    config_error: Option<String>,
    fetching: bool,
    remote_models: Vec<ModelInfo>,
    providers: Vec<ProviderListItem>,
    /// The clock the 3s badge deadline reads.
    now: f64,
    /// The payloads the host's save received, plus any host-dummy markers
    /// (the four form slots the dialog owns must never run the dummies).
    saved: Vec<String>,
    /// What the host's save answers (React's resolved `onSave` bool).
    save_result: bool,
    closes: usize,
}

impl ModelProfileDialogHost for AuditState {
    fn model_profile_dialog_draft(&mut self) -> &mut ModelProfileDraft {
        &mut self.draft
    }

    fn model_profile_dialog_providers(&self) -> Vec<ProviderListItem> {
        self.providers.clone()
    }

    fn save_model_profile(&mut self, profile: AiModelProfile) -> bool {
        self.saved.push(profile.id);
        self.save_result
    }

    /// The host's `onClose` response: count it and drop the open flag.
    fn close_model_profile_dialog(&mut self) {
        self.closes += 1;
        self.open = false;
    }
}

fn providers() -> Vec<ProviderListItem> {
    PROVIDER_ORDER.iter().map(|t| provider_preset(*t)).collect()
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
    /// The add flow: open with no profile, the draft reseeded from a fresh
    /// empty profile (the host's open-path `reseed`, the hook's
    /// `[profile, open]` effect).
    fn add() -> Self {
        let providers = providers();
        let mut draft = ModelProfileDraft::default();
        draft.reseed(None, "new-1", &providers);
        Self {
            draft,
            dark: true,
            open: true,
            epoch: 1,
            profile: None,
            open_select: None,
            validating: false,
            config_error: None,
            fetching: false,
            remote_models: Vec::new(),
            providers,
            now: 100.0,
            saved: Vec::new(),
            save_result: true,
            closes: 0,
        }
    }

    /// The edit flow: open on a profile, the draft reseeded from it.
    fn edit(profile: &AiModelProfile) -> Self {
        let mut scene = Self::add();
        scene.profile = Some(profile.clone());
        let providers = scene.providers.clone();
        scene.draft.reseed(Some(profile), profile.id.as_str(), &providers);
        scene
    }
}

fn form_slots() -> ModelProfileFormSlots<AuditState> {
    ModelProfileFormSlots {
        draft: |s: &AuditState| &s.draft,
        draft_mut: |s: &mut AuditState| &mut s.draft,
        open: |s: &AuditState| s.open_select,
        set_open: |s: &mut AuditState, which| s.open_select = which,
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
        on_fetch_models: |_s: &mut AuditState| {},
        on_toggle_manual_model: |s: &mut AuditState| {
            s.draft.manual_model_input = !s.draft.manual_model_input;
        },
        on_model_change: |s: &mut AuditState, value| {
            s.draft.model = TextEdit::new(value);
        },
        on_validate: |_s: &mut AuditState| {},
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
        // The four slots the dialog owns: dummies that would leave a marker
        // if they ever ran (the tests below assert they never do).
        on_cancel: |s: &mut AuditState| s.saved.push("host-cancel-dummy".to_owned()),
        on_save: |s: &mut AuditState| s.saved.push("host-save-dummy".to_owned()),
        on_cancel_high_risk: |s: &mut AuditState| s.saved.push("host-cancel-risk-dummy".to_owned()),
        on_confirm_high_risk: |s: &mut AuditState| s.saved.push("host-confirm-dummy".to_owned()),
        name_edit: |s: &mut AuditState| &mut s.draft.name,
        api_key_edit: |s: &mut AuditState| &mut s.draft.api_key,
        endpoint_edit: |s: &mut AuditState| &mut s.draft.endpoint,
        model_edit: |s: &mut AuditState| &mut s.draft.model,
        max_tokens_edit: |s: &mut AuditState| &mut s.draft.max_tokens,
        cli_path_edit: |s: &mut AuditState| &mut s.draft.local_cli_path,
        cli_workspace_edit: |s: &mut AuditState| &mut s.draft.local_cli_workspace,
    }
}

fn slots() -> ModelProfileDialogSlots<AuditState> {
    ModelProfileDialogSlots {
        open: |s: &AuditState| s.open,
        open_epoch: |s: &AuditState| s.epoch,
        profile: |s: &AuditState| s.profile.as_ref(),
        dark: |s: &AuditState| s.dark,
        form: form_slots(),
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    ModelProfileDialog::<AuditState>::new(slots()).build(s, &mut layer);
    // The layer children ride the root so elements() sees them.
    let mut root = col().key("probe");
    for child in layer {
        root = root.child(child);
    }
    root
}

fn new_rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
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

/// A kit Button's label resolves by path prefix (the form audit's helper);
/// fall back to the first text node under the button.
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

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted controls are pressable (the sibling audits' helper).
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

/// Press a control inside the dialog's scrolling body: the body caps at
/// 448pt (React's `max-h-[70vh]` at the 640pt minimum window), so the
/// actions row sits under the fold — wheel the body to the bottom first
/// (the base's own audit's technique).
fn press_in_body(rt: &mut Runtime<AuditState>, key: &str) {
    let (bx, by) = rt
        .find("model-profile-dialog-body")
        .expect("the body is laid out");
    for _ in 0..6 {
        rt.wheel(bx + 8.0, by + 8.0, -400.0);
        rt.frame(1.0 / 60.0);
    }
    settle(rt);
    press(rt, key);
}

fn escape(rt: &mut Runtime<AuditState>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

const DIALOG: &str = "model-profile-dialog";

// ---- the open/close gate -----------------------------------------------------

#[test]
fn while_closed_nothing_mounts() {
    let mut scene = AuditState::add();
    scene.open = false;
    let rt = new_rt(scene);

    assert!(!has(&rt, DIALOG), "no panel while closed");
    assert!(!has(&rt, "model-profile-dialog-scrim"));
    assert!(!has(&rt, "model-profile-dialog-layer"));
    assert!(!has(&rt, "ai-profile-form"), "no form while closed");
    assert_eq!(rt.state.closes, 0);
}

#[test]
fn add_mode_mounts_the_dialog_around_the_form() {
    let rt = new_rt(AuditState::add());

    assert_eq!(text_of(&rt, "model-profile-dialog-title"), t("settings.ai.addModel"));
    // The base's parts: scrim, layer, header close, body.
    for key in [
        DIALOG,
        "model-profile-dialog-scrim",
        "model-profile-dialog-layer",
        "model-profile-dialog-close",
        "model-profile-dialog-body",
        "model-profile-dialog-content-1",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    // The form inside, in the dialog variant (Cancel + Save present).
    for key in [
        "ai-profile-form",
        "ai-profile-form-name",
        "ai-provider-select-trigger",
        "ai-profile-validate",
        "ai-profile-cancel",
        "ai-profile-save",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    assert!(!has(&rt, "onboarding-ai-form"), "not the inline variant");
}

#[test]
fn edit_mode_titles_and_carries_the_profile() {
    let rt = new_rt(AuditState::edit(&profile("p1", AiProviderType::OpenAi)));

    assert_eq!(text_of(&rt, "model-profile-dialog-title"), t("settings.ai.editModel"));
    // The host's open-path reseed filled the draft from the profile.
    assert_eq!(rt.state.draft.name.text, "Profile p1");
    assert_eq!(rt.state.draft.model.text, "gpt-4o");
    assert_eq!(rt.state.draft.id, "p1");
}

#[test]
fn escape_and_the_header_close_close_and_the_epoch_resets_the_body() {
    let mut rt = new_rt(AuditState::add());

    escape(&mut rt);
    assert_eq!(rt.state.closes, 1, "Escape runs the close");
    settle(&mut rt);
    assert!(!has(&rt, DIALOG), "unmounted after Escape");

    // Reopen with a bumped epoch: the body wrapper is a new element (the
    // scroll reset), the documented keys stay.
    rt.update(|s, _| {
        s.open = true;
        s.epoch += 1;
    });
    settle(&mut rt);
    assert!(has(&rt, DIALOG));
    assert!(has(&rt, "model-profile-dialog-content-2"), "the new epoch's body");
    assert!(!has(&rt, "model-profile-dialog-content-1"), "the old one is gone");

    press(&mut rt, "model-profile-dialog-close");
    assert_eq!(rt.state.closes, 2, "the header close runs the close");
    settle(&mut rt);
    assert!(!has(&rt, DIALOG));
}

// ---- the save wiring the dialog owns -------------------------------------------

#[test]
fn cancel_closes_without_saving() {
    let mut rt = new_rt(AuditState::add());

    press_in_body(&mut rt, "ai-profile-cancel");
    assert_eq!(rt.state.closes, 1);
    assert!(rt.state.saved.is_empty(), "no save, no dummy markers");
    settle(&mut rt);
    assert!(!has(&rt, DIALOG));
}

#[test]
fn save_runs_the_dialogs_glue_and_closes_on_success() {
    let mut rt = new_rt(AuditState::add());

    press_in_body(&mut rt, "ai-profile-save");
    // The dialog's glue: the payload reached the host's save under the
    // draft's id, the dummy never ran, and onSaved closed the dialog.
    assert_eq!(rt.state.saved, vec!["new-1".to_owned()]);
    assert_eq!(rt.state.closes, 1);
    assert!(!rt.state.draft.saving, "end_save ran");
    settle(&mut rt);
    assert!(!has(&rt, DIALOG));
}

#[test]
fn a_failed_save_keeps_the_dialog_open() {
    let mut scene = AuditState::add();
    scene.save_result = false;
    let mut rt = new_rt(scene);

    press_in_body(&mut rt, "ai-profile-save");
    assert_eq!(rt.state.saved, vec!["new-1".to_owned()]);
    assert_eq!(rt.state.closes, 0, "onSaved only runs on a successful save");
    assert!(has(&rt, DIALOG), "the dialog stays for another try");
    assert!(!rt.state.draft.saving);
}

#[test]
fn unrestricted_remote_egress_confirms_then_saves_and_closes() {
    let mut scene = AuditState::edit(&profile("risk-1", AiProviderType::OpenAi));
    scene.draft.safety_gate.data_egress_level = AiDataEgressLevel::Unrestricted;
    scene.draft.endpoint = TextEdit::new("https://api.remote.com".to_owned());
    let mut rt = new_rt(scene);

    // The Save press only arms the confirmation (the form renders it).
    press_in_body(&mut rt, "ai-profile-save");
    assert!(rt.state.draft.show_high_risk_modal);
    assert!(has(&rt, "ai-profile-high-risk-title"), "the confirmation mounts");
    assert!(has(&rt, DIALOG), "the outer dialog stays under it");
    assert!(rt.state.saved.is_empty());
    assert_eq!(rt.state.closes, 0);

    // Confirm: the confirmation closes, the payload saves, onSaved closes.
    press(&mut rt, "ai-profile-confirm-risk");
    assert!(!rt.state.draft.show_high_risk_modal);
    assert_eq!(rt.state.saved, vec!["risk-1".to_owned()]);
    assert_eq!(rt.state.closes, 1);
    settle(&mut rt);
    assert!(!has(&rt, "ai-profile-high-risk-title"));
    assert!(!has(&rt, DIALOG));
}

#[test]
fn high_risk_cancel_clears_the_flag_without_saving() {
    let mut scene = AuditState::edit(&profile("risk-2", AiProviderType::OpenAi));
    scene.draft.safety_gate.data_egress_level = AiDataEgressLevel::Unrestricted;
    scene.draft.endpoint = TextEdit::new("https://api.remote.com".to_owned());
    let mut rt = new_rt(scene);

    press_in_body(&mut rt, "ai-profile-save");
    assert!(has(&rt, "ai-profile-high-risk-title"));
    press(&mut rt, "ai-profile-high-risk-cancel");
    assert!(!rt.state.draft.show_high_risk_modal, "setShowHighRiskModal(false)");
    settle(&mut rt);
    assert!(!has(&rt, "ai-profile-high-risk-title"));
    assert!(rt.state.saved.is_empty(), "nothing saved, no dummy markers");
    assert_eq!(rt.state.closes, 0, "only the confirmation closed");
    assert!(has(&rt, DIALOG), "the edit dialog stays open");
}

#[test]
fn the_saving_flag_swaps_the_label_and_blocks_the_press() {
    let mut rt = new_rt(AuditState::add());
    // `update` marks the view dirty — a bare `rt.state` write would leave
    // the committed view reading the old state.
    rt.update(|s, _| s.draft.saving = true);
    settle(&mut rt);
    assert_eq!(button_label(&rt, "ai-profile-save"), t("settings.ai.saving"));
    press_in_body(&mut rt, "ai-profile-save");
    assert!(rt.state.saved.is_empty(), "a saving button does not fire");
}

// ---- geometry -------------------------------------------------------------------

#[test]
fn the_panel_fits_the_window() {
    let mut rt = new_rt(AuditState::add());
    settle(&mut rt);

    let panel = maybe(&rt, DIALOG).expect("the panel is laid out");
    let [x, y, w, h] = panel.rect;
    assert!(x >= 0.0 && y >= 0.0, "inside the window at ({x},{y})");
    assert!(
        x + w <= 960.0 && y + h <= 640.0,
        "inside the window at ({},{}) size {}x{}",
        x + w,
        y + h,
        w,
        h
    );
    // React's `max-w-xl` (576pt) plus the layer's 16pt padding each side.
    assert!(w <= 608.0, "panel width {w}");
}
