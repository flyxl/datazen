//! T408 audit: the SkillsSection panel, verified against the React
//! source re-read line by line
//! (`windows/settings/SkillsSection.tsx`). Element evidence comes
//! from `Runtime::elements()`, presses go through real pointer
//! events.

use datazen_ui_api::{AiSkillSource, AiSkillSummary};
use rune::{col, El, Env, Runtime, TextEdit};

use datazen_ui::views::skills_section::{SkillsSection, SkillsSectionState};

#[derive(Clone)]
struct AuditState {
    panel: SkillsSectionState,
    dark: bool,
    // the host's records
    enabled_sets: Vec<bool>,
    auto_use_sets: Vec<bool>,
    skill_toggles: Vec<String>,
    directory_saves: usize,
    refreshes: usize,
    browse_presses: usize,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            panel: SkillsSectionState::new(None),
            dark: true,
            enabled_sets: Vec::new(),
            auto_use_sets: Vec::new(),
            skill_toggles: Vec::new(),
            directory_saves: 0,
            refreshes: 0,
            browse_presses: 0,
        }
    }

    fn skill(id: &str, name: &str, source: AiSkillSource) -> AiSkillSummary {
        AiSkillSummary {
            id: id.to_owned(),
            name: name.to_owned(),
            description: format!("{name} description"),
            source,
            path: format!("~/.agents/skills/{id}"),
            version: None,
        }
    }
}

/// The host's shared refresh body (`refresh()`): count the run,
/// arm the read, answer instantly in the audit.
fn do_refresh(s: &mut AuditState) {
    s.refreshes += 1;
    if s.panel.begin_refresh() {
        s.panel.note_listing(vec![
            AuditState::skill("alpha", "Alpha", AiSkillSource::User),
            AuditState::skill("beta", "Beta", AiSkillSource::Custom),
        ]);
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let k = datazen_ui::ui::kit::kit(s.dark);
    let section = SkillsSection::<AuditState>::new(&s.panel, s.dark, |s: &mut AuditState| {
        &mut s.panel.custom_dir
    })
    .on_set_enabled(|s, enabled| {
        s.enabled_sets.push(enabled);
        let mut next = s.panel.skills.clone();
        next.enabled = enabled;
        s.panel.persist(next);
        // The refresh effect refires when the enabled identity
        // changes (the useCallback [skills.enabled] rule).
        if s.panel.refresh_due() {
            do_refresh(s);
        }
    })
    .on_set_auto_use(|s, auto_use| {
        s.auto_use_sets.push(auto_use);
        let mut next = s.panel.skills.clone();
        next.auto_use = auto_use;
        s.panel.persist(next);
    })
    .on_toggle_skill(|s, id| {
        s.skill_toggles.push(id.clone());
        let checked = s.panel.skills.selected.iter().any(|current| *current == id);
        let next = s.panel.toggle_selected(&id, !checked);
        s.panel.persist(next);
    })
    .on_save_directory(|s| {
        s.directory_saves += 1;
        let next = s.panel.directory_settings();
        s.panel.persist(next);
        // `await refresh()` after the save.
        do_refresh(s);
    })
    .on_refresh(|s: &mut AuditState| do_refresh(s))
    .on_browse(|s| {
        s.browse_presses += 1;
    })
    .build(&k, &mut layer);
    col().max_w(680.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (900.0, 1200.0),
        Env { system_dark: true, scale: 1.0 },
    );
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

fn reseed(rt: &mut Runtime<AuditState>) {
    rt.invalidate();
    settle(rt);
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

/// The disabled face: the switches and the disabled hint; no
/// listing, no selection hint.
#[test]
fn the_disabled_face_mounts() {
    let rt = rt(AuditState::empty());
    for key in [
        "ai-skills-section",
        "ai-skills-cli-hint",
        "ai-skills-enabled-switch",
        "ai-skills-auto-use-switch",
        "ai-skills-dir-input",
        "ai-skills-dir-browse",
        "ai-skills-save-dir",
        "ai-skills-refresh",
        "ai-skills-disabled",
        "ai-skills-default-dir",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "ai-skills-list"));
    assert!(!has(&rt, "ai-skills-empty"));
    assert!(!has(&rt, "ai-skills-selection-hint"));
}

/// The enabled, loaded face: the selection hint (manual mode), the
/// list, one row per skill with its checkbox.
#[test]
fn the_enabled_face_lists_the_skills() {
    let mut scene = AuditState::empty();
    scene.panel.skills.enabled = true;
    scene.panel.note_listing(vec![
        AuditState::skill("alpha", "Alpha", AiSkillSource::User),
        AuditState::skill("beta", "Beta", AiSkillSource::Custom),
    ]);
    let rt = rt(scene);
    for key in [
        "ai-skills-selection-hint",
        "ai-skills-list",
        "ai-skills-row-alpha",
        "ai-skills-row-beta",
        "ai-skills-select-alpha",
        "ai-skills-select-beta",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "ai-skills-disabled"));
    assert!(!has(&rt, "ai-skills-empty"));
}

/// The empty face: enabled, loaded, nothing found.
#[test]
fn the_empty_face_answers_nothing_found() {
    let mut scene = AuditState::empty();
    scene.panel.skills.enabled = true;
    scene.panel.note_listing(Vec::new());
    let rt = rt(scene);
    assert!(has(&rt, "ai-skills-empty"));
    assert!(!has(&rt, "ai-skills-list"));
}

/// The enabled toggle writes through and the disabled hint swaps for
/// the empty listing face.
#[test]
fn the_enabled_toggle_swaps_the_faces() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "ai-skills-enabled-switch");
    assert_eq!(rt.state.enabled_sets, vec![true]);
    assert!(rt.state.panel.skills.enabled, "optimistic");
    // The audit's on_refresh answered with the two-skill listing.
    assert!(has(&rt, "ai-skills-list"));
    assert!(!has(&rt, "ai-skills-disabled"));
    // Off again: the listing clears, the disabled hint returns
    // (the effect refires on the disabled identity too).
    press(&mut rt, "ai-skills-enabled-switch");
    assert_eq!(rt.state.enabled_sets, vec![true, false]);
    assert!(!rt.state.panel.skills.enabled);
    assert!(rt.state.panel.listing.is_empty(), "the disabled refresh cleared");
    reseed(&mut rt);
    assert!(has(&rt, "ai-skills-disabled"));
    assert!(!has(&rt, "ai-skills-list"));
}

/// A skill checkbox toggles its selection (the set semantics).
#[test]
fn a_skill_checkbox_toggles_selection() {
    let mut scene = AuditState::empty();
    scene.panel.skills.enabled = true;
    scene.panel.note_listing(vec![AuditState::skill(
        "alpha",
        "Alpha",
        AiSkillSource::User,
    )]);
    let mut rt = rt(scene);
    press(&mut rt, "ai-skills-select-alpha");
    assert_eq!(rt.state.skill_toggles, vec!["alpha".to_owned()]);
    assert_eq!(
        rt.state.panel.skills.selected,
        vec!["alpha".to_owned()],
        "checked on the first press"
    );
    press(&mut rt, "ai-skills-select-alpha");
    assert!(rt.state.panel.skills.selected.is_empty(), "off on the second");
}

/// Auto-use checks every skill and disables the checkboxes (React's
/// `disabled={skills.autoUse}` + the `checked` short-circuit).
#[test]
fn auto_use_forces_every_skill_checked() {
    let mut scene = AuditState::empty();
    scene.panel.skills.enabled = true;
    scene.panel.skills.auto_use = true;
    scene.panel.note_listing(vec![AuditState::skill(
        "alpha",
        "Alpha",
        AiSkillSource::User,
    )]);
    let rt = rt(scene);
    assert!(has(&rt, "ai-skills-select-alpha"));
    // The selection hint is gone in auto-use mode.
    assert!(!has(&rt, "ai-skills-selection-hint"));
}

/// The save-directory button persists the trimmed draft and
/// refreshes; the browse routes to the host.
#[test]
fn the_save_directory_button_persists_and_refreshes() {
    let mut rt = rt(AuditState::empty());
    rt.state.panel.skills.enabled = true;
    rt.state.panel.custom_dir = TextEdit::new("  /my/skills  ".to_owned());
    reseed(&mut rt);
    press(&mut rt, "ai-skills-save-dir");
    assert_eq!(rt.state.directory_saves, 1);
    assert_eq!(
        rt.state.panel.skills.custom_dir.as_deref(),
        Some("/my/skills"),
        "trimmed on save"
    );
    press(&mut rt, "ai-skills-refresh");
    assert_eq!(rt.state.refreshes, 2, "the button refreshes");
    press(&mut rt, "ai-skills-dir-browse");
    assert_eq!(rt.state.browse_presses, 1);
}

/// The refresh failure renders the error line.
#[test]
fn a_failed_refresh_renders_the_error() {
    let mut scene = AuditState::empty();
    scene.panel.skills.enabled = true;
    scene.panel.begin_refresh();
    scene.panel.note_refresh_error("discovery failed");
    let rt = rt(scene);
    assert!(has(&rt, "ai-skills-error"));
}
