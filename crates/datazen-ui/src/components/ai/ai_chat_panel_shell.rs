//! `AiChatPanel` shell — T009-S6, the sixth slice of T009.
//!
//! Ported from `packages/frontend/src/components/ai/AiChatPanel.tsx`
//! 405–949 (the render half; the 93–404 state half lives in
//! [`super::ai_chat_panel_state::AiChatPanelState`]): the panel root,
//! the unconfigured early return, the tab switch, and the assembly of
//! the S3 header, S4 message list and S5 input area into one panel.
//! The panel itself is still a view-model placeholder for the `aiStore`
//! slice (T088, not done); see the state module's docs for the
//! boundary.
//!
//! | React source | Rune item |
//! |---|---|
//! | `!isConfigured` early return (385–403) | [`AiChatPanel::unconfigured`] |
//! | panel root `ai-chat-panel` (406) | [`AiChatPanel::build`] |
//! | header + tabs (405–644) | S3 [`AiChatHeader`] |
//! | `AiSessionHistory` mount (490–492) | `ai-session-history` shell (T195) |
//! | workflows tab (647–651) | `ai-chat-workflows` + `ai-workflows-panel` shell (T334) |
//! | chat tab: toasts + messages (642–790) | S4 [`AiChatMessageList`] |
//! | chat tab: input area (777–949) | S5 [`AiChatInputArea`] |
//!
//! # Wiring
//!
//! The shell owns no state of its own. It takes `&AiChatPanelState`
//! plus one `panel_mut` accessor — `|state| &mut state.panel` — and
//! wires every child callback through it, so a press lands in the
//! state machine instead of in the test probe. The three host-owned
//! props (`onInsertSql` / `onRunCode` / `onNewQuery`) and the two
//! window openers (`openDocsWindow` for the help buttons,
//! `openSettingsWindow('ai')` for the unconfigured button) stay
//! pass-through callbacks, exactly as the S3 header reports its help
//! and clear presses instead of owning them. With no accessor the
//! children render unwired, the same as every S1–S5 component with no
//! callbacks.
//!
//! The header's selects push their listboxes into `layer`, so `build`
//! takes the layer the caller renders alongside the panel — the
//! `rune_kit::root(col, layer)` shape the S3 tests already use.
//!
//! # Key convention
//!
//! Keys are React's `data-testid` values verbatim where React has one:
//! `ai-chat-panel`, `ai-not-configured`. Internal keys (no React
//! testid) are documented here so the S7 `Section` wiring can find
//! them: `ai-not-configured-icon`, `ai-not-configured-text`,
//! `ai-not-configured-settings`, `ai-session-history`,
//! `ai-chat-workflows`, `ai-workflows-panel`. Every other key comes
//! from the S1–S5 slices.
//!
//! # Deliberate boundaries
//!
//! * `AiSessionHistory` (T195) is not ported: while the history toggle
//!   is open the shell renders an empty keyed `ai-session-history`
//!   shell — the mount point T195 fills — and nothing while it is
//!   closed, matching React's closed drawer rendering nothing.
//! * `WorkflowPanel` (T334) is not ported: the workflows tab renders
//!   React's scrollable wrapper (`flex-1 overflow-y-auto px-3 py-2`)
//!   around an empty keyed `ai-workflows-panel` shell. The panel's
//!   `dbSessionId` prop is already on the state for T334 to read.
//! * The unconfigured button is the kit `Contrast` variant — the
//!   variant S1 used for the primary action, since kit has no
//!   `Primary` — and React's `h-7` button renders at the kit's small
//!   control size.
//! * The jump-to-bottom pill is stacked under the viewport by S4, not
//!   absolutely positioned over it; the shell inherits that.

// The panel view (S7) is the production consumer. The dead-code lint
// stays silenced here rather than in a crate-wide allow: the
// T088/T169-owned surface (store seeders, on_run_code / on_new_query
// pass-throughs) still has no production caller.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, text, El, Key, KeyEvent, ScrollSpec, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::ai_chat_header::{AiChatHeader, AiChatTab};
use super::ai_chat_input_area::AiChatInputArea;
use super::ai_chat_message_list::AiChatMessageList;
use super::ai_chat_panel_state::AiChatPanelState;
use crate::i18n::t;
use crate::icons::{ICON_SETTINGS, ICON_SPARKLES};
use crate::ui::kit::kit;

/// `AiChatPanel`'s shell (`AiChatPanel.tsx:385–949`): the unconfigured
/// early return, the tab switch, and the S3 + S4 + S5 assembly. The
/// caller owns the [`AiChatPanelState`] and passes `&state` each frame;
/// presses call back through the `panel_mut` accessor into the state
/// machine.
pub(crate) struct AiChatPanel<'a, S: 'static> {
    state: &'a AiChatPanelState,
    panel: Option<Rc<dyn Fn(&mut S) -> &mut AiChatPanelState>>,
    on_insert_sql: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_run_code: Option<Rc<dyn Fn(&mut S, &str, &str)>>,
    on_new_query: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_open_ai_help: Option<Rc<dyn Fn(&mut S)>>,
    on_open_workflow_help: Option<Rc<dyn Fn(&mut S)>>,
    on_open_settings: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> AiChatPanel<'a, S> {
    pub(crate) fn new(state: &'a AiChatPanelState) -> Self {
        Self {
            state,
            panel: None,
            on_insert_sql: None,
            on_run_code: None,
            on_new_query: None,
            on_open_ai_help: None,
            on_open_workflow_help: None,
            on_open_settings: None,
        }
    }

    /// The state accessor every child callback drives, e.g.
    /// `|state| &mut state.panel`. One accessor wires the whole panel;
    /// the host-owned props stay separate pass-through callbacks below.
    pub(crate) fn panel_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut AiChatPanelState + 'static,
    ) -> Self {
        self.panel = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onInsertSql` prop (T169). Stored only
    /// until that slice lands; passed through to the message list.
    pub(crate) fn on_insert_sql(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_insert_sql = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onRunCode` prop (T169). Stored only.
    pub(crate) fn on_run_code(mut self, f: impl Fn(&mut S, &str, &str) + 'static) -> Self {
        self.on_run_code = Some(Rc::new(f));
        self
    }

    /// `AiMessageContent`'s `onNewQuery` prop (T169). Stored only.
    pub(crate) fn on_new_query(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_new_query = Some(Rc::new(f));
        self
    }

    /// Called when the AI help button is pressed; the caller owns
    /// `openDocsWindow('context')`.
    pub(crate) fn on_open_ai_help(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_ai_help = Some(Rc::new(f));
        self
    }

    /// Called when the workflow help button is pressed; the caller
    /// owns `openDocsWindow('workflows')`.
    pub(crate) fn on_open_workflow_help(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_workflow_help = Some(Rc::new(f));
        self
    }

    /// Called when the unconfigured view's settings button is pressed;
    /// the caller owns `openSettingsWindow('ai')`.
    pub(crate) fn on_open_settings(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_settings = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        if !self.state.is_configured() {
            return self.unconfigured(dark);
        }
        let mut root = col()
            .key("ai-chat-panel")
            .w_full()
            .h_full()
            .min_w(0.0)
            .min_h(0.0);
        root = root.child(self.header(dark, layer));
        if self.state.header().tab() == AiChatTab::Chat
            && self.state.header().session_history_open()
        {
            // T195's mount point: an empty keyed shell while open,
            // nothing while closed (see the module docs).
            root = root.child(col().key("ai-session-history").w_full().min_w(0.0));
        }
        if self.state.header().tab() == AiChatTab::Workflows {
            root = root.child(self.workflows());
        } else {
            root = root.child(self.message_list(dark));
            root = root.child(self.input_area(dark));
        }
        root
    }

    /// The `!isConfigured` early return (`AiChatPanel.tsx:385–403`):
    /// a centred Sparkles glyph, the `common.aiNotConfigured` line,
    /// and the `settings.ai.goToConfigure` button.
    fn unconfigured(&self, dark: bool) -> El {
        let k = kit(dark);
        let label = t("settings.ai.goToConfigure").to_string();
        let mut button = Button::new(label.clone())
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .icon(&ICON_SETTINGS)
            .help(label)
            .build(k)
            .key("ai-not-configured-settings");
        if let Some(action) = &self.on_open_settings {
            let press = Rc::clone(action);
            let keyed = Rc::clone(action);
            button = button.on_press(move |state: &mut S| press(state)).on_key(
                move |state: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    if event.key == Key::Enter {
                        keyed(state);
                        true
                    } else {
                        false
                    }
                },
            );
        }
        col()
            .key("ai-not-configured")
            .w_full()
            .h_full()
            .min_w(0.0)
            .min_h(0.0)
            .center()
            .padding(16.0)
            .gap(8.0)
            .child(
                rune::icon(&ICON_SPARKLES, 20.0)
                    .color(k.c.text_muted)
                    .key("ai-not-configured-icon"),
            )
            .child(
                text(t("common.aiNotConfigured"))
                    .key("ai-not-configured-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(button)
    }

    /// The workflows tab (`AiChatPanel.tsx:647–651`): React's
    /// scrollable wrapper around the `WorkflowPanel` mount. T334 owns
    /// the panel itself; this shell is the mount point.
    fn workflows(&self) -> El {
        col()
            .key("ai-chat-workflows")
            .w_full()
            .min_w(0.0)
            .grow(1.0)
            .min_h(0.0)
            .px(12.0)
            .py(8.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(col().key("ai-workflows-panel").w_full().min_w(0.0))
    }

    fn header(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let mut header = AiChatHeader::new(self.state.header());
        if let Some(access) = &self.panel {
            let tab_access = Rc::clone(access);
            header = header.on_select_tab(move |state: &mut S, tab| {
                tab_access(state).header_mut().set_tab(tab);
            });
            let history = Rc::clone(access);
            header = header.on_toggle_session_history(move |state: &mut S| {
                history(state).header_mut().toggle_session_history();
            });
            let clear = Rc::clone(access);
            header = header.on_clear_chat(move |state: &mut S| {
                clear(state).clear_chat();
            });
            let mode_access = Rc::clone(access);
            header = header.on_select_agent_mode(move |state: &mut S, mode| {
                mode_access(state).header_mut().set_agent_mode(mode);
            });
            let toggle_template = Rc::clone(access);
            header = header.on_toggle_template(move |state: &mut S| {
                toggle_template(state).header_mut().toggle_template_open();
            });
            let close_template = Rc::clone(access);
            header = header.on_close_template(move |state: &mut S| {
                close_template(state).header_mut().close_template_select();
            });
            let choose_template = Rc::clone(access);
            header = header.on_choose_template(move |state: &mut S, id: &str, _tx: &mut Tx| {
                choose_template(state).header_mut().choose_template(id);
            });
            let toggle_profile = Rc::clone(access);
            header = header.on_toggle_profile(move |state: &mut S| {
                toggle_profile(state).header_mut().toggle_profile_open();
            });
            let close_profile = Rc::clone(access);
            header = header.on_close_profile(move |state: &mut S| {
                close_profile(state).header_mut().close_profile_select();
            });
            let choose_profile = Rc::clone(access);
            header = header.on_choose_profile(move |state: &mut S, id: &str, _tx: &mut Tx| {
                choose_profile(state).header_mut().choose_profile(id);
            });
            let sharing = Rc::clone(access);
            header = header.on_toggle_sharing(move |state: &mut S| {
                let panel = sharing(state);
                let next = !panel.header().sharing();
                panel.header_mut().set_sharing(next);
            });
        }
        if let Some(action) = &self.on_open_ai_help {
            let press = Rc::clone(action);
            header = header.on_open_ai_help(move |state: &mut S| press(state));
        }
        if let Some(action) = &self.on_open_workflow_help {
            let press = Rc::clone(action);
            header = header.on_open_workflow_help(move |state: &mut S| press(state));
        }
        header.build(dark, layer)
    }

    fn message_list(&self, dark: bool) -> El {
        let mut list = AiChatMessageList::new(self.state.list());
        if let Some(dialect) = self.state.sql_dialect() {
            list = list.sql_dialect(dialect.to_string());
        }
        if let Some(action) = &self.on_insert_sql {
            let f = Rc::clone(action);
            list = list.on_insert_sql(move |state: &mut S, sql: &str| f(state, sql));
        }
        if let Some(action) = &self.on_run_code {
            let f = Rc::clone(action);
            list = list.on_run_code(move |state: &mut S, sql: &str, dialect: &str| {
                f(state, sql, dialect);
            });
        }
        if let Some(action) = &self.on_new_query {
            let f = Rc::clone(action);
            list = list.on_new_query(move |state: &mut S, sql: &str| f(state, sql));
        }
        if let Some(access) = &self.panel {
            let reasoning = Rc::clone(access);
            list = list.on_toggle_reasoning(move |state: &mut S, index: usize| {
                reasoning(state)
                    .list_mut()
                    .bubble_state_mut(index)
                    .toggle_reasoning();
            });
            let tool = Rc::clone(access);
            list = list.on_toggle_tool(move |state: &mut S, index: usize| {
                tool(state).list_mut().bubble_state_mut(index).toggle_tool();
            });
            let memory = Rc::clone(access);
            list = list.on_toggle_memory(move |state: &mut S, index: usize| {
                memory(state)
                    .list_mut()
                    .bubble_state_mut(index)
                    .toggle_memory();
            });
            let cite = Rc::clone(access);
            list = list.on_cite_memory(move |state: &mut S, index: usize, reference: &str| {
                cite(state)
                    .list_mut()
                    .bubble_state_mut(index)
                    .open_memory_citation(reference);
            });
            let remember = Rc::clone(access);
            list = list.on_remember(move |state: &mut S, message| {
                remember(state).handle_remember(message);
            });
            let toggle_question = Rc::clone(access);
            list = list.on_toggle_question(
                move |state: &mut S, index: usize, qid: &str, oid: &str, multi: bool| {
                    toggle_question(state)
                        .list_mut()
                        .question_state_mut(index)
                        .toggle(qid, oid, multi);
                },
            );
            let custom_edit = Rc::clone(access);
            list = list.on_custom_edit(move |state: &mut S, index: usize, qid: &str| {
                custom_edit(state)
                    .list_mut()
                    .question_state_mut(index)
                    .custom_edit(qid)
            });
            let submit_questions = Rc::clone(access);
            list = list.on_submit_questions(move |state: &mut S, index: usize| {
                submit_questions(state).send_answers(index);
            });
            let dismiss_candidate = Rc::clone(access);
            list = list.on_dismiss_memory_candidate(move |state: &mut S| {
                dismiss_candidate(state).dismiss_memory_candidate();
            });
            let dismiss_remember = Rc::clone(access);
            list = list.on_dismiss_remember_notice(move |state: &mut S| {
                dismiss_remember(state).dismiss_remember_notice();
            });
            let jump = Rc::clone(access);
            list = list.on_jump_to_bottom(move |state: &mut S, tx: &mut Tx| {
                jump(state).list_mut().jump_to_bottom();
                tx.scroll_to_bottom("ai-chat-message-list-viewport");
            });
            let at_bottom = Rc::clone(access);
            list = list.on_at_bottom(move |state: &mut S, at: bool| {
                at_bottom(state).list_mut().set_at_bottom(at);
            });
        }
        list.build(dark)
    }

    fn input_area(&self, dark: bool) -> El {
        let mut area = AiChatInputArea::new(self.state.area());
        if let Some(access) = &self.panel {
            let edit = Rc::clone(access);
            area = area.on_input_edit(move |state: &mut S| edit(state).area_mut().input_edit());
            let submit = Rc::clone(access);
            area = area.on_submit(move |state: &mut S| {
                submit(state).send();
            });
            let stop = Rc::clone(access);
            area = area.on_stop(move |state: &mut S| {
                stop(state).stop();
            });
            let remove = Rc::clone(access);
            area = area.on_remove_snapshot_field(move |state: &mut S, field: &str| {
                remove(state).area_mut().remove_snapshot_field(field);
            });
            let resolve = Rc::clone(access);
            area = area.on_resolve_conflict(move |state: &mut S, strategy| {
                resolve(state).resolve_conflict(strategy);
            });
            let dismiss = Rc::clone(access);
            area = area.on_dismiss_conflict(move |state: &mut S| {
                dismiss(state).dismiss_conflict();
            });
        }
        area.build(dark)
    }
}
