//! Port of
//! `packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx`
//! (T474, split with `value_presentation_section_helpers.rs`) —
//! Settings → Table data → time display (issue-32 T07, PRD
//! §3.1/§3.3, design §4/§5/§6). Mounted at the tail of the
//! TableDataSettingsSection, riding the data-browsing settings
//! page.
//!
//! ## What maps where
//!
//! - Draft + explicit save: the controls only edit the local draft
//!   ([`ValuePresentationSettingsSectionState`]); the Save button
//!   walks the normal save rail (`begin_save` → the host's
//!   `on_save` slot → `note_save_ok` / `note_save_error`). An
//!   illegal configuration (the validator, an invalid IANA name)
//!   is intercepted before the save with the reason in place; a
//!   failed save keeps the old configuration live (the draft
//!   survives, never shown as saved).
//! - The global default only governs native time values: the
//!   draft's interpretation stays native (the save never writes
//!   unix/text); the preview's time-source choice is preview-only.
//! - The preview runs the same road as the runtime (the helpers'
//!   [`compute_timestamp_preview`](crate::views::settings::
//!   value_presentation_section_helpers::compute_timestamp_preview));
//!   a missing contribution only degrades the preview (the
//!   configuration stays savable).
//! - The editable rule list (T08): new / edit / delete / enable /
//!   reorder inside the collapsible list; the rule drafts save
//!   with the panel draft (the one save rail). The editor itself
//!   is T469's, mounted inline over [`Self::editor`].
//! - T11 cross-window sync: the save is updater-shaped — only the
//!   paths the user actually changed (the format/switch, the rule
//!   list) enter the write; everything else comes from the fresh
//!   authoritative read ([`SavePlan`]'s two dirty flags). An
//!   external save while edits are pending only prompts
//!   ([`Self::note_stored`]'s per-path unsaved ruling); the
//!   "reload saved settings" button ([`Self::reset_to_stored`])
//!   is what overwrites the draft.
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`value-presentation-
//! settings`, `-external-update`, `-reload-saved`,
//! `-contribution-missing`, `-enable`, `-format`, `-timezone`,
//! `-iana-name`, `-iana-error`, `-preview-source`, `-preview`,
//! `-preview-unix-badge`, `-preview-result`, `-rules-toggle`,
//! `-rules-empty`, `-rules-list`, `-rule-{i}`, the per-rule
//! `-edit/-delete/-up/-down/-toggle-{i}` actions, `-rule-add`,
//! `-error`, `-save`, `-reset`, `-saved`).
//!
//! Known deltas: the `border-t` divider is a 1pt filled rule (the
//! kit border is four-sided); the rule list's small link-styled
//! `<button>`s are ghost kit buttons; the rules toggle's chevron
//! folds into its label.

// No production consumer yet (the table-data section's
// value_presentation_section slot mounts it once a host wires it);
// this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::support::value_presentation::contracts::{
    ConfigValidationResult, PresentationEnvironment, ValuePresentationContribution,
};
use crate::support::value_presentation::settings::{
    parse_stored_value_presentation_settings, StoredValuePresentationSettings,
};
use crate::support::value_presentation::timestamp_config::{
    validate_timestamp_options, FormatSerde, TimestampOptions, TimestampOptionsValidation,
};
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::temporal_math::is_valid_time_zone_name;
use crate::support::value_presentation::timestamp_format::{is_valid_iana_time_zone_name, TimeZoneSpec};
use crate::support::value_presentation::value_presentation_rules::{
    describe_rule_match, validate_value_presentation_rule, MAX_PATTERN_LENGTH,
    MAX_VALUE_PRESENTATION_RULES, ValuePresentationRule, ValuePresentationRuleValidation,
};
use crate::support::value_presentation::value_presentation_save::{
    with_timestamp_renderer, with_value_presentation_rules,
};
use crate::ui::select::Select;
use crate::views::settings::settings_ui::{setting_row, toggle_row, ToggleRow};
use crate::views::settings::value_presentation_rule_editor::{
    InterpretationChoice, TimeZoneKindChoice, ValuePresentationRuleEditor,
    ValuePresentationRuleEditorState,
};
use crate::views::settings::value_presentation_section_helpers::{
    build_preview_options, compute_timestamp_preview, draft_from_stored,
    format_preview_sample, preview_sample_for, TimestampDisplayDraft, TimestampPreviewResult,
    DEFAULT_TIMESTAMP_OPTIONS,
};

/// `text-sm font-semibold` — the section title.
const TEXT_SM_SEMIBOLD: TextStyle = TextStyle::new(14.0, 600.0);
/// `text-sm` — the body copy.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
/// `text-xs` — the descriptions and the mono rule match.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-sm font-medium` — a rule row's name.
const TEXT_SM_MEDIUM: TextStyle = TextStyle::new(14.0, 500.0);

/// Which of the section's selects is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionSelect {
    Format,
    TimeZone,
    PreviewSource,
}

/// The editing target: `'new'` or `{ index }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditingRule {
    New,
    Index(usize),
}

/// The save plan — the updater-shaped write the host applies
/// against the fresh authoritative read (T11: only the dirty
/// paths).
#[derive(Clone, Debug, PartialEq)]
pub struct SavePlan {
    /// Write the global switch + options (the `withTimestampRenderer`
    /// path).
    pub format_dirty: bool,
    /// Write the rule list (the `withValuePresentationRules` path).
    pub rules_dirty: bool,
    pub enabled: bool,
    pub options: TimestampOptions,
    pub rules: Vec<ValuePresentationRule>,
}

impl SavePlan {
    /// The updater body: apply the dirty paths over `fresh`.
    pub fn apply(&self, fresh: &StoredValuePresentationSettings) -> StoredValuePresentationSettings {
        let mut next = fresh.clone();
        if self.format_dirty {
            let options_value =
                serde_json::to_value(&self.options).unwrap_or_else(|_| serde_json::Value::Null);
            next = with_timestamp_renderer(fresh, self.enabled, &options_value);
        }
        if self.rules_dirty {
            next = with_value_presentation_rules(&next, &self.rules);
        }
        next
    }
}

/// The section's state.
#[derive(Clone, PartialEq)]
pub struct ValuePresentationSettingsSectionState {
    /// The store's last reported value (the is-dirty reference).
    pub stored: StoredValuePresentationSettings,
    /// `appliedStoredJson` — the baseline the draft last synced
    /// from (the per-path dirty checks read it, never the live
    /// store: the live value may be another window's fresh write).
    applied_stored_json: String,
    pub draft: TimestampDisplayDraft,
    pub iana_name: TextEdit,
    /// T11: another window saved while edits were pending — prompt
    /// only, never overwrite.
    pub external_update: bool,
    pub error: Option<String>,
    pub saved: bool,
    pub saving: bool,
    pub preview_interpretation: InterpretationChoice,
    pub rules_open: bool,
    /// The rule drafts (T08): saved with the panel draft.
    pub rule_drafts: Vec<ValuePresentationRule>,
    pub editing_rule: Option<EditingRule>,
    /// The inline editor's state (T469), re-armed per edit.
    pub editor: ValuePresentationRuleEditorState,
    pub open_select: Option<SectionSelect>,
}

fn stored_to_json(stored: &StoredValuePresentationSettings) -> String {
    serde_json::to_string(stored).unwrap_or_else(|_| "{}".to_owned())
}

fn parse_stored_json(json: &str) -> StoredValuePresentationSettings {
    serde_json::from_str(json).unwrap_or_else(|_| {
        parse_stored_value_presentation_settings(&serde_json::Value::Null)
    })
}

fn iana_text_of(draft: &TimestampDisplayDraft) -> String {
    match &draft.options.display_time_zone {
        TimeZoneSpec::Iana { name } => name.clone(),
        _ => String::new(),
    }
}

impl ValuePresentationSettingsSectionState {
    /// The mount: the store's first read initializes the draft.
    pub fn new(stored: &StoredValuePresentationSettings) -> Self {
        let draft = draft_from_stored(stored);
        let iana = iana_text_of(&draft);
        Self {
            stored: stored.clone(),
            applied_stored_json: stored_to_json(stored),
            draft,
            iana_name: TextEdit::new(iana),
            external_update: false,
            error: None,
            saved: false,
            saving: false,
            preview_interpretation: InterpretationChoice::Native,
            rules_open: false,
            rule_drafts: stored.rules.clone().unwrap_or_default(),
            editing_rule: None,
            editor: ValuePresentationRuleEditorState::default(),
            open_select: None,
        }
    }

    /// The store's answer (the T11 effect): a change with pending
    /// edits on a path only prompts; a change the draft already
    /// echoes rebuilds it and advances the baseline.
    pub fn note_stored(&mut self, stored: &StoredValuePresentationSettings) {
        self.stored = stored.clone();
        let json = stored_to_json(stored);
        if json == self.applied_stored_json {
            return;
        }
        let new_draft = draft_from_stored(stored);
        let new_rules = stored.rules.clone().unwrap_or_default();
        let applied = parse_stored_json(&self.applied_stored_json);
        let applied_draft = draft_from_stored(&applied);
        let applied_rules = applied.rules.unwrap_or_default();
        let options = self.options_to_save();
        // A path is unsaved when it differs from BOTH the last
        // synced value and the new saved value (own save echoes and
        // pure external updates are not unsaved edits).
        let format_unsaved = (self.draft.enabled != applied_draft.enabled
            || options != applied_draft.options)
            && (self.draft.enabled != new_draft.enabled || options != new_draft.options);
        let rules_unsaved = self.rule_drafts != applied_rules && self.rule_drafts != new_rules;
        if format_unsaved || rules_unsaved {
            self.external_update = true;
            return;
        }
        self.applied_stored_json = json;
        self.external_update = false;
        self.draft = new_draft;
        self.iana_name.text = iana_text_of(&self.draft);
        self.rule_drafts = new_rules;
        self.editing_rule = None;
        self.error = None;
        self.saved = false;
    }

    /// `optionsToSave`: the draft's options with the IANA name read
    /// live from the input (trimmed).
    pub fn options_to_save(&self) -> TimestampOptions {
        let mut options = self.draft.options.clone();
        if matches!(options.display_time_zone, TimeZoneSpec::Iana { .. }) {
            options.display_time_zone = TimeZoneSpec::Iana {
                name: self.iana_name.text.trim().to_owned(),
            };
        }
        options
    }

    fn iana_trimmed(&self) -> &str {
        self.iana_name.text.trim()
    }

    /// The IANA name is syntactically invalid (the format error).
    pub fn iana_format_invalid(&self) -> bool {
        let name = self.iana_trimmed();
        matches!(self.draft.options.display_time_zone, TimeZoneSpec::Iana { .. })
            && !name.is_empty()
            && !is_valid_iana_time_zone_name(name)
    }

    /// The IANA name is well-formed but unknown (the zone error).
    pub fn iana_unknown(&self) -> bool {
        let name = self.iana_trimmed();
        matches!(self.draft.options.display_time_zone, TimeZoneSpec::Iana { .. })
            && !name.is_empty()
            && is_valid_iana_time_zone_name(name)
            && !is_valid_time_zone_name(name)
    }

    fn saved_draft(&self) -> TimestampDisplayDraft {
        draft_from_stored(&self.stored)
    }

    fn saved_rules(&self) -> Vec<ValuePresentationRule> {
        self.stored.rules.clone().unwrap_or_default()
    }

    /// `isDirty` — against the CURRENT saved value.
    pub fn is_dirty(&self) -> bool {
        let saved = self.saved_draft();
        self.draft.enabled != saved.enabled
            || self.options_to_save() != saved.options
            || self.rule_drafts != self.saved_rules()
    }

    // The edits (each `setSaved(false)` where React does).

    pub fn set_enabled(&mut self, enabled: bool) {
        self.draft.enabled = enabled;
        self.saved = false;
    }

    pub fn set_format(&mut self, format: FormatSerde) {
        self.draft.options.format = format;
        self.saved = false;
    }

    pub fn set_time_zone_kind(&mut self, kind: TimeZoneKindChoice) {
        self.draft.options.display_time_zone = match kind {
            TimeZoneKindChoice::Iana => TimeZoneSpec::Iana {
                name: self.iana_trimmed().to_owned(),
            },
            TimeZoneKindChoice::Local => TimeZoneSpec::Local,
            TimeZoneKindChoice::Utc => TimeZoneSpec::Utc,
        };
        self.saved = false;
    }

    /// The IANA input's edit (the `setSaved(false)` rides the kit's
    /// change event).
    pub fn iana_edit_landed(&mut self) {
        self.saved = false;
    }

    pub fn set_preview_interpretation(&mut self, interpretation: InterpretationChoice) {
        self.preview_interpretation = interpretation;
    }

    pub fn toggle_rules_open(&mut self) {
        self.rules_open = !self.rules_open;
    }

    pub fn toggle_select(&mut self, which: SectionSelect) {
        self.open_select = if self.open_select == Some(which) {
            None
        } else {
            Some(which)
        };
    }

    pub fn close_select(&mut self) {
        self.open_select = None;
    }

    // The rule list (T08).

    /// `setEditingRule`.
    pub fn set_editing_rule(&mut self, editing: Option<EditingRule>) {
        if let Some(EditingRule::Index(index)) = editing {
            let initial = self.rule_drafts.get(index);
            self.editor = ValuePresentationRuleEditorState::new(initial);
        } else {
            self.editor = ValuePresentationRuleEditorState::new(None);
        }
        self.editing_rule = editing;
    }

    /// `handleRuleSave`: append (new) or replace at the index.
    pub fn save_rule_draft(&mut self, rule: ValuePresentationRule) {
        match self.editing_rule {
            Some(EditingRule::New) => self.rule_drafts.push(rule),
            Some(EditingRule::Index(index)) => {
                if let Some(entry) = self.rule_drafts.get_mut(index) {
                    *entry = rule;
                }
            }
            None => {}
        }
        self.editing_rule = None;
        self.saved = false;
    }

    /// The delete (React's inline filter — no `setSaved(false)`,
    /// verbatim).
    pub fn delete_rule(&mut self, index: usize) {
        if index < self.rule_drafts.len() {
            self.rule_drafts.remove(index);
        }
    }

    /// The enable toggle (verbatim: no `setSaved(false)`).
    pub fn toggle_rule_enabled(&mut self, index: usize) {
        if let Some(rule) = self.rule_drafts.get_mut(index) {
            rule.enabled = !rule.enabled;
        }
    }

    /// `moveRule(index, direction)` — the bounds-checked swap.
    pub fn move_rule(&mut self, index: usize, direction: i8) {
        let target = index as i64 + direction as i64;
        if target < 0 || target >= self.rule_drafts.len() as i64 {
            return;
        }
        let target = target as usize;
        self.rule_drafts.swap(index, target);
        self.saved = false;
    }

    /// The add button's gate: no edit in flight and under the cap.
    pub fn rules_add_gate(&self) -> bool {
        self.editing_rule.is_none() && self.rule_drafts.len() < MAX_VALUE_PRESENTATION_RULES
    }

    /// The editor's `otherNames` (the list minus the edited row).
    pub fn editing_other_names(&self) -> Vec<String> {
        self.rule_drafts
            .iter()
            .enumerate()
            .filter(|(index, _)| match self.editing_rule {
                Some(EditingRule::Index(editing)) => *index != editing,
                _ => true,
            })
            .map(|(_, rule)| rule.name.clone())
            .collect()
    }

    // The save.

    /// `validateDraftForSave`: the IANA gates, the options
    /// validator, the contribution's own validateConfig (when
    /// present).
    fn validate_draft_for_save(
        &mut self,
        contribution: Option<&Rc<dyn ValuePresentationContribution>>,
    ) -> Option<TimestampOptions> {
        if matches!(self.draft.options.display_time_zone, TimeZoneSpec::Iana { .. }) {
            let name = self.iana_trimmed().to_owned();
            if name.is_empty() || !is_valid_iana_time_zone_name(&name) {
                self.error = Some(t("settings.valuePresentation.ianaInvalid").to_owned());
                return None;
            }
            if !is_valid_time_zone_name(&name) {
                self.error = Some(t("settings.valuePresentation.ianaUnknown").to_owned());
                return None;
            }
        }
        let options = self.options_to_save();
        let options_value =
            serde_json::to_value(&options).unwrap_or_else(|_| serde_json::Value::Null);
        if let TimestampOptionsValidation::Err(problems) = validate_timestamp_options(&options_value)
        {
            let joined = problems
                .iter()
                .map(|problem| format!("{}: {}", problem.path, problem.message))
                .collect::<Vec<_>>()
                .join("; ");
            self.error = Some(format!(
                "{}: {}",
                t("settings.valuePresentation.invalidConfig"),
                joined
            ));
            return None;
        }
        if let Some(contribution) = contribution {
            if let ConfigValidationResult::Err(problems) = contribution.validate_config(&options_value)
            {
                let joined = problems
                    .iter()
                    .map(|problem| format!("{}: {}", problem.path, problem.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                self.error = Some(format!(
                    "{}: {}",
                    t("settings.valuePresentation.invalidConfig"),
                    joined
                ));
                return None;
            }
        }
        Some(options)
    }

    /// `handleSave`'s decision half: the three gates, the defensive
    /// rule re-validation, then the plan. `None` = refused (the
    /// error is set).
    pub fn begin_save(
        &mut self,
        contribution: Option<&Rc<dyn ValuePresentationContribution>>,
    ) -> Option<SavePlan> {
        let options = self.validate_draft_for_save(contribution)?;
        // The rule drafts re-validate once more (the editor only
        // hands back validated rules; this is the defensive check).
        for rule in &self.rule_drafts {
            let rule_value = serde_json::to_value(rule).unwrap_or_else(|_| serde_json::Value::Null);
            if let ValuePresentationRuleValidation::Err(problems) =
                validate_value_presentation_rule(&rule_value)
            {
                let joined = problems
                    .iter()
                    .map(|problem| format!("{}: {}", problem.path, problem.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                self.error = Some(format!(
                    "{}: {}",
                    t("settings.valuePresentation.invalidConfig"),
                    joined
                ));
                return None;
            }
        }
        // The per-path dirty checks (against the synced baseline —
        // see `applied_stored_json`'s note).
        let applied = parse_stored_json(&self.applied_stored_json);
        let applied_draft = draft_from_stored(&applied);
        let applied_rules = applied.rules.unwrap_or_default();
        let format_dirty = self.draft.enabled != applied_draft.enabled
            || self.options_to_save() != applied_draft.options;
        let rules_dirty = self.rule_drafts != applied_rules;
        self.saving = true;
        self.error = None;
        Some(SavePlan {
            format_dirty,
            rules_dirty,
            enabled: self.draft.enabled,
            options,
            rules: self.rule_drafts.clone(),
        })
    }

    /// The save's success (the store's answer lands separately
    /// through `note_stored`).
    pub fn note_save_ok(&mut self) {
        self.saving = false;
        self.saved = true;
        self.external_update = false;
    }

    /// The save's failure: the old configuration stays live, the
    /// draft survives, never shown as saved.
    pub fn note_save_error(&mut self) {
        self.saving = false;
        self.saved = false;
        self.error = Some(t("settings.valuePresentation.saveFailed").to_owned());
    }

    /// `handleReset` — also the external-update prompt's "reload
    /// saved settings" (the same operation).
    pub fn reset_to_stored(&mut self) {
        let next = draft_from_stored(&self.stored);
        self.iana_name.text = iana_text_of(&next);
        self.draft = next;
        self.rule_drafts = self.stored.rules.clone().unwrap_or_default();
        self.editing_rule = None;
        self.error = None;
        self.saved = false;
        self.external_update = false;
    }

    /// The preview over the current options-to-save.
    pub fn preview(
        &self,
        contribution: Option<Rc<dyn ValuePresentationContribution>>,
        environment: PresentationEnvironment<'_>,
    ) -> TimestampPreviewResult {
        compute_timestamp_preview(
            contribution.as_ref(),
            &build_preview_options(&self.options_to_save(), self.preview_interpretation),
            self.preview_interpretation,
            environment,
        )
    }

    /// The saved marker's face: `saved && !isDirty`.
    pub fn shows_saved(&self) -> bool {
        self.saved && !self.is_dirty()
    }
}

impl std::fmt::Debug for ValuePresentationSettingsSectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValuePresentationSettingsSectionState")
            .field("draft", &self.draft)
            .field("external_update", &self.external_update)
            .field("error", &self.error)
            .field("saved", &self.saved)
            .field("saving", &self.saving)
            .field("preview_interpretation", &self.preview_interpretation)
            .field("rules_open", &self.rules_open)
            .field("rule_drafts", &self.rule_drafts)
            .field("editing_rule", &self.editing_rule)
            .field("open_select", &self.open_select)
            .finish_non_exhaustive()
    }
}

/// The host lens adapter for the IANA input (a top-level fn shape:
/// the HRTB needs the named form).
fn iana_edit_of<S: 'static>(
    section_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationSettingsSectionState>,
) -> Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> {
    let section_edit = Rc::clone(section_edit);
    Rc::new(move |s: &mut S| &mut section_edit(s).iana_name)
}

/// The host lens adapter for the inline editor's state.
fn editor_of<S: 'static>(
    section_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationSettingsSectionState>,
) -> Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState> {
    let section_edit = Rc::clone(section_edit);
    Rc::new(move |s: &mut S| &mut section_edit(s).editor)
}

/// One typed select over its vocabulary (the T434 `typed_select`
/// shape).
fn choice_select<S: 'static>(
    section_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationSettingsSectionState>,
    open: bool,
    which: SectionSelect,
    key: &str,
    entries: &[(String, String)],
    current_wire: &str,
    width: f32,
    apply: Rc<dyn Fn(&mut ValuePresentationSettingsSectionState, &str)>,
) -> Select {
    let labels: Vec<String> = entries.iter().map(|(_, label)| label.clone()).collect();
    let values: Vec<String> = entries.iter().map(|(value, _)| value.clone()).collect();
    let current_label = entries
        .iter()
        .find(|(value, _)| value == current_wire)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| current_wire.to_owned());
    let mut select = Select::new(key, &labels)
        .values(&values)
        .current(Some(current_label.as_str()))
        .width(width)
        .open(open);
    {
        let section_edit = Rc::clone(section_edit);
        select = select.on_toggle(move |s: &mut S| section_edit(s).toggle_select(which));
    }
    {
        let section_edit = Rc::clone(section_edit);
        select = select.on_close(move |s: &mut S| section_edit(s).close_select());
    }
    {
        let section_edit = Rc::clone(section_edit);
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
            let section = section_edit(s);
            apply(section, value);
            section.close_select();
        });
    }
    select
}

/// `ValuePresentationSettingsSection()`: the panel over the state,
/// the state lens and the host's save slot.
pub struct ValuePresentationSettingsSection<'a, S: 'static> {
    state: &'a ValuePresentationSettingsSectionState,
    dark: bool,
    section_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationSettingsSectionState>,
    /// The registered contribution (absent → the degraded preview).
    contribution: Option<Rc<dyn ValuePresentationContribution>>,
    /// The environment snapshot (the app locale + the machine
    /// zone).
    environment: PresentationEnvironment<'a>,
    /// The save rail: the host applies the plan against the fresh
    /// authoritative read and answers `note_save_ok` /
    /// `note_save_error`.
    on_save: Option<Rc<dyn Fn(&mut S, SavePlan)>>,
}

impl<'a, S: 'static> ValuePresentationSettingsSection<'a, S> {
    pub fn new(
        state: &'a ValuePresentationSettingsSectionState,
        dark: bool,
        section_edit: impl for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationSettingsSectionState + 'static,
        contribution: Option<Rc<dyn ValuePresentationContribution>>,
        environment: PresentationEnvironment<'a>,
    ) -> Self {
        Self {
            state,
            dark,
            section_edit: Rc::new(section_edit),
            contribution,
            environment,
            on_save: None,
        }
    }

    /// `updateValuePresentationSettings(updater)` — the host runs
    /// the rail.
    pub fn on_save(mut self, f: impl Fn(&mut S, SavePlan) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }

    /// The section (the border-top tail fragment).
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let section_edit = &self.section_edit;
        let dark = self.dark;

        // `space-y-4 border-t border-edge pt-4` — the divider is a
        // 1pt filled rule (the kit border is four-sided).
        let mut section = col()
            .key("value-presentation-settings")
            .w_full()
            .gap(16.0) // space-y-4
            .child(row().key("value-presentation-divider").w_full().h(1.0).fill(k.c.border_base))
            .pt(16.0);

        // The header.
        section = section.child(
            col()
                .key("value-presentation-header")
                .w_full()
                .gap(4.0)
                .child(
                    text(t("settings.valuePresentation.title").to_owned())
                        .font(TEXT_SM_SEMIBOLD)
                        .color(k.c.text_base),
                )
                .child(
                    text(t("settings.valuePresentation.description").to_owned())
                        .font(TEXT_XS)
                        .color(k.c.text_muted),
                ),
        );

        // The external-update prompt (T11).
        if state.external_update {
            let mut reload = Button::new(t("settings.valuePresentation.reloadSaved"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            {
                let section_edit = Rc::clone(section_edit);
                reload = reload.on_press(move |s: &mut S| section_edit(s).reset_to_stored());
            }
            section = section.child(
                col()
                    .key("value-presentation-external-update")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.surface_weak) // bg-surface-alt
                    .gap(8.0)
                    .child(
                        text(t("settings.valuePresentation.externalUpdate").to_owned())
                            .font(TEXT_SM)
                            .color(k.c.text_muted),
                    )
                    .child(reload.build(k).key("value-presentation-reload-saved")),
            );
        }

        // The missing-contribution notice (the preview degrades, the
        // configuration stays savable).
        if self.contribution.is_none() {
            section = section.child(
                text(t("settings.valuePresentation.contributionMissing").to_owned())
                    .key("value-presentation-contribution-missing")
                    .font(TEXT_SM)
                    .color(k.c.text_muted)
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.surface_weak),
            );
        }

        // The enable switch.
        {
            let on_change: Rc<dyn Fn(&mut S, bool)> = {
                let section_edit = Rc::clone(section_edit);
                Rc::new(move |s: &mut S, next: bool| section_edit(s).set_enabled(next))
            };
            section = section.child(toggle_row::<S>(
                k,
                layer,
                ToggleRow {
                    key: "value-presentation-enable".to_owned(),
                    label: t("settings.valuePresentation.enable"),
                    hint: Some(t("settings.valuePresentation.enableHint")),
                    checked: state.draft.enabled,
                    disabled: false,
                },
                on_change,
            ));
        }

        // The format select.
        let format_entries: Vec<(String, String)> =
            [FormatSerde::Iso, FormatSerde::IsoTz, FormatSerde::Locale]
                .iter()
                .map(|format| {
                    (
                        crate::views::settings::value_presentation_rule_editor::format_as_wire(
                            *format,
                        )
                        .to_owned(),
                        t(match format {
                            FormatSerde::Iso => "settings.valuePresentation.format.iso",
                            FormatSerde::IsoTz => "settings.valuePresentation.format.iso-tz",
                            FormatSerde::Locale => "settings.valuePresentation.format.locale",
                        })
                        .to_owned(),
                    )
                })
                .collect();
        let format_el = choice_select::<S>(
            section_edit,
            state.open_select == Some(SectionSelect::Format),
            SectionSelect::Format,
            "value-presentation-format",
            &format_entries,
            crate::views::settings::value_presentation_rule_editor::format_as_wire(
                state.draft.options.format,
            ),
            140.0,
            Rc::new(|section, value| {
                if let Some(format) =
                    crate::views::settings::value_presentation_rule_editor::format_from_wire(value)
                {
                    section.set_format(format);
                }
            }),
        )
        .build(k, layer);
        section = section.child(setting_row(
            k,
            layer,
            "value-presentation-format-row".to_owned(),
            t("settings.valuePresentation.format"),
            None,
            format_el,
        ));

        // The display time zone select.
        let tz_entries: Vec<(String, String)> = [
            TimeZoneKindChoice::Local,
            TimeZoneKindChoice::Utc,
            TimeZoneKindChoice::Iana,
        ]
        .iter()
        .map(|kind| {
            (
                kind.as_wire().to_owned(),
                t(match kind {
                    TimeZoneKindChoice::Local => "settings.valuePresentation.timeZone.local",
                    TimeZoneKindChoice::Utc => "settings.valuePresentation.timeZone.utc",
                    TimeZoneKindChoice::Iana => "settings.valuePresentation.timeZone.iana",
                })
                .to_owned(),
            )
        })
        .collect();
        let tz_wire = match &state.draft.options.display_time_zone {
            TimeZoneSpec::Local => "local",
            TimeZoneSpec::Utc => "utc",
            TimeZoneSpec::Iana { .. } => "iana",
        };
        let tz_el = choice_select::<S>(
            section_edit,
            state.open_select == Some(SectionSelect::TimeZone),
            SectionSelect::TimeZone,
            "value-presentation-timezone",
            &tz_entries,
            tz_wire,
            140.0,
            Rc::new(|section, value| {
                if let Some(kind) = TimeZoneKindChoice::from_wire(value) {
                    section.set_time_zone_kind(kind);
                }
            }),
        )
        .build(k, layer);
        section = section.child(setting_row(
            k,
            layer,
            "value-presentation-timezone-row".to_owned(),
            t("settings.valuePresentation.displayTimeZone"),
            None,
            tz_el,
        ));

        // The IANA name (the iana arm only).
        if matches!(state.draft.options.display_time_zone, TimeZoneSpec::Iana { .. }) {
            let iana_edit = iana_edit_of::<S>(section_edit);
            let field = crate::ui::field::TextField::new(
                "value-presentation-iana-name",
                &state.iana_name,
            )
            .height(36.0)
            .placeholder("Asia/Shanghai")
            .on_edit::<S>({
                let lens = Rc::clone(&iana_edit);
                move |s: &mut S| lens(s)
            })
            .on_change::<S>({
                let section_edit = Rc::clone(section_edit);
                move |s: &mut S, _tx: &mut rune::Tx| section_edit(s).iana_edit_landed()
            })
            .build(dark);
            let mut block = col().key("value-presentation-iana-block").w_full().gap(4.0);
            block = block.child(field);
            if state.iana_format_invalid() {
                block = block.child(
                    text(t("settings.valuePresentation.ianaInvalid").to_owned())
                        .key("value-presentation-iana-error")
                        .font(TEXT_XS)
                        .color(k.c.danger.fg),
                );
            }
            if state.iana_unknown() {
                block = block.child(
                    text(t("settings.valuePresentation.ianaUnknown").to_owned())
                        .key("value-presentation-iana-error")
                        .font(TEXT_XS)
                        .color(k.c.danger.fg),
                );
            }
            section = section.child(setting_row(
                k,
                layer,
                "value-presentation-iana-row".to_owned(),
                t("settings.valuePresentation.ianaName"),
                None,
                block,
            ));
        }

        // The preview block.
        let source_entries: Vec<(String, String)> = [
            InterpretationChoice::Native,
            InterpretationChoice::UnixSeconds,
            InterpretationChoice::UnixMilliseconds,
            InterpretationChoice::Text,
        ]
        .iter()
        .map(|choice| {
            (
                choice.as_wire().to_owned(),
                t(match choice {
                    InterpretationChoice::Native => {
                        "settings.valuePresentation.preview.source.native"
                    }
                    InterpretationChoice::UnixSeconds => {
                        "settings.valuePresentation.preview.source.unix-seconds"
                    }
                    InterpretationChoice::UnixMilliseconds => {
                        "settings.valuePresentation.preview.source.unix-milliseconds"
                    }
                    InterpretationChoice::Text => "settings.valuePresentation.preview.source.text",
                })
                .to_owned(),
            )
        })
        .collect();
        let source_el = choice_select::<S>(
            section_edit,
            state.open_select == Some(SectionSelect::PreviewSource),
            SectionSelect::PreviewSource,
            "value-presentation-preview-source",
            &source_entries,
            state.preview_interpretation.as_wire(),
            180.0,
            Rc::new(|section, value| {
                if let Some(choice) = InterpretationChoice::from_wire(value) {
                    section.set_preview_interpretation(choice);
                }
            }),
        )
        .build(k, layer);
        let preview = state.preview(self.contribution.clone(), self.environment);
        let mut preview_head = row()
            .key("value-presentation-preview-sample")
            .w_full()
            .gap(8.0)
            .center_y()
            .child(
                text(t("settings.valuePresentation.preview.original").to_owned())
                    .font(TEXT_SM)
                    .color(k.c.text_muted),
            )
            .child(
                text(format_preview_sample(match &preview {
                    TimestampPreviewResult::Unavailable { sample }
                    | TimestampPreviewResult::Invalid { sample, .. }
                    | TimestampPreviewResult::Text { sample, .. }
                    | TimestampPreviewResult::Unchanged { sample, .. } => sample,
                }))
                .font(k.ty.mono)
                .color(k.c.text_base),
            );
        if state.preview_interpretation == InterpretationChoice::UnixSeconds {
            preview_head = preview_head.child(
                text(t("settings.valuePresentation.preview.unixSeconds").to_owned())
                    .key("value-presentation-preview-unix-badge")
                    .font(TEXT_XS)
                    .color(k.c.text_accent),
            );
        }
        if state.preview_interpretation == InterpretationChoice::UnixMilliseconds {
            preview_head = preview_head.child(
                text(t("settings.valuePresentation.preview.unixMilliseconds").to_owned())
                    .key("value-presentation-preview-unix-badge")
                    .font(TEXT_XS)
                    .color(k.c.text_accent),
            );
        }
        let result_line = match &preview {
            TimestampPreviewResult::Text { text: hit, .. } => {
                text(hit.clone()).font(k.ty.mono).color(k.c.text_base)
            }
            TimestampPreviewResult::Unchanged { reason, .. } => {
                let mut line = t("settings.valuePresentation.preview.unchanged").to_owned();
                if let Some(reason) = reason {
                    line.push_str(": ");
                    line.push_str(reason);
                }
                text(line).font(TEXT_SM).color(k.c.text_muted)
            }
            TimestampPreviewResult::Invalid { problems, .. } => {
                let joined = problems
                    .iter()
                    .map(|(path, message)| format!("{path}: {message}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                text(format!(
                    "{}: {joined}",
                    t("settings.valuePresentation.preview.invalid")
                ))
                .font(TEXT_SM)
                .color(k.c.danger.fg)
            }
            TimestampPreviewResult::Unavailable { .. } => {
                text(t("settings.valuePresentation.preview.unavailable").to_owned())
                    .font(TEXT_SM)
                    .color(k.c.text_muted)
            }
        };
        let preview_box = col()
            .key("value-presentation-preview")
            .w_full()
            .gap(6.0)
            .padding(12.0)
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak) // bg-surface-alt
            .child(preview_head)
            .child(result_line.key("value-presentation-preview-result"));
        section = section.child(
            col()
                .key("value-presentation-preview-block")
                .w_full()
                .gap(8.0)
                .child(setting_row(
                    k,
                    layer,
                    "value-presentation-preview-source-row".to_owned(),
                    t("settings.valuePresentation.preview.source"),
                    None,
                    source_el,
                ))
                .child(preview_box),
        );

        // The rules list (T08).
        let mut rules_card = col()
            .key("value-presentation-rules")
            .w_full()
            .radius(6.0)
            .border(1.0, k.c.border_base);
        {
            let chevron = if state.rules_open { "▾" } else { "▸" };
            let mut toggle = Button::new(format!(
                "{} ({}) {chevron}",
                t("settings.valuePresentation.rules.title"),
                state.rule_drafts.len()
            ))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
            {
                let section_edit = Rc::clone(section_edit);
                toggle = toggle.on_press(move |s: &mut S| section_edit(s).toggle_rules_open());
            }
            rules_card = rules_card.child(
                row()
                    .key("value-presentation-rules-toggle")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .child(toggle.build(k)),
            );
        }
        if state.rules_open {
            let mut body = col().key("value-presentation-rules-body").w_full().gap(8.0);
            if state.rule_drafts.is_empty() {
                body = body.child(
                    text(t("settings.valuePresentation.rules.empty").to_owned())
                        .key("value-presentation-rules-empty")
                        .font(TEXT_SM)
                        .color(k.c.text_muted),
                );
            } else {
                let mut list = col().key("value-presentation-rules-list").w_full().gap(8.0);
                for (index, rule) in state.rule_drafts.iter().enumerate() {
                    let mut item = col()
                        .key(format!("value-presentation-rule-{index}"))
                        .w_full()
                        .gap(6.0)
                        .px(12.0)
                        .py(8.0)
                        .radius(6.0)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.surface_weak);
                    item = item.child(
                        row()
                            .key(format!("value-presentation-rule-{index}-head"))
                            .w_full()
                            .justify_between()
                            .center_y()
                            .child(
                                text(rule.name.clone())
                                    .font(TEXT_SM_MEDIUM)
                                    .color(k.c.text_base),
                            )
                            .child(
                                text(
                                    if rule.enabled {
                                        t("settings.valuePresentation.rules.enabled")
                                    } else {
                                        t("settings.valuePresentation.rules.disabled")
                                    }
                                    .to_owned(),
                                )
                                .font(TEXT_XS)
                                .color(k.c.text_muted),
                            ),
                    );
                    item = item.child(
                        text(describe_rule_match(&rule.r#match))
                            .font(k.ty.mono)
                            .color(k.c.text_muted),
                    );
                    let mut actions = row()
                        .key(format!("value-presentation-rule-{index}-actions"))
                        .w_full()
                        .gap(4.0)
                        .center_y();
                    let editing = state.editing_rule.is_some();
                    for (label_key, action_key, danger, action) in [
                        (
                            "settings.valuePresentation.rules.edit",
                            format!("value-presentation-rule-edit-{index}"),
                            false,
                            RuleAction::Edit(index),
                        ),
                        (
                            "settings.valuePresentation.rules.delete",
                            format!("value-presentation-rule-delete-{index}"),
                            true,
                            RuleAction::Delete(index),
                        ),
                        (
                            "settings.valuePresentation.rules.moveUp",
                            format!("value-presentation-rule-up-{index}"),
                            false,
                            RuleAction::Up(index),
                        ),
                        (
                            "settings.valuePresentation.rules.moveDown",
                            format!("value-presentation-rule-down-{index}"),
                            false,
                            RuleAction::Down(index),
                        ),
                        (
                            if rule.enabled {
                                "settings.valuePresentation.rules.disable"
                            } else {
                                "settings.valuePresentation.rules.enable"
                            },
                            format!("value-presentation-rule-toggle-{index}"),
                            false,
                            RuleAction::Toggle(index),
                        ),
                    ] {
                        let mut button = Button::new(t(label_key))
                            .variant(ButtonVariant::Ghost)
                            .control_size(ControlSize::Small)
                            .disabled(editing || action.disabled_face(index, state.rule_drafts.len()));
                        if danger {
                            button = button.variant(ButtonVariant::Danger);
                        }
                        {
                            let section_edit = Rc::clone(section_edit);
                            button = button.on_press(move |s: &mut S| {
                                action.apply(section_edit(s));
                            });
                        }
                        actions = actions.child(button.build(k).key(action_key));
                    }
                    item = item.child(actions);
                    list = list.child(item);
                }
                body = body.child(list);
            }
            // The inline editor (T469).
            if state.editing_rule.is_some() {
                let editor = ValuePresentationRuleEditor::<S>::new(
                    &state.editor,
                    dark,
                    {
                        let section_edit = Rc::clone(section_edit);
                        move |s: &mut S| {
                            let editor_edit = editor_of::<S>(&section_edit);
                            editor_edit(s)
                        }
                    },
                    self.contribution.clone(),
                    self.environment,
                )
                .with_other_names(state.editing_other_names())
                .on_save({
                    let section_edit = Rc::clone(section_edit);
                    move |s: &mut S, rule| section_edit(s).save_rule_draft(rule)
                })
                .on_cancel({
                    let section_edit = Rc::clone(section_edit);
                    move |s: &mut S| section_edit(s).set_editing_rule(None)
                });
                body = body.child(editor.build(k, layer));
            }
            // The add button.
            let mut add = Button::new(t("settings.valuePresentation.rules.add"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .disabled(!state.rules_add_gate());
            {
                let section_edit = Rc::clone(section_edit);
                add = add.on_press(move |s: &mut S| {
                    section_edit(s).set_editing_rule(Some(EditingRule::New));
                });
            }
            body = body.child(
                row()
                    .key("value-presentation-rule-add-row")
                    .w_full()
                    .gap(8.0)
                    .child(add.build(k).key("value-presentation-rule-add")),
            );
            // The limits hint.
            body = body.child(
                text(
                    t("settings.valuePresentation.rules.limits")
                        .replace("{maxRules}", &MAX_VALUE_PRESENTATION_RULES.to_string())
                        .replace("{maxPattern}", &MAX_PATTERN_LENGTH.to_string()),
                )
                .font(TEXT_XS)
                .color(k.c.text_muted),
            );
            rules_card = rules_card.child(
                col()
                    .key("value-presentation-rules-open-body")
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .child(row().key("value-presentation-rules-open-divider").w_full().h(1.0).fill(k.c.border_base))
                    .child(body),
            );
        }
        section = section.child(rules_card);

        // The error.
        if let Some(error) = &state.error {
            section = section.child(
                text(error.clone())
                    .key("value-presentation-error")
                    .font(TEXT_SM)
                    .color(k.c.danger.fg),
            );
        }

        // The footer.
        let mut save = Button::new(if state.saving {
            t("settings.valuePresentation.saving")
        } else {
            t("settings.valuePresentation.save")
        })
        .control_size(ControlSize::Small) // React primary
        .disabled(!state.is_dirty() || state.saving);
        {
            let on_save = self.on_save.clone();
            let section_edit = Rc::clone(section_edit);
            let contribution = self.contribution.clone();
            save = save.on_press(move |s: &mut S| {
                let plan = section_edit(s).begin_save(contribution.as_ref());
                if let (Some(plan), Some(on_save)) = (plan, &on_save) {
                    on_save(s, plan);
                }
            });
        }
        let mut reset = Button::new(t("settings.valuePresentation.reset"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(!state.is_dirty());
        {
            let section_edit = Rc::clone(section_edit);
            reset = reset.on_press(move |s: &mut S| section_edit(s).reset_to_stored());
        }
        let mut footer = row()
            .key("value-presentation-footer")
            .w_full()
            .gap(8.0)
            .center_y()
            .child(save.build(k).key("value-presentation-save"))
            .child(reset.build(k).key("value-presentation-reset"));
        if state.shows_saved() {
            footer = footer.child(
                text(t("settings.valuePresentation.saved").to_owned())
                    .key("value-presentation-saved")
                    .font(TEXT_XS)
                    .color(k.c.success.fg),
            );
        }
        section = section.child(footer);

        section
    }
}

/// One rule-row action (the small link-styled buttons).
#[derive(Clone, Copy, Debug)]
enum RuleAction {
    Edit(usize),
    Delete(usize),
    Up(usize),
    Down(usize),
    Toggle(usize),
}

impl RuleAction {
    /// The action's disabled face beyond `editingRule !== null`:
    /// the first row's up and the last row's down.
    fn disabled_face(self, index: usize, len: usize) -> bool {
        match self {
            RuleAction::Up(_) => index == 0,
            RuleAction::Down(_) => index == len.saturating_sub(1),
            _ => false,
        }
    }

    fn apply(self, section: &mut ValuePresentationSettingsSectionState) {
        match self {
            RuleAction::Edit(index) => {
                section.set_editing_rule(Some(EditingRule::Index(index)));
            }
            RuleAction::Delete(index) => section.delete_rule(index),
            RuleAction::Up(index) => section.move_rule(index, -1),
            RuleAction::Down(index) => section.move_rule(index, 1),
            RuleAction::Toggle(index) => section.toggle_rule_enabled(index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::value_presentation::contracts::SemanticKind;
    use crate::support::value_presentation::settings::parse_stored_value_presentation_settings;
    use crate::support::value_presentation::timestamp_contribution::timestamp_contribution;
    use serde_json::json;

    fn stored_with_renderer(enabled: bool, options: serde_json::Value) -> StoredValuePresentationSettings {
        parse_stored_value_presentation_settings(&json!({
            "version": 1,
            "renderers": {
                "datazen.timestamp": { "enabled": enabled, "options": options }
            }
        }))
    }

    fn valid_options() -> serde_json::Value {
        json!({
            "interpretation": { "kind": "native" },
            "format": "iso-tz",
            "displayTimeZone": { "kind": "local" }
        })
    }

    fn sample_rule(name: &str) -> ValuePresentationRule {
        let input = json!({
            "name": name,
            "enabled": true,
            "match": { "column": { "kind": "exact", "value": "created_at" } },
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso",
                "displayTimeZone": { "kind": "utc" }
            }
        });
        let ValuePresentationRuleValidation::Ok(rule) = validate_value_presentation_rule(&input)
        else {
            panic!("the fixture must validate");
        };
        *rule
    }

    #[test]
    fn the_draft_reads_the_store() {
        // No entry → the default-off face.
        let empty = parse_stored_value_presentation_settings(&json!({"version": 1}));
        let draft = draft_from_stored(&empty);
        assert!(!draft.enabled);
        assert_eq!(draft.options, DEFAULT_TIMESTAMP_OPTIONS);
        // A valid entry rides through.
        let stored = stored_with_renderer(true, valid_options());
        let draft = draft_from_stored(&stored);
        assert!(draft.enabled);
        assert_eq!(draft.options.format, FormatSerde::IsoTz);
        // An illegal options value keeps the switch, falls the
        // options back.
        let stored = stored_with_renderer(true, json!({ "format": "nope" }));
        let draft = draft_from_stored(&stored);
        assert!(draft.enabled);
        assert_eq!(draft.options, DEFAULT_TIMESTAMP_OPTIONS);
    }

    #[test]
    fn the_preview_samples_and_options() {
        // The PRD §4 double anchors.
        let native = preview_sample_for(InterpretationChoice::Native);
        assert_eq!(native.value, json!("2026-01-15T14:30:00Z"));
        assert_eq!(native.semantic_kind, SemanticKind::Instant);
        let millis = preview_sample_for(InterpretationChoice::UnixMilliseconds);
        assert_eq!(millis.value, json!(1705316400000u64));
        assert_eq!(millis.semantic_kind, SemanticKind::Number);
        let text = preview_sample_for(InterpretationChoice::Text);
        assert_eq!(text.semantic_kind, SemanticKind::LocalDatetime);
        // buildPreviewOptions: unix drops the source zone; text
        // defaults it to UTC; native keeps the draft's.
        let mut draft_options = DEFAULT_TIMESTAMP_OPTIONS;
        draft_options.source_time_zone = Some(TimeZoneSpec::Iana { name: "Asia/Shanghai".to_owned() });
        let unix = build_preview_options(&draft_options, InterpretationChoice::UnixSeconds);
        assert_eq!(unix.source_time_zone, None, "unix carries no source zone");
        assert!(matches!(
            unix.interpretation,
            crate::support::value_presentation::timestamp_config::TimestampInterpretation::Unix {
                unit: crate::support::value_presentation::timestamp_config::UnixUnitSerde::Seconds
            }
        ));
        let text = build_preview_options(&draft_options, InterpretationChoice::Text);
        assert_eq!(text.source_time_zone, Some(TimeZoneSpec::Iana { name: "Asia/Shanghai".to_owned() }));
        let bare = build_preview_options(&DEFAULT_TIMESTAMP_OPTIONS, InterpretationChoice::Text);
        assert_eq!(bare.source_time_zone, Some(TimeZoneSpec::Utc), "text defaults to UTC");
        let native = build_preview_options(&draft_options, InterpretationChoice::Native);
        assert_eq!(native.source_time_zone, draft_options.source_time_zone);
        // format_preview_sample.
        assert_eq!(format_preview_sample(&json!("abc")), "abc");
        assert_eq!(format_preview_sample(&json!(1705316400u64)), "1705316400");
        assert_eq!(format_preview_sample(&json!(null)), "");
    }

    #[test]
    fn the_preview_walks_its_faces() {
        let environment = PresentationEnvironment { locale: "en-US", local_time_zone: "Asia/Shanghai" };
        // No contribution → unavailable.
        assert!(matches!(
            compute_timestamp_preview(None, &DEFAULT_TIMESTAMP_OPTIONS, InterpretationChoice::Native, environment),
            TimestampPreviewResult::Unavailable { .. }
        ));
        // A bad IANA zone in the options: the config validates
        // structurally, and present's honest fallback carries the
        // reason (the same road as the runtime — the typed options
        // cannot be structurally invalid here).
        let mut bad = DEFAULT_TIMESTAMP_OPTIONS;
        bad.display_time_zone = TimeZoneSpec::Iana { name: "Nope/Nope".to_owned() };
        match compute_timestamp_preview(
            Some(&timestamp_contribution()),
            &bad,
            InterpretationChoice::Native,
            environment,
        ) {
            TimestampPreviewResult::Unchanged { reason, .. } => {
                assert_eq!(reason.as_deref(), Some("invalid-time-zone"));
            }
            other => panic!("a bad zone falls back unchanged: {other:?}"),
        }
        // A good native preview presents.
        match compute_timestamp_preview(
            Some(&timestamp_contribution()),
            &DEFAULT_TIMESTAMP_OPTIONS,
            InterpretationChoice::Native,
            environment,
        ) {
            TimestampPreviewResult::Text { text, .. } => {
                assert!(!text.is_empty(), "the native sample formats");
            }
            other => panic!("a good preview presents: {other:?}"),
        }
    }

    #[test]
    fn the_panel_edits_and_dirties() {
        let stored = stored_with_renderer(false, valid_options());
        let mut state = ValuePresentationSettingsSectionState::new(&stored);
        assert!(!state.is_dirty());
        state.note_save_ok();
        assert!(state.saved);
        // Every edit clears the saved marker.
        state.set_enabled(true);
        assert!(!state.saved);
        assert!(state.is_dirty());
        state.note_save_ok();
        state.set_format(FormatSerde::Locale);
        assert!(!state.saved);
        state.note_save_ok();
        state.set_time_zone_kind(TimeZoneKindChoice::Iana);
        assert!(!state.saved);
        assert!(state.is_dirty());
        // The IANA name rides live and trimmed into optionsToSave.
        state.iana_name.text = "  Asia/Shanghai  ".to_owned();
        assert_eq!(
            state.options_to_save().display_time_zone,
            TimeZoneSpec::Iana { name: "Asia/Shanghai".to_owned() }
        );
        // The format-invalid / unknown gates.
        assert!(!state.iana_format_invalid(), "Asia/Shanghai is known");
        assert!(!state.iana_unknown());
        // A bad charset is the format error; a well-formed but
        // unknown zone is the zone error.
        state.iana_name.text = "Not A Zone".to_owned();
        assert!(state.iana_format_invalid());
        assert!(!state.iana_unknown());
        state.iana_name.text = "Nope/Nope".to_owned();
        assert!(!state.iana_format_invalid());
        assert!(state.iana_unknown(), "the charset passes, the zone is unknown");
        // The saved marker's face needs !isDirty.
        assert!(!state.shows_saved());
    }

    #[test]
    fn the_save_gates_and_plan() {
        let stored = parse_stored_value_presentation_settings(&json!({"version": 1}));
        let mut state = ValuePresentationSettingsSectionState::new(&stored);
        // An invalid IANA name refuses before anything runs.
        state.set_time_zone_kind(TimeZoneKindChoice::Iana);
        state.iana_name.text = "Not A Zone".to_owned();
        assert!(state.begin_save(None).is_none());
        assert_eq!(state.error.as_deref(), Some(t("settings.valuePresentation.ianaInvalid")));
        assert!(!state.saving);
        // A well-formed but unknown zone refuses with its own
        // sentence.
        state.iana_name.text = "Nope/Nope".to_owned();
        assert!(state.begin_save(None).is_none());
        assert_eq!(state.error.as_deref(), Some(t("settings.valuePresentation.ianaUnknown")));
        assert!(!state.saving);
        // A valid form plans the write.
        state.set_time_zone_kind(TimeZoneKindChoice::Local);
        state.set_enabled(true);
        state.set_format(FormatSerde::IsoTz);
        let plan = state.begin_save(None).expect("the plan");
        assert!(plan.format_dirty);
        assert!(!plan.rules_dirty);
        assert!(state.saving);
        assert_eq!(state.error, None);
        // apply(): only the format path writes over the fresh read.
        let fresh = parse_stored_value_presentation_settings(&json!({
            "version": 1,
            "renderers": { "other.renderer": { "enabled": true, "options": {} } }
        }));
        let next = plan.apply(&fresh);
        let entry = next.renderers.get("datazen.timestamp").expect("the write");
        assert!(entry.enabled);
        assert_eq!(
            entry.options,
            serde_json::to_value(&plan.options).unwrap_or_else(|_| serde_json::Value::Null)
        );
        assert!(next.renderers.contains_key("other.renderer"), "the untouched renderer survives");
        assert_eq!(next.rules, None, "the rules path did not write");
        // The answers settle.
        state.note_save_ok();
        assert!(state.saved);
        assert!(!state.saving);
        state.note_save_error();
        assert!(!state.saved);
        assert_eq!(state.error.as_deref(), Some(t("settings.valuePresentation.saveFailed")));
        assert!(!state.saving);
    }

    #[test]
    fn the_rules_plan_writes_the_list() {
        let stored = parse_stored_value_presentation_settings(&json!({"version": 1}));
        let mut state = ValuePresentationSettingsSectionState::new(&stored);
        state.set_editing_rule(Some(EditingRule::New));
        state.save_rule_draft(sample_rule("epoch-ms"));
        let plan = state.begin_save(None).expect("the plan");
        assert!(!plan.format_dirty, "the untouched default draft is clean");
        assert!(plan.rules_dirty);
        let next = plan.apply(&stored);
        assert_eq!(next.rules.as_ref().map(|r| r.len()), Some(1));
        // An unknown version keeps the rules verbatim (T06's
        // ruling inside with_value_presentation_rules).
        let future = parse_stored_value_presentation_settings(&json!({"version": 99}));
        let kept = with_value_presentation_rules(&future, &[sample_rule("x")]);
        assert_eq!(kept.rules, future.rules);
    }

    #[test]
    fn the_rule_list_edits() {
        let mut stored = parse_stored_value_presentation_settings(&json!({"version": 1}));
        stored.rules = Some(vec![sample_rule("a"), sample_rule("b"), sample_rule("c")]);
        let mut state = ValuePresentationSettingsSectionState::new(&stored);
        assert_eq!(state.rule_drafts.len(), 3);
        // The move with bounds.
        state.move_rule(0, -1);
        assert_eq!(
            state.rule_drafts.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        state.move_rule(0, 1);
        assert_eq!(
            state.rule_drafts.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
        state.move_rule(2, 1);
        assert_eq!(
            state.rule_drafts.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "a", "c"],
            "the last row cannot move down"
        );
        // The toggle flips in place (and does NOT clear the saved
        // marker — React's own verbatim).
        state.note_save_ok();
        state.toggle_rule_enabled(1);
        assert!(!state.rule_drafts[1].enabled);
        assert!(state.saved, "the toggle keeps the saved marker (verbatim)");
        // The delete removes (same verbatim rule).
        state.delete_rule(0);
        assert_eq!(
            state.rule_drafts.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert!(state.saved);
        // The editor arms per edit.
        state.set_editing_rule(Some(EditingRule::Index(0)));
        assert_eq!(state.editor.form.name.text, "a");
        assert_eq!(state.editing_other_names(), vec!["c".to_owned()], "the other row");
        state.save_rule_draft(sample_rule("renamed"));
        assert_eq!(state.rule_drafts[0].name, "renamed");
        assert!(!state.saved, "a rule save clears the marker");
        assert_eq!(state.editing_rule, None);
        // A new edit arms the blank editor and lists the others.
        state.set_editing_rule(Some(EditingRule::New));
        assert_eq!(state.editor.form.name.text, "");
        assert_eq!(state.editing_other_names(), vec!["renamed".to_owned(), "c".to_owned()]);
        // The add gate.
        assert!(!state.rules_add_gate(), "an edit in flight blocks the add");
        state.set_editing_rule(None);
        assert!(state.rules_add_gate());
        // The cap: at MAX rules the add refuses.
        for index in 0..MAX_VALUE_PRESENTATION_RULES {
            state.rule_drafts.push(sample_rule(&format!("r{index}")));
        }
        assert!(!state.rules_add_gate(), "the cap refuses the add");
    }

    #[test]
    fn the_t11_external_update_ruling() {
        let stored = parse_stored_value_presentation_settings(&json!({"version": 1}));
        let mut state = ValuePresentationSettingsSectionState::new(&stored);
        // A local edit, then another window saves.
        state.set_format(FormatSerde::Locale);
        let mut other = stored.clone();
        other.renderers.insert(
            "datazen.timestamp".to_owned(),
            crate::support::value_presentation::settings::StoredRendererSettings {
                enabled: true,
                options: valid_options(),
            },
        );
        state.note_stored(&other);
        assert!(state.external_update, "the pending edit only prompts");
        assert_eq!(state.draft.options.format, FormatSerde::Locale, "the draft survives");
        // The reload button overwrites.
        state.reset_to_stored();
        assert!(!state.external_update);
        assert_eq!(state.draft.options.format, FormatSerde::IsoTz, "the saved value lands");
        assert!(state.draft.enabled);
        // A change the draft already echoes (our own save's echo)
        // rebuilds without prompting.
        state.set_enabled(false);
        let mut echoed = other.clone();
        echoed.renderers.insert(
            "datazen.timestamp".to_owned(),
            crate::support::value_presentation::settings::StoredRendererSettings {
                enabled: false,
                options: valid_options(),
            },
        );
        state.note_stored(&echoed);
        assert!(!state.external_update);
        assert!(!state.draft.enabled);
        // A pure external change (no local edits) also rebuilds.
        let mut pure = echoed.clone();
        pure.renderers.insert(
            "datazen.timestamp".to_owned(),
            crate::support::value_presentation::settings::StoredRendererSettings {
                enabled: true,
                options: valid_options(),
            },
        );
        state.note_stored(&pure);
        assert!(!state.external_update);
        assert!(state.draft.enabled);
        assert!(!state.is_dirty());
    }
}
