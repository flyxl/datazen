//! The query error panel, ported from
//! `packages/frontend/src/components/query/QueryErrorPanel.tsx` (128 lines,
//! T100 with `QueryContextSelectors`).
//!
//! A failed query's error message stays readable: the header ("execution
//! failed" + copy), the message as monospace wrapping text, and an optional
//! action row (explain / fix SQL / retry / ask in chat).
//!
//! ## Key convention
//!
//! React's `data-testid`s become the element keys:
//!
//! | part | key |
//! |---|---|
//! | root | `query-error-panel` (new; React gives the root none) |
//! | header | `query-error-header` (new) |
//! | copy control | `query-copy-error` |
//! | message | `query-error-message` |
//! | action row | `query-error-actions` (new) |
//! | explain / diagnose | `query-explain-error` |
//! | fix SQL | `query-fix-sql` |
//! | retry | `query-retry` |
//! | ask in chat | `query-ask-in-chat` |
//!
//! ## Deliberate boundaries
//!
//! * **The copy control is the `CopyButton` base** (`ui/copyable.rs`, the
//!   `packages/ui` `CopyableError` port): a press copies the message,
//!   schedules the 1500ms feedback wake, and invokes
//!   [`QueryErrorPanel::on_copy_tx`]. The caller owns `copied` state
//!   (`copied_until`), exactly like every other copyable port.
//! * **`explain` is `onExplain ?? onDiagnose`** (React's fallback): with
//!   only `onDiagnose` the row shows the stethoscope and
//!   `diagnosis.diagnose`; with `onExplain` the lightbulb and
//!   `query.explainError`.
//! * **Icons** come from the registry: `Check`/`Copy` (the CopyButton),
//!   `Lightbulb`/`Stethoscope`, `Wand2` (the registry's `wand-sparkles`,
//!   which `lucide-react@0.487.0` re-exports as `Wand2`), `RotateCcw`,
//!   `MessageSquare` — three new registry entries in this task.
//! * **Action buttons** are small ghost rows (React's `text-[11px]`
//!   `text-accent` `px-2 py-1`): the kit has no 11pt button, so the rows
//!   follow the CopyButton construction (ghost ink over accent, 11pt
//!   `k.ty.small`), with the accent colour React paints them in.
//! * **No `selectable` / `whitespace-pre-wrap` port surface.** Rune text
//!   wraps by default; selection is the runtime's text model, not a style.
//! * **`title` tooltips** become the row `label` (the a11y name) plus
//!   `help` where the kit carries it; there is no hover tooltip engine in
//!   the test surface.
//!
//! No production consumer yet: the query workspace wiring task mounts it
//! on a failed execution.

// The production consumer is the query-workspace wiring task; until then
// the component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, Tx};

use crate::i18n::t;
use crate::icons::{
    ICON_LIGHTBULB, ICON_MESSAGE_SQUARE, ICON_ROTATE_CCW, ICON_STETHOSCOPE, ICON_WAND_SPARKLES,
};
use crate::ui::copyable::CopyButton;
use crate::ui::kit::kit;

/// How long the "copied" label stays before reverting — React's
/// `COPIED_FEEDBACK_MS` (`QueryErrorPanel.tsx:7`).
pub const COPIED_FEEDBACK_MS: u64 = 1500;

/// The panel. Build it with the message and the caller's copy-feedback
/// state, then attach the optional actions.
pub struct QueryErrorPanel<'a, S: 'static> {
    message: &'a str,
    copied: bool,
    on_diagnose: Option<Rc<dyn Fn(&mut S)>>,
    on_explain: Option<Rc<dyn Fn(&mut S)>>,
    on_fix_sql: Option<Rc<dyn Fn(&mut S)>>,
    on_retry: Option<Rc<dyn Fn(&mut S)>>,
    on_ask_in_chat: Option<Rc<dyn Fn(&mut S)>>,
    on_copy: Option<Rc<dyn Fn(&mut S, &mut Tx)>>,
}

impl<'a, S: 'static> QueryErrorPanel<'a, S> {
    pub fn new(message: &'a str) -> Self {
        Self {
            message,
            copied: false,
            on_diagnose: None,
            on_explain: None,
            on_fix_sql: None,
            on_retry: None,
            on_ask_in_chat: None,
            on_copy: None,
        }
    }

    /// Show `common.copied` + the check glyph instead of `common.copy`:
    /// the caller's `copied_until > now` evaluation.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// React's `onDiagnose` — the explain fallback.
    pub fn on_diagnose(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_diagnose = Some(Rc::new(f));
        self
    }

    /// React's `onExplain`.
    pub fn on_explain(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_explain = Some(Rc::new(f));
        self
    }

    pub fn on_fix_sql(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_fix_sql = Some(Rc::new(f));
        self
    }

    pub fn on_retry(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_retry = Some(Rc::new(f));
        self
    }

    pub fn on_ask_in_chat(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_ask_in_chat = Some(Rc::new(f));
        self
    }

    /// React's `onCopy`, invoked after the clipboard write and the wake.
    pub fn on_copy_tx(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// The panel. React: `rounded-md border border-danger/20 bg-danger/10
    /// px-4 py-3`.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let c = &k.c;

        // The header: the uppercase danger label grows, the copy control
        // sits at the right (`flex items-center gap-2`).
        let header = row()
            .key("query-error-header")
            .w_full()
            .items_center()
            .gap(8.0)
            .child(
                text(t("common.executionFailed"))
                    .key("query-error-title")
                    .grow(1.0)
                    .min_w(0.0)
                    .font(k.ty.text_xsmall_medium)
                    .color(c.danger.fg),
            )
            .child(self.copy_control());

        // React's `<pre>`: monospace, wrapping, `mt-2`.
        let message = text(self.message)
            .key("query-error-message")
            .w_full()
            .mt(8.0)
            .font(k.ty.mono)
            .color(c.danger.fg)
            .wrap();

        let mut panel = col()
            .key("query-error-panel")
            .w_full()
            .px(16.0)
            .py(12.0)
            .radius(k.radius.sm)
            .border(1.0, k.alpha(c.danger.fg, 0.2))
            .fill(k.alpha(c.danger.fg, 0.1))
            .items_stretch()
            .child(header)
            .child(message);
        if let Some(actions) = self.action_row(dark) {
            panel = panel.child(actions);
        }
        panel
    }

    /// The header's copy control: the `CopyButton` base with the panel's
    /// message and copy callback (React's `handleCopyError`).
    fn copy_control(&self) -> El {
        let mut button = CopyButton::new("query-copy-error", self.message)
            .copied(self.copied)
            .help(&t("common.copy"));
        if let Some(on_copy) = &self.on_copy {
            let on_copy = Rc::clone(on_copy);
            button = button.on_copy_tx(move |s: &mut S, tx: &mut Tx| on_copy(s, tx));
        }
        button.build::<S>(true)
    }

    /// The action row: `mt-3 flex flex-wrap items-center gap-1`, one small
    /// accent row per provided action. Absent when no action is provided.
    fn action_row(&self, dark: bool) -> Option<El> {
        // React's `explain = onExplain ?? onDiagnose`.
        let explain = match (&self.on_explain, &self.on_diagnose) {
            (Some(f), _) => Some((Rc::clone(f), true)),
            (None, Some(f)) => Some((Rc::clone(f), false)),
            (None, None) => None,
        };
        if explain.is_none()
            && self.on_fix_sql.is_none()
            && self.on_retry.is_none()
            && self.on_ask_in_chat.is_none()
        {
            return None;
        }
        let k = kit(dark);
        let mut row = row()
            .key("query-error-actions")
            .w_full()
            .mt(12.0)
            .items_center()
            .flex_wrap()
            .gap(4.0);
        if let Some((explain, is_explain)) = explain {
            row = row.child(action_button(
                "query-explain-error",
                if is_explain {
                    (&ICON_LIGHTBULB, t("query.explainError").to_string())
                } else {
                    (&ICON_STETHOSCOPE, t("diagnosis.diagnose").to_string())
                },
                explain,
                &k,
            ));
        }
        if let Some(on_fix_sql) = &self.on_fix_sql {
            row = row.child(action_button(
                "query-fix-sql",
                (&ICON_WAND_SPARKLES, t("query.fixSql").to_string()),
                Rc::clone(on_fix_sql),
                &k,
            ));
        }
        if let Some(on_retry) = &self.on_retry {
            row = row.child(action_button(
                "query-retry",
                (&ICON_ROTATE_CCW, t("common.retry").to_string()),
                Rc::clone(on_retry),
                &k,
            ));
        }
        if let Some(on_ask_in_chat) = &self.on_ask_in_chat {
            row = row.child(action_button(
                "query-ask-in-chat",
                (&ICON_MESSAGE_SQUARE, t("query.editor.askInChat").to_string()),
                Rc::clone(on_ask_in_chat),
                &k,
            ))
            .help(t("query.editor.askInChatHint"));
        }
        Some(row)
    }
}

/// One action button: React's `inline-flex items-center gap-1 rounded
/// px-2 py-1 text-[11px] text-accent hover:bg-accent/10` around a 12pt
/// glyph. The kit has no 11pt button, so the row follows the
/// CopyButton's construction: ghost ink, small type, the accent colour.
fn action_button<S: 'static>(
    key: &'static str,
    (glyph, label): (&'static rune::Icon, String),
    on_press: Rc<dyn Fn(&mut S)>,
    k: &rune_kit::Kit,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label.clone())
        .help(label.clone())
        .items_center()
        .gap(4.0)
        .px(8.0)
        .py(4.0)
        .radius(k.radius.xs)
        .fill(k.alpha(k.c.bg_accent, 0.0))
        .child(
            icon(glyph, 12.0)
                .key("icon")
                .color(k.c.text_accent)
                .a11y_hidden(),
        )
        .child(
            text(label)
                .key("label")
                .font(k.ty.small)
                .color(k.c.text_accent),
        )
        .on_press(move |s: &mut S| on_press(s))
}
