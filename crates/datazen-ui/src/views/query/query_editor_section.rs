//! `QueryEditorSection`, ported 1:1 from
//! `packages/frontend/src/windows/connection/query/QueryEditorSection.tsx`.
//!
//! React is the query panel's editor column: an icon-only toolbar, a
//! CodeMirror `SqlEditor`, the optional NL2SQL / visual-builder surfaces, an
//! idle hint, a splitter handle and a transient builder toast. Everything with
//! a command behind it (execute, format, explain, transactions, history/favourites
//! toggles, completion refresh) is emitted as a [`QueryEditorAction`] through an
//! injected host port — no fake IPC, promise, thread or extension registry.
//!
//! ## Documented deviations
//!
//! * CodeMirror and the Pro query-builder canvas are separate ports; the editor
//!   host and builder host are honest keyed regions the host fills, and the SQL
//!   is a `TextEdit` bound through an accessor. Remove those regions when the
//!   engine ports land.
//! * React's native execution-strategy context menu is represented by a small
//!   keyed dropdown using the exact four strategy locale keys; swap it for the
//!   native menu when that port lands.
//! * The 2500ms builder toast is host-timed (React uses `setTimeout`); the VM
//!   carries the visible message, and the host clears it on its own clock.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this section yet. The query-panel host will own
//! the VM and translate the actions; until then every reusable item carries a
//! per-item `#[allow(dead_code)]` with this removal stage.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

mod context;
mod toolbar;

pub(crate) use context::{qb_context_key, qb_is_open, qb_needs_rebind, qb_rebind_action};

/// React `SqlExecutionStrategy`.
#[allow(dead_code)] // VM mirror; consumed by the dropdown and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExecutionStrategy {
    #[default]
    CurrentStatement,
    EntireScript,
    LargestStatement,
    Ask,
}

impl ExecutionStrategy {
    pub(crate) const ALL: [Self; 4] = [
        Self::CurrentStatement,
        Self::EntireScript,
        Self::LargestStatement,
        Self::Ask,
    ];

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::CurrentStatement => "query.executionStrategy.currentStatement",
            Self::EntireScript => "query.executionStrategy.entireScript",
            Self::LargestStatement => "query.executionStrategy.largestStatement",
            Self::Ask => "query.executionStrategy.ask",
        }
    }
}

/// Host-owned projection of the section props that affect painting.
#[allow(dead_code)] // VM surface; owned by the query-panel host.
#[derive(Clone, Debug, Default)]
pub(crate) struct QueryEditorSectionVm {
    pub(crate) panel_id: String,
    pub(crate) db_session_id: String,
    pub(crate) connection_id: String,
    pub(crate) sql: String,
    /// Live editor value the host keeps in sync with `sql` (React's onChange).
    pub(crate) sql_edit: TextEdit,
    pub(crate) running: bool,
    pub(crate) compact_toolbar: bool,
    pub(crate) safe_mode: bool,
    pub(crate) in_transaction: bool,
    pub(crate) tx_busy: bool,
    pub(crate) supports_explain: bool,
    pub(crate) has_context_selectors: bool,
    pub(crate) selected_database: Option<String>,
    pub(crate) selected_schema: Option<String>,
    pub(crate) execution_strategy: ExecutionStrategy,
    pub(crate) execution_strategy_open: bool,
    pub(crate) query_builder_available: bool,
    /// React `openPanelId` from the Pro contribution.
    pub(crate) qb_open_panel: Option<String>,
    /// React's `boundQbContextRef.current.contextKey`.
    pub(crate) qb_bound_context_key: Option<String>,
    pub(crate) nl2sql_visible: bool,
    pub(crate) history_visible: bool,
    pub(crate) favorites_visible: bool,
    pub(crate) show_idle_hint: bool,
    pub(crate) editor_height: f32,
    pub(crate) editor_height_pinned: bool,
    pub(crate) execution_time_ms: Option<u64>,
    pub(crate) row_count: Option<u64>,
    pub(crate) affected_rows: Option<u64>,
    pub(crate) qb_toast: Option<String>,
}

/// The host intents this section can emit.
#[allow(dead_code)] // host port; remove when the query-panel host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueryEditorAction {
    Execute,
    Cancel,
    OpenAddFavorite(String),
    ToggleNl2sql,
    Format,
    Explain,
    RefreshCompletion,
    /// React `queryBuilder.openFor(panelId, contextKey)` (also the rebind path).
    OpenVisualBuilder { panel_id: String, context_key: String },
    CloseVisualBuilder { mode: String },
    ToggleExecutionStrategy,
    SetExecutionStrategy(ExecutionStrategy),
    ToggleHistory,
    ToggleFavorites,
    BeginTx,
    CommitTx,
    RollbackTx,
    UpdateSql(String),
}

pub(crate) type ActionFn<S> = Rc<dyn Fn(&mut S, QueryEditorAction) + 'static>;
pub(crate) type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

pub(crate) fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: QueryEditorAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the section into a single column. A closed/omitted surface is simply
/// not added, matching React's conditional JSX.
#[allow(dead_code)] // consumer: the query-panel host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &QueryEditorSectionVm,
    sql_edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let qb_open = qb_is_open(vm);
    let mut column = col()
        .key("query-editor-section")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(toolbar::build(vm, &on_action, dark));
    if vm.execution_strategy_open {
        column = column.child(toolbar::strategy_menu(&on_action, dark));
    }

    // NL2SQL panel is host-owned; the keyed region makes its presence observable.
    if vm.nl2sql_visible {
        column = column.child(
            row()
                .key("query-nl2sql-panel")
                .label(t("nl2sql.title"))
                .w_full()
                .min_h(120.0)
                .fill(k.c.layer_01),
        );
    }
    if qb_open && vm.query_builder_available {
        column = column.child(
            row()
                .key("query-builder-host")
                .label(t("query.visualBuilder.title"))
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .fill(k.c.layer_01),
        );
    }

    let mut editor_viewport = row()
        .key("query-editor-host")
        .w_full()
        .min_h(0.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.bg_base);
    if !qb_open {
        editor_viewport = editor_viewport.child(editor_field(vm, sql_edit, dark));
    }
    let mut editor_region = col()
        .key("query-editor-region")
        .w_full()
        .min_w(0.0)
        .min_h(0.0)
        .grow(1.0)
        .child(editor_viewport);
    if vm.show_idle_hint && !qb_open {
        editor_region = editor_region.child(
            row()
                .key("query-idle-hint")
                .w_full()
                .center()
                .child(text(t("query.shortcutHint")).color(k.c.text_muted)),
        );
    }
    if !qb_open {
        editor_region = editor_region.child(
            row()
                .key("query-editor-resize-handle")
                .w_full()
                .h(6.0)
                .shrink(),
        );
    }
    column = column.child(editor_region);

    if let Some(message) = vm.qb_toast.as_deref() {
        column = column.child(
            row()
                .key("qb-toast")
                .role("status")
                .w_full()
                .label(message.to_owned())
                .child(text(message.to_owned()).color(k.c.text_base)),
        );
    }
    column
}

fn editor_field<S: 'static>(
    vm: &QueryEditorSectionVm,
    sql_edit: Option<EditFn<S>>,
    dark: bool,
) -> El {
    // The VM's TextEdit is the last painted value; the bound TextEdit is the
    // live value the host owns (React's `SqlEditor` value/onChange pair).
    let mut field = TextField::new("query-editor-input", &vm.sql_edit)
        .height(vm.editor_height.max(120.0))
        .grow()
        .pad_x(12.0)
        .text_style(TextStyle::new(13.0, 400.0))
        .placeholder(t("query.placeholder"));
    if let Some(edit) = sql_edit {
        let accessor = edit.clone();
        field = field.on_edit(move |s: &mut S| accessor(s));
    } else {
        field = field.disabled(true);
    }
    field.build(dark)
}
