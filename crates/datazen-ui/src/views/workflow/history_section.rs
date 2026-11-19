//! Port of `packages/frontend/src/windows/workflow/WorkflowHistorySection.tsx`.
//!
//! The React file exports two list views over the same recorded runs
//! (`HistoryListItem`, the backend contract's `WorkflowRunListItem`):
//!
//! - [`WorkflowHistoryList`] — the workflows page's sidebar history list:
//!   a right-aligned clear link above full-width run rows (clock icon,
//!   name, formatted `createdAt`, ✓/✗ at the right edge), or the empty
//!   sentence when no run is recorded.
//! - [`WorkflowHistoryTab`] — the AI panel's inline history tab: the same
//!   rows (plus each run's `totalTimeMs` in the meta line,
//!   `justify-between`) under a red clear link — or, with a `detail`
//!   result, the back link above the ported [`WorkflowExecutionResultPanel`].
//!
//! # Immediate-mode state
//!
//! React's two components keep no local state; every interaction is a prop
//! callback (`onView` / `onClear` / `onCloseDetail`). Rune keeps that shape:
//! the builders are stateless and hand each press to the host's `S` through
//! function-pointer slots ([`HistoryListSlots`], [`HistoryTabSlots`]). The
//! detail branch embeds the result panel, whose per-step expansion/chart
//! state stays the host's business and rides [`HistoryTabSlots::panel`].
//!
//! # Consumers
//!
//! Neither React caller is ported yet: `WorkflowPage.tsx` (T147) renders the
//! list in the sidebar's history side-tab, and the AI `WorkflowPanel.tsx`
//! (T334, segment-3) renders the tab in its history tab. The module therefore
//! has no production mount yet — the mod-level `dead_code` allow and the
//! re-export's `unused_imports` allow in `views/workflow/mod.rs` come off
//! when the first of them lands.
//!
//! # Deliberate boundaries
//!
//! - **`toLocaleString()`**: `new Date(item.createdAt).toLocaleString()`
//!   becomes [`locale_string`] — the stance `formatLastConnected` already
//!   took: a stable `YYYY-MM-DD HH:MM:SS` in the local zone (locale
//!   ordering is the i18n track's business), over the support layer's
//!   zoned-instant parser. The contract's `createdAt` always carries a zone
//!   (the desktop host writes `to_rfc3339()`), so JS's zone-less
//!   local-time branch never runs here; zone-less or unparseable text
//!   renders verbatim instead of React's degenerate `Invalid Date` — a
//!   refusal to guess, not a loss.
//! - **Text-level hovers**: `hover:text-red-400` / `hover:underline` restyle
//!   text on hover, which the El model paints as a surface overlay; the
//!   kit's standard hover fill stands in (the ruling the form's accent
//!   links took).
//! - **`onMouseDown` / `onContextMenu` suppression**: React's
//!   preventDefault tricks (keep the editor's focus, suppress the native
//!   context menu) target DOM behaviours Rune rows do not have — the rows
//!   take focus on press like the sibling panel's step rows, and no context
//!   menu exists to suppress. `cursor-pointer` on the tab's div rows folds
//!   into the row's `button` role.
//! - **✓/✗ stay text**: every pool font shapes them (the panel's ruling);
//!   the Clock/X marks are their lucide registry icons, like React's.
//!
//! ## Keys (prefix `workflow-history-list` / `workflow-history-tab`)
//!
//! | part | key |
//! |------|-----|
//! | list root | `workflow-history-list` |
//! | clear bar / link / text | `<list>-clear-bar`, `<list>-clear`, `<list>-clear-text` |
//! | run row | `<list>-item-{id}` |
//! | clock / body / name / created / mark | `<list>-item-{id}-clock`, `-body`, `-name`, `-created`, `-status` |
//! | empty sentence | `<list>-empty`, `<list>-empty-text` |
//! | tab root | `workflow-history-tab` |
//! | back link / icon / text | `<tab>-back`, `<tab>-back-icon`, `<tab>-back-text` |
//! | clear link / text | `<tab>-clear`, `<tab>-clear-text` |
//! | run row | `<tab>-item-{id}` |
//! | clock / body / name / meta / created / time / mark | `<tab>-item-{id}-clock`, `-body`, `-name`, `-meta`, `-created`, `-time`, `-status` |
//! | empty sentence | `<tab>-empty`, `<tab>-empty-text` |
//!
//! The detail branch embeds the result panel under its own
//! `workflow-execution-result` prefix (see that module's key table).

use rune::{col, icon, row, text, Align, Color, El, TextStyle};
use rune_kit::Kit;

use super::execution_result_panel::{StepUiSlots, WorkflowExecutionResultPanel};
use super::step_result_utils::WorkflowExecutionResult;
use crate::i18n::t;
use crate::icons::{ICON_CLOCK, ICON_X};
use crate::support::formatters::parse_zoned_instant;
use crate::ui::kit::kit;

/// Mirror of the contract's `HistoryListItem` — the backend-client
/// `WorkflowRunListItem` both surfaces answer with (`id`, `workflowId`,
/// `workflowName`, `success`, `totalTimeMs`, `createdAt`; the desktop IPC
/// and the HTTP surface agree on the one shape). It lives here, beside the
/// section that renders it, until the host contract formalises the run-list
/// shape — the same stance the run-result VMs in `step_result_utils` took
/// (the API crate carries only the lite run report today).
#[derive(Clone, Debug)]
pub(crate) struct HistoryListItem {
    pub(crate) id: String,
    /// The run's workflow id is part of the contract's shape, but no render
    /// in this section reads it (React hands `onView` the *name*; the row
    /// key needs only the run id) — kept for the shape's parity until the
    /// consumers that will read it (T147/T334) land.
    #[allow(dead_code)]
    pub(crate) workflow_id: String,
    pub(crate) workflow_name: String,
    pub(crate) success: bool,
    pub(crate) total_time_ms: u64,
    /// Zoned ISO text (the desktop host writes `to_rfc3339()`); the rows
    /// render it through [`locale_string`].
    pub(crate) created_at: String,
}

/// React's `new Date(item.createdAt).toLocaleString()`: the local-zone
/// `YYYY-MM-DD HH:MM:SS` the connections list already renders (see the
/// module docs for the stance). Zone-less or unparseable text renders
/// verbatim.
pub(crate) fn locale_string(created_at: &str) -> String {
    match parse_zoned_instant(created_at) {
        Some(millis) => match chrono::DateTime::from_timestamp_millis(millis) {
            Some(instant) => instant
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            None => created_at.to_owned(),
        },
        None => created_at.to_owned(),
    }
}

/// The offset-pinned core of [`locale_string`] (`offset_seconds` is east of
/// UTC, `+08:00` is `28_800`) — the same seam `format_last_connected` tests
/// through, so the clock math pins without a TZ-pinned process.
pub(crate) fn locale_string_with_offset(created_at: &str, offset_seconds: i32) -> String {
    match (
        parse_zoned_instant(created_at).and_then(chrono::DateTime::from_timestamp_millis),
        chrono::FixedOffset::east_opt(offset_seconds),
    ) {
        (Some(instant), Some(zone)) => instant
            .with_timezone(&zone)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        // An offset beyond the IANA range (|seconds| ≥ 24h) is not a zone a
        // clock would answer with; the raw text stays, like every other
        // unrenderable input.
        _ => created_at.to_owned(),
    }
}

/// Where the sidebar list's interactions live in the host's `S`. Function
/// pointers, so the host keeps the state and the list stays stateless —
/// React's `onView`/`onClear` props with the press wiring the page owns.
pub(crate) struct HistoryListSlots<S: 'static> {
    /// React's `onView(id, workflowName)`: open the run's detail elsewhere
    /// (the page's separate panel).
    pub(crate) view: fn(&mut S, String, String),
    /// React's `onClear`: clear the recorded runs.
    pub(crate) clear: fn(&mut S),
}

/// Where the inline tab's interactions live in the host's `S`, plus the
/// embedded result panel's per-step state.
pub(crate) struct HistoryTabSlots<S: 'static> {
    /// React's `onView(id)`: fetch the run and set the detail.
    pub(crate) view: fn(&mut S, String),
    /// React's `onClear`.
    pub(crate) clear: fn(&mut S),
    /// React's `onCloseDetail`: drop the detail and show the list again.
    pub(crate) close_detail: fn(&mut S),
    /// The detail branch embeds `WorkflowExecutionResultPanel`; its per-step
    /// expansion/chart state rides along ([`StepUiSlots`]).
    pub(crate) panel: StepUiSlots<S>,
}

/// The sidebar history list (React `WorkflowHistoryList`). `items` are the
/// recorded runs; `slots` locate the interactions in the app's `S`.
pub(crate) struct WorkflowHistoryList<'a, S: 'static> {
    items: &'a [HistoryListItem],
    slots: HistoryListSlots<S>,
}

impl<'a, S: 'static> WorkflowHistoryList<'a, S> {
    pub(crate) fn new(items: &'a [HistoryListItem], slots: HistoryListSlots<S>) -> Self {
        Self { items, slots }
    }

    /// Builds the list for one appearance (React's whole component body).
    pub(crate) fn build(self, dark: bool) -> El {
        let ctx = RenderCtx::new(kit(dark));
        // React's `p-1.5` root.
        let mut list = col()
            .key("workflow-history-list")
            .w_full()
            .shrink()
            .padding(6.0);
        if !self.items.is_empty() {
            list = list.child(clear_bar(self.slots.clear, &ctx));
        }
        for item in self.items {
            list = list.child(list_item(item, &self.slots, &ctx));
        }
        if self.items.is_empty() {
            list = list.child(empty_sentence(&ctx, "workflow-history-list", 24.0));
        }
        list
    }
}

/// The inline history tab (React `WorkflowHistoryTab`). `items` are the
/// recorded runs; `detail` is the run result the tab shows instead of the
/// list; `slots` locate the interactions (and the embedded panel's per-step
/// state) in the app's `S`.
pub(crate) struct WorkflowHistoryTab<'a, S: 'static> {
    items: &'a [HistoryListItem],
    detail: Option<&'a WorkflowExecutionResult>,
    slots: HistoryTabSlots<S>,
}

impl<'a, S: 'static> WorkflowHistoryTab<'a, S> {
    pub(crate) fn new(
        items: &'a [HistoryListItem],
        detail: Option<&'a WorkflowExecutionResult>,
        slots: HistoryTabSlots<S>,
    ) -> Self {
        Self {
            items,
            detail,
            slots,
        }
    }

    /// Builds the tab for one appearance: the detail branch (back link over
    /// the result panel) or the list branch (clear link, rows / empty).
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let ctx = RenderCtx::new(kit(dark));
        if let Some(detail) = self.detail {
            return col()
                .key("workflow-history-tab")
                .w_full()
                .shrink()
                .child(back_link(&self.slots, &ctx))
                .child(
                    WorkflowExecutionResultPanel::<S>::new(detail, self.slots.panel)
                        .build(s, dark),
                );
        }
        // React's `space-y-2` root.
        let mut tab = col()
            .key("workflow-history-tab")
            .w_full()
            .shrink()
            .gap(8.0);
        if !self.items.is_empty() {
            tab = tab.child(clear_link(
                "workflow-history-tab-clear",
                ctx.k.c.danger.fg,
                ctx.f11,
                self.slots.clear,
                &ctx,
            ));
        }
        if self.items.is_empty() {
            tab = tab.child(empty_sentence(&ctx, "workflow-history-tab", 16.0));
        } else {
            for item in self.items {
                tab = tab.child(tab_item(item, &self.slots, &ctx));
            }
        }
        tab
    }
}

/// Immutable render inputs shared by the row builders (the panel's
/// RenderCtx pattern, trimmed to what this section reads).
struct RenderCtx {
    k: &'static Kit,
    f10: TextStyle,
    f11: TextStyle,
    f11_medium: TextStyle,
}

impl RenderCtx {
    fn new(k: &'static Kit) -> Self {
        Self {
            k,
            f10: TextStyle {
                size: 10.0,
                line_height: 13.0,
                ..k.ty.text_xsmall
            },
            f11: TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.text_xsmall
            },
            f11_medium: TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.text_xsmall_medium
            },
        }
    }
}

/// React's two clear links differ only in size and ink (`text-[10px]
/// text-fg-muted` in the sidebar, `text-[11px] text-red-400` in the tab);
/// both read the same sentence and press the host's clear slot.
fn clear_link<S: 'static>(
    key: &str,
    ink: Color,
    font: TextStyle,
    clear: fn(&mut S),
    ctx: &RenderCtx,
) -> El {
    let label = t("workflows.history.clear");
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .hover(ctx.k.c.hover)
        .on_press(move |s: &mut S| clear(s))
        .child(
            text(label.to_owned())
                .key(format!("{key}-text"))
                .font(font)
                .color(ink),
        )
}

/// The sidebar's right-aligned clear bar (React `flex justify-end mb-1`).
fn clear_bar<S: 'static>(clear: fn(&mut S), ctx: &RenderCtx) -> El {
    row()
        .key("workflow-history-list-clear-bar")
        .w_full()
        .shrink()
        .justify_end()
        .mb(4.0)
        .child(clear_link(
            "workflow-history-list-clear",
            ctx.k.c.text_faint,
            ctx.f10,
            clear,
            ctx,
        ))
}

/// One sidebar run row (React's `<button>`: clock, the flex-1 name/created
/// block, ✓/✗ at the right edge).
fn list_item<S: 'static>(
    item: &HistoryListItem,
    slots: &HistoryListSlots<S>,
    ctx: &RenderCtx,
) -> El {
    let k = ctx.k;
    let view = slots.view;
    let (id, name) = (item.id.clone(), item.workflow_name.clone());
    row()
        .key(format!("workflow-history-list-item-{}", item.id))
        .w_full()
        .shrink()
        .items_center()
        .gap(8.0)
        .radius(k.radius.md)
        .px(12.0)
        .py(8.0)
        // React `hover:bg-surface-raised/50` — the sibling panel's mapping.
        .hover(k.alpha(k.c.layer_02, 0.5))
        .focusable(true)
        .role("button")
        .label(item.workflow_name.clone())
        .on_press(move |s: &mut S| view(s, id.clone(), name.clone()))
        .child(
            icon(&ICON_CLOCK, 14.0)
                .key(format!("workflow-history-list-item-{}-clock", item.id))
                .color(k.c.text_faint)
                .shrink(),
        )
        .child(
            col()
                .key(format!("workflow-history-list-item-{}-body", item.id))
                .grow(1.0)
                .min_w(0.0)
                .shrink()
                .child(
                    text(item.workflow_name.clone())
                        .key(format!("workflow-history-list-item-{}-name", item.id))
                        .font(k.ty.text_xsmall_medium)
                        .color(k.c.text_base)
                        .w_full()
                        .min_w(0.0)
                        .shrink()
                        .truncate(),
                )
                .child(
                    text(locale_string(&item.created_at))
                        .key(format!("workflow-history-list-item-{}-created", item.id))
                        .font(ctx.f10)
                        .color(k.c.text_faint)
                        .shrink(),
                ),
        )
        .child(status_mark(item, "workflow-history-list", ctx.f10, ctx))
}

/// One inline-tab run row (React's clickable `<div>`: the flex-1 name block
/// with its clock/createdAt/total-ms meta line, ✓/✗ at the right edge).
fn tab_item<S: 'static>(item: &HistoryListItem, slots: &HistoryTabSlots<S>, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let view = slots.view;
    let id = item.id.clone();
    row()
        .key(format!("workflow-history-tab-item-{}", item.id))
        .w_full()
        .shrink()
        .items_center()
        .justify_between()
        .px(12.0)
        .py(8.0)
        .radius(k.radius.md)
        // React `hover:bg-surface-raised` (full strength, unlike the
        // sidebar's `/50`) — the same token the sibling panel's `/50`
        // hover halves, at its full alpha.
        .hover(k.c.layer_02)
        .focusable(true)
        .role("button")
        .label(item.workflow_name.clone())
        .on_press(move |s: &mut S| view(s, id.clone()))
        .child(
            col()
                .key(format!("workflow-history-tab-item-{}-body", item.id))
                .grow(1.0)
                .min_w(0.0)
                .shrink()
                .child(
                    text(item.workflow_name.clone())
                        .key(format!("workflow-history-tab-item-{}-name", item.id))
                        .font(k.ty.text_xsmall_medium)
                        .color(k.c.text_base)
                        .w_full()
                        .min_w(0.0)
                        .shrink()
                        .truncate(),
                )
                .child(
                    row()
                        .key(format!("workflow-history-tab-item-{}-meta", item.id))
                        .items_center()
                        .gap(8.0)
                        .mt(2.0)
                        .child(
                            icon(&ICON_CLOCK, 12.0)
                                .key(format!("workflow-history-tab-item-{}-clock", item.id))
                                .color(k.c.text_faint)
                                .shrink(),
                        )
                        .child(
                            text(locale_string(&item.created_at))
                                .key(format!("workflow-history-tab-item-{}-created", item.id))
                                .font(ctx.f10)
                                .color(k.c.text_faint)
                                .shrink(),
                        )
                        .child(
                            text(format!("{}ms", item.total_time_ms))
                                .key(format!("workflow-history-tab-item-{}-time", item.id))
                                .font(ctx.f10)
                                .color(k.c.text_faint)
                                .shrink(),
                        ),
                ),
        )
        .child(
            status_mark(item, "workflow-history-tab", ctx.f11_medium, ctx).ml(8.0), // React `ml-2`
        )
}

/// The ✓/✗ mark both lists share: React's green/red span at the caller's
/// size. The glyphs stay text — every pool font shapes them.
fn status_mark(item: &HistoryListItem, prefix: &str, font: TextStyle, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let (mark, ink) = if item.success {
        ("✓", k.c.success.fg)
    } else {
        ("✗", k.c.danger.fg)
    };
    text(mark.to_owned())
        .key(format!("{prefix}-item-{}-status", item.id))
        .font(font)
        .color(ink)
        .shrink()
}

/// The detail branch's back link (React `flex items-center gap-1 text-xs
/// text-accent hover:underline mb-2` with the X icon).
fn back_link<S: 'static>(slots: &HistoryTabSlots<S>, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let label = t("workflows.history.back");
    let close = slots.close_detail;
    row()
        .key("workflow-history-tab-back")
        .mb(8.0)
        .gap(4.0)
        .items_center()
        .focusable(true)
        .role("button")
        .label(label.to_owned())
        .hover(k.c.hover)
        .on_press(move |s: &mut S| close(s))
        .child(
            icon(&ICON_X, 12.0)
                .key("workflow-history-tab-back-icon")
                .color(k.c.text_accent),
        )
        .child(
            text(label.to_owned())
                .key("workflow-history-tab-back-text")
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        )
}

/// The empty sentence (React `py-6`/`py-4` `text-center text-xs
/// text-fg-muted`): one centred muted line.
fn empty_sentence(ctx: &RenderCtx, prefix: &str, py: f32) -> El {
    col()
        .key(format!("{prefix}-empty"))
        .w_full()
        .shrink()
        .py(py)
        .child(
            text(t("workflows.history.empty").to_owned())
                .key(format!("{prefix}-empty-text"))
                .font(ctx.k.ty.text_xsmall)
                .color(ctx.k.c.text_faint)
                .w_full()
                .align(Align::Center),
        )
}
