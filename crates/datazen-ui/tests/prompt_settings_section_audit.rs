//! T369 audit: the PromptSettingsSection port, verified against the
//! React source re-read line by line
//! (`windows/settings/PromptSettingsSection.tsx` — no React test file;
//! the expectations are re-derived from its JSX and handlers).
//! Element evidence comes from `Runtime::elements()`, presses go
//! through real pointer events (the `ai_settings_section_audit`
//! harness pattern).

use std::rc::Rc;

use datazen_ui::commands::ai::wire::{PromptInfo, PromptScenario, PromptSource};
use rune::{col, El, Env, Runtime, TextEdit};

use datazen_ui::views::prompt_settings_section::{
    PromptSettingsSection, PromptSettingsSectionState, PROMPT_ALL_DRIVERS, PROMPT_FEEDBACK_WINDOW,
    PROMPT_PREVIEW_CUT, scenario_variables,
};

fn info(scenario: PromptScenario, source: PromptSource, system: &str) -> PromptInfo {
    PromptInfo {
        scenario,
        label: format!("{scenario:?}"),
        source,
        system: system.to_owned(),
        default_system: "default".to_owned(),
    }
}

#[derive(Clone)]
struct AuditState {
    section: PromptSettingsSectionState,
    dark: bool,
    now: f64,
    // the host's records
    driver_types: Vec<String>,
    edit_begun: Vec<usize>,
    saves: usize,
    resets_begun: Vec<usize>,
    resets_resolved: Vec<bool>,
    // the seeded system text the host's "typing" produces
    typed_system: String,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: PromptSettingsSectionState::new(),
            dark: true,
            now: 100.0,
            driver_types: Vec::new(),
            edit_begun: Vec::new(),
            saves: 0,
            resets_begun: Vec::new(),
            resets_resolved: Vec::new(),
            typed_system: String::new(),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let drivers: Vec<(String, String)> = vec![
        ("postgres".to_owned(), "PostgreSQL".to_owned()),
        ("mysql".to_owned(), "MySQL".to_owned()),
    ];
    let section = PromptSettingsSection::<AuditState>::new(
        &s.section,
        s.dark,
        s.now,
        &drivers,
        |s: &mut AuditState| &mut s.section.edit_system,
    )
    .on_set_driver_type(|s, value| {
        s.driver_types.push(value.clone());
        s.section.set_driver_type(&value);
    })
    .on_toggle_driver_select(|s| s.section.open_driver_select = !s.section.open_driver_select)
    .on_close_driver_select(|s| s.section.open_driver_select = false)
    .on_begin_edit(|s, idx| {
        s.edit_begun.push(idx);
        s.section.begin_edit(idx);
    })
    .on_save(|s| {
        s.saves += 1;
        let _ = s.section.begin_save(s.now);
    })
    .on_begin_reset(|s, idx| {
        s.resets_begun.push(idx);
        let _ = s.section.begin_reset(idx);
    })
    .on_reset_resolved(|s, ok| {
        s.resets_resolved.push(ok);
        let _ = s.section.resolve_reset(ok, s.now);
    })
    .build(&mut layer);
    col().max_w(720.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(scene, view, (960.0, 1600.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the pure rules ----------------------------------------------------------

/// `SCENARIO_VARIABLES`, verbatim.
#[test]
fn the_scenario_variables_table_is_verbatim() {
    use PromptScenario as P;
    assert_eq!(scenario_variables(P::Nl2sql), ["db_type", "version", "schema", "recent"]);
    assert_eq!(scenario_variables(P::Diagnose), ["db_type", "schema"]);
    assert_eq!(scenario_variables(P::NlFilter), ["db_type", "columns"]);
    assert_eq!(scenario_variables(P::SchemaDocSelectTables), ["db_type", "table_names"]);
    assert_eq!(scenario_variables(P::SchemaDoc), ["db_type", "schema"]);
    assert_eq!(scenario_variables(P::ConnectionDiagnose), Vec::<&str>::new());
    assert_eq!(scenario_variables(P::QuerySummary), Vec::<&str>::new());
    assert_eq!(scenario_variables(P::ExplainAnalysis), ["db_type"]);
    assert_eq!(scenario_variables(P::Chat), Vec::<&str>::new());
    assert_eq!(scenario_variables(P::WorkflowGenerate), ["db_type", "schema", "connections"]);
    assert_eq!(PROMPT_PREVIEW_CUT, 260);
    assert_eq!(PROMPT_FEEDBACK_WINDOW, 2.0);
    assert_eq!(PROMPT_ALL_DRIVERS, "*");
}

/// The list request: the `*` sentinel rides as `null`.
#[test]
fn the_list_request_maps_the_sentinel_to_null() {
    let state = PromptSettingsSectionState::new();
    assert_eq!(state.driver_type, "*");
    assert_eq!(state.list_request().value["driverType"], serde_json::Value::Null);
    let mut state = PromptSettingsSectionState::new();
    state.set_driver_type("postgres");
    assert_eq!(state.list_request().value["driverType"], "postgres");
}

/// `handleSave`'s entry build and the 2s feedback window.
#[test]
fn the_save_builds_the_entry_and_arms_the_feedback() {
    let mut state = PromptSettingsSectionState::new();
    state.set_driver_type("mysql");
    state.deliver_prompts(vec![info(PromptScenario::Nl2sql, PromptSource::Default, "base")]);
    state.begin_edit(0);
    state.edit_system = TextEdit::new("my override".to_owned());
    let Some((entry, request)) = state.begin_save(100.0) else {
        panic!("the save builds");
    };
    assert_eq!(entry.driver_type, "mysql", "the CURRENT filter rides");
    assert_eq!(entry.scenario, PromptScenario::Nl2sql);
    assert_eq!(entry.system, "my override");
    assert_eq!(request.value["entry"]["driverType"], "mysql");
    assert_eq!(request.value["entry"]["scenario"], "nl2sql");
    assert_eq!(state.editing_idx, None, "the form closed");
    assert_eq!(state.feedback_key, Some("settings.prompts.saved"));
    assert_eq!(state.feedback(101.9), Some("settings.prompts.saved"));
    assert_eq!(state.feedback(102.0), None, "the window closes at +2s");
    // No open edit: nothing saves.
    assert!(state.begin_save(200.0).is_none());
}

/// `handleReset`: the confirm gates the removal; a non-user source
/// never resets.
#[test]
fn the_reset_gates_on_the_confirm() {
    let mut state = PromptSettingsSectionState::new();
    state.set_driver_type("postgres");
    state.deliver_prompts(vec![
        info(PromptScenario::Chat, PromptSource::User, "mine"),
        info(PromptScenario::Diagnose, PromptSource::Default, "stock"),
    ]);
    // A default source never resets (the button's rule).
    assert!(state.begin_reset(1).is_none());
    assert!(!state.confirm.is_open(), "no dialog for a non-user source");
    // A user source opens the warning confirm.
    assert!(state.begin_reset(0).is_none(), "the dialog opens, no request yet");
    assert!(state.confirm.is_open());
    // A cancelled confirm removes nothing.
    assert!(state.resolve_reset(false, 100.0).is_none());
    assert!(!state.confirm.is_open());
    assert_eq!(state.feedback(100.5), None, "no feedback on cancel");
    // A confirmed one proceeds with the removal + the resetDone line.
    assert!(state.begin_reset(0).is_none());
    let request = state.resolve_reset(true, 200.0).expect("the removal proceeds");
    assert_eq!(request.value["driverType"], "postgres");
    assert_eq!(request.value["scenario"], "chat");
    assert_eq!(state.feedback(200.5), Some("settings.prompts.resetDone"));
}

/// The load answer closes any open edit.
#[test]
fn the_load_answer_closes_the_edit() {
    let mut state = PromptSettingsSectionState::new();
    state.deliver_prompts(vec![info(PromptScenario::Chat, PromptSource::User, "x")]);
    state.begin_edit(0);
    assert_eq!(state.editing_idx, Some(0));
    state.deliver_prompts(vec![info(PromptScenario::Chat, PromptSource::User, "x")]);
    assert_eq!(state.editing_idx, None, "loadPrompts resets the edit");
}

// ---- the element evidence ------------------------------------------------------

/// The mount shape: the driver row, the cards with their badges, the
/// user-only reset button, the preview cut.
#[test]
fn mount_renders_the_cards_by_source() {
    let mut scene = AuditState::empty();
    scene.section.deliver_prompts(vec![
        info(PromptScenario::Nl2sql, PromptSource::Default, "default text"),
        info(PromptScenario::Chat, PromptSource::User, "user text"),
        info(PromptScenario::ExplainAnalysis, PromptSource::Driver, "driver text"),
    ]);
    let rt = rt(scene);
    assert!(has(&rt, "prompt-settings-section"));
    assert!(has(&rt, "prompt-settings-driver-trigger"));
    for scenario in ["nl2sql", "chat", "explain_analysis"] {
        assert!(has(&rt, &format!("prompt-card-{scenario}")));
        assert!(has(&rt, &format!("prompt-card-{scenario}-source")));
        assert!(has(&rt, &format!("prompt-card-{scenario}-preview")));
    }
    // Only the user source carries the reset button.
    assert!(has(&rt, "prompt-card-chat-reset"));
    assert!(!has(&rt, "prompt-card-nl2sql-reset"));
    assert!(!has(&rt, "prompt-card-explain_analysis-reset"));
    // Every card has the edit button.
    for scenario in ["nl2sql", "chat", "explain_analysis"] {
        assert!(has(&rt, &format!("prompt-card-{scenario}-edit")));
    }
}

/// The original card's box includes its border; badges use the translated
/// label rather than the translation key shown by the old generic badge.
#[test]
fn source_pills_are_localized_and_card_border_reserves_layout() {
    let rows = [
        (PromptScenario::Nl2sql, PromptSource::Default, "nl2sql", "default"),
        (PromptScenario::Chat, PromptSource::User, "chat", "user"),
        (PromptScenario::ExplainAnalysis, PromptSource::Driver, "explain_analysis", "driver"),
    ];
    for dark in [false, true] {
        let mut scene = AuditState::empty();
        scene.dark = dark;
        scene.section.deliver_prompts(rows.iter().map(|&(scenario, source, _, _)| {
            info(scenario, source, "preview line")
        }).collect());
        let rt = rt(scene);
        let elements = rt.elements();
        let element = |key: &str| elements.iter()
            .find(|el| !el.exiting && el.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"));
        let mut previous_bottom: Option<f32> = None;
        for &(_, _, scenario, source) in &rows {
            let base = format!("prompt-card-{scenario}");
            let source_label = format!("settings.prompts.source.{source}");
            let badge = element(&format!("{base}-source")).rect;
            let label = element(&format!("{base}-source-text"));
            let expected = datazen_ui::i18n::t(&source_label);
            assert_ne!(expected, source_label.as_str());
            assert_eq!(label.text.as_deref(), Some(expected));
            assert!((badge[3] - 19.0).abs() <= 0.25, "{badge:?}");

            let card = element(&base).rect;
            let head = element(&format!("{base}-head")).rect;
            let preview = element(&format!("{base}-preview-box")).rect;
            assert!((head[0] - card[0] - 13.0).abs() <= 0.25, "{head:?} {card:?}");
            assert!((head[1] - card[1] - 13.0).abs() <= 0.25, "{head:?} {card:?}");
            assert!((card[1] + card[3] - preview[1] - preview[3] - 13.0).abs() <= 0.25,
                "{preview:?} {card:?}");
            if let Some(previous_bottom) = previous_bottom {
                assert!((card[1] - previous_bottom - 8.0).abs() <= 0.25, "{card:?}");
            }
            previous_bottom = Some(card[1] + card[3]);
        }
    }
}

/// The edit flow: pencil → form with the seeded field → save.
#[test]
fn the_edit_flow_renders_and_saves() {
    let mut scene = AuditState::empty();
    scene.section.deliver_prompts(vec![info(PromptScenario::Nl2sql, PromptSource::Default, "base template")]);
    let mut rt = rt(scene);
    assert!(!has(&rt, "prompt-card-nl2sql-form"));
    press(&mut rt, "prompt-card-nl2sql-edit");
    assert!(has(&rt, "prompt-card-nl2sql-form"));
    assert!(has(&rt, "prompt-edit-system"));
    assert!(has(&rt, "prompt-edit-save"));
    assert!(has(&rt, "prompt-edit-cancel"));
    assert_eq!(rt.state.section.edit_system.text, "base template", "the seed");
    press(&mut rt, "prompt-edit-save");
    assert_eq!(rt.state.saves, 1);
    assert!(!has(&rt, "prompt-card-nl2sql-form"), "the form closed");
    assert!(has(&rt, "prompt-settings-feedback"), "the saved line shows");
}
