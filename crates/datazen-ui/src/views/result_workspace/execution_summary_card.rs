//! Port of `windows/connection/result-workspace/ExecutionSummaryCard.tsx`.
//!
//! React shows one success card: a check badge, `query.executionSuccess`, a
//! rows-affected pill (`query.rowsAffectedCount` with `{count}`), an elapsed
//! pill (`{ms} ms`), the executed SQL and a copy button whose label flips to
//! the literal `已复制` for 2s. The Rune port keeps the same keys and copy
//! contract: [`SummaryAction::CopySql`] carries the SQL to the injected host,
//! which owns the clipboard write and the timed feedback (`vm.copied`).
//! No clipboard call, timer or promise lives in the view.
//!
//! The lucide `CheckCircle2` / `Rows` glyphs are not in the registry; the
//! registered `ICON_CHECK` and `ICON_TABLE` stand in for them.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this card yet; the result workspace owns the VM,
//! performs the copy and clears `copied` after 2000ms. Until then every
//! reusable item carries a per-item `#[allow(dead_code)]` with this stage.

use std::rc::Rc;

use rune::{col, icon, row, text, El};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_CLOCK, ICON_COPY, ICON_TABLE};
use crate::ui::kit::kit;

const ROOT_KEY: &str = "execution-summary-card";
pub const TITLE_KEY: &str = "execution-summary-title";
pub const ROWS_KEY: &str = "execution-summary-rows";
pub const ELAPSED_KEY: &str = "execution-summary-elapsed";
pub const SQL_KEY: &str = "execution-summary-sql";
pub const COPY_KEY: &str = "execution-summary-copy";
pub const STATUS_KEY: &str = "execution-summary-status";

/// How long React keeps the `已复制` feedback before reverting to `复制`.
#[allow(dead_code)] // contract mirror; consumed by the host's feedback timer.
pub const COPIED_FEEDBACK_MS: u64 = 2000;

/// React `StatementResult`, reduced to the four fields the card reads.
#[allow(dead_code)] // VM mirror; owned by the result-workspace host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SummaryResultVm {
    pub sql: String,
    pub rows_affected: Option<i64>,
    pub execution_time_ms: i64,
}

/// Host-owned copy feedback state.
#[allow(dead_code)] // VM mirror; owned by the result-workspace host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionSummaryCardVm {
    pub result: SummaryResultVm,
    /// React `useCopyFeedback`'s `copied` flag (host clears after 2s).
    pub copied: bool,
}

impl ExecutionSummaryCardVm {
    /// React's `rowsAffectedText`: null when the count is absent, else
    /// `t('query.rowsAffectedCount', { count })`.
    #[allow(dead_code)] // pure helper; pinned by the card tests.
    pub fn rows_affected_text(&self) -> Option<String> {
        self.result.rows_affected.map(|count| {
            t("query.rowsAffectedCount").replace("{count}", &count.to_string())
        })
    }
}

/// The single host intent the card emits.
#[allow(dead_code)] // host port; remove when the result host wires the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SummaryAction {
    CopySql { sql: String },
}

type ActionFn<S> = Rc<dyn Fn(&mut S, SummaryAction) + 'static>;

/// Build the card. `status_bar` is React's optional `statusBar` node slot.
#[allow(dead_code)] // view entry; remove when the result host wires the card.
pub fn build<S: 'static>(
    vm: &ExecutionSummaryCardVm,
    status_bar: Option<El>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut card = col()
        .key("execution-summary-inner")
        .w_full()
        .min_w(0.0)
        .gap(12.0)
        .center()
        .child(icon(&ICON_CHECK, 24.0).color(k.c.text_accent));
    card = card.child(
        text(t("query.executionSuccess"))
            .key(TITLE_KEY)
            .font(k.ty.text_small)
            .color(k.c.text_base),
    );

    let mut pills = row().key("execution-summary-pills").w_full().center().gap(8.0);
    if let Some(rows) = vm.rows_affected_text() {
        pills = pills.child(
            row()
                .key(ROWS_KEY)
                .h(24.0)
                .gap(6.0)
                .px(10.0)
                .center_y()
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .radius(12.0)
                .child(icon(&ICON_TABLE, 14.0).color(k.c.text_accent))
                .child(text(rows).font(k.ty.text_small).color(k.c.text_base)),
        );
    }
    let elapsed = format!("{} ms", vm.result.execution_time_ms);
    pills = pills.child(
        row()
            .key(ELAPSED_KEY)
            .h(24.0)
            .gap(6.0)
            .px(10.0)
            .center_y()
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base)
            .radius(12.0)
            .child(icon(&ICON_CLOCK, 14.0).color(k.c.text_muted))
            .child(text(elapsed).font(k.ty.text_small).color(k.c.text_muted)),
    );
    card = card.child(pills);

    if !vm.result.sql.is_empty() {
        let sql = vm.result.sql.clone();
        let mut copy = row()
            .key(COPY_KEY)
            .h(20.0)
            .gap(4.0)
            .px(6.0)
            .center_y()
            .fill(k.c.surface_weak)
            .border(1.0, k.c.border_base)
            .focusable(true)
            .help("复制 SQL")
            .child(
                icon(if vm.copied { &ICON_CHECK } else { &ICON_COPY }, 12.0)
                    .color(if vm.copied { k.c.text_accent } else { k.c.text_muted }),
            )
            .child(
                text(if vm.copied { "已复制" } else { "复制" }.to_owned())
                    .font(k.ty.text_xsmall)
                    .color(if vm.copied { k.c.text_accent } else { k.c.text_muted }),
            );
        if let Some(on_action) = on_action.clone() {
            copy = copy.on_press(move |s: &mut S| {
                if !sql.is_empty() {
                    on_action(s, SummaryAction::CopySql { sql: sql.clone() });
                }
            });
        }
        card = card.child(
            col()
                .key("execution-summary-sql-wrap")
                .w_full()
                .min_w(0.0)
                .gap(0.0)
                .child(
                    text(vm.result.sql.clone())
                        .key(SQL_KEY)
                        .font(k.ty.mono)
                        .color(k.c.text_base)
                        .max_h(112.0)
                        .clip_soft(0.0)
                        .wrap_anywhere(),
                )
                .child(copy),
        );
    }

    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .gap(0.0)
        .child(row().key("execution-summary-body").w_full().grow(1.0).min_h(0.0).center().child(card));
    if let Some(status) = status_bar {
        root = root.child(row().key(STATUS_KEY).w_full().child(status));
    }
    root
}
