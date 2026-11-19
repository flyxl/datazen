//! Port of `packages/frontend/src/windows/dashboard/WidgetEditorDrawer.tsx`
//! (T201).
//!
//! The widget edit drawer: a right-anchored overlay (`fixed inset-0 z-50
//! flex justify-end` — scrim plus a `max-w-md` aside with a left edge rule)
//! the dashboard panel mounts over the grid while a widget is being created
//! or edited. The scrollable body is the form over the widget config — the
//! title, then either the hidden-SQL pair (connection select + the SQL
//! editor) or the workflow source (select or read-only field, plus the
//! open-workflow-editor link), the refresh mode with its preset select and
//! the value/unit interval pair (with the dense-interval warning), the
//! T146 monitor-schedule authorization card for saved interval widgets, the
//! enabled checkbox, the default view mode, the chart type and y-axes when
//! the view is chart, and the alert-rule card — with the save/discard
//! footer. Saving applies React's two guards (a blank title, or a hidden-SQL
//! drawer without a connection or blank SQL) and normalizes the payload
//! (trimmed title, clamped refresh policy).
//!
//! ## What maps where
//!
//! - `useConnectionStore` is a host concern, not Rune's: the open effect's
//!   `void fetchConnections()` becomes the host polling
//!   [`WidgetEditorDrawerState::needs_resync`] (React's `[open, widget,
//!   hiddenSqlProp]` dep array; object identity is value equality here, the
//!   T146/T175 stance — the panel only swaps `editingWidget` when opening),
//!   applying [`WidgetEditorDrawerState::begin_open`], then fetching the
//!   connections out-of-band and delivering them through
//!   [`WidgetEditorDrawerState::deliver_connections`] (token-free, like the
//!   store subscription React reads — a late delivery after close just
//!   updates the list). `crypto.randomUUID()` in `emptyDraft()` is the
//!   host's too ([`begin_open`] takes the fresh id, the T112 ruling).
//! - The three `useState` values (`draft`, `hiddenSql`,
//!   `forceCustomRefresh`) plus the open-time props (`isNew`, `dashboardId`,
//!   the original `widget` / `hiddenSqlProp` the dirty guard compares
//!   against) live in [`WidgetEditorDrawerState`]. The string fields the
//!   React form controls keep `TextEdit` editing surfaces beside the draft
//!   (`title_edit`, `interval_edit`, `y_axes_edit`, `alert_column_edit`,
//!   `threshold_edit`, and the read-only `workflow_readonly_edit`); each
//!   `onChange` is an `apply_*_text` method the host runs from the field's
//!   `on_change` (the kit lands the keystroke first, the T070 coercion
//!   convention).
//! - React's controlled inputs re-derive their display from the draft on
//!   every keystroke — `value={secToUnitAndValue(...).value}`,
//!   `value={yAxes.join(', ')}`, `value={threshold}` — so the round-trip is
//!   part of the behaviour: typing `4` minutes clamps to 120s and re-renders
//!   as `2`, a trailing comma in the y-axes field vanishes, a typed `3.`
//!   threshold normalizes to `3`. The port reproduces this by re-seeding
//!   the `TextEdit` inside the same `apply_*_text` call whenever the
//!   derived text differs (one round converges; `replace_text` parks the
//!   caret at the end, a documented delta for mid-string edits).
//! - `handleSave` is [`WidgetEditorDrawerState::save_payload`]: `None`
//!   while a guard blocks the save (React's early `return`), else the
//!   normalized widget (trimmed title,
//!   [`normalize_refresh_policy`]-clamped refresh) with the alert rule and,
//!   in hidden-SQL mode, the `hiddenSql` pair. The save button stays
//!   pressable while blocked — React's is never `disabled` — but carries no
//!   action, so the press lands nothing (the same observable nothing).
//! - The monitor card's `dirty` prop — `JSON.stringify(draft) !==
//!   JSON.stringify(widget) || JSON.stringify(hiddenSql) !== JSON.stringify(
//!   hiddenSqlProp ?? default)` — is [`WidgetEditorDrawerState::is_dirty`]
//!   (a structural compare; `undefined` alert fields are absent from
//!   React's JSON in both operands, so `None` compares equal to `None`).
//!   Its mount gate `dashboardId && widget && !isNew &&
//!   widget.refresh.mode === 'interval'` reads the **saved** widget's mode,
//!   not the draft's, and is [`Self::monitor_card_pair`].
//! - `MonitorScheduleAuthorization` (T146) mounts with its state nested in
//!   this one (`monitor`); the host runs the card's own
//!   `needs_resync`/`begin_resync`/`deliver_*` cycle against the pair
//!   [`WidgetEditorDrawerState::monitor_card_pair`] returns, and its
//!   authorize/revoke IPC through the drawer's
//!   [`WidgetEditorDrawer::on_monitor_authorize`] /
//!   [`WidgetEditorDrawer::on_monitor_revoke`] slots.
//! - The `SqlEditor` (the CodeMirror engine batch) is a **seam**, the T069
//!   axis-configurator convention: the host builds the editing surface
//!   against the real app state and supplies it through
//!   [`WidgetEditorDrawer::sql_editor`]; the drawer paints it inside
//!   React's `h-40` border box (`data-testid="dashboard-sql-editor"`) and
//!   the host delivers the text back through
//!   [`WidgetEditorDrawerState::set_hidden_sql`]. The `databaseType` prop
//!   the React editor receives is the host's lookup (the drawer never reads
//!   it).
//! - `onClose` / `onSave` / `onOpenWorkflowEditor` are builder callbacks;
//!   every field-level transition (edits, selects, checkboxes) flows through
//!   the single [`state_fn`](WidgetEditorDrawer::state_fn) projection into
//!   the drawer state, the T112 `PanelStateFn` convention — without it the
//!   drawer renders but its presses no-op (the not-yet-wired host's case,
//!   the tile precedent).
//! - The kit `Select` is host-driven like every ported select: the one open
//!   listbox is [`WidgetEditorSelect`] in the state (the workflow-form
//!   convention — React's `ui/Select` keeps its own open state, and closing
//!   on a choice is the choose handler's job, reproduced by every
//!   `set_*` method).
//! - The refresh helpers (`clampRefreshSec`, `normalizeRefreshPolicy`,
//!   `shouldWarnRefreshSec`, `MIN_REFRESH_SEC`,
//!   `REFRESH_WARN_BELOW_SEC`) are the `types/dashboard.ts` runtime
//!   constants, ported here for this drawer; the future `types/dashboard.ts`
//!   logic task rehomes them.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; everything else derives under
//! one prefix — the root testid `widget-editor-` (the sibling key-family
//! rule; the T175 audit's repair):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid`) | `widget-editor-drawer` |
//! | scrim / aside panel | `widget-editor-drawer-scrim` / `widget-editor-drawer-panel` |
//! | header / title / close | `widget-editor-drawer-header` / `-title` / `-close` |
//! | body scroll | `widget-editor-drawer-body` |
//! | title field | `widget-editor-title` |
//! | hidden branch connection label + select | `widget-editor-connection` / `widget-editor-connection-select` (trigger `-trigger` from `ui/select.rs`) |
//! | hidden branch SQL label | `widget-editor-sql` |
//! | SQL editor box (`data-testid`) | `dashboard-sql-editor` |
//! | workflow block / label | `widget-editor-workflow-block` / `widget-editor-workflow` |
//! | workflow select / read-only field | `widget-editor-workflow-select` / `widget-editor-workflow-readonly` |
//! | open-workflow-editor (`data-testid`) | `widget-open-workflow-editor` |
//! | refresh mode label + select | `widget-editor-refresh-mode` / `widget-editor-refresh-mode-select` |
//! | interval block / label / row | `widget-editor-refresh-interval` / `-label` / `-row` |
//! | interval value / unit select | `widget-editor-interval-value` / `widget-editor-interval-unit` |
//! | interval hint / warn (`data-testid`) | `widget-editor-interval-hint` / `refresh-sec-warn` |
//! | monitor card | T146's `monitor-schedule-*` family |
//! | enabled checkbox | `widget-editor-enabled` |
//! | view mode label + select | `widget-editor-view-mode` / `widget-editor-view-mode-select` |
//! | chart type label + select | `widget-editor-chart-type` / `widget-editor-chart-type-select` |
//! | y-axes label + field | `widget-editor-y-axes` / `widget-editor-y-axes-field` |
//! | alert card / enabled / fields | `widget-editor-alert` / `-enabled` / `-fields` |
//! | alert column / operator / threshold | `widget-editor-alert-column` / `-operator` / `-threshold` (fields `-field`, operator select `-select`) |
//! | footer / cancel / save (`data-testid`) | `widget-editor-drawer-footer` / `widget-editor-cancel` / `widget-editor-save` |
//!
//! Known deltas (documented, not silent): `Input` maps to the kit
//! `TextField` at React's `h-10` (40pt) in the mono 13px face (the
//! column-mapping mapping); the read-only workflow field is a static mono
//! box (Rune fields without `on_edit` cannot take a caret — React's
//! `readOnly` input keeps normal looks and text selection); `type="number"`
//! has no spinner and accepts any text (the parse rules stand in: JS
//! `Number` semantics, unparseable → 0, and a fractional typed interval
//! truncates to the whole second — `RefreshPolicyVm`'s `u32`, T112's
//! stance); kit button skins stand in for the React variants (`ghost` →
//! `Ghost`, default `primary` → `Contrast`, the T018/T022 mappings) with
//! `h-8`/`h-7` both at `ControlSize::Large` (the area precedent);
//! `shadow-xl` is `k.elevate(elevation.overlay)`; `space-y-*` are column
//! gaps; the checkboxes are the kit `Checkbox` (no `rounded border-edge`
//! hand-rolling).
//!
//! The file is split at the 800-line ceiling: `refresh.rs` carries the
//! `types/dashboard.ts` vocabulary, `state.rs` the view models and the
//! drawer state, and `body.rs` the form body half of the React component
//! (everything inside the scroll column).

mod body;
mod refresh;
mod state;

// The re-exports below serve the audit and the dashboard host track;
// the lib target alone reaches only some of them.
#[allow(unused_imports)]
pub(crate) use refresh::{
    IntervalUnit, RefreshPreset, MIN_REFRESH_SEC, REFRESH_WARN_BELOW_SEC, clamp_refresh_sec,
    normalize_refresh_policy, refresh_policy_to_preset, sec_to_unit_and_value,
    should_warn_refresh_sec,
};
#[allow(unused_imports)]
pub(crate) use state::{
    AlertMetricVm, AlertOperatorVm, AlertRuleVm, ConnectionOptionVm, WidgetDraftVm,
    WidgetEditorDrawerState, WidgetEditorSelect, WidgetSaveVm, default_alert,
    default_hidden_sql, empty_draft,
};

use std::rc::Rc;

use rune::{col, row, text, Color, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::kit::kit;
use crate::views::dashboard::dashboard_panel::WorkflowListItemVm;

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React `max-w-md` on the aside.
const DRAWER_MAX_W: f32 = 448.0;
/// React `bg-black/40` on the scrim.
const SCRIM_ALPHA: f32 = 0.40;
/// React header / footer `px-4 py-3`.
const HEADER_PX: f32 = 16.0;
const HEADER_PY: f32 = 12.0;
/// React body `p-4` and `space-y-4`.
const BODY_PAD: f32 = 16.0;
const BODY_GAP: f32 = 16.0;
/// React footer `gap-2`.
const FOOTER_GAP: f32 = 8.0;

// ---- Keys -------------------------------------------------------------------

/// The drawer root (React `data-testid="widget-editor-drawer"`).
const ROOT_KEY: &str = "widget-editor-drawer";
/// The save button (React `data-testid="widget-editor-save"`).
const SAVE_KEY: &str = "widget-editor-save";

// ---- Builder ----------------------------------------------------------------

/// The `S` → drawer-state projection (the T112 `PanelStateFn` convention):
/// every field-level transition flows through it, so the host wires one
/// closure instead of one per control.
pub(crate) type DrawerStateFn<S> = Rc<dyn Fn(&mut S) -> &mut WidgetEditorDrawerState>;
type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type SaveFn<S> = Rc<dyn Fn(&mut S, WidgetSaveVm) + 'static>;

/// Builder for the drawer, generic over the app state `S` that owns the
/// state slice (the ported-dialog pattern of T018/T112/T175).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) struct WidgetEditorDrawer<'a, S: 'static> {
    state: &'a WidgetEditorDrawerState,
    /// React's `userWorkflows` prop.
    user_workflows: &'a [WorkflowListItemVm],
    /// `Date.now()` in Unix epoch millis, injected per build (the T146 card
    /// reads it).
    now_ms: i64,
    /// The SQL editor seam's host-built surface (the T069
    /// axis-configurator convention).
    sql_editor: Option<El>,
    state_fn: Option<DrawerStateFn<S>>,
    on_close: Option<ActionFn<S>>,
    on_open_workflow_editor: Option<ActionFn<S>>,
    on_save: Option<SaveFn<S>>,
    on_monitor_authorize: Option<ActionFn<S>>,
    on_monitor_revoke: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> WidgetEditorDrawer<'a, S> {
    pub(crate) fn new(
        state: &'a WidgetEditorDrawerState,
        user_workflows: &'a [WorkflowListItemVm],
        now_ms: i64,
    ) -> Self {
        Self {
            state,
            user_workflows,
            now_ms,
            sql_editor: None,
            state_fn: None,
            on_close: None,
            on_open_workflow_editor: None,
            on_save: None,
            on_monitor_authorize: None,
            on_monitor_revoke: None,
        }
    }

    /// The SQL editor seam: the host-built editing surface (the drawer
    /// paints it inside React's `h-40` border box; without a slot nothing
    /// paints there yet).
    pub(crate) fn sql_editor(mut self, editor: El) -> Self {
        self.sql_editor = Some(editor);
        self
    }

    /// The state projection every field-level transition flows through.
    pub(crate) fn state_fn(mut self, f: DrawerStateFn<S>) -> Self {
        self.state_fn = Some(f);
        self
    }

    /// React `onClose` (the header button and the footer cancel; React's
    /// scrim has no handler).
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// React `onOpenWorkflowEditor` (the open-workflow-editor link).
    pub(crate) fn on_open_workflow_editor(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_workflow_editor = Some(Rc::new(f));
        self
    }

    /// React `onSave` — fires only with a valid payload (a blocked save
    /// carries no action, React's early `return`).
    pub(crate) fn on_save(mut self, f: impl Fn(&mut S, WidgetSaveVm) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }

    /// The T146 card's authorize press (`change(false)`'s dispatch).
    pub(crate) fn on_monitor_authorize(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_monitor_authorize = Some(Rc::new(f));
        self
    }

    /// The T146 card's revoke press (`change(true)`'s dispatch).
    pub(crate) fn on_monitor_revoke(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_monitor_revoke = Some(Rc::new(f));
        self
    }

    /// Paints the drawer into `layer`; nothing at all while closed (React's
    /// `if (!open) return null`). The body's selects push their anchored
    /// listboxes into the layer as they build; anchored elements draw above
    /// later siblings, so the scrim and the root pushed after them still sit
    /// under the listboxes (the `ui/dialog.rs` order).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.state.open {
            return;
        }
        let k = kit(dark);
        let aside = self.aside(&k, dark, layer);
        // React's scrim: `absolute inset-0 bg-black/40`, `aria-hidden`, and
        // inert — only the header button and the footer cancel close.
        layer.push(
            col()
                .key(format!("{ROOT_KEY}-scrim"))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .fill(Color::BLACK.alpha(SCRIM_ALPHA))
                .a11y_hidden(),
        );
        // React's root: `fixed inset-0 z-50 flex justify-end`; the aside is
        // the in-flow child the flex end-aligns.
        layer.push(
            row()
                .key(ROOT_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .justify_end()
                .child(aside),
        );
    }

    // ---- the aside ------------------------------------------------------------

    /// React's `<aside>`: `relative flex h-full w-full max-w-md flex-col
    /// border-l border-edge bg-surface shadow-xl`.
    fn aside(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let aside = col()
            .key(format!("{ROOT_KEY}-panel"))
            .h_full()
            .w_full()
            .max_w(DRAWER_MAX_W) // React max-w-md
            .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_base) // border-l
            .fill(k.c.bg_base) // bg-surface
            .child(self.header(k))
            .child(self.body(k, dark, layer))
            .child(self.footer(k));
        // React's `shadow-xl`; the dialog's elevation mapping.
        k.elevate(aside, &k.elevation.overlay)
    }

    /// The header strip: `flex shrink-0 items-center justify-between
    /// border-b border-edge px-4 py-3` with the title and the ghost close
    /// button.
    fn header(&self, k: &Kit) -> El {
        let mut close = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Large) // React h-7 w-7, glyph h-4 w-4
            .label(t("common.close"));
        if let Some(on_close) = self.on_close.as_ref() {
            let on_close = Rc::clone(on_close);
            close = close.on_press(move |s: &mut S| on_close(s));
        }
        let close = close.build(k).key(format!("{ROOT_KEY}-close"));

        row()
            .key(format!("{ROOT_KEY}-header"))
            .w_full()
            .items_center()
            .justify_between()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base) // border-b
            .px(HEADER_PX)
            .py(HEADER_PY)
            .child(
                text(if self.state.is_new {
                    t("dashboard.newWidget")
                } else {
                    t("dashboard.editWidget")
                })
                .key(format!("{ROOT_KEY}-title"))
                .font(TextStyle::new(14.0, 600.0)) // text-sm font-semibold
                .color(k.c.text_base), // text-fg
            )
            .child(close)
    }

    /// The footer strip: `flex shrink-0 justify-end gap-2 border-t
    /// border-edge px-4 py-3` with the cancel and save buttons.
    fn footer(&self, k: &Kit) -> El {
        row()
            .key(format!("{ROOT_KEY}-footer"))
            .w_full()
            .justify_end()
            .gap(FOOTER_GAP)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base) // border-t
            .px(HEADER_PX)
            .py(HEADER_PY)
            .child(self.cancel_button(k))
            .child(self.save_button(k))
    }

    /// The footer cancel: `variant="ghost" className="h-8 px-3 text-xs"`.
    fn cancel_button(&self, k: &Kit) -> El {
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large); // React h-8 (the area mapping)
        if let Some(on_close) = self.on_close.as_ref() {
            let on_close = Rc::clone(on_close);
            cancel = cancel.on_press(move |s: &mut S| on_close(s));
        }
        cancel.build(k).key("widget-editor-cancel")
    }

    /// The footer save: `className="h-8 px-3 text-xs"` (React's default
    /// `primary` variant), `data-testid="widget-editor-save"`. React never
    /// disables it — a blocked `handleSave` just returns early — so the
    /// port keeps it pressable but carries no action while the payload is
    /// blocked (the same observable nothing).
    fn save_button(&self, k: &Kit) -> El {
        let mut save = Button::new(t("common.save"))
            .variant(ButtonVariant::Contrast) // React primary (the T018 mapping)
            .control_size(ControlSize::Large); // React h-8 px-3 text-xs
        if let (Some(on_save), Some(payload)) = (&self.on_save, self.state.save_payload()) {
            let on_save = Rc::clone(on_save);
            save = save.on_press(move |s: &mut S| on_save(s, payload.clone()));
        }
        save.build(k).key(SAVE_KEY)
    }
}
