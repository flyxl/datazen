//! The NL2SQL panel, ported from React
//! `packages/frontend/src/components/ai/Nl2SqlPanel.tsx` (T280).
//!
//! The natural-language-to-SQL composer under the SQL editor: the
//! unconfigured notice, the AI input row with its generate/stop/copy/
//! clear buttons, the SQL preview area, the used-memory chip with its
//! citations and expanded list, and the error bar.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useAiStore((s) => s.nl2sql / nl2sqlError / isConfigured)` | the VM fields (the host copies the store slice) |
//! | `memoryMode ?? memoryModeFor(settingsConfig?.memory)` | the host's effective mode — a prop override or the settings fallback, computed before the VM (the AiChatPanel state's precedent) |
//! | the SQL write effect (70-76) | [`sql_write_action`] — a pure query the host applies through its `onSqlChange` channel when the nl2sql slice changes |
//! | `handleGenerate` (78-104) | [`can_generate`] + the [`Nl2SqlPanel::on_generate`] seam — the host builds the request (with [`split_context_items`], the memory mode and scope) and drives the store |
//! | `handleStop` (106-110) | [`Nl2SqlPanel::on_stop`] — the cancel with the request id |
//! | `handleCopySql` (112-116) | [`Nl2SqlPanel::on_copy`] — the host owns the copied cell (`COPIED_FEEDBACK_MS` = 2000, React 28) |
//! | `clearNl2Sql` | [`Nl2SqlPanel::on_clear`] |
//! | `openSettingsWindow('ai')` (141) | [`Nl2SqlPanel::on_open_ai_settings`] — the ExplainPanel's seam precedent |
//! | `setContextItems` (92, 165) | the input state's own items (T137's contract) — the generate seam clears them, the host's half |
//! | `focusMemory` (122-131) | the host's focused-reference cell (the chat bubble's `memory_focused` precedent); the `scrollIntoView`/`focus` half is the host's timer seam |
//! | `AiInput` (156-170) | the ported [`AiInput`] over the host-held `AiInputState`, `hideSubmit` (169), the select-database-first placeholder (161) |
//! | `AiEgressNotice` (153) | the ported notice over the input's context items |
//! | `SqlCodeBlock` (222) | the ported block over a host-held `SqlCodeBlockState` |
//! | the memory chip / citations / expanded list (227-277) | the chat bubble's own memory-section precedent, with the panel's own citation buttons and `ai-memory-citation-{reference}` keys |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `nl2sql-panel` |
//! | the unconfigured bar (internal) | `nl2sql-panel-not-configured` |
//! | the settings button (internal) | `nl2sql-panel-open-settings` |
//! | the generate button (internal) | `nl2sql-generate` |
//! | the stop button (internal) | `nl2sql-stop` |
//! | the copy button (internal) | `nl2sql-copy` |
//! | the clear button (internal) | `nl2sql-clear` |
//! | the preview card (internal) | `nl2sql-preview-card` |
//! | the preview header label (internal) | `nl2sql-preview-label` |
//! | the preview spinner (internal) | `nl2sql-preview-spinner` |
//! | the memory card (internal) | `nl2sql-memory-card` |
//! | the memory chip (React test id) | `ai-memory-chip` |
//! | one citation (React test id) | `ai-memory-citation-{reference}` |
//! | the expanded list (React test id) | `ai-memory-expanded` |
//! | one expanded row (React test id) | `ai-memory-item-{reference}` |
//! | the error bar (internal) | `nl2sql-error` |
//!
//! ## Documented deviations from React
//!
//! * **The effect is a pure query.** The SQL write effect fires when
//!   generation finishes with a new SQL string; a render pass cannot
//!   write host state, so [`sql_write_action`] returns the write the
//!   host performs (and updates its `last_written` cell).
//! * **The copy feedback is the host's cell**, driven by
//!   `COPIED_FEEDBACK_MS` = 2000 (this panel's own constant; the
//!   query error panel's is 1500 — each component's React value).
//! * **`select-text` on the error bar** is the runtime's default text
//!   selection channel; no port half exists.
//! * **The citation rows are mono 10pt buttons** — the same shape the
//!   expanded rows' references use.

use std::rc::Rc;

use rune::{col, row, text, Color, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::ai::ai_egress_notice::{AiEgressNotice, ContextItem, ContextKind};
use crate::components::ai::ai_input::{AiInput, AiInputState};
use crate::components::ai::chat_bubble::AiMemoryReference;
use crate::components::sql_code_block::{SqlCodeBlock, SqlCodeBlockState};
use crate::i18n::t;
use crate::icons::{
    ICON_BOOK_OPEN, ICON_CHECK, ICON_COPY, ICON_SPARKLES, ICON_SQUARE, ICON_TRASH,
};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

/// How long the copy button keeps its check icon before reverting
/// (React 28).
pub const COPIED_FEEDBACK_MS: u64 = 2000;

/// The panel's data: the store slice (the host copies it), the
/// configured flag, the copy feedback, and the memory list's open and
/// focused cells.
#[derive(Clone, Debug, Default)]
pub struct Nl2SqlPanelVm {
    /// `nl2sql.input`.
    pub input: String,
    /// `nl2sql.isGenerating`.
    pub is_generating: bool,
    /// `nl2sql.streamingPreview` — the preview while generating.
    pub streaming_preview: String,
    /// `nl2sql.generatedSql`.
    pub generated_sql: String,
    /// `nl2sql.memoryUsed ?? []`.
    pub memory_used: Vec<AiMemoryReference>,
    /// `nl2sql.requestId` — the stop button's target.
    pub request_id: Option<String>,
    /// `nl2sqlError`.
    pub error: Option<String>,
    /// `isConfigured`.
    pub is_configured: bool,
    /// `copied` — the copy feedback cell (`useCopyFeedback`).
    pub copied: bool,
    /// `memoryOpen` — the expanded list's `useState`.
    pub memory_open: bool,
    /// The focused memory reference (`focusMemory`'s cell).
    pub memory_focused: Option<String>,
}

/// React 79's guard: `!nl2sql.input.trim() || nl2sql.isGenerating ||
/// !database` refuses the generate.
pub fn can_generate(input: &str, is_generating: bool, database: &str) -> bool {
    !input.trim().is_empty() && !is_generating && !database.is_empty()
}

/// The SQL write effect (React 70-76) as a pure query: the SQL the host
/// writes into the editor — `Some(sql)` when generation finished with a
/// generated SQL that differs from the last written one, `None`
/// otherwise (still generating, no SQL yet, or already written).
/// The host updates its `last_written` cell when it performs the write.
pub fn sql_write_action(
    is_generating: bool,
    generated_sql: &str,
    last_written: &str,
) -> Option<String> {
    if is_generating || generated_sql.is_empty() || generated_sql == last_written {
        return None;
    }
    Some(generated_sql.to_owned())
}

/// `splitContextItems` (`lib/contextItems.ts:3-14`): the files and the
/// tables of a context list — tables by id, files and dirs by path
/// (falling back to the id). The chat panel's host keeps its own
/// request construction; this port of the panel owns the named helper
/// the React source shares.
pub fn split_context_items(items: &[ContextItem]) -> (Vec<String>, Vec<String>) {
    let mut context_files: Vec<String> = Vec::new();
    let mut context_tables: Vec<String> = Vec::new();
    for item in items {
        match item.kind {
            ContextKind::Table => {
                context_tables.push(item.id.clone());
            }
            ContextKind::File | ContextKind::Dir => {
                // React reads `it.path ?? it.id`; the minimal ContextItem
                // mirror the egress notice's port ruled on carries no
                // path, so the id is the fallback the source itself
                // defines.
                context_files.push(item.id.clone());
            }
            _ => {}
        }
    }
    (context_files, context_tables)
}

/// React 118-120: what SQL the preview shows — the streaming preview
/// while generating, the generated SQL otherwise; `None` when there
/// is nothing to show (`showPreview`'s own two disjuncts folded in:
/// generating OR a generated SQL).
pub fn preview_sql(vm: &Nl2SqlPanelVm) -> Option<String> {
    if vm.is_generating {
        if vm.streaming_preview.is_empty() {
            return None;
        }
        Some(vm.streaming_preview.clone())
    } else if vm.generated_sql.is_empty() {
        None
    } else {
        Some(vm.generated_sql.clone())
    }
}

/// The NL2SQL panel. `S` is the host state that owns the input state,
/// the SQL code block's state, and every action.
pub struct Nl2SqlPanel<'a, S: 'static> {
    vm: &'a Nl2SqlPanelVm,
    input: &'a AiInputState,
    /// The host's database prop (the empty string is the
    /// not-selected state).
    database: String,
    dark: bool,
    on_generate: Option<Rc<dyn Fn(&mut S)>>,
    on_stop: Option<Rc<dyn Fn(&mut S)>>,
    on_copy: Option<Rc<dyn Fn(&mut S)>>,
    on_clear: Option<Rc<dyn Fn(&mut S)>>,
    on_open_ai_settings: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_memory: Option<Rc<dyn Fn(&mut S)>>,
    on_focus_memory: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<'a, S: 'static> Nl2SqlPanel<'a, S> {
    /// The panel over its VM, the shared input state, and the database
    /// prop.
    pub fn new(vm: &'a Nl2SqlPanelVm, input: &'a AiInputState, database: &str) -> Self {
        Self {
            vm,
            input,
            database: database.to_owned(),
            dark: true,
            on_generate: None,
            on_stop: None,
            on_copy: None,
            on_clear: None,
            on_open_ai_settings: None,
            on_toggle_memory: None,
            on_focus_memory: None,
        }
    }

    /// The panel's theme.
    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// `handleGenerate`: the generate press (the host builds the
    /// request through `split_context_items` and drives the store).
    pub fn on_generate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_generate = Some(Rc::new(f));
        self
    }

    /// `handleStop`: the stop press (the host cancels the request id).
    pub fn on_stop(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// `handleCopySql`: the copy press (the host owns the copied cell
    /// and the clipboard).
    pub fn on_copy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// `clearNl2Sql`.
    pub fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// `openSettingsWindow('ai')` — the unconfigured bar's button.
    pub fn on_open_ai_settings(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_ai_settings = Some(Rc::new(f));
        self
    }

    /// The memory chip's press (`setMemoryOpen((open) => !open)`).
    pub fn on_toggle_memory(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_memory = Some(Rc::new(f));
        self
    }

    /// A citation's press (`focusMemory(reference)` — the host's
    /// focus cell).
    pub fn on_focus_memory(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_focus_memory = Some(Rc::new(f));
        self
    }

    /// The panel over the host's dialect key (the SQL code block's
    /// dialect — React's `<SqlCodeBlock code={previewSql} />` creates
    /// its block from the code each render; the port seeds one state
    /// per frame from the preview SQL). The caller roots it once.
    pub fn build(self, k: &Kit, dialect: &str) -> El {
        // React 133-148: the unconfigured bar — the notice and the
        // settings button, nothing else.
        if !self.vm.is_configured {
            return self.not_configured_bar(k);
        }
        let mut panel = col()
            .key("nl2sql-panel")
            .w_full()
            .shrink()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01) // bg-surface-alt (the T141 ruling)
            .child(self.egress_row(k))
            .child(self.input_row(k));
        if let Some(sql) = preview_sql(self.vm) {
            panel = panel.child(self.preview_card(k, dialect, sql));
        }
        if !self.vm.memory_used.is_empty() {
            panel = panel.child(self.memory_card(k));
        }
        if let Some(error) = self.vm.error.clone().filter(|e| !e.is_empty()) {
            panel = panel.child(self.error_bar(k, error));
        }
        panel
    }

    /// React 133-148: the unconfigured bar.
    fn not_configured_bar(&self, k: &Kit) -> El {
        let mut settings = Button::new(t("settings.ai.goToConfigure"))
            .variant(ButtonVariant::Submit)
            .control_size(ControlSize::Small)
            .icon(&crate::icons::ICON_SETTINGS);
        if let Some(on_open) = &self.on_open_ai_settings {
            let action = Rc::clone(on_open);
            settings = settings.on_press(move |s: &mut S| action(s));
        }
        row()
            .key("nl2sql-panel-not-configured")
            .w_full()
            .items_center()
            .gap(8.0)
            .px(12.0)
            .py(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .child(rune::icon(&ICON_SPARKLES, 14.0).key("nl2sql-panel-not-configured-icon").color(k.c.text_muted))
            .child(
                text(t("common.aiNotConfigured"))
                    .key("nl2sql-panel-not-configured-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .grow(1.0)
                    .min_w(0.0)
                    .wrap(),
            )
            .child(settings.build(k).key("nl2sql-panel-open-settings"))
    }

    /// React 152-154: the egress notice row over the input's context
    /// items — the notice mounts only when it has something to say
    /// (its own `Option<El>` contract).
    fn egress_row(&self, k: &Kit) -> El {
        let notice = AiEgressNotice::new(self.input.items(), None, Some(&self.database)).build(self.dark);
        let mut row = col()
            .key("nl2sql-panel-egress-row")
            .w_full()
            .px(8.0)
            .pt(8.0);
        if let Some(notice) = notice {
            row = row.child(notice);
        }
        let _ = k;
        row
    }

    /// React 155-210: the input row — the AI input and the action
    /// buttons.
    fn input_row(&self, k: &Kit) -> El {
        let vm = self.vm;
        let mut buttons = row().key("nl2sql-panel-actions").shrink().gap(4.0);
        if vm.is_generating {
            let mut stop = Button::new(t("common.stop"))
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Regular)
                .icon(&ICON_SQUARE);
            if let Some(on_stop) = &self.on_stop {
                let action = Rc::clone(on_stop);
                stop = stop.on_press(move |s: &mut S| action(s));
            }
            buttons = buttons.child(stop.build(k).key("nl2sql-stop"));
        } else {
            let mut generate = Button::new(t("nl2sql.generate"))
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::Regular)
                .icon(&ICON_SPARKLES)
                .disabled(!can_generate(&vm.input, vm.is_generating, &self.database));
            if let Some(on_generate) = &self.on_generate {
                let action = Rc::clone(on_generate);
                generate = generate.on_press(move |s: &mut S| action(s));
            }
            buttons = buttons.child(generate.build(k).key("nl2sql-generate"));
            if !vm.generated_sql.is_empty() {
                let mut copy = Button::new(t("common.copy"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Regular)
                    .icon(if vm.copied { &ICON_CHECK } else { &ICON_COPY })
                    .help(t("common.copy"));
                if let Some(on_copy) = &self.on_copy {
                    let action = Rc::clone(on_copy);
                    copy = copy.on_press(move |s: &mut S| action(s));
                }
                if vm.copied {
                    copy = copy.state(rune_kit::State::Rest);
                }
                buttons = buttons.child(copy.build(k).key("nl2sql-copy"));
                let mut clear = Button::new(t("nl2sql.clear"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Regular)
                    .icon(&ICON_TRASH);
                if let Some(on_clear) = &self.on_clear {
                    let action = Rc::clone(on_clear);
                    clear = clear.on_press(move |s: &mut S| action(s));
                }
                buttons = buttons.child(clear.build(k).key("nl2sql-clear"));
            }
        }
        // React 156-170: the AI input — the select-database-first
        // placeholder (161), the !database || isGenerating disable
        // (162), the generating loader (163), and hideSubmit (169).
        let mut input = AiInput::<S>::new(self.input)
            .placeholder(t("nl2sql.selectDatabaseFirst"))
            .disabled(self.database.is_empty() || self.vm.is_generating)
            .loading(self.vm.is_generating)
            .hide_submit(true);
        if let Some(on_generate) = &self.on_generate {
            let action = Rc::clone(on_generate);
            input = input.on_submit(move |s: &mut S| action(s));
        }
        let input_el = input.build(self.dark);
        row()
            .key("nl2sql-panel-input-row")
            .w_full()
            .shrink()
            .items_center()
            .gap(8.0)
            .padding(8.0)
            .child(
                col()
                    .key("nl2sql-panel-input-slot")
                    .grow(1.0)
                    .min_w(0.0)
                    .child(input_el),
            )
            .child(buttons)
    }

    /// React 213-225: the preview card — the header (the preview or
    /// result label and the generating spinner) over the SQL code
    /// block.
    fn preview_card(&self, k: &Kit, dialect: &str, sql: String) -> El {
        let vm = self.vm;
        let mut header = row()
            .key("nl2sql-preview-header")
            .w_full()
            .items_center()
            .justify_between()
            .px(8.0)
            .py(4.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .child(
                text(if vm.is_generating {
                    t("nl2sql.preview")
                } else {
                    t("nl2sql.result")
                })
                .key("nl2sql-preview-label")
                .font(TextStyle { size: 10.0, ..k.ty.small })
                .color(k.c.text_muted),
            );
        if vm.is_generating {
            header = header.child(
                Spinner::new("nl2sql-preview-spinner")
                    .size(SpinnerSize::Sm)
                    .tone(SpinnerTone::Accent)
                    .build(self.dark),
            );
        }
        let code_state = SqlCodeBlockState::new(sql, dialect);
        col()
            .key("nl2sql-preview-card")
            .mx(8.0)
            .mb(8.0)
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .clip_rounded(k.radius.sm)
            .child(header)
            .child(
                col()
                    .key("nl2sql-preview-body")
                    .max_h(240.0)
                    .child(SqlCodeBlock::<S>::new(&code_state).build(self.dark)),
            )
    }

    /// React 227-277: the memory card — the chip with its count and
    /// chevron, the citation buttons, and the expanded list.
    fn memory_card(&self, k: &Kit) -> El {
        let vm = self.vm;
        let count = vm.memory_used.len();
        let label = t("chat.memory.used").replacen("{count}", &count.to_string(), 1);
        let mut chip = Button::new(label)
            .variant(ButtonVariant::GhostMuted)
            .control_size(ControlSize::Small)
            .icon(&ICON_BOOK_OPEN)
            .expanded(vm.memory_open);
        if let Some(on_toggle) = &self.on_toggle_memory {
            let action = Rc::clone(on_toggle);
            chip = chip.on_press(move |s: &mut S| action(s));
        }
        let mut chips = row()
            .key("nl2sql-memory-chips")
            .w_full()
            .flex_wrap()
            .items_center()
            .gap(6.0)
            .child(chip.build(k).key("ai-memory-chip"));
        for memory in &vm.memory_used {
            let citation = Button::new(format!("[{}]", memory.reference))
                .variant(ButtonVariant::GhostMuted)
                .control_size(ControlSize::Small);
            let mut citation = citation;
            if let Some(on_focus) = &self.on_focus_memory {
                let action = Rc::clone(on_focus);
                let reference = memory.reference.clone();
                citation = citation.on_press(move |s: &mut S| action(s, reference.clone()));
            }
            chips = chips.child(
                citation
                    .build(k)
                    .key(format!("ai-memory-citation-{}", memory.reference)),
            );
        }
        let mut card = col()
            .key("nl2sql-memory-card")
            .mx(8.0)
            .mb(8.0)
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .px(8.0)
            .py(6.0)
            .child(chips);
        if vm.memory_open {
            let mut list = col().key("ai-memory-expanded").w_full().gap(2.0).mt(4.0);
            for memory in &vm.memory_used {
                let focused = vm.memory_focused.as_deref() == Some(memory.reference.as_str());
                let reference = format!("[{}]", memory.reference);
                let detail = format!("{} · {}", memory.kind, memory.scope);
                list = list.child(
                    row()
                        .key(format!("ai-memory-item-{}", memory.reference))
                        .center_y()
                        .gap(4.0)
                        .min_w(0.0)
                        .px(6.0)
                        .py(4.0)
                        .radius(k.radius.sm)
                        .focus_selected(focused)
                        .fill(if focused {
                            k.alpha(k.c.text_accent, 0.15)
                        } else {
                            Color::TRANSPARENT
                        })
                        .child(
                            text(reference)
                                .key(format!("ai-memory-item-{}-reference", memory.reference))
                                .font(TextStyle { size: 10.0, ..k.ty.mono })
                                .color(k.c.text_muted),
                        )
                        .child(
                            text(memory.subject.clone())
                                .key(format!("ai-memory-item-{}-subject", memory.reference))
                                .font(TextStyle { size: 10.0, ..k.ty.small })
                                .color(k.c.text_base)
                                .wrap()
                                .min_w(0.0)
                                .shrink(),
                        )
                        .child(
                            text(detail)
                                .key(format!("ai-memory-item-{}-detail", memory.reference))
                                .font(TextStyle { size: 10.0, ..k.ty.small })
                                .color(k.c.text_faint),
                        ),
                );
            }
            card = card.child(list);
        }
        card
    }

    /// React 279-283: the error bar.
    fn error_bar(&self, k: &Kit, error: String) -> El {
        row()
            .key("nl2sql-error")
            .w_full()
            .shrink()
            .mx(8.0)
            .mb(8.0)
            .radius(k.radius.sm)
            .border(1.0, k.alpha(k.c.danger.fg, 0.2))
            .fill(k.alpha(k.c.danger.fg, 0.1))
            .px(12.0)
            .py(8.0)
            .child(
                text(error)
                    .key("nl2sql-error-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap()
                    .min_w(0.0),
            )
    }
}
