//! T193: the `AiSessionHistory.tsx` port — the navigable conversation
//! history panel for the current connection / database.
//!
//! React's component is a `shrink-0 border-b` strip above the chat: a title
//! row with the new-session and close buttons, the migration / history
//! notices, the scrollable session list (the active row accented, each row
//! a select button plus a delete button), and the export footer with the
//! JSON and Markdown buttons. It renders nothing while closed (React's
//! `if (!open) return null`).
//!
//! # State ownership
//!
//! Like the family's other slices, the component is a pure render + event
//! router: the caller owns the store slice (the host `S`), passes its
//! projection in through [`AiSessionHistoryState`], and drives the store
//! (`ai_store_chat.rs` already owns `new_chat_session`,
//! `begin_select_session`, `begin_delete_session` and the export seams —
//! the callbacks here only report, they never touch state).
//!
//! # What maps where
//!
//! | React | here |
//! |---|---|
//! | `open` / `onClose` | [`AiSessionHistory::open`] / [`AiSessionHistory::on_close`] |
//! | `chatSessions` (`useAiStore`) | [`AiSessionHistoryState::sessions`] — the row VM list |
//! | `chatSession` (current) | [`AiSessionHistoryState::current_id`] + [`AiSessionHistoryState::current_has_messages`] |
//! | `sessionHistoryLoading` / `sessionHistoryError` | the two state fields |
//! | `sessionMigrationError` / `sessionMigrationSuccess` | the two state fields |
//! | `selectChatSession` / `newChatSession` / `deleteChatSession` | `on_select_session` / `on_new_session` / `on_delete_session` |
//! | `exportChatSession('json' \| 'markdown')` | `on_export_session` |
//! | the `useEffect` refresh-on-open | the host's concern (the panel slice owns the open flag; it refreshes when it toggles — see the module docs of `ai_chat_header.rs`) |
//!
//! The row VM ([`AiSessionHistoryItem`]) carries the three fields the
//! list reads (`id`, `title`, `preview`, `messageCount`); the store's lean
//! `ChatSessionSummary` (`{id, title}`) is the host's to widen from the
//! wire the same way it widens `ChartWidgetVm`.
//!
//! # Key convention
//!
//! React `data-testid` values verbatim: `ai-session-history`,
//! `ai-session-new`, `ai-session-migration-success`,
//! `ai-session-migration-error`, `ai-session-history-error`,
//! `ai-session-row` (`ai-session-row-{index}`, since Rune keys must be
//! unique per element), `ai-session-export-json`,
//! `ai-session-export-markdown`. Internal keys (no React testid) are
//! documented so the panel slice can find them:
//! `ai-session-history-title`, `ai-session-close`, `ai-session-row-text`,
//! `ai-session-row-preview`, `ai-session-row-delete-{index}`,
//! `ai-session-list`, `ai-session-loading`, `ai-session-empty`,
//! `ai-session-export`, `ai-session-export-row`.
//!
//! # Deliberate deviations from the React source
//!
//! * **`aria-current`** has no Rune hook (the a11y surface has no
//!   current-item relation); the active row is accented and its select
//!   button is `focus_selected` — the same channel the header's pressed
//!   buttons use.
//! * **`data-session-id`** rides the key: `ai-session-row-{index}` plus a
//!   zero-size `a11y_hidden()` key-encoded probe carrying the id (the
//!   selection bar's `data-dt-*` precedent).
//! * **`max-h-56`** (224pt) is `max_h` on the list column; the scroll is
//!   the runtime's (`overflow-y-auto` is its scroll container business).
//! * **The two ghost buttons** are kit `Button::Ghost` small — React's
//!   `h-6 px-1.5 text-[11px]` is the kit's own small scale; the danger
//!   hover (`hover:text-danger`) is a static muted ink, hover being the
//!   runtime's.
//! * **`migrationSuccess` shows only without a migration error** — both
//!   banners share React's precedence (error first, the success line
//!   only on clean migration), exactly as the header's banners do.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::icons::{ICON_FILE_JSON, ICON_FILE_TEXT, ICON_PLUS, ICON_TRASH, ICON_X};
use crate::i18n::t;
use crate::ui::kit::kit;

/// React `max-h-56` on the list.
const LIST_MAX_H: f32 = 224.0;
/// React `gap-1` between a row's children.
const ROW_GAP: f32 = 4.0;
/// React `gap-2` in the title row.
const TITLE_GAP: f32 = 8.0;
/// React `h-3 w-3` icons.
const ICON_SIZE: f32 = 12.0;

/// One history row — the `AiSessionSummary` slice the list reads
/// (`title` / `preview` / `messageCount`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiSessionHistoryItem {
    pub id: String,
    /// React `title` — the saved name.
    pub title: String,
    /// React `preview` — the first-message excerpt.
    pub preview: String,
    /// React `messageCount`.
    pub message_count: u32,
}

impl AiSessionHistoryItem {
    /// The primary line: `title || preview || t('chat.sessions.untitled')`.
    pub fn primary_label(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        if !self.preview.is_empty() {
            return self.preview.clone();
        }
        t("chat.sessions.untitled").to_owned()
    }

    /// The secondary line: `{count} 条消息` (+ ` · preview` when both a
    /// title and a preview exist, React's `preview ? ... : ''`).
    pub fn secondary_label(&self) -> String {
        let mut line = t("chat.sessions.messages")
            .replacen("{count}", &self.message_count.to_string(), 1)
            .to_string();
        if !self.preview.is_empty() && !self.title.is_empty() {
            line.push_str(" · ");
            line.push_str(&self.preview);
        }
        line
    }
}

/// The store slice's projection the panel renders.
#[derive(Clone, Debug, Default)]
pub struct AiSessionHistoryState {
    /// React `chatSessions`.
    pub sessions: Vec<AiSessionHistoryItem>,
    /// The current session's id (`chatSession?.id`); `None` = no session.
    pub current_id: Option<String>,
    /// Whether the current session has messages (`!current?.messages.length`
    /// gates the export buttons).
    pub current_has_messages: bool,
    /// React `sessionHistoryLoading`.
    pub loading: bool,
    /// React `sessionHistoryError`.
    pub history_error: Option<String>,
    /// React `sessionMigrationError`.
    pub migration_error: Option<String>,
    /// React `sessionMigrationSuccess` (count).
    pub migration_success: u32,
}

impl AiSessionHistoryState {
    /// The session row that is active, if any.
    pub fn active_index(&self) -> Option<usize> {
        self.current_id
            .as_ref()
            .and_then(|current| self.sessions.iter().position(|s| &s.id == current))
    }
}

type CloseFn<S> = Rc<dyn Fn(&mut S)>;
type SessionFn<S> = Rc<dyn Fn(&mut S, &str)>;
type ExportFn<S> = Rc<dyn Fn(&mut S, AiSessionExportFormat)>;

/// React `exportChatSession(format)`'s two formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiSessionExportFormat {
    Json,
    Markdown,
}

impl AiSessionExportFormat {
    /// The button label (React's literal `JSON` / `Markdown`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }
}

/// The history panel. Build with the state's projection and the callbacks;
/// `build` returns `None` while closed (React's `null`).
pub struct AiSessionHistory<S: 'static> {
    state: AiSessionHistoryState,
    open: bool,
    on_close: Option<CloseFn<S>>,
    on_new_session: Option<CloseFn<S>>,
    on_select_session: Option<SessionFn<S>>,
    on_delete_session: Option<SessionFn<S>>,
    on_export_session: Option<ExportFn<S>>,
}

impl<S: 'static> AiSessionHistory<S> {
    /// The panel over the caller's state slice.
    pub fn new(state: AiSessionHistoryState) -> Self {
        AiSessionHistory {
            state,
            open: false,
            on_close: None,
            on_new_session: None,
            on_select_session: None,
            on_delete_session: None,
            on_export_session: None,
        }
    }

    /// React `open`.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// React `onClose` — the title row's X.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// React `newChatSession` (the button also closes the panel).
    pub fn on_new_session(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_new_session = Some(Rc::new(f));
        self
    }

    /// React `selectChatSession(id)` (the row press also closes).
    pub fn on_select_session(
        mut self,
        f: impl Fn(&mut S, &str) + 'static,
    ) -> Self {
        self.on_select_session = Some(Rc::new(f));
        self
    }

    /// React `deleteChatSession(id)`.
    pub fn on_delete_session(
        mut self,
        f: impl Fn(&mut S, &str) + 'static,
    ) -> Self {
        self.on_delete_session = Some(Rc::new(f));
        self
    }

    /// React `exportChatSession(format)`.
    pub fn on_export_session(
        mut self,
        f: impl Fn(&mut S, AiSessionExportFormat) + 'static,
    ) -> Self {
        self.on_export_session = Some(Rc::new(f));
        self
    }

    /// Builds the strip — `None` while closed (React's `null` return).
    pub fn build(self, dark: bool) -> Option<El> {
        if !self.open {
            return None;
        }
        let k = kit(dark);
        let mut panel = col()
            .key("ai-session-history")
            .w_full()
            .min_w(0.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
            .fill(k.alpha(k.c.surface_hover, 0.4));

        // ── the title row ──
        let mut title_row = row()
            .key("ai-session-history-title-row")
            .items_center()
            .gap(TITLE_GAP)
            .px(12.0)
            .py(6.0)
            .child(
                text(t("chat.sessions.title"))
                    .key("ai-session-history-title")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        // `ml-auto`: the spacer pushes the buttons right.
        title_row = title_row.child(el().key("ai-session-title-spacer").grow(1.0).min_w(0.0));
        title_row = title_row.child(self.new_button(&k));
        title_row = title_row.child(self.close_button(&k));
        panel = panel.child(title_row);

        // ── the notices (error first, success only without error) ──
        if let Some(error) = &self.state.migration_error {
            panel = panel.child(notice(
                "ai-session-migration-error",
                error.clone(),
                &k,
                k.c.danger.fg,
            ));
        } else if self.state.migration_success > 0 {
            let message = t("chat.sessions.migrationSuccess").replacen(
                "{count}",
                &self.state.migration_success.to_string(),
                1,
            );
            panel = panel.child(notice(
                "ai-session-migration-success",
                message,
                &k,
                k.c.success.fg,
            ));
        }
        if let Some(error) = &self.state.history_error {
            panel = panel.child(notice(
                "ai-session-history-error",
                error.clone(),
                &k,
                k.c.danger.fg,
            ));
        }

        // ── the list ──
        let mut list = col()
            .key("ai-session-list")
            .max_h(LIST_MAX_H)
            .min_h(0.0)
            .px(8.0)
            .pb(4.0);
        if self.state.sessions.is_empty() {
            // Loading wins over empty exactly as React's two guards order
            // (loading + empty → the loading line; loaded + empty → the
            // empty line).
            let (key, label_key) = if self.state.loading {
                ("ai-session-loading", "chat.sessions.loading")
            } else {
                ("ai-session-empty", "chat.sessions.empty")
            };
            list = list.child(
                text(t(label_key))
                    .key(key)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .px(4.0)
                    .py(8.0),
            );
        } else {
            for (index, session) in self.state.sessions.iter().enumerate() {
                list = list.child(self.session_row(index, session, &k));
            }
        }
        panel = panel.child(list);

        // ── the export footer ──
        let mut footer = row()
            .key("ai-session-export-row")
            .items_center()
            .gap(ROW_GAP)
            .px(12.0)
            .py(4.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.alpha(k.c.border_weak, 0.6))
            .child(
                text(t("chat.sessions.export"))
                    .key("ai-session-export")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(el().key("ai-session-export-spacer").grow(1.0).min_w(0.0));
        footer = footer.child(self.export_button(&k, AiSessionExportFormat::Json));
        footer = footer.child(self.export_button(&k, AiSessionExportFormat::Markdown));
        panel = panel.child(footer);

        Some(panel)
    }

    /// React's "new session" button (a press starts the session and closes).
    fn new_button(&self, k: &rune_kit::Kit) -> El {
        let mut button = Button::new(t("chat.sessions.new"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(&ICON_PLUS);
        if let Some(start_new) = self.on_new_session.clone() {
            let close = self.on_close.clone();
            button = button.on_press(move |s: &mut S| {
                start_new(s);
                if let Some(close) = close.clone() {
                    close(s);
                }
            });
        }
        button.build(k).key("ai-session-new")
    }

    /// React's X button (the close, `aria-label` on the button).
    fn close_button(&self, k: &rune_kit::Kit) -> El {
        let label = t("chat.sessions.close").to_owned();
        let mut button = Button::new(label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(label);
        if let Some(close) = self.on_close.clone() {
            button = button.on_press(move |s: &mut S| close(s));
        }
        button
            .build(k)
            .key("ai-session-close")
            .child(icon(&ICON_X, ICON_SIZE).key("ai-session-close-icon").color(k.c.text_muted).a11y_hidden())
    }

    /// One session row: the select button (two lines), the delete button,
    /// the active accent.
    fn session_row(&self, index: usize, session: &AiSessionHistoryItem, k: &rune_kit::Kit) -> El {
        let active = self.state.active_index() == Some(index);
        let mut row_el = row()
            .key(format!("ai-session-row-{index}"))
            .items_center()
            .gap(ROW_GAP)
            .radius(k.radius.sm)
            .px(6.0)
            .py(4.0)
            .fill(if active {
                k.alpha(k.c.text_accent, 0.1)
            } else {
                rune::Color::TRANSPARENT
            })
            .child(el().key(format!("ai-session-row-{index}-id-{}", session.id)).a11y_hidden());

        // The select button: the primary + secondary lines. React's press
        // selects the session and closes the panel.
        let on_select = self.on_select_session.clone();
        let on_close = self.on_close.clone();
        let session_id = session.id.clone();
        let mut select = col()
            .role("button")
            .key(format!("ai-session-row-{index}-select"))
            .min_w(0.0)
            .grow(1.0)
            .focusable(true)
            .on_press(move |s: &mut S| {
                if let Some(select) = &on_select {
                    select(s, &session_id);
                }
                if let Some(close) = &on_close {
                    close(s);
                }
            });

        let primary = text(session.primary_label())
            .key(format!("ai-session-row-{index}-text"))
            .font(k.ty.text_xsmall)
            .color(if active {
                k.c.text_accent
            } else {
                k.c.text_muted
            })
            .truncate();
        let secondary = text(session.secondary_label())
            .key(format!("ai-session-row-{index}-preview"))
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted)
            .truncate();
        select = select.child(primary).child(secondary);
        // The selected channel for the active row (no `aria-current` hook).
        select = select.focus_selected(active);
        row_el = row_el.child(select);

        // The delete button.
        let delete_label = t("chat.sessions.delete").to_owned();
        let mut delete = Button::new(delete_label.clone())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(delete_label)
            .icon(&ICON_TRASH);
        if let Some(delete_session) = self.on_delete_session.clone() {
            let session_id = session.id.clone();
            delete = delete.on_press(move |s: &mut S| delete_session(s, &session_id));
        }
        row_el = row_el.child(
            delete
                .build(k)
                .key(format!("ai-session-row-{index}-delete"))
                .child(
                    icon(&ICON_TRASH, ICON_SIZE)
                        .key(format!("ai-session-row-{index}-delete-icon"))
                        .color(k.c.text_muted)
                        .a11y_hidden(),
                ),
        );

        row_el
    }

    /// One export footer button (disabled until the current session has
    /// messages).
    fn export_button(&self, k: &rune_kit::Kit, format: AiSessionExportFormat) -> El {
        let key = match format {
            AiSessionExportFormat::Json => "ai-session-export-json",
            AiSessionExportFormat::Markdown => "ai-session-export-markdown",
        };
        let icon_body = match format {
            AiSessionExportFormat::Json => &ICON_FILE_JSON,
            AiSessionExportFormat::Markdown => &ICON_FILE_TEXT,
        };
        let mut button = Button::new(format.label())
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(icon_body)
            .disabled(!self.state.current_has_messages);
        if let Some(export) = self.on_export_session.clone() {
            button = button.on_press(move |s: &mut S| export(s, format));
        }
        button.build(k).key(key)
    }
}

/// One notice line — React's `mx-3 mb-1 rounded border border-{tone}/30
/// bg-{tone}/5 px-2 py-1 text-[10px] text-{tone}`.
fn notice(key: &str, message: String, k: &rune_kit::Kit, tone: rune::Color) -> El {
    let _ = k;
    let kit_colors = k;
    row()
        .key(key)
        .mx(12.0)
        .mb(4.0)
        .radius(kit_colors.radius.sm)
        .border_sides([1.0, 1.0, 1.0, 1.0], kit_colors.alpha(tone, 0.3))
        .fill(kit_colors.alpha(tone, 0.05))
        .px(8.0)
        .py(4.0)
        .child(
            text(message)
                .key(format!("{key}-text"))
                .font(k.ty.text_xsmall)
                .color(tone)
                .wrap(),
        )
}
