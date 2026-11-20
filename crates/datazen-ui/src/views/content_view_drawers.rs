//! Right-hand drawers for the connection content view, ported from
//! `packages/frontend/src/windows/connection/ContentViewDrawers.tsx`.
//!
//! React owns the open flags in `ContentView`; this component is pure render +
//! state extraction. The Rune port keeps that split: [`DrawerState`] is a
//! snapshot the host fills, [`DrawerAction`] is the enum of host calls a press
//! emits, and the pure derivations live in the sibling logic module. No IPC is
//! performed here — the host executes each action through its own port.
//!
//! ## Immediate-mode adaptation
//!
//! There is no `useResizable`, no store subscription and no `window` here. The
//! sidebar width is clamped to React's `240..=600` bounds, per-pane execs are
//! handed in as a map, and the drawer reads the *focused* pane's exec (the
//! split-pane source rule). The `on_action` callback is the injected host port.
//!
//! ## Local mirrors (removal stage)
//!
//! `StatementResult` / `QueryExec` / `ServerEditability` etc. are minimal
//! mirrors of types that live in `stores/panelStore` and `lib/resultEditability`
//! on the React side; remove them when those stores/libs are ported and feed
//! the real values in. `DrawerAction` is only needed until the panel-store view
//! track wires the real host.
//!
//! ## Keys (the existing view tests pin the first six)
//!
//! `content-view-drawers` / `-absent`, `content-view-detail` / `-close`,
//! `content-view-ai` / `-close`, `content-view-dialogs`, plus the newly ported
//! `conn-detail-readonly-reason`, `conn-detail-row-<n>`,
//! `conn-kv-key-props-sidebar`, `conn-ai-*`.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El};

use crate::i18n::t;
#[path = "content_view_drawers_logic.rs"]
pub(crate) mod logic;

pub(crate) use logic::*;

const ROOT: &str = "content-view-drawers";
const DETAIL: &str = "content-view-detail";
const DETAIL_CLOSE: &str = "content-view-detail-close";
const AI: &str = "content-view-ai";
const AI_CLOSE: &str = "content-view-ai-close";

/// Host calls a press asks for. No IPC happens in this module.
#[allow(dead_code)] // host port surface; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DrawerAction {
    CloseDetail,
    CloseAi,
    /// React `updateQuerySql(activePanel.id, sql)` from `onInsertSql`.
    InsertSql { panel_id: String, sql: String },
    RunCode { code: String, language: String },
    NewQuery { code: String },
    DraftConsumed { request_id: String },
}

type ActionFn<S> = Rc<dyn Fn(&mut S, DrawerAction) + 'static>;

/// The snapshot the host fills for one frame.
#[allow(dead_code)] // VM surface; consumed by build and the tests.
#[derive(Clone, Debug)]
pub(crate) struct DrawerState {
    pub detail_open: bool,
    pub ai_chat_open: bool,
    pub detail_panel_applicable: bool,
    /// React gates the AI drawer on `aiChatOpen && dbSessionId`.
    pub has_db_session: bool,
    pub panel_id: String,
    pub panel_kind: PanelKind,
    /// React `focusedPaneId` — `None` means the default pane.
    pub focused_pane: Option<String>,
    /// `queryExec` entries for this panel's panes (bare panel id = default).
    pub panes: HashMap<String, QueryExec>,
    pub editor_selection: String,
    /// The backend's origin verdict; `None` is `UNJUDGED`.
    pub verdict: Option<ServerEditability>,
    /// React `resolvingOrigin` — swaps the reason text for "resolving".
    pub resolving_origin: bool,
    /// A KV panel with a driver-contributed key-props sidebar.
    pub key_props_slot: bool,
    pub sidebar_width: f32,
    /// Table/view panel row detail (the value the grid put on the drawer).
    pub table_columns: Vec<ColumnMeta>,
    pub table_rows: Vec<Vec<serde_json::Value>>,
    pub table_detail_row_index: Option<usize>,
    /// React `useKvSlotSelectedKey(kvPanelState)` for the assistant's KV context.
    pub kv_selected_key: Option<String>,
}

#[allow(dead_code)] // VM surface; consumed by build and the tests.
impl Default for DrawerState {
    fn default() -> Self {
        Self {
            detail_open: false,
            ai_chat_open: false,
            detail_panel_applicable: false,
            has_db_session: false,
            panel_id: String::new(),
            panel_kind: PanelKind::Other,
            focused_pane: None,
            panes: HashMap::new(),
            editor_selection: String::new(),
            verdict: None,
            resolving_origin: false,
            key_props_slot: false,
            sidebar_width: 320.0,
            table_columns: Vec::new(),
            table_rows: Vec::new(),
            table_detail_row_index: None,
            kv_selected_key: None,
        }
    }
}

/// The answers a render derived, observable for tests and the host.
#[allow(dead_code)] // VM surface; consumed by the tests.
#[derive(Clone, Debug, Default)]
pub(crate) struct DrawerFacts {
    pub auto_context: Option<AutoContext>,
    pub read_only_reason: Option<ReadOnlyReason>,
    pub editable: bool,
    pub detail_row: Vec<(String, serde_json::Value)>,
}

impl DrawerState {
    /// React `detailPanelApplicable && detailOpen`.
    #[allow(dead_code)] // VM surface; consumed by build and the tests.
    pub(crate) fn show_detail(&self) -> bool {
        self.detail_open && self.detail_panel_applicable
    }

    /// React `aiChatOpen && dbSessionId`.
    #[allow(dead_code)] // VM surface; consumed by build and the tests.
    pub(crate) fn show_ai(&self) -> bool {
        self.ai_chat_open && self.has_db_session
    }

    /// The exec of the focused pane (split-pane source rule).
    pub(crate) fn active_exec(&self) -> Option<&QueryExec> {
        if self.panel_kind != PanelKind::Query {
            return None;
        }
        resolve_active_exec(&self.panel_id, self.focused_pane.as_deref(), &self.panes)
    }

    /// Every derived value for this frame, in one place.
    pub(crate) fn facts(&self) -> DrawerFacts {
        let exec = self.active_exec();
        let verdict = self.verdict.clone().unwrap_or_else(unjudged);
        let auto_context = build_auto_context(self.panel_kind, exec, &self.editor_selection);
        let result = exec.and_then(QueryExec::active_result);
        let row_index = if self.panel_kind == PanelKind::Query {
            exec.and_then(|e| e.result_detail_row_index)
        } else {
            self.table_detail_row_index
        };
        let read_only_reason = if self.panel_kind == PanelKind::Query {
            query_read_only_reason(self.panel_kind, result, &verdict, row_index)
        } else {
            None
        };
        let editable = if self.panel_kind == PanelKind::Query {
            let binding = result.is_some_and(|r| r.has_runtime_binding);
            query_result_editable(self.panel_kind, result, &verdict) && binding
        } else {
            true
        };
        let detail_row = self.detail_row(result, row_index);
        DrawerFacts { auto_context, read_only_reason, editable, detail_row }
    }

    fn detail_row(
        &self,
        result: Option<&StatementResult>,
        row_index: Option<usize>,
    ) -> Vec<(String, serde_json::Value)> {
        let (columns, rows) = match self.panel_kind {
            PanelKind::Table | PanelKind::View => {
                (self.table_columns.as_slice(), self.table_rows.as_slice())
            }
            PanelKind::Query => match result {
                Some(result) => (result.columns.as_slice(), result.rows.as_slice()),
                None => (&[][..], &[][..]),
            },
            PanelKind::Other => (&[][..], &[][..]),
        };
        let Some(index) = row_index else {
            return Vec::new();
        };
        let Some(row) = rows.get(index) else {
            return Vec::new();
        };
        columns
            .iter()
            .zip(row.iter())
            .map(|(column, value)| (column.name.clone(), value.clone()))
            .collect()
    }
}

/// Render the drawers into `layer`, or mount nothing when both are closed.
#[allow(dead_code)] // consumer: content_view composition (wired by T245).
pub(crate) fn build<S: 'static>(
    state: &DrawerState,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    // React mounts the keyed KV sidebar wrapper whenever the slot is
    // applicable, even while the drawer is collapsed (`detailOpen` controls the
    // inner content, not the wrapper).
    let show_kv_slot = state.key_props_slot && state.detail_panel_applicable;
    if !state.show_detail() && !state.show_ai() && !show_kv_slot {
        return;
    }
    let facts = state.facts();
    let mut root = col()
        .key(ROOT)
        .w(280.0)
        .min_w(0.0)
        .h_full()
        .gap(8.0)
        .label(if dark { "dark" } else { "light" });

    if show_kv_slot {
        root = root.child(key_props_sidebar(state, &on_action));
    } else if state.show_detail() {
        root = root.child(detail_drawer(state, &facts, &on_action));
    }
    if state.show_ai() {
        root = root.child(ai_handle()).child(ai_drawer(state, &facts, &on_action));
    }
    layer.push(root);
}

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: DrawerAction) -> impl Fn(&mut S) {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

fn close_button<S: 'static>(key: &str, action: DrawerAction, on_action: &Option<ActionFn<S>>) -> El {
    row()
        .key(key)
        .role("button")
        .label("Close")
        .focusable(true)
        .w(20.0)
        .h(20.0)
        .center()
        .child(text("×").wrap())
        .on_press(emit(on_action, action))
}

fn key_props_sidebar<S: 'static>(state: &DrawerState, on_action: &Option<ActionFn<S>>) -> El {
    // React's slot wrapper always mounts (even when the driver renders nothing
    // while collapsed); the host owns `open`.
    let mut wrapper = row()
        .key("conn-kv-key-props-sidebar")
        .min_w(0.0)
        .shrink()
        .label("kv-key-props-sidebar")
        .on_press(emit(on_action, DrawerAction::CloseDetail));
    if state.detail_open {
        wrapper = wrapper.child(
            text(t("detail.title"))
                .key("conn-kv-key-props-title")
                .label("open")
                .wrap(),
        );
    }
    wrapper
}

fn detail_drawer<S: 'static>(
    state: &DrawerState,
    facts: &DrawerFacts,
    on_action: &Option<ActionFn<S>>,
) -> El {
    let mut panel = col()
        .key(DETAIL)
        .label(t("detail.title"))
        .w_full()
        .min_w(0.0)
        .h(160.0)
        .gap(8.0);

    // React stacks the reason banner on top of the drawer (a sibling of the
    // grid squeezed the editor toolbar at a narrow window). It only shows while
    // the drawer is open.
    if let Some(reason) = facts.read_only_reason {
        let message = if state.resolving_origin {
            t("query.gridEdit.resolving").to_owned()
        } else {
            t(&reason.message_key()).to_owned()
        };
        panel = panel.child(
            row()
                .key("conn-detail-readonly-reason")
                .label(reason.as_str())
                .w_full()
                .child(text(message).key("conn-detail-readonly-reason-text").wrap()),
        );
    }

    panel = panel.child(text(t("detail.title")).key("conn-detail-title").wrap());
    for (index, (name, value)) in facts.detail_row.iter().enumerate() {
        let shown = match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Null => String::new(),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
        panel = panel.child(
            row()
                .key(format!("conn-detail-row-{index}"))
                .label(name.clone())
                .w_full()
                .child(text(shown).key(format!("conn-detail-row-{index}-value")).wrap()),
        );
    }
    // React's `data-editable` is observable through the element label here.
    panel = panel.child(
        row()
            .key("conn-detail-editable")
            .label(if facts.editable { "true" } else { "false" })
            .a11y_hidden(),
    );
    panel.child(close_button(DETAIL_CLOSE, DrawerAction::CloseDetail, on_action))
}

fn ai_handle() -> El {
    row().key("conn-ai-handle").w(4.0).shrink()
}

fn ai_drawer<S: 'static>(
    state: &DrawerState,
    facts: &DrawerFacts,
    on_action: &Option<ActionFn<S>>,
) -> El {
    let width = clamp_sidebar_width(state.sidebar_width);
    let mut aside = col()
        .key(AI)
        .w(width)
        .shrink()
        .min_w(0.0)
        .h_full()
        .gap(8.0);
    if let Some(context) = &facts.auto_context {
        aside = aside.child(render_auto_context(context));
    } else if let Some(key) = &state.kv_selected_key {
        aside = aside.child(text(key.clone()).key("conn-ai-kv-key").wrap());
    }
    aside.child(close_button(AI_CLOSE, DrawerAction::CloseAi, on_action))
}

fn render_auto_context(context: &AutoContext) -> El {
    let mut root = col().key("conn-ai-auto-context").w_full().gap(4.0);
    if let Some(sql) = &context.editor_sql {
        root = root.child(text(sql.clone()).key("conn-ai-auto-sql").wrap());
    }
    if let Some(selection) = &context.selection {
        root = root.child(text(selection.clone()).key("conn-ai-auto-selection").wrap());
    }
    if let Some(error) = &context.last_error {
        root = root.child(text(error.clone()).key("conn-ai-auto-error").wrap());
    }
    if let Some(preview) = &context.result_preview {
        root = root.child(text(preview.row_count.to_string()).key("conn-ai-auto-row-count").wrap());
        if let Some(elapsed) = preview.elapsed_ms {
            root = root.child(text(elapsed.to_string()).key("conn-ai-auto-elapsed").wrap());
        }
        for (index, row) in preview.rows.iter().enumerate() {
            let cells = row
                .iter()
                .map(|value| match value {
                    serde_json::Value::String(text) => text.clone(),
                    serde_json::Value::Null => String::new(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                })
                .collect::<Vec<_>>()
                .join(" | ");
            root = root.child(text(cells).key(format!("conn-ai-auto-row-{index}")).wrap());
        }
    }
    root
}

/// The split helper the composition layer calls: the drawer's two gates.
#[allow(dead_code)] // composition helper; remove when the panel-store view lands.
pub(crate) fn any_open(state: &DrawerState) -> bool {
    state.show_detail() || state.show_ai()
}
