//! Port of `packages/frontend/src/windows/settings/SkillsSection.tsx`
//! (T408) — Settings → AI → Skills.
//!
//! The host owns discovery and path policy; this panel only toggles
//! opt-in switches and renders the name/description listing.
//!
//! ## What maps where
//!
//! - `useState(normalizeAiSkillsSettings(config.skills))` + the
//!   `useEffect([config.skills])` resync are
//!   [`SkillsSectionState::new`] / [`SkillsSectionState::resync`]
//!   (the normalize fills fields omitted by configs written before
//!   T17: strict `enabled`/`autoUse`, string-or-null `customDir`,
//!   deduped string `selected`).
//! - `customDir`'s draft is the state's `custom_dir` `TextEdit`
//!   (trimmed to null on save).
//! - The `refresh` callback + its effect are the refresh plan:
//!   [`SkillsSectionState::refresh_due`] (the effect refires when
//!   `skills.enabled` changes — the `useCallback [skills.enabled]`
//!   identity rule), [`SkillsSectionState::begin_refresh`] (disabled
//!   clears the listing; enabled arms loading), and
//!   [`SkillsSectionState::note_listing`] /
//!   [`SkillsSectionState::note_refresh_error`] (the `finally` arm
//!   clears loading either way).
//! - `persist` is optimistic with NO revert: [`Self::persist`] sets
//!   the skills; the failure only raises the load error
//!   ([`Self::note_save_failed`]) — the memory section's revert rule
//!   does NOT apply here (the React file's own shape).
//! - `saveDirectory` is persist + refresh: the trimmed draft (empty →
//!   null) then the listing re-reads.
//! - `toggleSelected` is the set semantics: add (deduped) or remove.
//! - `aiCommands.listSkills()` / `pickPath` / `saveSettingsConfig`
//!   are the host slots (`on_refresh`, `on_browse`, `on_save`); the
//!   `{ ...config, skills: next }` spread is the host's (it owns the
//!   config).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `ai-skills-section`,
//! `ai-skills-cli-hint`, `ai-skills-enabled`, `ai-skills-auto-use`,
//! `ai-skills-dir` (the input), `ai-skills-refresh`,
//! `ai-skills-selection-hint`, `ai-skills-disabled`, `ai-skills-empty`,
//! `ai-skills-list`, `ai-skills-row-{id}`, `ai-skills-select-{id}`.
//! The unkeyed save button and the footer carry structural
//! `ai-skills-save-dir` / `ai-skills-default-dir` keys.
//!
//! Known deltas: the source chip's `· {path}` join is one text run;
//! the `title`/`aria-label` on the refresh button ride the kit
//! `help`.

// No production consumer yet (the AI settings section's
// skills_section slot mounts it); this crate's tests are the callers
// meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use datazen_ui_api::{AiSkillSource, AiSkillSummary, AiSkillsSettings, DEFAULT_AI_SKILLS_SETTINGS};
use rune::{col, icon, row, text, tok, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::settings_ui::{section_title, toggle_row, ToggleRow as ToggleRowSpec};
use crate::i18n::t;
use crate::icons::ICON_REFRESH;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::path_input::PathInput;
use crate::ui::toggle::Checkbox;

/// `text-xs` — the hint, error, loading, empty and listing lines.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-[11px]` — the cli hint, the selection hint, the chip, the footer.
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// `text-xs font-medium` — the custom-dir label.
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);
/// `text-sm font-medium` — a skill's name.
const TEXT_SM_MEDIUM: TextStyle = TextStyle::new(14.0, 500.0);
/// `text-[10px]` — the source chip.
const TEXT_10: TextStyle = TextStyle::new(10.0, 400.0);

/// The panel's state.
#[derive(Clone, PartialEq)]
pub struct SkillsSectionState {
    /// The normalized skills settings (the optimistic copy).
    pub skills: AiSkillsSettings,
    /// The custom-dir draft (trimmed to null on save).
    pub custom_dir: TextEdit,
    /// The discovered listing.
    pub listing: Vec<AiSkillSummary>,
    /// The refresh/save error sentence.
    pub error: Option<String>,
    /// The refresh in flight.
    pub loading: bool,
    /// The enabled flag the last refresh armed under (the effect's
    /// dependency — a change re-fires; `None` is the never-armed
    /// mount).
    last_refresh_enabled: Option<bool>,
}

impl std::fmt::Debug for SkillsSectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillsSectionState")
            .field("skills", &self.skills)
            .field("custom_dir", &self.custom_dir.text)
            .field("listing", &self.listing)
            .field("error", &self.error)
            .field("loading", &self.loading)
            .field("last_refresh_enabled", &self.last_refresh_enabled)
            .finish()
    }
}

impl SkillsSectionState {
    /// The mount: normalize the config's skills; the draft mirrors
    /// `customDir ?? ''`.
    pub fn new(config_skills: Option<&AiSkillsSettings>) -> Self {
        let skills = normalize_ai_skills_settings(config_skills);
        let custom_dir = skills.custom_dir.clone().unwrap_or_default();
        Self {
            skills,
            custom_dir: TextEdit::new(custom_dir),
            listing: Vec::new(),
            error: None,
            loading: false,
            last_refresh_enabled: None,
        }
    }

    /// The `useEffect([config.skills])` resync: a changed config
    /// re-normalizes and re-seeds the draft.
    pub fn resync(&mut self, config_skills: Option<&AiSkillsSettings>) {
        let normalized = normalize_ai_skills_settings(config_skills);
        self.custom_dir = TextEdit::new(normalized.custom_dir.clone().unwrap_or_default());
        self.skills = normalized;
    }

    /// Whether the refresh effect should fire now (the mount, or a
    /// changed `skills.enabled` since the last arm).
    pub fn refresh_due(&mut self) -> bool {
        if self.last_refresh_enabled != Some(self.skills.enabled) {
            self.last_refresh_enabled = Some(self.skills.enabled);
            true
        } else {
            false
        }
    }

    /// `refresh`'s head: a disabled panel clears the listing (no
    /// read); an enabled one arms loading and clears the error.
    /// Answers whether the host should run the `listSkills` read.
    pub fn begin_refresh(&mut self) -> bool {
        if !self.skills.enabled {
            self.listing.clear();
            self.loading = false;
            self.error = None;
            return false;
        }
        self.loading = true;
        self.error = None;
        true
    }

    /// The read's answer (`setListing(await listSkills())` + the
    /// `finally` arm's `setLoading(false)`).
    pub fn note_listing(&mut self, listing: Vec<AiSkillSummary>) {
        self.listing = listing;
        self.loading = false;
    }

    /// The read's failure (`cause.message` + the `finally` arm).
    pub fn note_refresh_error(&mut self, message: &str) {
        self.error = Some(message.to_owned());
        self.loading = false;
    }

    /// `persist`'s optimistic half: the next settings land
    /// immediately; the host runs the save and answers through
    /// [`Self::note_save_failed`] on a refusal.
    pub fn persist(&mut self, next: AiSkillsSettings) {
        self.skills = next;
    }

    /// `if (!ok) setError(t('settings.ai.skills.loadError'))` — the
    /// failure only raises the error (no revert; the React shape).
    pub fn note_save_failed(&mut self) {
        self.error = Some(t("settings.ai.skills.loadError").to_owned());
    }

    /// `toggleSelected`: add (deduped) or remove one id.
    pub fn toggle_selected(&mut self, id: &str, checked: bool) -> AiSkillsSettings {
        let mut selected = self.skills.selected.clone();
        if checked {
            if !selected.iter().any(|current| current == id) {
                selected.push(id.to_owned());
            }
        } else {
            selected.retain(|current| current != id);
        }
        let mut next = self.skills.clone();
        next.selected = selected;
        next
    }

    /// One skill's rendered check (`skills.autoUse ||
    /// skills.selected.includes(skill.id)` — auto-use owns every
    /// selection).
    pub fn skill_checked(&self, id: &str) -> bool {
        self.skills.auto_use || self.skills.selected.iter().any(|current| current == id)
    }

    /// `saveDirectory`'s settings: the trimmed draft (empty → null).
    pub fn directory_settings(&self) -> AiSkillsSettings {
        let mut next = self.skills.clone();
        let trimmed = self.custom_dir.text.trim().to_owned();
        next.custom_dir = if trimmed.is_empty() { None } else { Some(trimmed) };
        next
    }
}

/// `normalizeAiSkillsSettings` (`lib/aiSkills.ts`, the T17 backfill):
/// the defaults under the config's fields, with the strict coercions.
pub fn normalize_ai_skills_settings(
    skills: Option<&AiSkillsSettings>,
) -> AiSkillsSettings {
    match skills {
        Some(skills) => AiSkillsSettings {
            // `skills?.enabled === true` — anything else is false.
            enabled: skills.enabled,
            auto_use: skills.auto_use,
            custom_dir: skills.custom_dir.clone(),
            // The dedup + string filter rides the typed Vec (the
            // wire's duplicates are the host's to shed on read).
            selected: {
                let mut seen: Vec<String> = Vec::new();
                for id in &skills.selected {
                    if !seen.iter().any(|current| current == id) {
                        seen.push(id.clone());
                    }
                }
                seen
            },
        },
        None => DEFAULT_AI_SKILLS_SETTINGS,
    }
}

/// `SkillsSection({ config })`: the panel over the state and the
/// host's action slots.
pub struct SkillsSection<'a, S: 'static> {
    state: &'a SkillsSectionState,
    dark: bool,
    custom_dir_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    // host actions
    on_set_enabled: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_set_auto_use: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_toggle_skill: Option<Rc<dyn Fn(&mut S, String)>>,
    on_save_directory: Option<Rc<dyn Fn(&mut S)>>,
    on_refresh: Option<Rc<dyn Fn(&mut S)>>,
    on_browse: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> SkillsSection<'a, S> {
    pub fn new(
        state: &'a SkillsSectionState,
        dark: bool,
        custom_dir_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            custom_dir_edit: Rc::new(custom_dir_edit),
            on_set_enabled: None,
            on_set_auto_use: None,
            on_toggle_skill: None,
            on_save_directory: None,
            on_refresh: None,
            on_browse: None,
        }
    }

    /// The master switch (`persist({ ...skills, enabled })` — the
    /// host composes the next settings through the state and saves).
    pub fn on_set_enabled(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_set_enabled = Some(Rc::new(f));
        self
    }

    /// The auto-use switch (`persist({ ...skills, autoUse })`).
    pub fn on_set_auto_use(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_set_auto_use = Some(Rc::new(f));
        self
    }

    /// One skill's checkbox (`toggleSelected(id, checked)`).
    pub fn on_toggle_skill(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_skill = Some(Rc::new(f));
        self
    }

    /// `saveDirectory` — persist the trimmed directory then refresh.
    pub fn on_save_directory(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_directory = Some(Rc::new(f));
        self
    }

    /// `refresh` — the host runs `aiCommands.listSkills()` and
    /// delivers through the note methods.
    pub fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(f));
        self
    }

    /// `pickPath` — the custom-dir input's browse.
    pub fn on_browse(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_browse = Some(Rc::new(f));
        self
    }

    /// The panel (`tid('ai-skills-section')`).
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut panel = col()
            .key("ai-skills-section")
            .w_full()
            .gap(12.0)
            .pt(16.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted.alpha(0.6)); // border-t border-edge/60

        panel = panel.child(section_title(
            k,
            layer,
            "ai-skills-title".to_owned(),
            t("settings.ai.skills.title"),
            Some(t("settings.ai.skills.description")),
        ));

        panel = panel.child(
            text(t("settings.ai.skills.cliInstallHint").to_owned())
                .key("ai-skills-cli-hint")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );

        // The master switch.
        if let Some(on_set_enabled) = &self.on_set_enabled {
            let on_set_enabled = Rc::clone(on_set_enabled);
            panel = panel.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "ai-skills-enabled".to_owned(),
                    label: t("settings.ai.skills.enabled"),
                    hint: Some(t("settings.ai.skills.enabledHint")),
                    checked: state.skills.enabled,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, enabled: bool| on_set_enabled(s, enabled)),
            ));
        }

        // The auto-use switch.
        if let Some(on_set_auto_use) = &self.on_set_auto_use {
            let on_set_auto_use = Rc::clone(on_set_auto_use);
            panel = panel.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "ai-skills-auto-use".to_owned(),
                    label: t("settings.ai.skills.autoUse"),
                    hint: Some(t("settings.ai.skills.autoUseHint")),
                    checked: state.skills.auto_use,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, auto_use: bool| on_set_auto_use(s, auto_use)),
            ));
        }

        // The custom directory: the label, the input + save +
        // refresh row, the hint.
        let mut dir_block = col().key("ai-skills-dir-block").w_full().gap(6.0);
        dir_block = dir_block.child(
            text(t("settings.ai.skills.customDir").to_owned())
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_base),
        );
        let lens = Rc::clone(&self.custom_dir_edit);
        let mut dir_row = row().key("ai-skills-dir-row").w_full().gap(8.0).center_y();
        let browse = self.on_browse.clone();
        let mut path = match browse {
            Some(on_browse) => PathInput::new("ai-skills-dir", &state.custom_dir)
                .placeholder("~/.agents/skills")
                .on_browse(move |s: &mut S| on_browse(s)),
            None => PathInput::new("ai-skills-dir", &state.custom_dir)
                .placeholder("~/.agents/skills")
                .on_browse(|_s: &mut S| {}),
        };
        path = path.on_edit(move |s: &mut S| lens(s));
        dir_row = dir_row.child(path.build(self.dark).grow(1.0).min_w(0.0));

        // The save button (`variant="secondary"` → Outline):
        // persist the trimmed directory then refresh.
        let mut save = Button::new(t("settings.ai.skills.saveDir"))
            .variant(ButtonVariant::Outline);
        if let Some(on_save_directory) = &self.on_save_directory {
            let on_save_directory = Rc::clone(on_save_directory);
            save = save.on_press(move |s: &mut S| on_save_directory(s));
        }
        dir_row = dir_row.child(save.build(k).key("ai-skills-save-dir").shrink());

        // The refresh button (ghost, icon-only).
        let mut refresh = Button::new("")
            .icon(&ICON_REFRESH)
            .variant(ButtonVariant::Ghost)
            .help(t("settings.ai.skills.refresh"));
        if let Some(on_refresh) = &self.on_refresh {
            let on_refresh = Rc::clone(on_refresh);
            refresh = refresh.on_press(move |s: &mut S| on_refresh(s));
        }
        dir_row = dir_row
            .child(refresh.build(k).key("ai-skills-refresh").size(36.0, 36.0).shrink());
        dir_block = dir_block.child(dir_row);
        dir_block = dir_block.child(
            text(t("settings.ai.skills.customDirHint").to_owned())
                .font(TEXT_11)
                .color(k.c.text_muted),
        );
        panel = panel.child(dir_block);

        // The error line.
        if let Some(error) = &state.error {
            panel = panel.child(
                text(error.clone())
                    .key("ai-skills-error")
                    .font(TEXT_XS)
                    .color(k.c.danger.fg),
            );
        }

        // The selection hint (enabled, manual selection).
        if state.skills.enabled && !state.skills.auto_use {
            panel = panel.child(
                text(t("settings.ai.skills.selectHint").to_owned())
                    .key("ai-skills-selection-hint")
                    .font(TEXT_11)
                    .color(k.c.text_muted),
            );
        }

        // The listing face: disabled / loading / empty / the list.
        if !state.skills.enabled {
            panel = panel.child(
                text(t("settings.ai.skills.disabledHint").to_owned())
                    .key("ai-skills-disabled")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else if state.loading {
            panel = panel.child(
                text(t("settings.ai.skills.loading").to_owned())
                    .key("ai-skills-loading")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else if state.listing.is_empty() {
            panel = panel.child(
                text(t("settings.ai.skills.empty").to_owned())
                    .key("ai-skills-empty")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else {
            let mut list = col()
                .key("ai-skills-list")
                .w_full()
                .radius(12.0)
                .border(1.0, k.c.border_muted)
                .fill(k.c.surface_base);
            for skill in &state.listing {
                list = list.child(self.skill_row(k, layer, skill));
            }
            panel = panel.child(list);
        }

        // The footer (the default directory note + the glyph).
        panel = panel.child(
            row()
                .key("ai-skills-default-dir")
                .w_full()
                .gap(4.0)
                .center_y()
                .child(icon(&crate::icons::ICON_FOLDER_OPEN, 12.0))
                .child(
                    text(t("settings.ai.skills.defaultDir").to_owned())
                        .font(TEXT_11)
                        .color(k.c.text_muted),
                ),
        );

        panel
    }

    /// One skill row: the checkbox + name, the source chip, the
    /// optional description.
    fn skill_row(
        &self,
        k: &Kit,
        _layer: &mut Vec<El>,
        skill: &AiSkillSummary,
    ) -> El {
        let checked = self.state.skill_checked(&skill.id);
        let mut body = col()
            .key(format!("ai-skills-row-{}", skill.id))
            .w_full()
            .gap(4.0)
            .px(12.0)
            .py(8.0);

        let mut head = row()
            .key(format!("ai-skills-row-head-{}", skill.id))
            .w_full()
            .gap(8.0)
            .center_y()
            .justify_between();

        let mut label = row()
            .key(format!("ai-skills-row-label-{}", skill.id))
            .gap(8.0)
            .center_y()
            .min_w(0.0);
        let mut checkbox = Checkbox::new(format!("ai-skills-select-{}", skill.id), checked)
            .label(format!("{} {}", t("settings.ai.skills.selectSkill"), skill.name))
            .disabled(self.state.skills.auto_use);
        if !self.state.skills.auto_use {
            if let Some(on_toggle_skill) = &self.on_toggle_skill {
                let on_toggle_skill = Rc::clone(on_toggle_skill);
                let id = skill.id.clone();
                checkbox = checkbox.on_toggle(move |s: &mut S| on_toggle_skill(s, id.clone()));
            }
        }
        label = label.child(checkbox.build(k));
        label = label.child(
            text(skill.name.clone())
                .font(TEXT_SM_MEDIUM)
                .color(k.c.text_base),
        );
        head = head.child(label);

        head = head.child(
            row()
                .key(format!("ai-skills-row-source-{}", skill.id))
                .shrink()
                .px(6.0)
                .py(2.0)
                .radius(4.0)
                .fill(k.c.surface_weak) // bg-surface-alt
                .child(
                    text(format!(
                        "{} · {}",
                        match skill.source {
                            AiSkillSource::Custom => t("settings.ai.skills.sourceCustom"),
                            AiSkillSource::User => t("settings.ai.skills.sourceUser"),
                        },
                        skill.path,
                    ))
                    .font(TEXT_10)
                    .color(k.c.text_muted),
                ),
        );
        body = body.child(head);

        if !skill.description.is_empty() {
            body = body.child(
                text(skill.description.clone())
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_backfills_and_dedups() {
        assert_eq!(normalize_ai_skills_settings(None), DEFAULT_AI_SKILLS_SETTINGS);
        let raw = AiSkillsSettings {
            enabled: true,
            custom_dir: Some("/skills".to_owned()),
            auto_use: false,
            selected: vec!["a".to_owned(), "a".to_owned(), "b".to_owned()],
        };
        let normalized = normalize_ai_skills_settings(Some(&raw));
        assert_eq!(normalized.selected, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn the_refresh_refires_on_enabled_changes() {
        let mut state = SkillsSectionState::new(None);
        // Mount: due.
        assert!(state.refresh_due());
        assert!(!state.refresh_due(), "not due again until enabled changes");
        // Enabled flips: due again.
        state.skills.enabled = true;
        assert!(state.refresh_due());
        assert!(!state.refresh_due());
    }

    #[test]
    fn a_disabled_panel_clears_without_reading() {
        let mut state = SkillsSectionState::new(None);
        state.listing = vec![AiSkillSummary {
            id: "s".to_owned(),
            name: "S".to_owned(),
            description: String::new(),
            source: AiSkillSource::User,
            path: "~".to_owned(),
            version: None,
        }];
        assert!(!state.begin_refresh(), "no read while disabled");
        assert!(state.listing.is_empty());
        // Enabled arms the read.
        state.skills.enabled = true;
        assert!(state.begin_refresh());
        assert!(state.loading);
        assert!(state.error.is_none());
    }

    #[test]
    fn the_read_answers_listing_or_error() {
        let mut state = SkillsSectionState::new(None);
        state.skills.enabled = true;
        state.begin_refresh();
        state.note_listing(vec![]);
        assert!(!state.loading);
        assert!(state.listing.is_empty());
        state.begin_refresh();
        state.note_refresh_error("boom");
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(!state.loading);
    }

    #[test]
    fn persist_is_optimistic_without_revert() {
        let mut state = SkillsSectionState::new(None);
        let mut next = state.skills.clone();
        next.enabled = true;
        state.persist(next.clone());
        assert!(state.skills.enabled);
        // The refusal only raises the error (no revert).
        state.note_save_failed();
        assert!(state.skills.enabled, "no revert — the React shape");
        assert!(state.error.is_some());
    }

    #[test]
    fn selection_toggles_dedup_and_remove() {
        let mut state = SkillsSectionState::new(None);
        let next = state.toggle_selected("a", true);
        assert_eq!(next.selected, vec!["a".to_owned()]);
        // The persist lands before the second toggle (the React
        // flow: every toggle persists).
        state.persist(next);
        let next = state.toggle_selected("a", true);
        assert_eq!(next.selected, vec!["a".to_owned()], "the set dedups");
        state.persist(next);
        let next = state.toggle_selected("a", false);
        assert!(next.selected.is_empty());
    }

    #[test]
    fn auto_use_checks_every_skill() {
        let mut state = SkillsSectionState::new(None);
        assert!(!state.skill_checked("any"));
        state.skills.auto_use = true;
        assert!(state.skill_checked("any"), "auto-use owns every selection");
        state.skills.auto_use = false;
        let selected = state.toggle_selected("any", true);
        state.persist(selected);
        assert!(state.skill_checked("any"), "an explicit selection checks");
    }

    #[test]
    fn the_directory_trims_to_null() {
        let mut state = SkillsSectionState::new(None);
        state.custom_dir = TextEdit::new("  /my/skills  ".to_owned());
        assert_eq!(state.directory_settings().custom_dir.as_deref(), Some("/my/skills"));
        state.custom_dir = TextEdit::new("   ".to_owned());
        assert_eq!(state.directory_settings().custom_dir, None);
    }
}
