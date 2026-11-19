//! Port of `packages/frontend/src/windows/main/RunningExecutionCloseHost.tsx`
//! (T071 — the segment-2 coordinator's label for this task is T070).
//!
//! The prompt behind "close a tab whose query is still running". The store
//! (`stores/runningExecutionClose.ts`) cannot open a dialog, so it asks a
//! gate; the React component installs that gate for as long as the main
//! window lives and renders the confirm dialog the gate opens. Mounting it
//! at the workspace root (rather than inside one connection's content view)
//! is deliberate: tabs are also removed when a whole connection is closed,
//! which can happen while a different page is on screen, and a prompt nobody
//! can render would leave the close waiting forever.
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! |---|---|
//!| `useConfirmDialog()`'s single pending `{options, resolve}` state | [`AppState::running_close_request`] (+ [`AppState::running_close_epoch`]: each ask is a fresh mount, so the body reopens at the top) |
//!| `setRunningExecutionGate(gate)` on mount | the host element being composed by [`crate::views::main_page`] every frame — the single-window host never unmounts, so the gate is installed for as long as the window lives |
//!| the gate closure deriving `kind/title/message/confirmLabel/badge/codePreview` from the request | the same branches in [`running_close_dialog`] |
//!| `handleClose(true/false)` resolving the ask | [`AppState::answer_running_execution_close`] |
//!| `return dialog` (null while idle) | the layer entry mounts nothing while no ask is pending |
//!| MainPage mounts the host only on the healthy branch (`MainPage.tsx:140`; the load-error early return at `:112-133` omits it) | `main_page` composes [`running_execution_close_host`] next to the modal layer, on the healthy branch only |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - The **store half** — `askRunningExecutionGate`'s ask queue that
//!   serializes several closes behind one prompt ("close all" over several
//!   running tabs), `settleRunningPanes`, `createRunningCloseGuard` — is
//!   `stores/runningExecutionClose.ts`, T396; `hooks/useConfirmDialog.tsx`
//!   is T038. Until the guard ports, no production close path calls the ask:
//!   the tab strip's close button keeps closing directly, exactly as before.
//!   The host's half of the contract — [`AppState::ask_running_execution_close`]
//!   and [`AppState::answer_running_execution_close`] — is the surface that
//!   guard will call.
//! - The gate-unmount branch (`setRunningExecutionGate(null)` releasing a
//!   still-open prompt as *allowed*) has no Rune counterpart: the host is
//!   composed by the main window's root every frame and never unmounts.
//! - The confirm-dialog surface comes from `packages/ui/src/ConfirmDialog.tsx`
//!   (the frontend's `components/ui/ConfirmDialog.tsx` is a re-export shell,
//!   not a ledger line of its own), rendered here inline; a shared
//!   `ui/confirm` base gets extracted when the second consumer (T227's
//!   `TransferExecuteConfirmDialog` / T038's hook) lands.
//! - React's `useCopyFeedback(2000)` reverts the copy control's label after
//!   two seconds. The port has no view-side clock (the same documented gap
//!   the production `CopyButton` mounts carry), so the copied mark stays
//!   until the prompt closes; the next ask resets it.
//! - React's warning icon and badge are Tailwind `amber-500`/`amber-400`;
//!   the kit has no amber token, so they read the kit's `c.warning` tokens —
//!   the precedent set by the tunnel-missing warning
//!   (`components/connection/connection_advanced_settings.rs`).
//!
//! ## Keys
//!
//! | part | key |
//!|---|---|
//! | panel / header / title / close / body / footer / scrim / layer | `running-close-dialog`, `-header`, `-title`, `-close`, `-body`, `-footer`, `-scrim`, `-layer` (from the `ui/dialog` base) |
//! | message row / message / warning glyph / badge | `running-close-dialog-message-row`, `-message`, `-warning`, `-badge` (pill) / `-badge-text` (label) |
//! | code box / header / mono text / truncation notice | `running-close-dialog-code`, `-code-head`, `-code-pre` (the scrolling pre) / `-code-text` (its text) / `-code-truncated` |
//! | footer buttons / copy control | `confirm-dialog-ok`, `confirm-dialog-cancel`, `confirm-dialog-copy-sql` (React's testids) |

use crate::i18n::t;
use crate::icons::{ICON_ALERT_TRIANGLE, ICON_COPY};
use crate::state_def::AppState;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use rune::{col, icon, row, text, El, ScrollSpec, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

/// The ask the store hands the host before a tab whose statement has not
/// finished is taken away — React `RunningExecutionRequest`
/// (`stores/runningExecutionClose.ts:56-65`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningExecutionCloseRequest {
    /// The tab's panel id (React `panelId`).
    #[allow(dead_code)] // the close guard reads it when it ports (T396); carried for the ask payload's parity
    pub(crate) panel_id: String,
    /// The tab title, so the prompt can say which tab it is asking about
    /// (React `title` — the badge next to the message).
    pub(crate) title: String,
    /// The pane whose SQL the prompt shows (React `paneId`).
    #[allow(dead_code)] // the close guard keys its cancels by it when it ports (T396)
    pub(crate) pane_id: String,
    /// The pane's SQL (React `sql` — the code preview; empty means no
    /// preview, as React's `{preview && ...}` leaves it out for `''`).
    pub(crate) sql: String,
    /// How many panes of this tab are still running (React `runningPanes`)
    /// — the message's singular/plural branch.
    pub(crate) running_panes: u32,
}

/// The code preview's line cap (React `ConfirmDialog.tsx`'s
/// `CODE_PREVIEW_MAX_LINES`).
const CODE_PREVIEW_MAX_LINES: usize = 12;

/// The code preview's character cap (React's `CODE_PREVIEW_MAX_CHARS`).
const CODE_PREVIEW_MAX_CHARS: usize = 2000;

/// The host's layer entry — React's `return dialog`: nothing while no ask is
/// pending, the confirm dialog the gate opened while one is. Mounted by
/// `main_page` next to the root modal layer, on the healthy branch only.
pub(crate) fn running_execution_close_host(state: &AppState, dark: bool) -> Vec<El> {
    let mut layer = Vec::new();
    if let Some(request) = &state.running_close_request {
        running_close_dialog(
            request,
            state.running_close_epoch,
            state.running_close_sql_copied,
            dark,
        )
        .build(kit(dark), &mut layer);
    }
    layer
}

/// The confirm dialog React derives from an ask (`RunningExecutionCloseHost.tsx:24-36`
/// over `packages/ui/src/ConfirmDialog.tsx`): the warning kind, the
/// singular/plural message, the tab title as the badge, the SQL as the code
/// preview, and the two answers — "stop and close" (danger) and "keep the
/// tab" (ghost). The header close and Escape answer *keep the tab*, as
/// React's `onClose={onCancel}` does.
fn running_close_dialog(
    request: &RunningExecutionCloseRequest,
    epoch: u64,
    copied: bool,
    dark: bool,
) -> Dialog {
    // React's message branch: the plural sentence names the count.
    let message = if request.running_panes > 1 {
        t("query.session.closeTabActiveExecutionMany")
            .replace("{count}", &request.running_panes.to_string())
    } else {
        t("query.session.closeTabActiveExecution").to_owned()
    };

    Dialog::new("running-close-dialog", t("query.session.closeTabActiveExecutionTitle"))
        .open(true)
        .open_epoch(epoch)
        // React's `max-w-lg` on the packages/ui Dialog.
        .width(512.0)
        .fit()
        // React caps the body at 70vh; 448 keeps the same figure inside the
        // 640pt minimum window (the import dialog's documented cap).
        .body_max_height(448.0)
        .child(running_close_body(request, message, copied, dark))
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                // React's `h-8` (32pt) has no kit metric; Large (28pt) is the
                // nearest and keeps the confirmation more compact than the md
                // dialogs' h-9 → ExtraLarge.
                .control_size(ControlSize::Large)
                .on_press(|state: &mut AppState| state.answer_running_execution_close(false))
                .build(kit(dark))
                .key("confirm-dialog-cancel"),
        )
        .footer(
            Button::new(t("query.session.closeTabActiveExecutionConfirm"))
                // React's warning kind: the confirm button is the danger
                // variant.
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Large)
                .on_press(|state: &mut AppState| state.answer_running_execution_close(true))
                .build(kit(dark))
                .key("confirm-dialog-ok"),
        )
        // React's Dialog `onClose={onCancel}`: the header close and Escape
        // are the same answer as the cancel button — keep the tab.
        .on_close(|state: &mut AppState| state.answer_running_execution_close(false))
}

/// The dialog's body: React's `space-y-3` column — the warning message row
/// (icon + message + badge), then the SQL preview when the ask carries SQL.
fn running_close_body(
    request: &RunningExecutionCloseRequest,
    message: String,
    copied: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut body = col().w_full().gap(12.0).child(
        // React: `flex items-start gap-3` — the warning glyph leads the
        // message (kind === 'warning').
        row()
            .key("running-close-dialog-message-row")
            .w_full()
            .items_start()
            .gap(12.0)
            .child(
                icon(&ICON_ALERT_TRIANGLE, 20.0)
                    .key("running-close-dialog-warning")
                    // React `mt-0.5`: the glyph centers on the first line.
                    .mt(2.0)
                    .color(k.c.warning.fg),
            )
            .child(
                col().grow(1.0).min_w(0.0).child(
                    row()
                        .key("running-close-dialog-message-line")
                        .items_center()
                        .gap(8.0)
                        .child(
                            text(message)
                                .key("running-close-dialog-message")
                                // React `text-sm text-fg-secondary`.
                                .font(k.ty.body)
                                .color(k.c.text_muted)
                                .wrap()
                                .grow(1.0)
                                .min_w(0.0),
                        )
                        .child(badge_pill(&request.title, &k)),
                ),
            ),
    );
    if !request.sql.is_empty() {
        body = body.child(code_preview(&request.sql, copied, dark));
    }
    body
}

/// The tab-title badge — React's amber pill
/// (`bg-amber-500/15 text-amber-400 text-[10px] font-medium rounded-full px-2 py-0.5`)
/// on the kit's warning tokens.
fn badge_pill(label: &str, k: &Kit) -> El {
    row()
        .key("running-close-dialog-badge")
        .shrink()
        .h(18.0)
        .px(8.0)
        .center_y()
        .radius(9.0)
        .fill(k.alpha(k.c.warning.fg, 0.15))
        .child(
            text(label.to_owned())
                .key("running-close-dialog-badge-text")
                .font(TextStyle::new(10.0, 500.0))
                .color(k.c.warning.fg),
        )
}

/// The SQL preview — React's code section: a bordered box whose header row
/// carries the truncation notice and the copy control, over the mono text.
fn code_preview(sql: &str, copied: bool, dark: bool) -> El {
    let k = kit(dark);
    let (shown, truncated) = truncate_code_preview(sql);
    let visible = if copied { "✓" } else { t("query.editor.executionConfirm.copySql") };

    let mut header = row()
        .key("running-close-dialog-code-head")
        .w_full()
        .items_center()
        .justify_between()
        .px(12.0)
        .py(6.0)
        // React's `border-b border-edge`.
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
        .border_layout();
    if truncated {
        header = header.child(
            text(
                t("query.editor.executionConfirm.previewTruncated")
                    .replace("{lines}", &CODE_PREVIEW_MAX_LINES.to_string()),
            )
            .key("running-close-dialog-code-truncated")
            .font(TextStyle::new(10.0, 400.0))
            .color(k.c.text_muted),
        );
    } else {
        // React renders `{false && …}` — an empty slot, so the copy control
        // keeps the row to itself.
        header = header.child(col().grow(1.0));
    }
    let copy_sql = sql.to_owned();
    header = header.child(
        row()
            .key("confirm-dialog-copy-sql")
            .role("button")
            .focusable(true)
            .items_center()
            .gap(4.0)
            .label(visible)
            .on_press_tx(move |state: &mut AppState, tx: &mut Tx| {
                tx.copy(copy_sql.clone());
                state.running_close_sql_copied = true;
            })
            .child(icon(&ICON_COPY, 12.0).color(k.c.text_muted))
            .child(
                text(visible.to_owned())
                    .font(TextStyle::new(10.0, 400.0))
                    .color(k.c.text_muted),
            ),
    );

    col()
        .key("running-close-dialog-code")
        .w_full()
        .radius(k.radius.md)
        .border(1.0, k.c.border_muted)
        .fill(k.c.layer_01)
        .child(header)
        .child(
            // React's `<pre>`: `max-h-40 overflow-auto p-3 font-mono text-xs
            // text-fg-secondary`. The pre keeps `white-space: pre` in React
            // and scrolls a long line; the Rune box wraps instead so the
            // 512pt panel never widens past its cap.
            col()
                .key("running-close-dialog-code-pre")
                .w_full()
                .max_h(160.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .px(12.0)
                .py(12.0)
                .child(
                    text(shown)
                        .key("running-close-dialog-code-text")
                        .font(k.ty.mono)
                        .color(k.c.text_muted)
                        .wrap_anywhere()
                        .selectable(true),
                ),
        )
}

/// React's `truncateCodePreview`: whole when the SQL fits both caps, else the
/// first [`CODE_PREVIEW_MAX_LINES`] lines with an ellipsis line.
fn truncate_code_preview(raw: &str) -> (String, bool) {
    let lines = raw.split('\n').count();
    if lines <= CODE_PREVIEW_MAX_LINES && raw.chars().count() <= CODE_PREVIEW_MAX_CHARS {
        return (raw.to_owned(), false);
    }
    let mut shown: String = raw
        .split('\n')
        .take(CODE_PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    shown.push('\n');
    shown.push('…');
    (shown, true)
}
