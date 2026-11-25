//! T076-S2: the `ProgressLog` component (`windows/backup/ProgressLog.tsx`).
//!
//! S1 ([`super::progress_log_state`]) owns the virtual-window math, the
//! auto-scroll decision and the copy-feedback state machine. This slice
//! renders that model: the header, the copy button, and the log body.
//! The Backup window (T026) calls [`ProgressLog::build`] from its
//! options column in place of the inline line loop.
//!
//! ## Keys
//!
//! React `data-testid` values are the element keys, verbatim:
//! `backup-progress-log` (the scroll box) and `backup-progress-log-copy`.
//! Everything else is internal, matching the window slice so its tests
//! keep finding the same nodes:
//!
//! | part | key |
//! |---|---|
//! | wrap | `backup-progress-log-wrap` |
//! | header | `backup-progress-log-header` |
//! | copy button | `backup-progress-log-copy` |
//! | scroll box | `backup-progress-log` |
//! | row | `backup-progress-log-line-{i}` |
//! | empty log | `backup-progress-log-spacer` |
//!
//! ## Empty log
//!
//! React always mounts the header and the copy button, and an empty
//! virtualizer body under them. The Backup window's T026 contract — and
//! the tests that pin it — replaces that whole block with the
//! `backup-progress-log-spacer` while `lines` is empty (the audit calls
//! this out: the copy button exists only for a non-empty log). This
//! component keeps that contract. A host that wants React's always-on
//! header can pass a non-empty placeholder; the window does not.
//!
//! ## Virtualization
//!
//! Rows go through [`El::virtual_rows`] at [`ROW_HEIGHT`], inside the
//! scroll box. Layout builds only the mounted window, so a 1500-line
//! pump does not become 1500 text nodes. That is the engine's window,
//! not a hand-placed `translateY` of S1's [`visible_range`]: Rune lays
//! the mounted rows out itself, and S1's arithmetic stays the spec the
//! model tests pin. The engine mounts the viewport plus up to two
//! viewports of overscan, which is the same idea as React's
//! `overscan: 20` (a band around the visible rows, not the whole log).
//!
//! ## Auto-scroll
//!
//! React's effect sets `scrollTop = scrollHeight` while
//! `shouldAutoScroll` is set. The scroll box's `stick_bottom` is that
//! flag ([`ProgressLog::auto_scroll`], default `true` — React's
//! `useRef(true)`). `on_scroll_offset` cannot write the host state, so
//! the host still owns [`ProgressLogState::on_scroll`] and passes the
//! result back in; this slice does not invent a second scroll listener.
//!
//! ## Copy
//!
//! The button label is `backup.logCopied` while [`ProgressLog::copied`]
//! is set, otherwise `backup.copyLog`. The press reports through
//! `on_copy`; the host runs [`copy_text`] and
//! [`ProgressLogState::copy_pressed`]. The clipboard write stays on the
//! host, as in S1.
//!
//! ## Other deviations
//!
//! * **No native `title` tooltip on a row.** React sets `title` to the
//!   full line. The row is still single-line and truncated
//!   (`truncate`, `whitespace-nowrap`); a hover tooltip would be a
//!   second overlay and is not part of this slice.
//! * **Row height is the estimate.** Every line is [`ROW_HEIGHT`]
//!   whether or not the glyphs are wider than the box. S1 already
//!   documents that nothing here measures text.

use std::borrow::Cow;
use std::rc::Rc;

use rune::{col, icon, row, text, El, GrowthSpec, ScrollSpec, TextStyle};
use rune_kit::Kit;

use super::progress_log_state::ROW_HEIGHT;
use crate::i18n::t;
use crate::icons::ICON_COPY;

/// `windows/backup/ProgressLog.tsx`. `S` is the host state the copy
/// press calls back into. The host owns the lines and the
/// [`super::progress_log_state::ProgressLogState`]; this value only
/// borrows the lines for the build.
pub struct ProgressLog<'a, S: 'static> {
    lines: &'a [String],
    copied: bool,
    auto_scroll: bool,
    on_copy: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> ProgressLog<'a, S> {
    /// A log over `lines`. Copy confirmation is off and auto-scroll is
    /// pinned, matching React's initial `useRef(true)` / `copied === false`.
    pub fn new(lines: &'a [String]) -> Self {
        Self {
            lines,
            copied: false,
            auto_scroll: true,
            on_copy: None,
        }
    }

    /// React `copied`: show `backup.logCopied` instead of `backup.copyLog`.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// React `shouldAutoScroll.current`, rendered as the scroll box's
    /// `stick_bottom`. The host updates it from
    /// [`super::progress_log_state::ProgressLogState::on_scroll`].
    pub fn auto_scroll(mut self, auto_scroll: bool) -> Self {
        self.auto_scroll = auto_scroll;
        self
    }

    /// The copy button's press. The host copies [`super::progress_log_state::copy_text`]
    /// and arms the feedback timer.
    pub fn on_copy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// The element tree. Empty `lines` is the spacer (see the module
    /// docs); otherwise the header, the copy button and the virtualized
    /// body.
    pub fn build(self, k: &Kit) -> El {
        if self.lines.is_empty() {
            return col()
                .key("backup-progress-log-spacer")
                .grow(1.0)
                .min_h(0.0);
        }
        let copy = self.on_copy.unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        let label = if self.copied {
            t("backup.logCopied")
        } else {
            t("backup.copyLog")
        };
        let lines = self.lines.to_vec();
        let count = lines.len();
        let row_h = ROW_HEIGHT as f32;
        let ink = k.c.text_muted;
        let rows = col()
            .key("backup-progress-log-rows")
            .w_full()
            .growth(GrowthSpec::default())
            .virtual_rows(
                count,
                row_h,
                |_state: &S, index| Cow::Owned(format!("backup-progress-log-line-{index}")),
                move |_state: &S, index| {
                    let line = lines.get(index).cloned().unwrap_or_default();
                    text(line)
                        .font(TextStyle::new(11.0, 400.0).mono().lh(row_h))
                        .color(ink)
                        .h(row_h)
                        .truncate()
                },
            );
        col()
            .key("backup-progress-log-wrap")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .mb(12.0)
            .child(
                row()
                    .key("backup-progress-log-header")
                    .w_full()
                    .mb(4.0)
                    .center_y()
                    .child(
                        text(t("backup.progressLog").to_owned())
                            .font(k.ty.small_medium)
                            .color(k.c.text_faint)
                            .grow(1.0)
                            .min_w(0.0),
                    )
                    .child(
                        row()
                            .key("backup-progress-log-copy")
                            .px(6.0)
                            .py(2.0)
                            .gap(4.0)
                            .center_y()
                            .radius(4.0)
                            .hover(k.c.hover)
                            .on_press(move |state: &mut S| copy(state))
                            .child(icon(&ICON_COPY, 12.0).color(k.c.text_faint))
                            .child(
                                text(label.to_owned())
                                    .font(k.ty.small)
                                    .color(k.c.text_faint),
                            ),
                    ),
            )
            .child(
                col()
                    .key("backup-progress-log")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .px(8.0)
                    .py(4.0)
                    .radius(6.0)
                    .fill(k.c.layer_01)
                    .border(1.0, k.c.border_base)
                    .scroll(ScrollSpec {
                        stick_bottom: self.auto_scroll,
                    })
                    .child(rows),
            )
    }
}
