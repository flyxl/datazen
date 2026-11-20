//! T060: `components/ai/AiCodeBlock.tsx` ported to Rune.
//!
//! React renders one fenced code block from an assistant message: a header
//! with the language label, a SQL-dialect select for SQL blocks, and a
//! toolbar (run / insert-to-editor / new-query for SQL, copy, fullscreen),
//! plus the code itself — highlighted by `SqlCodeBlock` (a read-only
//! CodeMirror) for SQL, a plain `<pre>` otherwise. A second, fullscreen
//! rendering replaces the block while `fullscreen` is set.
//!
//! Rune is immediate mode, so the component-local `useState` (`fullscreen`,
//! `selectedDialect`) and the `useCopyFeedback` confirmation (`copied`, its
//! 1500 ms timer, the stale-request guard) live in the app's state `S` as an
//! [`AiCodeBlockState`]. The host owns the clipboard write and the timer:
//! the copy press marks the state optimistically through
//! [`AiCodeBlockState::copy_pressed`] (which returns the request id) and
//! fires `on_copy`; the host reports a failed write with
//! [`AiCodeBlockState::fail_copy`] and the timer expiry with
//! [`AiCodeBlockState::expire_copy_feedback`], both of which only act on
//! the request that is still current — the same stale-request guard
//! `useCopyFeedback` keeps (`packages/ui/src/useCopyFeedback.ts`).
//!
//! The SQL callbacks and the dialect come from the `AiMessageContent`
//! wiring (T169, not ported yet): it renders `AiCodeBlock` per fenced block
//! and passes `onInsertSql` / `onRunCode` / `onNewQuery` / `sqlDialect`
//! through. Until T169 lands this component has no production consumer;
//! reusable API items carry per-item `#[allow(dead_code)]` (not a
//! module-wide blanket) until that caller lands. The `ChatBubble`
//! placeholder (`components/ai/chat_bubble.rs`) deliberately does not fake
//! this rendering, and nothing here changes that: T169 wires this module
//! in and drops the placeholder.
//!
//! ## Keys (prefix defaults to `ai-code-block`)
//!
//! React reuses the same `data-testid`s in the inline and fullscreen
//! renderings (only one is mounted at a time); Rune does the same, so the
//! action keys below exist in both modes.
//!
//! | part | key |
//! |------|-----|
//! | inline root | `<prefix>` |
//! | fullscreen root | `<prefix>-fullscreen` |
//! | header | `<prefix>-header` |
//! | header left group | `<prefix>-header-left` |
//! | language label | `<prefix>-label` |
//! | dialect field (prefix) | `<prefix>-dialect-field` |
//! | dialect trigger | `<prefix>-dialect-field-trigger` |
//! | dialect listbox content | `<prefix>-dialect-field-content` (while open) |
//! | dialect dismissal scrim | `<prefix>-dialect-field-scrim` (while open) |
//! | actions group | `<prefix>-actions` |
//! | run button | `<prefix>-run` |
//! | insert button | `<prefix>-insert` |
//! | new-query button | `<prefix>-new-query` |
//! | copy button | `<prefix>-copy` |
//! | fullscreen toggle | `<prefix>-fullscreen-toggle` |
//! | button text label | `<prefix>-<action>-label` (only when the button shows text) |
//! | content | `<prefix>-content` |
//! | code text | `<prefix>-code` |
//!
//! ## Deliberate deviations
//!
//! * **No syntax highlighting.** `SqlCodeBlock` is a CodeMirror editor and
//!   has no Rune port yet, so SQL renders as the same mono selectable text
//!   as a non-SQL block (`k.ty.mono`, `text_base` for React's
//!   `text-fg-secondary`). The dialect still selects which dialect the
//!   future editor will highlight with; the value is kept in state.
//! * **No fixed fullscreen overlay.** React mounts the fullscreen tree
//!   `fixed inset-0 z-50`; Rune has no fixed positioning in a component,
//!   so `build` returns the fullscreen tree (key `<prefix>-fullscreen`)
//!   and the wiring track mounts it as the fullscreen layer, the same way
//!   the modal layer mounts dialogs.
//! * **No fixed 128pt SQL viewport.** React wraps the editor in an `h-32
//!   min-h-[4rem]` box inside a `max-h-60` scroller. Rune text sizes to
//!   its content, so the content is capped at 240pt (`max-h-60`) and
//!   otherwise flows; the 128pt fixed height is not replicated.
//! * **Streaming pulse via the same loop as `ChatBubble`.** React's
//!   `animate-pulse` on the root while streaming is
//!   `LoopSpec::opacity(2.0, 0.0, 0.5, 1.0)`, the Tailwind pulse
//!   keyframes (2s, opacity 1 → 0.5 → 1).
//! * **Run button colour.** React's `text-emerald-400` has no kit token;
//!   it maps to `k.c.success.fg`, the success green other AI surfaces use
//!   for the same "go" meaning. Insert maps to `k.c.text_accent` (React
//!   `text-accent`); the remaining buttons stay `k.c.text_muted`.
//! * **Icon-only buttons keep their `title` as the a11y label.** React's
//!   inline run / new-query / copy / fullscreen buttons show only an icon
//!   with a native `title` tooltip; the kit has no `title`, so the same
//!   `chat.*` string is the button's a11y label. The hover brightening
//!   (`hover:text-fg`) has no Rune equivalent on a hand-built row; the
//!   hover fill still signals hover, as in `query_artifact_export.rs`.
//! * **The fullscreen copy label is omitted while copied.** React renders
//!   an empty string there (`copied ? '' : t('chat.copyCode')`); Rune
//!   leaves the label child out instead of mounting an empty text node.
//! * **`selectedDialect` never re-syncs from the prop.** React seeds the
//!   state from `sqlDialect` once (`useState(sqlDialect)`) and ignores
//!   later prop changes; [`AiCodeBlockState::new`] does the same. A
//!   dialect change after mount needs a fresh state slot, as a React
//!   remount would.
//! * **An unknown initial dialect falls back to PostgreSQL.** React would
//!   keep the unknown string as the select's value with no matching
//!   option; the prop only ever arrives from `AiMessageContent`'s
//!   `sqlDialect`, which is one of the seven known values, so the port
//!   normalises it instead of carrying an unrepresentable state.
//! * **Copy without a host does nothing.** React always has
//!   `navigator.clipboard`; here the copy button only gets a press
//!   handler when `on_copy` is set, so a block without a clipboard host
//!   never claims a copy it did not make.

use rune::{col, icon, row, text, Color, El, LoopSpec, Tx};
use rune_kit::Kit;

use super::ai_code_block_state::{
    apply_dialect_choice, display_label, is_sql_code_block, run_language, AiCodeBlockProps,
    AiCodeBlockState, CloseDialectFn, CopyPressedFn, ReadCodeBlockStateFn, SetDialectFn,
    SqlDialect, ToggleDialectOpenFn, ToggleFullscreenFn,
};
use crate::i18n::t;
use std::rc::Rc;
use crate::icons::{
    ICON_CHECK, ICON_CODE, ICON_COPY, ICON_MAXIMIZE_2, ICON_MINIMIZE_2, ICON_PLAY, ICON_PLUS,
};
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// React `onInsertSql`: insert `code` into the editor.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub(crate) type InsertSqlFn<S> = Rc<dyn Fn(&mut S, String)>;

/// React `onRunCode`: run `code` as `language`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub(crate) type RunCodeFn<S> = Rc<dyn Fn(&mut S, String, String)>;

/// React `onNewQuery`: open a new query tab with `code`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub(crate) type NewQueryFn<S> = Rc<dyn Fn(&mut S, String)>;

/// The host's clipboard write starter: write `code`, report a failure with
/// [`AiCodeBlockState::fail_copy`] and the timer expiry with
/// [`AiCodeBlockState::expire_copy_feedback`], both keyed by `request`.
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub(crate) type CopyCodeFn<S> = Rc<dyn Fn(&mut S, String, u64)>;

/// The code block. `read_state` and the five state functions locate the
/// state slot in the app's `S`; the four `on_*` callbacks are the host's
/// actions. A block's state slot belongs to that block alone — two blocks
/// sharing one slot would share fullscreen, dialect and copy feedback, so
/// the wiring track must keep one slot per rendered block (and a fresh
/// slot when the block's `sqlDialect` prop changes, as a React remount
/// would).
#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
pub(crate) struct AiCodeBlock<S: 'static> {
    key: String,
    props: AiCodeBlockProps,
    read_state: ReadCodeBlockStateFn<S>,
    toggle_fullscreen: ToggleFullscreenFn<S>,
    set_dialect: SetDialectFn<S>,
    toggle_dialect_open: ToggleDialectOpenFn<S>,
    close_dialect: CloseDialectFn<S>,
    copy_pressed: CopyPressedFn<S>,
    on_insert_sql: Option<InsertSqlFn<S>>,
    on_run_code: Option<RunCodeFn<S>>,
    on_new_query: Option<NewQueryFn<S>>,
    on_copy: Option<CopyCodeFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by the AiMessageContent track (T169)
impl<S: 'static> AiCodeBlock<S> {
    /// `read_state` returns a clone of the block's state slot (the message
    /// wiring addresses one slot per rendered block by index — see
    /// `ai_message_content.rs`); the five mutators locate that same slot.
    pub(crate) fn new(
        props: AiCodeBlockProps,
        read_state: impl Fn(&S) -> AiCodeBlockState + 'static,
        toggle_fullscreen: impl Fn(&mut S) + 'static,
        set_dialect: impl Fn(&mut S, SqlDialect) + 'static,
        toggle_dialect_open: impl Fn(&mut S) + 'static,
        close_dialect: impl Fn(&mut S) + 'static,
        copy_pressed: impl Fn(&mut S) -> u64 + 'static,
    ) -> Self {
        Self {
            key: "ai-code-block".to_string(),
            props,
            read_state: Rc::new(read_state),
            toggle_fullscreen: Rc::new(toggle_fullscreen),
            set_dialect: Rc::new(set_dialect),
            toggle_dialect_open: Rc::new(toggle_dialect_open),
            close_dialect: Rc::new(close_dialect),
            copy_pressed: Rc::new(copy_pressed),
            on_insert_sql: None,
            on_run_code: None,
            on_new_query: None,
            on_copy: None,
        }
    }

    /// Override the key prefix (two blocks can then share one state slot
    /// only if their callers keep the prefixes distinct — and they
    /// should not share a slot at all; see the struct docs).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// React `onInsertSql`; without it the insert button is not rendered.
    pub(crate) fn on_insert_sql(
        mut self,
        on_insert_sql: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_insert_sql = Some(Rc::new(on_insert_sql));
        self
    }

    /// React `onRunCode`; without it the run button is not rendered.
    pub(crate) fn on_run_code(
        mut self,
        on_run_code: impl Fn(&mut S, String, String) + 'static,
    ) -> Self {
        self.on_run_code = Some(Rc::new(on_run_code));
        self
    }

    /// React `onNewQuery`; without it the new-query button is not rendered.
    pub(crate) fn on_new_query(
        mut self,
        on_new_query: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_new_query = Some(Rc::new(on_new_query));
        self
    }

    /// The host's clipboard write starter; without it the copy button
    /// renders but does nothing, so it never claims a copy it did not make.
    pub(crate) fn on_copy(mut self, on_copy: impl Fn(&mut S, String, u64) + 'static) -> Self {
        self.on_copy = Some(Rc::new(on_copy));
        self
    }

    /// Builds the inline block, or the fullscreen tree while
    /// `state.fullscreen` is set. `layer` takes the dialect listbox while
    /// it is open (the same two-part build as `ui/select.rs`).
    pub(crate) fn build(self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let state = (self.read_state)(s);
        self.build_with_state(&state, dark, layer)
    }

    /// The state-snapshot build: the caller renders from a slot it
    /// already holds (the message content wiring passes the message's
    /// code-block slots), while the event closures still route through
    /// `S` when they fire.
    pub(crate) fn build_with_state(
        self,
        state: &AiCodeBlockState,
        dark: bool,
        layer: &mut Vec<El>,
    ) -> El {
        let k = kit(dark);
        let is_sql = is_sql_code_block(&self.props.language, &self.props.code);
        if state.fullscreen {
            self.fullscreen_tree(k, state, is_sql, layer)
        } else {
            self.inline_tree(k, state, is_sql, layer)
        }
    }

    /// React's inline rendering: bordered `bg-surface` card, header plus
    /// code, `animate-pulse` while streaming.
    fn inline_tree(
        &self,
        k: &Kit,
        state: &AiCodeBlockState,
        is_sql: bool,
        layer: &mut Vec<El>,
    ) -> El {
        // React: `my-2 overflow-hidden rounded-md border border-edge
        // bg-surface` + `animate-pulse` while streaming.
        let mut root = col()
            .key(self.key.clone())
            .w_full()
            .border(1.0, k.c.border_base)
            .radius(k.radius.sm)
            .fill(k.c.bg_base);
        if self.props.is_streaming {
            root = root.looping(LoopSpec::opacity(2.0, 0.0, 0.5, 1.0));
        }
        root.child(self.header(k, state, is_sql, layer, false))
            .child(self.content(k, is_sql, false))
    }

    /// React's fullscreen rendering: `fixed inset-0 z-50 flex flex-col
    /// bg-surface`, header plus code. The fixed positioning is the wiring
    /// track's job (see the module docs); this is the tree it mounts.
    fn fullscreen_tree(
        &self,
        k: &Kit,
        state: &AiCodeBlockState,
        is_sql: bool,
        layer: &mut Vec<El>,
    ) -> El {
        col()
            .key(format!("{}-fullscreen", self.key))
            .w_full()
            .h_full()
            .fill(k.c.bg_base)
            .child(self.header(k, state, is_sql, layer, true))
            .child(self.content(k, is_sql, true))
    }

    /// The header: `Code2` icon, uppercase language label, the dialect
    /// select for SQL blocks, and the toolbar. Inline the toolbar shows
    /// only when `showActions && !isStreaming`; fullscreen always shows it.
    fn header(
        &self,
        k: &Kit,
        state: &AiCodeBlockState,
        is_sql: bool,
        layer: &mut Vec<El>,
        fullscreen: bool,
    ) -> El {
        // React inline: `flex items-center justify-between gap-2 border-b
        // border-edge bg-surface-alt px-2 py-1`; fullscreen: `px-3 py-1.5`
        // with no alt fill.
        let mut header = row()
            .key(format!("{}-header", self.key))
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(if fullscreen { 12.0 } else { 8.0 })
            .py(if fullscreen { 6.0 } else { 4.0 });
        if !fullscreen {
            header = header.fill(k.c.layer_01);
        }

        // React: `flex items-center gap-1.5 min-w-0`, Code2 plus the label.
        let mut left = row()
            .key(format!("{}-header-left", self.key))
            .items_center()
            .gap(6.0)
            .child(
                icon(&ICON_CODE, if fullscreen { 14.0 } else { 12.0 })
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .child(
                text(display_label(&self.props.language, is_sql))
                    .key(format!("{}-label", self.key))
                    .font(if fullscreen {
                        k.ty.text_small_medium
                    } else {
                        k.ty.text_xsmall_medium
                    })
                    .color(k.c.text_muted)
                    .truncate(),
            );
        if is_sql {
            left = left.child(self.dialect_select(k, state, fullscreen, layer));
        }
        header = header.child(left);

        let show_actions = fullscreen || (self.props.show_actions && !self.props.is_streaming);
        if show_actions {
            header = header.child(self.actions(k, state, is_sql, fullscreen));
        }
        header
    }

    /// React's `Select` for the dialect: the seven `SQL_DIALECTS` options,
    /// `w-28` inline / `w-32` fullscreen, `title={t('chat.selectDialect')}`
    /// as the accessible label.
    fn dialect_select(
        &self,
        k: &Kit,
        state: &AiCodeBlockState,
        fullscreen: bool,
        layer: &mut Vec<El>,
    ) -> El {
        let labels: Vec<&str> = SqlDialect::ALL.iter().map(|d| d.label()).collect();
        let values: Vec<&str> = SqlDialect::ALL.iter().map(|d| d.value()).collect();
        let set_dialect = self.set_dialect.clone();
        let close_dialect = self.close_dialect.clone();
        let close_dialect_for_close = self.close_dialect.clone();
        let toggle_dialect_open = self.toggle_dialect_open.clone();
        let mut select = Select::new(format!("{}-dialect-field", self.key), &labels)
            .values(&values)
            .current(Some(state.selected_dialect.value()))
            .label(t("chat.selectDialect"))
            .width(if fullscreen { 128.0 } else { 112.0 })
            .open(state.dialect_open);
        select = select.on_toggle(move |s: &mut S| toggle_dialect_open(s));
        select = select.on_close(move |s: &mut S| close_dialect_for_close(s));
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            apply_dialect_choice(set_dialect.clone(), close_dialect.clone(), s, value);
        });
        select.build(k, layer)
    }

    /// The toolbar: run / insert / new-query for SQL blocks (each only
    /// when its callback is set), copy, fullscreen toggle. Inline the
    /// icon-only buttons carry their `title` as the a11y label;
    /// fullscreen shows the text beside the icon, as React does.
    fn actions(&self, k: &Kit, state: &AiCodeBlockState, is_sql: bool, fullscreen: bool) -> El {
        let mut actions = row()
            .key(format!("{}-actions", self.key))
            .items_center()
            .gap(2.0);

        // Run (SQL only): React `text-emerald-400`, icon-only inline,
        // icon + `chat.runCode` fullscreen.
        if is_sql {
            if let Some(on_run_code) = self.on_run_code.clone() {
                let code = self.props.code.clone();
                let language = run_language(&self.props.language).to_owned();
                let mut run = self.tiny_button(
                    k,
                    format!("{}-run", self.key),
                    t("chat.runCode"),
                    Some(&ICON_PLAY),
                    k.c.success.fg,
                    fullscreen.then(|| t("chat.runCode").to_owned()),
                );
                run = run.on_press(move |s: &mut S| {
                    on_run_code(s, code.clone(), language.clone());
                });
                actions = actions.child(run);
            }
        }

        // Insert to Editor (SQL only): React text-only in both modes.
        if is_sql {
            if let Some(on_insert_sql) = self.on_insert_sql.clone() {
                let code = self.props.code.clone();
                let mut insert = self.tiny_button(
                    k,
                    format!("{}-insert", self.key),
                    t("chat.insertSql"),
                    None,
                    k.c.text_accent,
                    Some(t("chat.insertSql").to_owned()),
                );
                insert = insert.on_press(move |s: &mut S| {
                    on_insert_sql(s, code.clone());
                });
                actions = actions.child(insert);
            }
        }

        // New Query (SQL only): icon-only inline, icon + text fullscreen.
        if is_sql {
            if let Some(on_new_query) = self.on_new_query.clone() {
                let code = self.props.code.clone();
                let mut new_query = self.tiny_button(
                    k,
                    format!("{}-new-query", self.key),
                    t("chat.newQuery"),
                    Some(&ICON_PLUS),
                    k.c.text_muted,
                    fullscreen.then(|| t("chat.newQuery").to_owned()),
                );
                new_query = new_query.on_press(move |s: &mut S| {
                    on_new_query(s, code.clone());
                });
                actions = actions.child(new_query);
            }
        }

        // Copy: Check while copied, else Copy; fullscreen shows
        // `chat.copyCode` beside it unless copied (React renders an empty
        // string there).
        let copy_icon = if state.copied {
            &ICON_CHECK
        } else {
            &ICON_COPY
        };
        let copy_text = if fullscreen && !state.copied {
            Some(t("chat.copyCode").to_owned())
        } else {
            None
        };
        let mut copy = self.tiny_button(
            k,
            format!("{}-copy", self.key),
            t("chat.copyCode"),
            Some(copy_icon),
            k.c.text_muted,
            copy_text,
        );
        if let Some(on_copy) = self.on_copy.clone() {
            let code = self.props.code.clone();
            let copy_pressed = self.copy_pressed.clone();
            copy = copy.on_press(move |s: &mut S| {
                let request = copy_pressed(s);
                on_copy(s, code.clone(), request);
            });
        }
        actions = actions.child(copy);

        // Fullscreen toggle: Maximize2 inline (`chat.fullscreen`),
        // Minimize2 fullscreen (`chat.exitFullscreen`).
        let (toggle_icon, toggle_label) = if fullscreen {
            (&ICON_MINIMIZE_2, t("chat.exitFullscreen"))
        } else {
            (&ICON_MAXIMIZE_2, t("chat.fullscreen"))
        };
        let toggle_text = fullscreen.then(|| toggle_label.to_owned());
        let toggle_fullscreen = self.toggle_fullscreen.clone();
        let toggle = self
            .tiny_button(
                k,
                format!("{}-fullscreen-toggle", self.key),
                toggle_label,
                Some(toggle_icon),
                k.c.text_muted,
                toggle_text,
            )
            .on_press(move |s: &mut S| {
                toggle_fullscreen(s);
            });
        actions = actions.child(toggle);

        actions
    }

    /// One tiny toolbar button: a 10pt icon and/or label on a hand-built
    /// row (`role="button"`, `focusable`, `on_press`), like the rows in
    /// `components/query/query_artifact_export.rs` — React's buttons are
    /// tiny text buttons (`px-1.5 py-0.5 text-[10px]`), not 24pt kit
    /// controls. `label` is the a11y name (React's `title`).
    fn tiny_button(
        &self,
        k: &Kit,
        key: String,
        label: &str,
        glyph: Option<&'static rune::Icon>,
        icon_color: Color,
        text_label: Option<String>,
    ) -> El {
        let label_key = format!("{key}-label");
        let mut button = row()
            .key(key)
            .role("button")
            .focusable(true)
            .label(label.to_owned())
            .items_center()
            .gap(4.0)
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .hover(k.c.hover);
        if let Some(glyph) = glyph {
            button = button.child(icon(glyph, 10.0).color(icon_color).a11y_hidden());
        }
        if let Some(text_label) = text_label {
            button = button.child(
                text(text_label)
                    .key(label_key)
                    .font(k.ty.text_xsmall)
                    .color(icon_color),
            );
        }
        button
    }

    /// The code: SQL through the (unported) `SqlCodeBlock` in React, a
    /// plain `<pre>` otherwise — both render here as mono selectable text
    /// (see the module docs). Inline SQL is capped at 240pt (`max-h-60`);
    /// fullscreen takes the remaining height. Non-SQL wraps
    /// (`whitespace-pre-wrap`); SQL does not (CodeMirror's horizontal
    /// scroll).
    fn content(&self, k: &Kit, is_sql: bool, fullscreen: bool) -> El {
        let mut content = col().key(format!("{}-content", self.key)).w_full();
        if !fullscreen && is_sql {
            content = content.max_h(240.0);
        }
        if fullscreen {
            content = content.h_full();
        }
        if !is_sql {
            // React's `<pre>`: `p-2` inline, `p-4` fullscreen.
            content = content
                .px(if fullscreen { 16.0 } else { 8.0 })
                .py(if fullscreen { 16.0 } else { 8.0 });
        }
        let mut code = text(self.props.code.clone())
            .key(format!("{}-code", self.key))
            .font(k.ty.mono)
            .color(k.c.text_base)
            .selectable(true);
        if !is_sql {
            code = code.wrap();
        }
        content.child(code)
    }
}
