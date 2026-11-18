//! `AiChatPanel` header — T009-S3, the third slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 405–644: the tab header row, the session-migration banners, the
//! agent-mode row with its prompt-template select, the AI profile row, and
//! the share-editor row. The panel itself is a later slice: this file has
//! no `aiStore` wiring and is not mounted in `app.rs`'s `Section`
//! dispatch.
//!
//! | React source | Rune item |
//! |---|---|
//! | tab header (405–487) | [`AiChatHeader::header_row`] |
//! | session-migration banners (496–524) | [`AiChatHeader::migration_banner`] |
//! | agent-mode row + template select (527–587) | [`AiChatHeader::agent_mode_row`] |
//! | profile row (589–614) | [`AiChatHeader::profile_row`] |
//! | share-editor row (616–644) | [`AiChatHeader::share_editor_row`] |
//! | `detectSafetyPreset` (`lib/aiSafetyPresets.ts`) | [`detect_safety_preset`] |
//! | `activeProfile` fallback chain (168–171) | [`AiChatHeaderState::active_profile`] |
//!
//! # State ownership
//!
//! Like S1's `QuestionBlock` and S2's `ChatBubble`, the component is a pure
//! render + event router: the caller owns an [`AiChatHeaderState`] and
//! passes it in as `&state` each frame; presses call back into the caller,
//! which drives the state machine.
//!
//! # View-model boundary (aiStore slice T088 not done)
//!
//! `aiStore` has no Rune slice yet, so the state is a view-model
//! placeholder, not a store binding:
//!
//! * tabs, history open, agent mode, template id, profiles, sharing, the
//!   migration notices and `has_messages` are plain fields on
//!   [`AiChatHeaderState`], seeded through its `set_*` methods; the panel
//!   slice wires them to the real store once T088 lands;
//! * `openDocsWindow` and `clearChat` are host / store actions, so the
//!   help and clear buttons only report through `on_open_ai_help`,
//!   `on_open_workflow_help` and `on_clear_chat` — nothing here opens a
//!   window or clears a session;
//! * React resets `templateId` when `connectionId` / `databaseType`
//!   change (`AiChatPanel.tsx`, the `useEffect` above the template row);
//!   that effect lives on the panel's props, so it belongs to the panel
//!   slice, not to this state machine — [`AiChatHeaderState`] never
//!   resets the template on its own;
//! * assistant message content is still S2's plain-text placeholder;
//!   this slice renders no messages at all.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim where React has one:
//! `ai-session-history-toggle`, `ai-agent-mode-row`, `ai-agent-mode-ask`,
//! `ai-agent-mode-agent`, `ai-agent-mode-hint`, `ai-template-select`,
//! `ai-share-editor-row`, `ai-share-editor-toggle`,
//! `ai-share-editor-error`, `ai-session-migration-banner`,
//! `ai-session-migration-banner-text`,
//! `ai-session-migration-success-banner`,
//! `ai-session-migration-success-banner-text`. Internal keys (no React testid)
//! are documented here so the panel slice can find them: `ai-chat-header`,
//! `ai-chat-header-row`, `ai-chat-tabs`, `ai-tab-chat`, `ai-tab-workflows`,
//! `ai-chat-header-actions`, `ai-open-ai-help`, `ai-open-workflow-help`,
//! `ai-clear-chat`, `ai-agent-mode-group`, `ai-profile-row`,
//! `ai-profile-label-group`, `ai-profile-shield`, `ai-profile-label`,
//! `ai-profile-select`, `ai-share-editor-unavailable`, and the `*-divider`
//! rows. The two selects take `ai-template-select-field` /
//! `ai-profile-select-field` as their kit key, so the kit derives
//! `{key}-trigger` / `{key}-content` from them; `ai-template-select`
//! itself is the wrapper row, matching React's testid on the wrapping div.
//!
//! # Deliberate boundaries
//!
//! * `AiSessionHistory` is a separate component, not this slice: the
//!   header renders only the history toggle button and owns the
//!   `sessionHistoryOpen` flag; the history panel itself is a later slice.
//! * The `!isConfigured` early return (`AiChatPanel.tsx:385–403`) is
//!   outside the 405–644 range and belongs to the panel slice.
//! * `aria-pressed` has no Rune hook (see S1's module docs); the tab and
//!   mode buttons use `El::focus_selected` — the same channel S1/S2 use —
//!   and the pressed state is observed through `ElementInfo::item_selected`
//!   plus the accent wash fill. The mode buttons' `title` rides the kit
//!   button's `help` slot; the history toggle's `aria-expanded` rides
//!   the kit button's `expanded` slot.
//! * React's clear button has no title or accessible name; a bare
//!   icon-only button with no name is unusable to a screen reader, so it
//!   gets `chat.clear` ("清除对话") as its label and help — an a11y
//!   addition, not a React behaviour change. It is wired only while
//!   enabled: React's disabled button never fires, and an unwired
//!   disabled button is the honest equivalent.
//! * No single-side borders in Rune (see S2): each React `border-b` row
//!   ends with a 1pt divider element (`navigation.rs`'s `section_divider`
//!   precedent), tinted `border-edge/60` — or `danger/30` / `success/30`
//!   under the migration banners — instead of a bottom border on the row.
//! * The kit type scale has no 10pt step, so React's 10px texts (mode
//!   buttons, hint, banners) and 11px texts (profile label, share label)
//!   render at 12pt `text_xsmall` / `text_xsmall_medium`; the 12px tab
//!   labels are exact at `text_xsmall`.
//! * The share row's `title` (the unavailable note as a hover tooltip on
//!   a disabled label) has no Rune hook, so the note is only rendered as
//!   the inline `ai-share-editor-unavailable` span React also renders.
//! * The shield icon is a bare `rune::icon` tinted per safety preset
//!   (`nav.rs` precedent); the hues are React's exact Tailwind values
//!   (`text-emerald-400` / `text-amber-400` / `text-blue-400`, kept in
//!   `ui/palette.rs`), and `custom` maps to blue exactly as React's
//!   fallback branch does.

// The panel view (a later T009 slice) is the production consumer; until it
// lands the component tests are the only callers, so the dead-code lint is
// silenced here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, Color, El, Key, KeyEvent, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

use crate::i18n::t;
use crate::icons::{
    ICON_BOOK_OPEN, ICON_HISTORY, ICON_MESSAGE_SQUARE, ICON_SHIELD, ICON_TRASH, ICON_WAND_SPARKLES,
};
use crate::ui::kit::kit;
use crate::ui::palette::{
    ai_shield_cloud_strict, ai_shield_enterprise_balanced, ai_shield_local_trust,
};
use crate::ui::select::Select;
use crate::ui::toggle::Switch;

/// React's panel tab (`useState<'chat' | 'workflows'>`, default `'chat'`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AiChatTab {
    #[default]
    Chat,
    Workflows,
}

/// React's `AiAgentMode` (`types/ai.ts`): `'ask'` (default) or `'agent'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AiAgentMode {
    #[default]
    Ask,
    Agent,
}

/// React's `AiSafetyGateConfig` (`types/ai.ts`): the four gate fields
/// `detectSafetyPreset` reads. Kept as strings so the preset detection
/// below can mirror the TypeScript comparisons field for field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiSafetyGate {
    pub data_egress_level: String,
    pub db_tool_policy: String,
    pub mcp_tool_policy: String,
    pub require_sql_confirm: bool,
}

impl AiSafetyGate {
    pub(crate) fn new(
        data_egress_level: impl Into<String>,
        db_tool_policy: impl Into<String>,
        mcp_tool_policy: impl Into<String>,
        require_sql_confirm: bool,
    ) -> Self {
        Self {
            data_egress_level: data_egress_level.into(),
            db_tool_policy: db_tool_policy.into(),
            mcp_tool_policy: mcp_tool_policy.into(),
            require_sql_confirm,
        }
    }

    /// `AI_SAFETY_PRESETS.cloud_strict`.
    pub(crate) fn cloud_strict() -> Self {
        Self::new("strict", "read_only", "disabled", true)
    }

    /// `AI_SAFETY_PRESETS.enterprise_balanced`.
    pub(crate) fn enterprise_balanced() -> Self {
        Self::new("sample_masked", "require_confirm", "require_confirm", true)
    }

    /// `AI_SAFETY_PRESETS.local_trust`.
    pub(crate) fn local_trust() -> Self {
        Self::new("unrestricted", "unrestricted", "unrestricted", false)
    }
}

/// React's `AiSafetyPresetType | 'custom'` (`lib/aiSafetyPresets.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AiSafetyPreset {
    CloudStrict,
    EnterpriseBalanced,
    LocalTrust,
    Custom,
}

/// React's `detectSafetyPreset` (`lib/aiSafetyPresets.ts`), line for line:
/// each preset is an exact conjunction of the four gate fields, anything
/// else is `custom`.
pub(crate) fn detect_safety_preset(gate: &AiSafetyGate) -> AiSafetyPreset {
    if gate.data_egress_level == "strict"
        && gate.db_tool_policy == "read_only"
        && gate.mcp_tool_policy == "disabled"
        && gate.require_sql_confirm
    {
        AiSafetyPreset::CloudStrict
    } else if gate.data_egress_level == "sample_masked"
        && gate.db_tool_policy == "require_confirm"
        && gate.mcp_tool_policy == "require_confirm"
        && gate.require_sql_confirm
    {
        AiSafetyPreset::EnterpriseBalanced
    } else if gate.data_egress_level == "unrestricted"
        && gate.db_tool_policy == "unrestricted"
        && gate.mcp_tool_policy == "unrestricted"
        && !gate.require_sql_confirm
    {
        AiSafetyPreset::LocalTrust
    } else {
        AiSafetyPreset::Custom
    }
}

/// One prompt template (`AiSettingsConfig.promptTemplates`): React's
/// template option is `{ value: template.id, label: template.name ||
/// template.id }` — an empty name falls back to the id, as JS `||` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiPromptTemplate {
    pub id: String,
    pub name: String,
}

impl AiPromptTemplate {
    pub(crate) fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    pub(crate) fn label(&self) -> String {
        if self.name.is_empty() {
            self.id.clone()
        } else {
            self.name.clone()
        }
    }
}

/// One AI profile (`AiSettingsConfig.profiles`): the id, the display name
/// and model, whether it is the default, and its safety gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiProfile {
    pub id: String,
    pub name: String,
    pub model: String,
    pub is_default: bool,
    pub safety_gate: AiSafetyGate,
}

impl AiProfile {
    pub(crate) fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            model: model.into(),
            is_default: false,
            safety_gate: AiSafetyGate::cloud_strict(),
        }
    }

    pub(crate) fn is_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
    }

    pub(crate) fn safety_gate(mut self, safety_gate: AiSafetyGate) -> Self {
        self.safety_gate = safety_gate;
        self
    }

    /// React's profile-option label: `p.name || p.model` (may be empty
    /// when both are, exactly as in React).
    pub(crate) fn display_name(&self) -> String {
        if self.name.is_empty() {
            self.model.clone()
        } else {
            self.name.clone()
        }
    }
}

/// The mutable state behind an [`AiChatHeader`]: the view-model placeholder
/// for the `aiStore` slice (T088, not done). The caller owns one of these
/// (in `AppState` once the panel lands; in the probe state in tests) and
/// hands `&state` to the builder each frame; presses call back into the
/// caller, which drives the state machine. Defaults mirror React's
/// `useState` initials: chat tab, history closed, ask mode, no template,
/// nothing shared, no migration notice, no messages.
#[derive(Clone, Debug, Default)]
pub(crate) struct AiChatHeaderState {
    tab: AiChatTab,
    session_history_open: bool,
    agent_mode: AiAgentMode,
    template_id: String,
    templates: Vec<AiPromptTemplate>,
    template_open: bool,
    profiles: Vec<AiProfile>,
    active_profile_id: String,
    profile_open: bool,
    sharing: bool,
    can_share: bool,
    share_error: Option<String>,
    session_migration_error: Option<String>,
    session_migration_success: u32,
    has_messages: bool,
}

impl AiChatHeaderState {
    pub(crate) fn tab(&self) -> AiChatTab {
        self.tab
    }

    pub(crate) fn session_history_open(&self) -> bool {
        self.session_history_open
    }

    pub(crate) fn agent_mode(&self) -> AiAgentMode {
        self.agent_mode
    }

    pub(crate) fn template_id(&self) -> &str {
        &self.template_id
    }

    pub(crate) fn templates(&self) -> &[AiPromptTemplate] {
        &self.templates
    }

    pub(crate) fn template_open(&self) -> bool {
        self.template_open
    }

    pub(crate) fn profiles(&self) -> &[AiProfile] {
        &self.profiles
    }

    pub(crate) fn active_profile_id(&self) -> &str {
        &self.active_profile_id
    }

    pub(crate) fn profile_open(&self) -> bool {
        self.profile_open
    }

    pub(crate) fn sharing(&self) -> bool {
        self.sharing
    }

    pub(crate) fn can_share(&self) -> bool {
        self.can_share
    }

    pub(crate) fn share_error(&self) -> Option<&str> {
        self.share_error.as_deref()
    }

    pub(crate) fn session_migration_error(&self) -> Option<&str> {
        self.session_migration_error.as_deref()
    }

    pub(crate) fn session_migration_success(&self) -> u32 {
        self.session_migration_success
    }

    pub(crate) fn has_messages(&self) -> bool {
        self.has_messages
    }

    /// React's `setTab`.
    pub(crate) fn set_tab(&mut self, tab: AiChatTab) {
        self.tab = tab;
    }

    /// React's `setSessionHistoryOpen((open) => !open)` on the toggle.
    pub(crate) fn toggle_session_history(&mut self) {
        self.session_history_open = !self.session_history_open;
    }

    /// React's `setSessionHistoryOpen(false)` — the history panel's
    /// `onClose`, kept for the later slice that renders it.
    pub(crate) fn set_session_history_open(&mut self, open: bool) {
        self.session_history_open = open;
    }

    /// React's `setAgentMode`.
    pub(crate) fn set_agent_mode(&mut self, mode: AiAgentMode) {
        self.agent_mode = mode;
    }

    /// A template choice: React's `setTemplateId` plus closing the
    /// listbox, which the kit select leaves to the app (see `ui/select.rs`).
    pub(crate) fn choose_template(&mut self, template_id: &str) {
        self.template_id = template_id.to_string();
        self.template_open = false;
    }

    pub(crate) fn toggle_template_open(&mut self) {
        self.template_open = !self.template_open;
    }

    pub(crate) fn close_template_select(&mut self) {
        self.template_open = false;
    }

    /// A profile choice: React's `setActiveProfile` plus closing the
    /// listbox.
    pub(crate) fn choose_profile(&mut self, profile_id: &str) {
        self.active_profile_id = profile_id.to_string();
        self.profile_open = false;
    }

    pub(crate) fn toggle_profile_open(&mut self) {
        self.profile_open = !self.profile_open;
    }

    pub(crate) fn close_profile_select(&mut self) {
        self.profile_open = false;
    }

    /// React's `setSharing` from `useAiEditorGrant`. The checkbox is
    /// disabled while `!can_share`, so a share change can only arrive
    /// when sharing is possible; the gate keeps the state machine honest
    /// if a caller drives it directly.
    pub(crate) fn set_sharing(&mut self, sharing: bool) {
        if self.can_share {
            self.sharing = sharing;
        }
    }

    /// React's `activeProfile` (`AiChatPanel.tsx:168–171`): the active id,
    /// else the default profile, else the first profile.
    pub(crate) fn active_profile(&self) -> Option<&AiProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_profile_id)
            .or_else(|| self.profiles.iter().find(|profile| profile.is_default))
            .or_else(|| self.profiles.first())
    }

    /// React's profile-row label: `{name || model || 'AI Model'}:`.
    pub(crate) fn profile_display_label(&self) -> String {
        let name = self
            .active_profile()
            .map(|profile| profile.display_name())
            .unwrap_or_default();
        let name = if name.is_empty() {
            "AI Model".to_string()
        } else {
            name
        };
        format!("{name}:")
    }

    /// The shield preset for the active profile; with no profile React
    /// falls back to `'cloud_strict'`.
    pub(crate) fn shield_preset(&self) -> AiSafetyPreset {
        self.active_profile()
            .map(|profile| detect_safety_preset(&profile.safety_gate))
            .unwrap_or(AiSafetyPreset::CloudStrict)
    }

    /// React's profile-row gate: chat tab, a settings config, and at least
    /// one profile. The config itself is the VM's `profiles` list.
    pub(crate) fn show_profile_row(&self) -> bool {
        self.tab == AiChatTab::Chat && !self.profiles.is_empty()
    }

    // -- VM seeding: the store writes the panel slice will make once T088
    // lands. Until then tests (and the panel slice's placeholder) drive
    // these directly. --

    pub(crate) fn set_templates(&mut self, templates: Vec<AiPromptTemplate>) {
        self.templates = templates;
    }

    pub(crate) fn set_profiles(&mut self, profiles: Vec<AiProfile>) {
        self.profiles = profiles;
    }

    pub(crate) fn set_active_profile_id(&mut self, profile_id: impl Into<String>) {
        self.active_profile_id = profile_id.into();
    }

    pub(crate) fn set_can_share(&mut self, can_share: bool) {
        self.can_share = can_share;
    }

    pub(crate) fn set_share_error(&mut self, error: Option<String>) {
        self.share_error = error;
    }

    pub(crate) fn set_session_migration_error(&mut self, error: Option<String>) {
        self.session_migration_error = error;
    }

    pub(crate) fn set_session_migration_success(&mut self, count: u32) {
        self.session_migration_success = count;
    }

    pub(crate) fn set_has_messages(&mut self, has_messages: bool) {
        self.has_messages = has_messages;
    }
}

/// `AiChatPanel`'s header (`AiChatPanel.tsx:405–644`): the tab row, the
/// migration banners, the agent-mode row, the profile row and the
/// share-editor row. The caller owns the [`AiChatHeaderState`] and wires
/// the callbacks; the header itself keeps no state. The two selects push
/// their listboxes into `layer` (the `ui/select.rs` two-part build), so
/// `build` takes the layer the panel slice renders alongside the header.
pub(crate) struct AiChatHeader<'a, S: 'static> {
    state: &'a AiChatHeaderState,
    on_select_tab: Option<Rc<dyn Fn(&mut S, AiChatTab)>>,
    on_toggle_session_history: Option<Rc<dyn Fn(&mut S)>>,
    on_open_ai_help: Option<Rc<dyn Fn(&mut S)>>,
    on_open_workflow_help: Option<Rc<dyn Fn(&mut S)>>,
    on_clear_chat: Option<Rc<dyn Fn(&mut S)>>,
    on_select_agent_mode: Option<Rc<dyn Fn(&mut S, AiAgentMode)>>,
    on_toggle_template: Option<Rc<dyn Fn(&mut S)>>,
    on_close_template: Option<Rc<dyn Fn(&mut S)>>,
    on_choose_template: Option<Rc<dyn Fn(&mut S, &str, &mut Tx)>>,
    on_toggle_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_close_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_choose_profile: Option<Rc<dyn Fn(&mut S, &str, &mut Tx)>>,
    on_toggle_sharing: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> AiChatHeader<'a, S> {
    pub(crate) fn new(state: &'a AiChatHeaderState) -> Self {
        Self {
            state,
            on_select_tab: None,
            on_toggle_session_history: None,
            on_open_ai_help: None,
            on_open_workflow_help: None,
            on_clear_chat: None,
            on_select_agent_mode: None,
            on_toggle_template: None,
            on_close_template: None,
            on_choose_template: None,
            on_toggle_profile: None,
            on_close_profile: None,
            on_choose_profile: None,
            on_toggle_sharing: None,
        }
    }

    /// Called with the tab when a tab button is pressed; the caller drives
    /// [`AiChatHeaderState::set_tab`].
    pub(crate) fn on_select_tab(mut self, f: impl Fn(&mut S, AiChatTab) + 'static) -> Self {
        self.on_select_tab = Some(Rc::new(f));
        self
    }

    /// Called when the session-history toggle is pressed; the caller drives
    /// [`AiChatHeaderState::toggle_session_history`].
    pub(crate) fn on_toggle_session_history(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_session_history = Some(Rc::new(f));
        self
    }

    /// Called when the AI help button is pressed; the caller owns
    /// `openDocsWindow('context')`.
    pub(crate) fn on_open_ai_help(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_ai_help = Some(Rc::new(f));
        self
    }

    /// Called when the workflow help button is pressed; the caller owns
    /// `openDocsWindow('workflows')`.
    pub(crate) fn on_open_workflow_help(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_workflow_help = Some(Rc::new(f));
        self
    }

    /// Called when the clear button is pressed; the caller owns `clearChat`.
    pub(crate) fn on_clear_chat(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_chat = Some(Rc::new(f));
        self
    }

    /// Called with the mode when an agent-mode button is pressed; the
    /// caller drives [`AiChatHeaderState::set_agent_mode`].
    pub(crate) fn on_select_agent_mode(
        mut self,
        f: impl Fn(&mut S, AiAgentMode) + 'static,
    ) -> Self {
        self.on_select_agent_mode = Some(Rc::new(f));
        self
    }

    /// Called when the template select's trigger is pressed; the caller
    /// drives [`AiChatHeaderState::toggle_template_open`].
    pub(crate) fn on_toggle_template(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_template = Some(Rc::new(f));
        self
    }

    /// Called when the template listbox closes; the caller drives
    /// [`AiChatHeaderState::close_template_select`].
    pub(crate) fn on_close_template(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_template = Some(Rc::new(f));
        self
    }

    /// Called with the chosen template id; the caller drives
    /// [`AiChatHeaderState::choose_template`].
    pub(crate) fn on_choose_template(
        mut self,
        f: impl Fn(&mut S, &str, &mut Tx) + 'static,
    ) -> Self {
        self.on_choose_template = Some(Rc::new(f));
        self
    }

    /// Called when the profile select's trigger is pressed; the caller
    /// drives [`AiChatHeaderState::toggle_profile_open`].
    pub(crate) fn on_toggle_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_profile = Some(Rc::new(f));
        self
    }

    /// Called when the profile listbox closes; the caller drives
    /// [`AiChatHeaderState::close_profile_select`].
    pub(crate) fn on_close_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_profile = Some(Rc::new(f));
        self
    }

    /// Called with the chosen profile id; the caller drives
    /// [`AiChatHeaderState::choose_profile`].
    pub(crate) fn on_choose_profile(mut self, f: impl Fn(&mut S, &str, &mut Tx) + 'static) -> Self {
        self.on_choose_profile = Some(Rc::new(f));
        self
    }

    /// Called when the share-editor switch is toggled; the caller drives
    /// [`AiChatHeaderState::set_sharing`].
    pub(crate) fn on_toggle_sharing(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_sharing = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let edge = k.alpha(k.c.border_base, 0.6);
        let mut header = col().key("ai-chat-header").w_full().min_w(0.0);
        header = header.child(self.header_row(dark));
        header = header.child(divider("ai-chat-header-divider", edge));
        if state.tab == AiChatTab::Chat {
            if let Some((banner, tint)) = self.migration_banner(dark) {
                header = header.child(banner);
                header = header.child(divider("ai-session-migration-divider", tint));
            }
            header = header.child(self.agent_mode_row(dark, layer));
            header = header.child(divider("ai-agent-mode-divider", edge));
            if state.show_profile_row() {
                header = header.child(self.profile_row(dark, layer));
                header = header.child(divider("ai-profile-divider", edge));
            }
            header = header.child(self.share_editor_row(dark));
            header = header.child(divider("ai-share-editor-divider", edge));
        }
        header
    }

    /// The tab header row (`AiChatPanel.tsx:405–487`): chat / workflows
    /// tabs on the left, history + help + clear (chat) or help only
    /// (workflows) on the right.
    fn header_row(&self, dark: bool) -> El {
        let state = self.state;
        let mut tabs = row().key("ai-chat-tabs").center_y().gap(4.0);
        tabs = tabs.child(self.tab_button(
            dark,
            AiChatTab::Chat,
            t("common.aiAssistant"),
            &ICON_MESSAGE_SQUARE,
            "ai-tab-chat",
        ));
        tabs = tabs.child(self.tab_button(
            dark,
            AiChatTab::Workflows,
            t("ai.workflows.tab"),
            &ICON_WAND_SPARKLES,
            "ai-tab-workflows",
        ));
        let mut actions = row().key("ai-chat-header-actions").center_y().gap(2.0);
        if state.tab == AiChatTab::Chat {
            actions = actions.child(self.history_button(dark));
            actions = actions.child(self.help_button(
                dark,
                t("docs.openAiHelp"),
                "ai-open-ai-help",
                &self.on_open_ai_help,
            ));
            actions = actions.child(self.clear_button(dark));
        } else {
            actions = actions.child(self.help_button(
                dark,
                t("docs.openWorkflowHelp"),
                "ai-open-workflow-help",
                &self.on_open_workflow_help,
            ));
        }
        row()
            .key("ai-chat-header-row")
            .w_full()
            .min_w(0.0)
            .center_y()
            .justify_between()
            .px(12.0)
            .py(6.0)
            .child(tabs)
            .child(actions)
    }

    fn tab_button(
        &self,
        dark: bool,
        tab: AiChatTab,
        label: &str,
        icon: &'static rune::Icon,
        key: &str,
    ) -> El {
        let k = kit(dark);
        let active = self.state.tab == tab;
        let accent = k.c.text_accent;
        let muted = k.c.text_muted;
        let label_text = label.to_string();
        let button = Button::new(label.to_string())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(icon)
            .label_with(move |font, _ink| {
                text(label_text)
                    .font(font)
                    .color(if active { accent } else { muted })
            })
            .build(k)
            .key(key.to_string())
            // No `aria-pressed` hook exists in Rune (see the module docs);
            // the selected state rides `focus_selected`, as in S1/S2.
            .focus_selected(active)
            .fill(if active {
                k.alpha(accent, 0.10)
            } else {
                Color::TRANSPARENT
            });
        wire_button_value(button, &self.on_select_tab, tab)
    }

    fn history_button(&self, dark: bool) -> El {
        let k = kit(dark);
        let label = t("chat.sessions.open").to_string();
        let button = IconButton::new(&ICON_HISTORY)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(label.clone())
            .help(label)
            .expanded(self.state.session_history_open)
            .build(k)
            .key("ai-session-history-toggle");
        wire_button(button, &self.on_toggle_session_history)
    }

    fn help_button(
        &self,
        dark: bool,
        label: &str,
        key: &str,
        action: &Option<Rc<dyn Fn(&mut S)>>,
    ) -> El {
        let k = kit(dark);
        let label = label.to_string();
        let button = IconButton::new(&ICON_BOOK_OPEN)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(label.clone())
            .help(label)
            .build(k)
            .key(key.to_string());
        wire_button(button, action)
    }

    fn clear_button(&self, dark: bool) -> El {
        let k = kit(dark);
        let enabled = self.state.has_messages;
        // React gives this button no title or accessible name; the kit
        // icon button needs one, so it gets `chat.clear` (see module docs).
        let label = t("chat.clear").to_string();
        let button = IconButton::new(&ICON_TRASH)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(label.clone())
            .help(label)
            .disabled(!enabled)
            .build(k)
            .key("ai-clear-chat");
        if enabled {
            wire_button(button, &self.on_clear_chat)
        } else {
            button
        }
    }

    /// The migration banners (`AiChatPanel.tsx:496–524`), with the tint of
    /// the divider that follows them: the error banner wins over success,
    /// exactly as React's `!sessionMigrationError` gate does.
    fn migration_banner(&self, dark: bool) -> Option<(El, Color)> {
        let k = kit(dark);
        let state = self.state;
        if let Some(error) = state.session_migration_error() {
            let banner = row()
                .key("ai-session-migration-banner")
                .w_full()
                .min_w(0.0)
                .center_y()
                .px(12.0)
                .py(4.0)
                .fill(k.alpha(k.c.danger.fg, 0.05))
                .child(
                    text(error.to_string())
                        .key("ai-session-migration-banner-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.danger.fg)
                        .wrap(),
                );
            Some((banner, k.alpha(k.c.danger.fg, 0.30)))
        } else if state.session_migration_success > 0 {
            let message = t("chat.sessions.migrationSuccess").replacen(
                "{count}",
                &state.session_migration_success.to_string(),
                1,
            );
            let banner = row()
                .key("ai-session-migration-success-banner")
                .w_full()
                .min_w(0.0)
                .center_y()
                .px(12.0)
                .py(4.0)
                .fill(k.alpha(k.c.success.fg, 0.05))
                .child(
                    text(message)
                        .key("ai-session-migration-success-banner-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.success.fg)
                        .wrap(),
                );
            Some((banner, k.alpha(k.c.success.fg, 0.30)))
        } else {
            None
        }
    }

    /// The agent-mode row (`AiChatPanel.tsx:527–587`): the ask/agent
    /// segmented group, the mode hint, and the template select at the
    /// right (only while templates exist).
    fn agent_mode_row(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut group = row()
            .key("ai-agent-mode-group")
            .center_y()
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base);
        group = group.child(self.mode_button(dark, AiAgentMode::Ask));
        group = group.child(self.mode_button(dark, AiAgentMode::Agent));
        let hint_key = if state.agent_mode == AiAgentMode::Ask {
            "chat.mode.askHint"
        } else {
            "chat.mode.agentHint"
        };
        let mut row_el = row()
            .key("ai-agent-mode-row")
            .w_full()
            .min_w(0.0)
            .center_y()
            .gap(8.0)
            .px(12.0)
            .py(4.0)
            .child(group)
            .child(
                text(t(hint_key))
                    .key("ai-agent-mode-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .truncate()
                    .min_w(0.0)
                    .shrink(),
            );
        if !state.templates.is_empty() {
            // React's `ml-auto`: the spacer pushes the select to the right
            // edge (`data_table_selection_bar.rs` precedent).
            row_el = row_el.child(el_spacer());
            row_el = row_el.child(self.template_select(dark, layer));
        }
        row_el
    }

    fn mode_button(&self, dark: bool, mode: AiAgentMode) -> El {
        let k = kit(dark);
        let active = self.state.agent_mode == mode;
        let (label_key, hint_key) = match mode {
            AiAgentMode::Ask => ("chat.mode.ask", "chat.mode.askHint"),
            AiAgentMode::Agent => ("chat.mode.agent", "chat.mode.agentHint"),
        };
        let label = t(label_key).to_string();
        let label_text = label.clone();
        let hint = t(hint_key).to_string();
        let accent = k.c.text_accent;
        let muted = k.c.text_muted;
        let key = match mode {
            AiAgentMode::Ask => "ai-agent-mode-ask",
            AiAgentMode::Agent => "ai-agent-mode-agent",
        };
        let button = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(hint)
            .label_with(move |font, _ink| {
                text(label_text)
                    .font(font)
                    .color(if active { accent } else { muted })
            })
            .build(k)
            .key(key.to_string())
            // No `aria-pressed` hook exists in Rune (see the module docs).
            .focus_selected(active)
            .fill(if active {
                k.alpha(accent, 0.15)
            } else {
                Color::TRANSPARENT
            });
        wire_button_value(button, &self.on_select_agent_mode, mode)
    }

    /// The template select (`AiChatPanel.tsx:566–584`): React's `w-36`
    /// field inside the `ai-template-select` wrapper, options `''` →
    /// `chat.template.auto` plus one per template.
    fn template_select(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut labels: Vec<String> = vec![t("chat.template.auto").to_string()];
        let mut values: Vec<String> = vec![String::new()];
        for template in &state.templates {
            labels.push(template.label());
            values.push(template.id.clone());
        }
        let current_label = state
            .templates
            .iter()
            .find(|template| template.id == state.template_id)
            .map(|template| template.label())
            .unwrap_or_else(|| t("chat.template.auto").to_string());
        let mut select = Select::new("ai-template-select-field", &labels)
            .values(&values)
            // The kit trigger shows `current` verbatim as its label
            // (`rune-kit/src/select.rs`), so it takes the current
            // template's label — not its id — and the auto label while
            // no template is chosen.
            .current(Some(current_label.as_str()))
            .label(t("chat.template.label"))
            .width(144.0)
            .open(state.template_open);
        if let Some(toggle) = &self.on_toggle_template {
            let toggle = Rc::clone(toggle);
            select = select.on_toggle(move |state: &mut S| toggle(state));
        }
        if let Some(close) = &self.on_close_template {
            let close = Rc::clone(close);
            select = select.on_close(move |state: &mut S| close(state));
        }
        if let Some(choose) = &self.on_choose_template {
            let choose = Rc::clone(choose);
            select = select
                .on_choose(move |state: &mut S, value: &str, tx: &mut Tx| choose(state, value, tx));
        }
        let field = select.build(k, layer);
        row()
            .key("ai-template-select")
            .w(144.0)
            .min_w(0.0)
            .child(field)
    }

    /// The profile row (`AiChatPanel.tsx:589–614`): the shield tinted by
    /// the active profile's safety preset, the `{name || model || 'AI
    /// Model'}:` label, and the `w-44` profile select.
    fn profile_row(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let shield = match state.shield_preset() {
            AiSafetyPreset::LocalTrust => ai_shield_local_trust(dark),
            AiSafetyPreset::EnterpriseBalanced => ai_shield_enterprise_balanced(dark),
            AiSafetyPreset::CloudStrict | AiSafetyPreset::Custom => ai_shield_cloud_strict(dark),
        };
        let left = row()
            .key("ai-profile-label-group")
            .center_y()
            .gap(6.0)
            .child(
                rune::icon(&ICON_SHIELD, 14.0)
                    .color(shield)
                    .key("ai-profile-shield"),
            )
            .child(
                text(state.profile_display_label())
                    .key("ai-profile-label")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base),
            );
        row()
            .key("ai-profile-row")
            .w_full()
            .min_w(0.0)
            .center_y()
            .justify_between()
            .px(12.0)
            .py(4.0)
            .fill(k.alpha(k.c.layer_01, 0.40))
            .child(left)
            .child(self.profile_select(dark, layer))
    }

    /// The profile select: React's `w-44` field with one option per
    /// profile (`{ value: p.id, label: p.name || p.model }`). React gives
    /// it no accessible label, so neither does this one.
    fn profile_select(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let labels: Vec<String> = state
            .profiles
            .iter()
            .map(|profile| profile.display_name())
            .collect();
        let values: Vec<String> = state
            .profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect();
        let mut select = Select::new("ai-profile-select-field", &labels)
            .values(&values)
            // The kit trigger shows `current` verbatim as its label (see
            // `template_select`), so it takes the active profile's
            // display name, not its id.
            .current(
                state
                    .active_profile()
                    .map(|profile| profile.display_name())
                    .as_deref(),
            )
            .width(176.0)
            .open(state.profile_open);
        if let Some(toggle) = &self.on_toggle_profile {
            let toggle = Rc::clone(toggle);
            select = select.on_toggle(move |state: &mut S| toggle(state));
        }
        if let Some(close) = &self.on_close_profile {
            let close = Rc::clone(close);
            select = select.on_close(move |state: &mut S| close(state));
        }
        if let Some(choose) = &self.on_choose_profile {
            let choose = Rc::clone(choose);
            select = select
                .on_choose(move |state: &mut S, value: &str, tx: &mut Tx| choose(state, value, tx));
        }
        let field = select.build(k, layer);
        row()
            .key("ai-profile-select")
            .w(176.0)
            .min_w(0.0)
            .child(field)
    }

    /// The share-editor row (`AiChatPanel.tsx:616–644`): the switch with
    /// its label, the unavailable note while sharing is impossible, and
    /// the share error as a `role="alert"` span.
    fn share_editor_row(&self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let mut switch = Switch::new("ai-share-editor-toggle", state.sharing)
            .label(t("chat.shareEditor.label"))
            .disabled(!state.can_share);
        if let Some(on_toggle) = &self.on_toggle_sharing {
            let toggle = Rc::clone(on_toggle);
            switch = switch.on_toggle(move |state: &mut S| toggle(state));
        }
        let mut row_el = row()
            .key("ai-share-editor-row")
            .w_full()
            .min_w(0.0)
            .center_y()
            .flex_wrap()
            .gap(8.0)
            .px(12.0)
            .py(4.0)
            .child(switch.build(k));
        if !state.can_share {
            row_el = row_el.child(
                text(t("chat.shareEditor.unavailable"))
                    .key("ai-share-editor-unavailable")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted.alpha(0.7)),
            );
        }
        if let Some(error) = state.share_error() {
            let message = t("chat.shareEditor.error").replacen("{error}", error, 1);
            row_el = row_el.child(
                text(message)
                    .key("ai-share-editor-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .role("alert"),
            );
        }
        row_el
    }
}

/// A 1pt divider standing in for React's `border-b` (see the module docs).
fn divider(key: &str, fill: Color) -> El {
    row().key(key.to_string()).w_full().h(1.0).fill(fill)
}

/// A flexible spacer: React's `ml-auto` on the template select.
fn el_spacer() -> El {
    rune::el().key("ai-agent-mode-spacer").grow(1.0).min_w(0.0)
}

/// Wire Space (kit `on_press`) and Enter on a built button element, the way
/// S1's chip and option buttons do — kit buttons fire on Space, React's
/// `<button>` fires on Enter too.
fn wire_button<S: 'static>(el: El, action: &Option<Rc<dyn Fn(&mut S)>>) -> El {
    let Some(action) = action else {
        return el;
    };
    let press = Rc::clone(action);
    let keyed = Rc::clone(action);
    el.on_press(move |state: &mut S| press(state)).on_key(
        move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Enter {
                keyed(state);
                true
            } else {
                false
            }
        },
    )
}

/// [`wire_button`] for callbacks that take one argument (the tab, the mode).
fn wire_button_value<S: 'static, A: Clone + 'static>(
    el: El,
    action: &Option<Rc<dyn Fn(&mut S, A)>>,
    arg: A,
) -> El {
    let Some(action) = action else {
        return el;
    };
    let press = Rc::clone(action);
    let keyed = Rc::clone(action);
    let press_arg = arg.clone();
    el.on_press(move |state: &mut S| press(state, press_arg.clone()))
        .on_key(move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Enter {
                keyed(state, arg.clone());
                true
            } else {
                false
            }
        })
}
