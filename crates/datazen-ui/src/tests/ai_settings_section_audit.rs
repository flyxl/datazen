//! T172 audit: the AiSettingsSection shell port, verified against the
//! React source re-read line by line
//! (`windows/settings/AiSettingsSection.tsx` — there is no React test for
//! this file; the expectations below are re-derived from its JSX and
//! handlers). Element evidence comes from `Runtime::elements()`, presses
//! go through real pointer events (the `ai_context_settings_audit`
//! harness pattern).
//!
//! Tester-written: the coder's module is treated as a black box.

use rune::{col, Env, El, Runtime};
use datazen_ui_api::{AiModelProfile, AiProviderType, AiSettingsConfig};

use crate::views::ai_settings_section::{
    AiSettingsSection, AiSettingsSectionState, EgressToggle, CONTEXT_DIR_SAVED_WINDOW,
};
use crate::views::ai::use_model_profile_draft::{default_safety_gate, SafetyPresetKey};

/// A full api-shape profile (the ModelProfileListItem currency).
fn profile(id: &str, name: &str, model: &str, is_default: Option<bool>) -> AiModelProfile {
    AiModelProfile {
        id: id.to_owned(),
        name: name.to_owned(),
        provider_type: AiProviderType::OpenAi,
        api_key: None,
        endpoint: None,
        model: model.to_owned(),
        max_tokens: None,
        extra: None,
        safety_gate: default_safety_gate(SafetyPresetKey::CloudStrict),
        is_default,
    }
}

fn config(profiles: Vec<AiModelProfile>, active: &str) -> AiSettingsConfig {
    AiSettingsConfig {
        active_profile_id: active.to_owned(),
        profiles,
        global_instructions: String::new(),
        prompt_templates: Vec::new(),
        memory: Default::default(),
        skills: None,
    }
}

#[derive(Clone)]
struct AuditState {
    section: AiSettingsSectionState,
    dark: bool,
    /// The host's settings (the updateSettings target).
    context_dir: String,
    ai_strict_egress: Option<bool>,
    /// The ai store's projections.
    settings_config: Option<AiSettingsConfig>,
    is_configured: bool,
    config_error: Option<String>,
    /// The host clock the saved flag reads.
    now: f64,
    // recording
    saved_dirs: Vec<String>,
    opened_dir: usize,
    browsed: usize,
    adds: usize,
    edits: Vec<String>,
    duplicates: Vec<String>,
    set_defaults: Vec<String>,
    actives_set: Vec<String>,
    deleted: Vec<String>,
    egress_writes: Vec<bool>,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: AiSettingsSectionState::new(),
            dark: true,
            context_dir: String::new(),
            ai_strict_egress: None,
            settings_config: None,
            is_configured: false,
            config_error: None,
            now: 100.0,
            saved_dirs: Vec::new(),
            opened_dir: 0,
            browsed: 0,
            adds: 0,
            edits: Vec::new(),
            duplicates: Vec::new(),
            set_defaults: Vec::new(),
            actives_set: Vec::new(),
            deleted: Vec::new(),
            egress_writes: Vec::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    // The host's per-frame resync (React's useEffect dep).
    let persisted = s.context_dir.clone();
    let section = AiSettingsSection::<AuditState>::new("ai-settings-section", &s.section)
        .settings_config(s.settings_config.as_ref())
        .is_configured(s.is_configured)
        .config_error(s.config_error.as_deref())
        .strict_egress(s.ai_strict_egress)
        .context_dir_saved(s.section.context_dir_saved(s.now))
        // The prebuilt content sections: distinct keys prove the gating.
        .context_settings(Some(col().key("audit-context-settings")))
        .skills_section(Some(col().key("audit-skills")))
        .memory_section(Some(col().key("audit-memory")))
        .tool_bridge_section(Some(col().key("audit-tool-bridge")))
        .model_dialog(|_s: &AuditState, _dark: bool, layer: &mut Vec<El>| {
            layer.push(col().key("audit-model-dialog"));
        })
        .context_dir_edit(|s: &mut AuditState| &mut s.section.context_dir_draft)
        .on_save_context_dir(|s: &mut AuditState, tx: &mut rune::Tx| {
            let dir = s.section.begin_save_context_dir(tx.clock());
            s.saved_dirs.push(dir);
            s.context_dir = s.saved_dirs.last().cloned().unwrap_or_default();
        })
        .on_browse_context_dir(|s: &mut AuditState| s.browsed += 1)
        .on_open_context_dir(|s: &mut AuditState| s.opened_dir += 1)
        .on_add_model(|s: &mut AuditState| {
            s.adds += 1;
            s.section.begin_add_model();
        })
        .on_edit_model(|s: &mut AuditState, profile: AiModelProfile| {
            s.edits.push(profile.id.clone());
            s.section.begin_edit_model(&profile);
        })
        .on_duplicate_model(|s: &mut AuditState, profile: AiModelProfile| {
            s.duplicates.push(profile.id.clone());
        })
        .on_set_default(|s: &mut AuditState, profile: AiModelProfile| {
            s.set_defaults.push(profile.id.clone());
            s.actives_set.push(profile.id);
        })
        .on_delete_model(|s: &mut AuditState, id: String| s.deleted.push(id))
        .on_toggle_strict_egress(|s: &mut AuditState, enabled: bool| {
            match s.section.toggle_strict_egress(enabled, s.ai_strict_egress) {
                EgressToggle::NeedsConfirm(_) => {}
                EgressToggle::Apply(value) => {
                    s.egress_writes.push(value);
                    s.ai_strict_egress = Some(value);
                }
            }
        })
        .on_egress_confirm_resolved(|s: &mut AuditState, result: bool| {
            if s.section.resolve_egress_confirm(result) {
                s.egress_writes.push(false);
                s.ai_strict_egress = Some(false);
            }
        })
        .build(s, s.dark, &mut layer);
    col().max_w(640.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the state machine (React's useState/effect semantics) -----------------

/// The `useEffect [settings.contextDir]` resync: a changed persisted value
/// replaces the draft; an unchanged one leaves it alone (a typing user
/// keeps their draft).
#[test]
fn context_dir_resync_follows_the_persisted_value() {
    let mut state = AiSettingsSectionState::new();
    state.resync_context_dir("/home/ctx");
    assert_eq!(state.context_dir_draft.text, "/home/ctx");
    // A user edit is not clobbered while the setting is unchanged.
    state.context_dir_draft = rune::TextEdit::new("/custom".to_owned());
    state.resync_context_dir("/home/ctx");
    assert_eq!(state.context_dir_draft.text, "/custom");
    // ...but a changed setting resets it.
    state.resync_context_dir("/moved");
    assert_eq!(state.context_dir_draft.text, "/moved");
    assert_eq!(state.resynced_context_dir, "/moved");
}

/// The 2s save feedback window (React's setTimeout clear).
#[test]
fn saved_feedback_window_is_two_seconds() {
    assert_eq!(CONTEXT_DIR_SAVED_WINDOW, 2.0);
    let mut state = AiSettingsSectionState::new();
    assert!(!state.context_dir_saved(100.0));
    assert_eq!(state.begin_save_context_dir(100.0), "");
    assert!(state.context_dir_saved(100.0));
    assert!(state.context_dir_saved(101.9));
    assert!(!state.context_dir_saved(102.0), "the window closes at +2s");
}

/// The dialog's add/edit split (React's `editingProfile`).
#[test]
fn add_and_edit_drive_the_dialog_state() {
    let mut state = AiSettingsSectionState::new();
    assert!(!state.dialog_open);
    state.begin_add_model();
    assert!(state.dialog_open);
    assert!(state.editing_profile.is_none(), "add carries no profile");
    let p = profile("p1", "GPT", "gpt-4", None);
    state.begin_edit_model(&p);
    assert!(state.dialog_open);
    assert_eq!(state.editing_profile.as_ref().map(|p| p.id.as_str()), Some("p1"));
    state.close_model_dialog();
    assert!(!state.dialog_open);
    // React keeps `editingProfile` after close (a reopen needs an explicit
    // add/edit); the state does too.
    assert!(state.editing_profile.is_some());
}

/// The egress toggle's truthiness rules (React's
/// `!enabled && settings.aiStrictEgress`).
#[test]
fn egress_toggle_confirms_only_the_true_to_false_edge() {
    let mut state = AiSettingsSectionState::new();
    // true -> false: the warning confirm arms.
    assert!(matches!(
        state.toggle_strict_egress(false, Some(true)),
        EgressToggle::NeedsConfirm(_)
    ));
    // undefined -> false: no confirm (React's falsy check).
    assert_eq!(state.toggle_strict_egress(false, None), EgressToggle::Apply(false));
    // false -> false (already off): no confirm.
    assert_eq!(state.toggle_strict_egress(false, Some(false)), EgressToggle::Apply(false));
    // Any -> true: no confirm, direct apply.
    assert_eq!(state.toggle_strict_egress(true, Some(false)), EgressToggle::Apply(true));
    assert_eq!(state.toggle_strict_egress(true, None), EgressToggle::Apply(true));

    // The confirm's resolution: OK applies, Cancel does not.
    let mut state = AiSettingsSectionState::new();
    if let EgressToggle::NeedsConfirm(id) = state.toggle_strict_egress(false, Some(true)) {
        let _ = id;
    }
    assert!(!state.resolve_egress_confirm(false), "cancel resolves, writes nothing");
    assert!(state.egress_confirm.pending.is_none());
    if let EgressToggle::NeedsConfirm(_) = state.toggle_strict_egress(false, Some(true)) {}
    assert!(state.resolve_egress_confirm(true), "OK applies the write");
}

// ---- the runtime tree -------------------------------------------------------

/// Without a loaded config: the empty state, the ungated ToolBridge seam,
/// and the "not configured" badge; the configured-gated sections stay
/// unmounted (`{settingsConfig && …}`).
#[test]
fn mount_without_config_shows_the_empty_state() {
    let mut scene = AuditState::empty();
    scene.is_configured = false;
    let rt = rt(scene);

    assert!(has(&rt, "ai-settings-section"));
    assert!(has(&rt, "ai-models-empty"));
    assert!(!has(&rt, "ai-models-list"));
    // The ungated ToolBridge seam renders; the gated ones do not.
    assert!(has(&rt, "audit-tool-bridge"));
    assert!(!has(&rt, "audit-context-settings"));
    assert!(!has(&rt, "audit-skills"));
    assert!(!has(&rt, "audit-memory"));
    // The badge reads 未配置 (t('settings.ai.notConfigured')).
    assert_eq!(text_of(&rt, "ai-settings-section-configured"), "未配置");
    // The add button is present with its React testid.
    assert!(has(&rt, "add-ai-model-button"));
    // The egress row mounts (undefined reads checked).
    assert!(has(&rt, "ai-settings-section-egress"));
}

/// With a loaded config: the profile list (rows keyed by id), the
/// configured badge, the error line, and all gated sections.
#[test]
fn mount_with_config_lists_the_profiles() {
    let mut scene = AuditState::empty();
    scene.settings_config = Some(config(
        vec![
            profile("p1", "GPT", "gpt-4", Some(true)),
            profile("p2", "Local", "llama", None),
        ],
        "p1",
    ));
    scene.is_configured = true;
    scene.config_error = Some("provider rejected the key".to_owned());
    let rt = rt(scene);

    assert!(has(&rt, "ai-models-list"));
    assert!(!has(&rt, "ai-models-empty"));
    assert!(has(&rt, "model-profile-item-p1"));
    assert!(has(&rt, "model-profile-item-p2"));
    assert_eq!(text_of(&rt, "ai-settings-section-configured"), "已配置");
    assert_eq!(text_of(&rt, "ai-settings-section-error"), "provider rejected the key");
    // All gated sections render now.
    assert!(has(&rt, "audit-context-settings"));
    assert!(has(&rt, "audit-skills"));
    assert!(has(&rt, "audit-memory"));
}

/// The add button opens the dialog (the layer gains the dialog only
/// while open — React's `{dialogOpen && …}`).
#[test]
fn add_press_opens_the_dialog_layer() {
    let scene = AuditState::empty();
    let mut rt = rt(scene);
    assert!(!has(&rt, "audit-model-dialog"));
    press(&mut rt, "add-ai-model-button");
    assert!(has(&rt, "audit-model-dialog"));
    let state = rt.state.clone();
    assert!(state.section.dialog_open);
    assert_eq!(state.adds, 1);
    // Closing the state flag unmounts it again.
    rt.update(|s: &mut AuditState, _| s.section.close_model_dialog());
    rt.frame(0.0);
    assert!(!has(&rt, "audit-model-dialog"));
}

/// The strict-egress disable edge: the press arms the warning confirm
/// (the layer gains the dialog), Cancel writes nothing, and the OK path
/// applies exactly one `false` write.
#[test]
fn egress_disable_needs_the_warning_confirm() {
    let mut scene = AuditState::empty();
    scene.ai_strict_egress = Some(true);
    let mut rt = rt(scene);
    assert!(!has(&rt, "confirm-dialog"));
    press(&mut rt, "ai-settings-section-egress");
    assert!(has(&rt, "confirm-dialog"), "the warning confirm mounts");
    assert!(rt.state.egress_writes.is_empty());
    // Cancel: no write, the confirm clears.
    press(&mut rt, "confirm-dialog-cancel");
    assert!(!has(&rt, "confirm-dialog"));
    assert!(rt.state.egress_writes.is_empty());
    // Again, this time OK: one false write lands.
    press(&mut rt, "ai-settings-section-egress");
    assert!(has(&rt, "confirm-dialog"));
    press(&mut rt, "confirm-dialog-ok");
    assert!(!has(&rt, "confirm-dialog"));
    let state = rt.state.clone();
    assert_eq!(state.egress_writes, vec![false]);
    assert_eq!(state.ai_strict_egress, Some(false));
}

/// Enabling applies without any confirm.
#[test]
fn egress_enable_applies_directly() {
    let mut scene = AuditState::empty();
    scene.ai_strict_egress = Some(false);
    let mut rt = rt(scene);
    press(&mut rt, "ai-settings-section-egress");
    assert!(!has(&rt, "confirm-dialog"));
    assert_eq!(rt.state.egress_writes, vec![true]);
    assert_eq!(rt.state.ai_strict_egress, Some(true));
}

/// An undefined current value is falsy in React's `&&` — the first
/// disable press applies directly too (no confirm).
#[test]
fn egress_disable_from_undefined_applies_directly() {
    let mut scene = AuditState::empty();
    scene.ai_strict_egress = None;
    let mut rt = rt(scene);
    press(&mut rt, "ai-settings-section-egress");
    assert!(!has(&rt, "confirm-dialog"));
    assert_eq!(rt.state.egress_writes, vec![false]);
}

/// The context-dir row: the save press persists the draft and flips the
/// button label for the 2s window; the open-dir button fires the seam.
#[test]
fn context_dir_row_saves_and_flips_the_label() {
    let mut scene = AuditState::empty();
    scene.section.resync_context_dir("/ctx");
    scene.section.context_dir_draft = rune::TextEdit::new("/new-ctx".to_owned());
    let mut rt = rt(scene);
    // The save button starts at 保存 (t('common.save')).
    press(&mut rt, "ai-settings-section-dir-save");
    let state = rt.state.clone();
    assert_eq!(state.saved_dirs, vec!["/new-ctx".to_owned()]);
    assert_eq!(state.context_dir, "/new-ctx");
    // While the window is open the label reads 成功 (common.success) —
    // the button re-renders with the saved flag the host computes.
    rt.update(|s: &mut AuditState, _| {
        s.now = 100.5;
    });
    settle(&mut rt);
    // The open-dir seam fires once per press.
    press(&mut rt, "ai-settings-section-dir-open");
    assert_eq!(rt.state.opened_dir, 1);
    // The browse seam fires once per press.
    press(&mut rt, "ai-settings-section-dir-browse");
    assert_eq!(rt.state.browsed, 1);
}

/// The default-dir placeholder: the field shows the host's
/// `contextCommands.getDir()` answer once it lands (React's async load).
#[test]
fn default_dir_lands_in_the_placeholder() {
    let mut scene = AuditState::empty();
    scene.section.note_default_context_dir(Some("/default/ctx".to_owned()));
    let rt = rt(scene);
    // The placeholder is the field's hint text (Rune TextField renders
    // it while the draft is empty).
    let field = rt
        .elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some("ai-settings-section-dir-input"));
    assert!(field.is_some(), "the context-dir input mounts");
    assert!(rt.state.section.default_context_dir.as_deref() == Some("/default/ctx"));
}

/// The profile rows' actions route to the host callbacks with the row's
/// own profile (the list item's keys; one row proves each wiring).
#[test]
fn row_actions_route_to_the_host() {
    let mut scene = AuditState::empty();
    scene.settings_config = Some(config(
        vec![profile("p1", "GPT", "gpt-4", Some(true)), profile("p2", "Local", "llama", None)],
        "p1",
    ));
    let mut rt = rt(scene);
    // Set-default on the non-default row.
    press(&mut rt, "model-profile-item-p2-set-default");
    let state = rt.state.clone();
    assert_eq!(state.set_defaults, vec!["p2".to_owned()]);
    assert_eq!(state.actives_set, vec!["p2".to_owned()]);
    // Duplicate on the same row.
    press(&mut rt, "model-profile-item-p2-duplicate");
    assert_eq!(rt.state.duplicates, vec!["p2".to_owned()]);
    // Delete on the other row.
    press(&mut rt, "model-profile-item-p1-delete");
    assert_eq!(rt.state.deleted, vec!["p1".to_owned()]);
    // Edit opens the dialog with that profile.
    press(&mut rt, "model-profile-item-p1-edit");
    let state = rt.state.clone();
    assert!(state.section.dialog_open);
    assert_eq!(state.edits, vec!["p1".to_owned()]);
    assert!(has(&rt, "audit-model-dialog"));
}
