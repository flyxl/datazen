//! Port of `packages/frontend/src/windows/workflow/WorkflowPage.tsx` (T145;
//! the ledger's older numbering says T147) — the workflow window's page: the
//! family's page-level assembly. It mounts the already-ported family pieces
//! (the visual `WorkflowForm`, the sidebar `WorkflowHistoryList`, the edit
//! pane's shared `WorkflowEditorActionBar`, the step-result utils ladder, the
//! `ChartView` container) and leaves honest seams for everything the migration
//! has not reached. `views/dashboards_workflows.rs`'s lite editor was this
//! page's stand-in; this port swaps it (the module comment there and the
//! `views/workflow/mod.rs` ledger both pointed here).
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! |---|---|
//! | `useAiStore` (`workflows`/`workflowsLoading`/`workflowError`) | `AppState::workflow_list` / `workflow_list_loading` / `workflow_store_error`, filled by `fetch_workflow_list` — the host's workflow registry (`host_workflows()`) stands in for `aiCommands.workflowList` |
//! | `panels`/`activePanelId`/`activeStepIndex` `useState`s | `AppState::workflow_panels` / `workflow_active_panel` / `workflow_active_step` |
//! | Run inputs | Declaration and editor pairs owned by each open run panel |
//! | `savedConnections` (connectionCommands.getConnections) | `AppState::workflow_form_connections` — T070's slot, filled by `reload_workflow_connections` (the adapter's `list_connections`) |
//! | the edit panel's `draft`/`yamlText`/`editorMode` | `AppState::workflow_draft` / `workflow_yaml` / `workflow_editor_mode` — one live edit panel's backing (see the seams below) |
//! | `appSettings.workflowStepResultOrder` + `updateSettings` | `AppState::workflow_step_order()` + `toggle_workflow_step_order`, shared with Data Browsing |
//! | `useConfirmDialog()`'s pending `{options, resolve}` | `AppState::workflow_confirm_request` (+ `workflow_confirm_epoch`, each ask a fresh mount) and `answer_workflow_confirm` — the wapps remove pattern |
//! | `useResizable` (256/180/420, `workflow.sidebar`) | `splitter::splitter_handle` over the same bounds and a valid native layout key |
//! | `useEffect` mount (loadSettings/loadConfig/loadWorkflows/getDir/reloadConnections/listeners) | `AppState::select`'s `Section::Workflows` arm — the real halves (list + connections) run, the command seams below stay |
//! | `viewStateRef` unmount snapshot (`workspacePanelStateStore`) | the `AppState` fields simply persist — the section switch cannot drop Rune state, so the round-trip is a no-op of the same observable behaviour (the wapps ruling) |
//! | the `localesReady` gate (`useLocaleDomains(['workflows'])`) | nothing — the zh-CN table is compiled in (`i18n.rs`), so the pack is always ready |
//! | `WindowChromeSlot`/`StatusBar` (the standalone window's chrome) | the app shell's own chrome — the Rune app is single-window and embeds the page, so React's `embedded` branch is the one that applies |
//!
//! ## Honest seams (each belongs to its own later task)
//!
//! - **`WorkflowYamlEditor` (T200; this note's older numbering says T202)**:
//!   the yaml mode's editor is the real port —
//!   [`super::workflow_yaml_editor`] mounts the document surface over
//!   `AppState::workflow_yaml` (the surface the workspace-tabs tests pin)
//!   with the yaml highlighting, the line-number gutter and the lint gutter
//!   ([`super::yaml_mode`]/[`super::yaml_lint`], T265/T244). The mode switch
//!   itself is real in both directions — `draftToYamlObject`+
//!   `YAML.stringify` becomes the lite writer (`views::workflow_yaml`),
//!   `parseWorkflowYaml`+`yamlObjectToDraft` (`lib/workflowYaml.ts`/
//!   `lib/workflowDraftYaml.ts`, T646) becomes the lean parse of exactly
//!   that writer's shape ([`parse_lite_workflow_yaml`]).
//! - **`workflowDraftConvert` (T225)**: the definition ↔ draft conversions are
//!   the lean projection over the host's lite `Workflow { name, steps }` shape
//!   ([`workflow_definition_to_draft`] / [`workflow_draft_to_steps`]) — every
//!   step rides as a query step carrying its text as SQL, and the round trip
//!   preserves it.
//! - **The `aiCommands` workflow surface** (reload/getDir/delete/history):
//!   the adapter carries none of them. The reload press runs the refetch half
//!   it owns; the dir stays empty so React's own conditional hides the
//!   open-dir button; delete/history run their confirm gates and then the
//!   failure branch (`workflow_operation_error` naming the missing use case) —
//!   the success transitions (`workflow_deleted`,
//!   `workflow_history_loaded`) are pinned for the wiring. `workflowSave`
//!   itself is real through the N13 registry surface the lite editor used
//!   (`desktop.save_workflow`, memory port fallback).
//! - **`listenCrossWindow` (`datazen:connections-changed`, T437)**: Rune is
//!   single-window with no event bus; no second window can broadcast the
//!   change, so the listener has nothing to install (the wapps ruling).
//! - **`WorkflowChatPanel` (components/ai, T334's family)**: the ai-create
//!   panel mounts React's pane geometry with an honest sentence — no chat
//!   surface is faked.
//! - **`AddToDashboardDialog` (T018) + `createEmptyDashboard`/`dashboardCommands`**:
//!   the add-to-dashboard press opens the flag React's dialog reads; the
//!   modal layer mounts an honest pending dialog (the chart-frame ruling) —
//!   the dashboard area's port swaps it.
//! - **`openDashboardWindow`/`openDocsWindow` (`lib/windowManager`)**: no
//!   second window exists in the Rune app; the docs button's press is the
//!   documented window-manager seam (the feedback line says so).
//! - **`showNativeContextMenu` + `buildWorkflowListContextMenuItems` (T646)**:
//!   the list row's secondary press records the seam's trigger
//!   (`workflow_context_menu`: the workflow id and the window point) — the
//!   entries builder and the native host land with T646; the same actions
//!   exist as the row's real edit/delete buttons meanwhile.
//! - **`DataTable` + `ColumnDef` (components/DataTable)**: the family has no
//!   container port yet; the step detail's table branch renders the same lean
//!   table the execution-result panel's preview took (columns + rows, no
//!   virtualisation, no row-height/export chrome) — the container task swaps
//!   it.
//! - **`ChartView`'s `lib/chart` pipeline** (fieldInference/recommend/transform,
//!   T079's family): the page injects the lean [`WorkflowChartPipeline`] —
//!   the chartability-relevant type slice T176 already ported
//!   ([`column_infers_numeric`]), no recommendation rule (ChartView's own
//!   default config covers it), and the ungrouped pass-through transform.
//!   The renderer factory stays unwired, so the canvas keeps its honest
//!   "renderer not ported" placeholder; the export request is the page's
//!   feedback seam.
//! - **The variables' engine binding**: the run panel's variable inputs are
//!   real state (React's `variables` record as `TextEdit`s); the lite host
//!   runner takes only the workflow name, so the binding is the host engine's
//!   seam (the VM carries no variables yet, so the row renders empty exactly
//!   like React's own empty-variables case).
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | page root (React's embedded testid) | `workflow-workspace` |
//! | toolbar / reload / create / ai-create / open-dir / docs | `workflow-toolbar`, `workflow-reload`, `workflow-create`, `workflow-ai-create-button` (React testid), `workflow-open-dir`, `workflow-open-docs` |
//! | sidebar / side tabs / splitter | `workflow-sidebar`, `workflow-side-tab-workflows` / `-history`, `splitter:workflow.sidebar` |
//! | feedback / operation error | `workflow-feedback`, `workflow-operation-error` (+ the banner base's `-text`/`-dismiss`) |
//! | list / row / name / scheduled / description | `workflow-list`, `workflow-{id}` (React `key={w.id}`; the memory port's id *is* its name), `-name`, `-scheduled`, `-description` |
//! | row edit / delete | `workflow-{id}-edit` / `-delete` (React's shared `workflow-item-edit` testid, keyed per row for `Runtime::find`) |
//! | list loading / empty | `workflow-list-loading`, `workflow-list-empty` |
//! | panel tabs / tab / label / status / close | `workflow-panel-tabs`, `workflow-tab-{id}`, `-label`, `-status`, `-close` |
//! | variables row / field / execute / add-to-dashboard | `workflow-variables`, `workflow-variable-{name}`, `workflow-execute-button` (React testid), `workflow-add-to-dashboard` (React testid) |
//! | run status / order toggle / steps bar / step tab | `workflow-run-status`, `workflow-step-order-toggle` (both React testids), `workflow-steps`, `workflow-step-{stepId}` |
//! | ai-create pane / edit pane / mode tabs | `workflow-ai-create-panel` (React testid), `workflow-edit-panel`, `workflow-editor-mode-tabs` + `workflow-mode-visual` / `workflow-mode-yaml` (React testids) |
//! | visual form / yaml pane / action bar | the form's own `workflow-form-root`, the lite `yaml-editor`, the bar's `workflow-editor-action-bar` + `workflow-yaml-save` (React's `saveTestId`) |
//! | step detail / sql / error / ai result | `workflow-step-detail`, `-sql`, `-error`, `-ai-result` |
//! | view toggle / table view / chart view | `workflow-step-view-toggle`, `workflow-view-table`, `workflow-view-chart` |
//! | sampled warning / lean table | `workflow-step-sampled-warning`, `workflow-step-table` (+ `-col-<j>`, `-row-<r>`, `-cell-<r>-<j>`) |
//! | select-step / store error / empty hint | `workflow-select-step`, `workflow-store-error`, `workflow-empty-hint` |
//! | confirm dialog (modals layer) | `workflow-confirm-dialog` + `-message` / `-warning` / `-cancel` / `-confirm` |
//! | add-to-dashboard pending dialog | `workflow-add-dashboard-dialog` + `-pending` / `-close` |

use rune::tok;
use rune::{
    col, el, icon, row, text, Color, El, PointerPress, ScrollSpec, TextEdit, TextStyle,
};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{
    ICON_ALERT_TRIANGLE, ICON_ARROW_DOWN_UP, ICON_BAR_CHART_3, ICON_BOOK_OPEN, ICON_CLOCK,
    ICON_HISTORY, ICON_PENCIL, ICON_PLAY, ICON_SPARKLES, ICON_TABLE_PROPERTIES, ICON_WAND_SPARKLES,
    ICON_X,
};
use crate::splitter::{self, SplitAxis};
use crate::state_def::AppState;
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::views::workflow::editor_action_bar::workflow_editor_action_bar;
use crate::views::workflow::history_section::{HistoryListSlots, WorkflowHistoryList};
use crate::views::workflow::step_result_utils::{
    column_infers_numeric, extract_step_column_names, is_chartable_preview, js_string, step_to_statement_preview,
    step_to_statement_result, StatementResult, StepExecutionResult, WorkflowExecutionResult,
    WorkflowStepStatus,
};
use crate::views::workflow::workflow_form::{
    connection_allows_multi_db, WorkflowConnectionOption, WorkflowDraft, WorkflowForm,
    WorkflowFormVariant, WorkflowStepDraft, WorkflowStepKind,
};

// ── geometry constants (Tailwind → points) ─────────────────────────────────

/// React's `useResizable` triple (initial 256, min 180, max 420).
const SIDEBAR_WIDTH_DEFAULT: u32 = 256;
const SIDEBAR_WIDTH_MIN: u32 = 180;
const SIDEBAR_WIDTH_MAX: u32 = 420;
/// Native layout settings accept lowercase letters, digits and hyphens.
const SIDEBAR_WIDTH_KEY: &str = "workflow-sidebar-width";
/// Preserve the existing observer/automation key (React called the region
/// `workflow.sidebar` in localStorage). This key is not written to settings.
const SIDEBAR_HANDLE_KEY: &str = "splitter:workflow.sidebar";
/// React's `w-1` on the actual pointer-capture element (1 × 0.25rem).
const SIDEBAR_HANDLE_WIDTH: f32 = 4.0;
/// The toolbar's `border-b border-edge px-3 py-2` (the embedded branch).
const TOOLBAR_PAD_X: f32 = 12.0;
const TOOLBAR_PAD_Y: f32 = 8.0;
/// The toolbar button's `h-6 w-6 !px-0` with the `h-3 w-3` glyph.
const TOOLBAR_BUTTON: f32 = 24.0;
const TOOLBAR_ICON: f32 = 12.0;
const SIDE_TAB_HEIGHT: f32 = 32.0; // text-xs (16px line-height) + py-2
const SIDE_TAB_UNDERLINE: f32 = 2.0;

/// The variables row's `px-4 py-3` and the `mb-3` between it and the fields.
const VARIABLES_PAD_X: f32 = 16.0;
const VARIABLES_PAD_Y: f32 = 12.0;
const VARIABLES_GAP: f32 = 12.0;
/// React's `h-7` variable input.
const VARIABLE_INPUT_H: f32 = 28.0;

/// The chart's sampling threshold (React `rows.length > 1000`).
const SAMPLE_LIMIT: usize = 1000;

// ── the list + panel view models ────────────────────────────────────────────

/// Mirror of the contract's `WorkflowVariable` — the run panel's variable
/// inputs read `name`/`type`/`description`/`required` (and the default the
/// select flow seeds). The host list use case is the seam; today's
/// projection carries none, so the page's variables row renders empty
/// exactly like React's own empty-variables case.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowVariableItem {
    pub name: String,
    /// `'string' | 'number' | 'connection'`.
    pub var_type: String,
    pub description: Option<String>,
    pub required: bool,
    /// The contract's `default` — the value the select flow seeds the input
    /// with (React's `v.default != null ? String(v.default) : ''`).
    pub default: Option<String>,
}

/// Mirror of the contract's `WorkflowListItem` (`id`, `name`, `description`,
/// `variables`, `scheduled?`). Built by [`WorkflowListItem::from_workflow`]:
/// the host registry's lite `Workflow { name, steps }` has no id, description,
/// variables or schedule, so the projection reads them as the lite shape says
/// — the id **is** the name (the registry is name-keyed), and React's own
/// `key={w.id}` therefore keys the row `workflow-{name}`, the keys the
/// workspace-tabs tests already pin. The typed list use case replaces this
/// projection when the host formalises it.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowListItem {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub variables: Vec<WorkflowVariableItem>,
    pub scheduled: bool,
}

impl WorkflowListItem {
    pub(crate) fn from_native(flow: datazen_ui_api::WorkflowListItem) -> Self {
        Self {
            // The existing native editor/run/remove routes are name keyed.
            id: flow.name.clone(),
            name: flow.name,
            description: (!flow.description.is_empty()).then_some(flow.description),
            variables: flow.variables.into_iter().map(|variable| WorkflowVariableItem {
                name: variable.name,
                var_type: variable.r#type,
                description: (!variable.description.is_empty()).then_some(variable.description),
                required: variable.required.unwrap_or(false),
                default: variable.default.filter(|value| !value.is_null()).map(|value| {
                    value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
                }),
            }).collect(),
            scheduled: flow.scheduled.unwrap_or(false),
        }
    }

    /// The honest projection of the host's lite workflow shape.
    pub fn from_workflow(flow: &datazen_ui_api::Workflow) -> Self {
        Self {
            id: flow.name.clone(),
            name: flow.name.clone(),
            description: None,
            variables: Vec::new(),
            scheduled: false,
        }
    }
}

/// The run panel (React `WorkflowRunPanel`).
#[derive(Clone, Debug)]
pub struct WorkflowRunPanel {
    pub id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    /// `new Date().toISOString()` at open time — the tab label's clock read.
    pub started_at: String,
    pub result: Option<WorkflowExecutionResult>,
    pub is_executing: bool,
    pub variables: Vec<(WorkflowVariableItem, TextEdit)>,
}

/// The edit panel (React `EditPanel`). The draft, the yaml text and the editor
/// mode live in the app-level slots (`workflow_draft` / `workflow_yaml` /
/// `workflow_editor_mode`) — the T070 form's own state contract — so one live
/// edit panel is fully backed; a second edit panel re-targets those slots
/// (React co-keeps both; the single-draft slot is the documented boundary,
/// and React's own reuse-by-editingId rule covers the common flows).
#[derive(Clone, Debug)]
pub struct WorkflowEditPanel {
    pub id: String,
    /// `None` is React's create flow (an unsaved draft).
    pub editing_id: Option<String>,
}

/// The history detail panel (React `HistoryDetailPanel`).
#[derive(Clone, Debug)]
pub struct WorkflowHistoryPanel {
    pub id: String,
    pub history_id: String,
    pub workflow_name: String,
    pub created_at: String,
    pub result: WorkflowExecutionResult,
}

/// One open panel (React `WorkflowPanel`'s tagged union).
#[derive(Clone, Debug)]
pub enum WorkflowPanel {
    Run(WorkflowRunPanel),
    Edit(WorkflowEditPanel),
    History(WorkflowHistoryPanel),
    AiCreate { id: String },
}

impl WorkflowPanel {
    /// The panel's tab id (React's `panel.id`, `wf-{n}` / `edit-{n}` /
    /// `hist-{n}` / `ai-{n}`).
    pub fn id(&self) -> &str {
        match self {
            Self::Run(panel) => &panel.id,
            Self::Edit(panel) => &panel.id,
            Self::History(panel) => &panel.id,
            Self::AiCreate { id } => id,
        }
    }

    /// React's `panelExecutionResult`: the run's answer or the history record;
    /// edit and ai-create panels carry none. The T223 port
    /// (`workflow_panel_guards.rs`) owns the definition; this method is the
    /// enum-side delegation.
    pub fn execution_result(&self) -> Option<&WorkflowExecutionResult> {
        super::workflow_panel_guards::panel_execution_result(self)
    }
}

/// React's `sideTab: 'workflows' | 'history'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowSideTab {
    Workflows,
    History,
}

/// React's `editorMode: 'visual' | 'yaml'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowEditorMode {
    Visual,
    Yaml,
}

/// The workflow view and the Settings port use the same checked preference.
pub use datazen_ui_api::types::WorkflowStepResultOrder as WorkflowStepOrder;

/// The short label the report toggle shows (React's `stepOrder.desc` / `asc`).
fn step_order_label(order: WorkflowStepOrder) -> &'static str {
    match order {
        WorkflowStepOrder::Asc => t("workflows.stepOrder.asc"),
        WorkflowStepOrder::Desc => t("workflows.stepOrder.desc"),
    }
}

/// The confirm dialog's ask kinds — the two `useConfirmDialog` asks the page
/// makes (delete a workflow, clear the history).
#[derive(Clone, Debug, PartialEq)]
pub enum WorkflowConfirmKind {
    DeleteWorkflow(String),
    ClearHistory,
}

/// The pending ask — React's options object at ask time (title, message, the
/// warning kind), resolved by `answer_workflow_confirm`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowConfirmRequest {
    pub kind: WorkflowConfirmKind,
    pub title: String,
    pub message: String,
}

/// The list row's secondary press — the native context-menu seam's trigger
/// (T646 owns the entries builder and the native host): the workflow the
/// press named and the window point it happened at, recorded so the builder
/// can replay the ask when it lands.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowContextMenuRequest {
    pub workflow_id: String,
    /// The window point (`e.clientX`/`e.clientY` in React's ask).
    pub point: (f32, f32),
}

// ── pure helpers (React's module scope) ─────────────────────────────────────

/// React's `firstShownIndex`: the step the result tab opens on — the last for
/// `desc`, the first for `asc`.
pub fn first_shown_index(count: usize, order: WorkflowStepOrder) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match order {
        WorkflowStepOrder::Desc => Some(count - 1),
        WorkflowStepOrder::Asc => Some(0),
    }
}

/// React's `stepNeedsDatabase` — true when any query/command step's effective
/// database (its own, or the inherited workflow default) is still missing for
/// a multi-db connection. Drives the shared Save button's disabled rule.
pub fn step_needs_database(
    draft: &WorkflowDraft,
    connections: &[WorkflowConnectionOption],
) -> bool {
    draft.steps.iter().any(|step| {
        if !matches!(step.kind, WorkflowStepKind::Query | WorkflowStepKind::Command) {
            return false;
        }
        let conn_id = step.connection.clone().or_else(|| draft.connection.clone());
        let conn = conn_id
            .as_deref()
            .and_then(|id| connections.iter().find(|c| c.id == id));
        if !connection_allows_multi_db(conn) {
            return false;
        }
        step.database
            .clone()
            .or_else(|| draft.database.clone())
            .is_none()
    })
}

/// React's `panelTabLabel(panel, labels)`: the ai-create/create labels, the
/// edit tab's draft-name read (the draft lives in the app-level slot this
/// build backs it with), and the run/history clocks. The T223 port
/// (`workflow_panel_guards.rs`) owns the ladder; this caller builds the
/// labels bag from `t()` and passes the app-level draft's name.
pub fn panel_tab_label(state: &AppState, panel: &WorkflowPanel) -> String {
    let labels = super::workflow_panel_guards::PanelTabLabels {
        ai_create: t("workflows.aiCreate.title").to_owned(),
        edit: t("workflows.edit").to_owned(),
        create: t("workflows.create").to_owned(),
    };
    super::workflow_panel_guards::panel_tab_label(
        panel,
        &labels,
        state.workflow_draft.name.text.as_str(),
    )
}

/// React's `workflowDefinitionToDraft` (`workflowDraftConvert.ts`) — the
/// lite stand-in over the host's lite `Workflow { name, steps }` seam: the
/// name fills both id and name, each step text rides as a query step's SQL.
/// The full T223 conversion (`workflow_draft_convert.rs`) speaks the
/// `WorkflowDefinition` the host's full seam (a cross-segment adapter task)
/// will hand this page; the round trip ([`workflow_draft_to_steps`])
/// preserves the lite projection.
pub fn workflow_definition_to_draft(flow: &datazen_ui_api::Workflow) -> WorkflowDraft {
    let mut draft = WorkflowDraft::empty();
    draft.id = TextEdit::new(flow.name.clone());
    draft.name = TextEdit::new(flow.name.clone());
    draft.steps = flow
        .steps
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let mut step = WorkflowStepDraft::query(&format!("step{}", index + 1));
            step.sql = text.clone();
            step
        })
        .collect();
    draft
}

/// React's `workflowDraftToDefinition` (T225) — the lean back-conversion the
/// save path takes: the draft's name and one text per step (the SQL it rides,
/// or the step id when the SQL is empty).
pub fn workflow_draft_to_steps(draft: &WorkflowDraft) -> Vec<String> {
    draft
        .steps
        .iter()
        .map(|step| {
            if step.sql.trim().is_empty() {
                step.id.text.clone()
            } else {
                step.sql.clone()
            }
        })
        .collect()
}

/// The lean parse of exactly the shape the lite writer (`views::workflow_yaml`)
/// serializes — `name: X` then a `steps:` list. The full
/// `parseWorkflowYaml`/`yamlObjectToDraft` pair is T646's; this reads what
/// this build can write. `None` (no name, or no steps) is the invalid shape.
pub fn parse_lite_workflow_yaml(document: &str) -> Option<(String, Vec<String>)> {
    let mut name: Option<String> = None;
    let mut steps = Vec::new();
    for line in document.lines() {
        if let Some(rest) = line.strip_prefix("name:") {
            name = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("  - ") {
            steps.push(rest.trim_end().to_owned());
        }
    }
    let name = name?;
    if name.is_empty() || steps.is_empty() {
        return None;
    }
    Some((name, steps))
}

// ── the lean lib/chart pipeline ─────────────────────────────────────────────

use crate::components::chart::chart_canvas::{
    ChartField, InferredFieldType,
};
use crate::components::chart::chart_view::{
    ChartPipeline, ChartRecommendation, StatementResult as ChartStatementResult, TransformResult,
};

/// The `lib/chart` pipeline the page mounts `ChartView` with (the T102
/// container's required trait injection): the lean stand-in for the
/// field-inference / recommendation / transform tasks. The type slice is the
/// chartability ladder T176 already ported ([`column_infers_numeric`]); no
/// recommendation rule fires (T079's are unported — `ChartView`'s own default
/// config plots the first numeric column, exactly what React does when
/// `recommendChart` returns null); the transform is the ungrouped
/// pass-through (one point per row, the x text and the configured y numbers —
/// the grouping/pivot/aggregation fold is T079's).
struct WorkflowChartPipeline;

impl ChartPipeline for WorkflowChartPipeline {
    fn infer_fields(&self, result: &ChartStatementResult) -> Vec<ChartField> {
        result
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let declared = column.data_type.to_lowercase();
                let inferred = if declared.contains("bool") {
                    InferredFieldType::Boolean
                } else if column_infers_numeric(
                    &crate::views::workflow::step_result_utils::StepColumn {
                        name: column.name.clone(),
                        data_type: Some(column.data_type.clone()),
                    },
                    &chartability_preview(result),
                    index,
                ) {
                    InferredFieldType::Numeric
                } else if declared.contains("timestamp")
                    || declared.contains("date")
                    || declared.contains("time")
                {
                    InferredFieldType::Datetime
                } else {
                    InferredFieldType::Categorical
                };
                ChartField {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    inferred_type: inferred,
                    distinct_count: None,
                    sample_values: Vec::new(),
                }
            })
            .collect()
    }

    fn recommend(&self, _fields: &[ChartField], _row_count: usize) -> Option<ChartRecommendation> {
        // T079's recommendation rules are unported; `None` is the honest
        // no-rule-fires answer ChartView's default config covers.
        None
    }

    fn transform(
        &self,
        result: &ChartStatementResult,
        config: &crate::components::chart::chart_canvas::ChartConfig,
    ) -> TransformResult {
        use crate::components::chart::chart_canvas::{ChartDataPoint, ChartValue};
        use crate::components::chart::chart_view::StatementCell;
        // The x field: the configured axis, else the first non-numeric column,
        // else the first column (React's default resolve).
        let fields = self.infer_fields(result);
        let x_axis = config
            .x_axis
            .clone()
            .or_else(|| {
                fields
                    .iter()
                    .find(|field| field.inferred_type != InferredFieldType::Numeric)
                    .map(|field| field.name.clone())
            })
            .or_else(|| result.columns.first().map(|column| column.name.clone()));
        let value_of = |cell: &StatementCell| match cell {
            StatementCell::Text(text) => ChartValue::Text(text.clone()),
            StatementCell::Number(number) => ChartValue::Number(*number),
            _ => ChartValue::Null,
        };
        let mut data = Vec::new();
        for row in &result.rows {
            let mut point = ChartDataPoint::new();
            if let Some(x_axis) = &x_axis {
                if let Some(index) = result.columns.iter().position(|c| &c.name == x_axis) {
                    if let Some(cell) = row.get(index) {
                        point.insert(x_axis.clone(), value_of(cell));
                    }
                }
            }
            for y_axis in &config.y_axes {
                if let Some(index) = result.columns.iter().position(|c| &c.name == y_axis) {
                    if let Some(cell) = row.get(index) {
                        point.insert(y_axis.clone(), value_of(cell));
                    }
                }
            }
            data.push(point);
        }
        TransformResult {
            data,
            // The ungrouped fold: the configured y axes are the series.
            series_keys: config.y_axes.clone(),
        }
    }
}

/// The chartability slice's preview shape over the chart mirror's statement
/// result — the same columns/rows the ladder reads, with the cell values
/// carried as the JSON space it samples.
fn chartability_preview(result: &ChartStatementResult) -> crate::views::workflow::step_result_utils::StatementPreview {
    use crate::components::chart::chart_view::StatementCell;
    crate::views::workflow::step_result_utils::StatementPreview {
        columns: result
            .columns
            .iter()
            .map(|column| crate::views::workflow::step_result_utils::StepColumn {
                name: column.name.clone(),
                data_type: Some(column.data_type.clone()),
            })
            .collect(),
        rows: result
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        StatementCell::Text(text) => {
                            Some(serde_json::Value::String(text.clone()))
                        }
                        StatementCell::Number(number) => {
                            Some(serde_json::Number::from_f64(*number)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null))
                        }
                        StatementCell::Bool(flag) => Some(serde_json::Value::Bool(*flag)),
                        _ => None,
                    })
                    .collect()
            })
            .collect(),
        rows_count: result.rows.len(),
    }
}

// ── the page ────────────────────────────────────────────────────────────────

/// The workflow page — React's `WorkflowPage` body, branch by branch: the
/// toolbar, the sidebar (the workflows list or the history list, the feedback
/// and operation-error lines), the main column (the panel tab bar, the
/// variables + execute row, the run status and steps bars, the active panel's
/// content) and the dialogs the root modal layer mounts. The confirm dialog
/// and the add-to-dashboard pending dialog live in `views/modals.rs`.
pub fn workflow_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let _ = theme; // the kit tokens carry the page's paint (the wapps ruling)
    let k = kit(dark);
    let mut layer = Vec::new();
    let sidebar_w = splitter::splitter_size(state, SIDEBAR_WIDTH_KEY, SIDEBAR_WIDTH_DEFAULT) as f32;
    // Enlarge the actual drag/cursor target, not a spacer beside it. The
    // native shared drag handlers, keyboard actions, and width bounds stay.
    let sidebar_resize = splitter::splitter_handle(
        state,
        SIDEBAR_WIDTH_KEY,
        SplitAxis::Vertical,
        SIDEBAR_WIDTH_MIN,
        SIDEBAR_WIDTH_MAX,
        SIDEBAR_WIDTH_DEFAULT,
    )
    .key(SIDEBAR_HANDLE_KEY)
    .w(SIDEBAR_HANDLE_WIDTH)
    .cursor(rune::Cursor::ResizeColumn)
    .fill(Color::TRANSPARENT)
    .hover(k.c.bg_accent.alpha(0.30));

    // React: `flex h-full flex-col` — the page fills its slot in the shell's
    // page column (the embedded branch's `h-full min-h-0`).
    let page = col()
        .key("workflow-workspace")
        .fill(k.c.page)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(workflow_toolbar(state, k))
        .child(
            row()
                .key("workflow-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(workflow_sidebar(state, k, dark, sidebar_w))
                .child(sidebar_resize)
                .child(workflow_main(state, k, dark, &mut layer)),
        );
    rune_kit::root(page, layer)
}

// ── the toolbar ──────────────────────────────────────────────────────────────

/// React's `workflowToolbar` (the embedded branch: `border-b px-3 py-2`) —
/// reload, create, ai-create, the open-dir button (rendered only while the
/// dir seam has a value — React's own `workflowsDir &&` conditional) and the
/// docs button.
fn workflow_toolbar(state: &AppState, k: &Kit) -> El {
    let mut toolbar = row()
        .key("workflow-toolbar")
        .w_full()
        .shrink()
        .items_center()
        .gap(4.0)
        .px(TOOLBAR_PAD_X)
        .py(TOOLBAR_PAD_Y)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .border_layout()
        .child(toolbar_icon_button(
            "workflow-reload",
            t("workflows.reload"),
            &crate::icons::ICON_REFRESH,
            |state: &mut AppState| state.reload_workflows(),
            k,
        ))
        .child(toolbar_icon_button(
            "workflow-create",
            t("workflows.create"),
            &crate::icons::ICON_PLUS,
            |state: &mut AppState| state.open_workflow_edit_panel(None),
            k,
        ))
        .child(toolbar_icon_button(
            "workflow-ai-create-button",
            t("workflows.aiCreate.title"),
            &crate::icons::ICON_SPARKLES,
            |state: &mut AppState| state.open_workflow_ai_create_panel(),
            k,
        ));
    if state.workflow_dir.as_deref().is_some_and(|dir| !dir.is_empty()) {
        toolbar = toolbar.child(toolbar_icon_button(
            "workflow-open-dir",
            t("workflows.openDir"),
            &crate::icons::ICON_FOLDER_OPEN,
            |state: &mut AppState| state.open_workflow_dir(),
            k,
        ));
    }
    toolbar.child(toolbar_icon_button(
        "workflow-open-docs",
        t("docs.openWorkflowHelp"),
        &ICON_BOOK_OPEN,
        |state: &mut AppState| state.open_workflow_docs(),
        k,
    ))
}

/// One icon-only toolbar button (React's `h-6 w-6 !px-0` secondary buttons
/// with the `h-3 w-3` glyph and a `title` tooltip) — the chart view's export
/// button shape: an `El` row with the button role, the tooltip as the label,
/// and the kit's hover overlay.
fn toolbar_icon_button(
    key: &'static str,
    tooltip: &str,
    glyph: &'static rune::Icon,
    on_press: impl Fn(&mut AppState) + 'static,
    k: &Kit,
) -> El {
    row()
        .key(key)
        .size(TOOLBAR_BUTTON, TOOLBAR_BUTTON)
        .center()
        .radius(k.radius.sm)
        .role("button")
        .label(tooltip.to_owned())
        .help(tooltip.to_owned())
        .focusable(true)
        .hover(k.c.hover)
        .child(icon(glyph, TOOLBAR_ICON).color(k.c.text_muted))
        .on_press(move |state: &mut AppState| on_press(state))
}

// ── the sidebar ──────────────────────────────────────────────────────────────

/// The left sidebar — the two side tabs (React's underline indicators), the
/// feedback line, the operation error banner (the `ErrorBanner` port's strip
/// variant — its first production consumer), and the workflows list or the
/// history list below.
fn workflow_sidebar(state: &AppState, k: &Kit, dark: bool, width: f32) -> El {
    let mut sidebar = col()
        .key("workflow-sidebar")
        .w(width)
        .shrink()
        .min_h(0.0)
        .fill(k.c.surface_weak)
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
        .border_layout()
        .child(side_tab_header(state, k));

    // React: the feedback paragraph — green unless it starts with `Error`.
    if !state.workflow_feedback.is_empty() {
        let ink = if state.workflow_feedback.starts_with("Error") {
            k.c.danger.fg
        } else {
            k.c.success.fg
        };
        sidebar = sidebar.child(
            text(state.workflow_feedback.clone())
                .key("workflow-feedback")
                .font(k.ty.text_xsmall)
                .color(ink)
                .px(12.0)
                .py(4.0)
                .wrap(),
        );
    }

    // React: the `ErrorBanner variant="strip"` with the alert glyph and the
    // dismiss control (`common.close`).
    if let Some(error) = state
        .workflow_operation_error
        .as_ref()
        .or(state.workflow_step_order_error.as_ref())
    {
        let banner = Banner::new("workflow-operation-error", error.clone())
            .variant(BannerVariant::Strip)
            .icon(
                icon(&ICON_ALERT_TRIANGLE, 14.0)
                    .key("workflow-operation-error-icon")
                    .color(k.c.danger.fg)
                    .shrink(),
            )
            .on_dismiss(t("common.close"), |state: &mut AppState| {
                if state.workflow_operation_error.is_some() {
                    state.dismiss_workflow_operation_error();
                } else {
                    state.workflow_step_order_error = None;
                }
            })
            .build(k);
        if let Some(banner) = banner {
            sidebar = sidebar.child(banner);
        }
    }

    // React: `flex-1 overflow-y-auto` — the list scrolls its own column.
    let list = if state.workflow_side_tab == WorkflowSideTab::History {
        WorkflowHistoryList::<AppState>::new(
            &state.workflow_history_items,
            HistoryListSlots {
                view: |state, id, name| state.open_workflow_history_detail(&id, &name),
                clear: |state| state.ask_workflow_history_clear(),
            },
        )
        .build(dark)
    } else {
        workflow_sidebar_list(state, k, dark)
    };
    sidebar.child(
        col()
            .key("workflow-sidebar-scroll")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(list),
    )
}

/// The side tab header — React's two `flex-1` tab buttons with the history
/// icon and the accent underline on the active one.
fn side_tab_header(state: &AppState, k: &Kit) -> El {
    row()
        .key("workflow-side-tabs")
        .w_full()
        .shrink()
        // The interactive buttons themselves split the complete inner width.
        // Growing a button in an intrinsic-width wrapper left a dead region.
        .equal_columns(2)
        .child(side_tab_button(
            "workflow-side-tab-workflows",
            "Workflows",
            None,
            state.workflow_side_tab == WorkflowSideTab::Workflows,
            |state: &mut AppState| state.set_workflow_side_tab(WorkflowSideTab::Workflows),
            k,
        ))
        .child(side_tab_button(
            "workflow-side-tab-history",
            t("workflows.history.title"),
            Some(&ICON_HISTORY),
            state.workflow_side_tab == WorkflowSideTab::History,
            |state: &mut AppState| state.set_workflow_side_tab(WorkflowSideTab::History),
            k,
        ))
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .border_layout()
}

/// One full-width sidebar tab; the two-pixel selection indicator is an
/// overlay inside the hit area, not a separate row that changes its height.
fn side_tab_button(
    key: &'static str,
    label: &str,
    glyph: Option<&'static rune::Icon>,
    active: bool,
    on_press: impl Fn(&mut AppState) + 'static,
    k: &Kit,
) -> El {
    let ink = if active { k.c.text_base } else { k.c.text_faint };
    let tab_type = TextStyle::new(12.0, if active { 500.0 } else { 400.0 }).lh(16.0);
    let mut button = row()
        .key(key)
        .w_full()
        .min_w(0.0)
        .h(SIDE_TAB_HEIGHT)
        .px(12.0)
        .center()
        .gap(4.0)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .on_press(on_press);
    if !active {
        button = button.hover_group();
    }
    if let Some(glyph) = glyph {
        let glyph = icon(glyph, 12.0).color(ink).a11y_hidden();
        button = button.child(if active { glyph } else { glyph.group_color(k.c.text_base) });
    }
    let label = text(label.to_owned())
        .key(format!("{key}-text"))
        .font(tab_type)
        .color(ink)
        .min_w(0.0)
        .truncate();
    button
        .child(if active { label } else { label.group_color(k.c.text_base) })
        .child(
            el().key(format!("{key}-underline"))
                .pin(Some(0.0), None, Some(0.0), Some(0.0))
                .h(SIDE_TAB_UNDERLINE)
                .fill(if active { k.c.bg_accent } else { Color::TRANSPARENT })
                .a11y_hidden(),
        )
}

/// React's `WorkflowSidebarList` — the loading spinner, the workflow rows
/// (wand glyph, name + scheduled clock + description, the hover edit/delete
/// affordances, the secondary-press context-menu seam) and the empty
/// sentence.
fn workflow_sidebar_list(state: &AppState, k: &Kit, dark: bool) -> El {
    if state.workflow_list_loading {
        return row()
            .key("workflow-list-loading")
            .w_full()
            .shrink()
            .py(32.0)
            .center_y()
            .justify_center()
            .gap(4.0)
            .child(
                Spinner::new("workflow-list-loading-spinner")
                    .size(SpinnerSize::Lg)
                    .build(dark),
            )
            .child(
                text(t("workflows.loading").to_owned())
                    .key("workflow-list-loading-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            );
    }
    let active_run = state.active_workflow_run_panel();
    let mut list = col().key("workflow-list").w_full().shrink().padding(6.0);
    for workflow in &state.workflow_list {
        let active = active_run
            .as_ref()
            .is_some_and(|run| run.workflow_id == workflow.id);
        list = list.child(workflow_row(workflow, active, k));
    }
    if state.workflow_list.is_empty() {
        list = list.child(
            col()
                .key("workflow-list-empty")
                .w_full()
                .shrink()
                .py(24.0)
                .child(
                    text(t("workflows.noWorkflows").to_owned())
                        .key("workflow-list-empty-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint)
                        .w_full()
                        .align(rune::Align::Center),
                ),
        );
    }
    list
}

/// One workflow row (React's `group flex items-center gap-2 rounded-md px-3
/// py-2` row): the wand glyph, the name (with the scheduled clock) and
/// description, and the edit/delete affordances React reveals on hover (the
/// `hidden group-hover:flex` reveal is a DOM trick — the port keeps them
/// visible, the history-section ruling). The row's secondary press records
/// the context-menu seam's trigger.
fn workflow_row(workflow: &WorkflowListItem, active: bool, k: &Kit) -> El {
    let id = workflow.id.clone();
    let mut row_el = row()
        .key(format!("workflow-{}", workflow.id))
        .w_full()
        .shrink()
        .items_center()
        .gap(8.0)
        .radius(k.radius.md)
        .px(12.0)
        .py(8.0)
        .focusable(true)
        .role("button")
        .label(workflow.name.clone())
        // React: active `bg-accent/10 text-accent`, else the raised hover.
        .fill(if active {
            k.alpha(k.c.bg_accent, 0.1)
        } else {
            Color::TRANSPARENT
        })
        .hover(if active {
            k.alpha(k.c.bg_accent, 0.1)
        } else {
            k.alpha(k.c.layer_02, 0.5)
        })
        .on_press({
            // React's `onSelect(w)`: the run panel plus the variable defaults.
            let workflow = workflow.clone();
            move |state: &mut AppState| state.select_workflow_list_item(&workflow)
        })
        // The context-menu seam's trigger (T646 owns the entries builder and
        // the native host): the id and the window point.
        .on_secondary_press(move |state: &mut AppState, point: PointerPress, _: &mut rune::Tx| {
            state.open_workflow_context_menu(&id, point);
        });

    let mut name_row = row().items_center().gap(4.0);
    name_row = name_row.child(
        text(workflow.name.clone())
            .key(format!("workflow-{}-name", workflow.id))
            .font(k.ty.text_xsmall_medium)
            .color(if active { k.c.text_accent } else { k.c.text_base })
            .w_full()
            .min_w(0.0)
            .shrink()
            .truncate(),
    );
    if workflow.scheduled {
        name_row = name_row.child(
            icon(&ICON_CLOCK, 12.0)
                .key(format!("workflow-{}-scheduled", workflow.id))
                .color(k.c.text_accent)
                .shrink(),
        );
    }
    let mut body = col()
        .key(format!("workflow-{}-body", workflow.id))
        .grow(1.0)
        .min_w(0.0)
        .shrink()
        .child(name_row);
    if let Some(description) = workflow.description.as_deref().filter(|d| !d.is_empty()) {
        body = body.child(
            text(description.to_owned())
                .key(format!("workflow-{}-description", workflow.id))
                .font(TextStyle {
                    size: 10.0,
                    line_height: 13.0,
                    ..k.ty.text_xsmall
                })
                .color(k.c.text_faint)
                .w_full()
                .min_w(0.0)
                .shrink()
                .truncate(),
        );
    }

    let ink = if active { k.c.text_accent } else { k.c.text_base };
    row_el
        .child(icon(&ICON_WAND_SPARKLES, 14.0).color(ink).shrink())
        .child(body)
        .child(row_action_button(
            &workflow.id,
            "edit",
            t("workflows.edit"),
            &ICON_PENCIL,
            k.c.text_muted,
            {
                let id = workflow.id.clone();
                move |state: &mut AppState| state.edit_workflow_from_list(&id)
            },
            k,
        ))
        .child(row_action_button(
            &workflow.id,
            "delete",
            t("workflows.delete"),
            &crate::icons::ICON_TRASH,
            k.c.danger.fg,
            {
                let id = workflow.id.clone();
                move |state: &mut AppState| state.ask_workflow_delete(&id)
            },
            k,
        ))
}

/// One hover affordance button (React's `p-0.5 rounded` icon buttons; React's
/// shared `workflow-item-edit` testid becomes the per-row key —
/// `Runtime::find` needs unique keys to reach the second row's button).
fn row_action_button(
    workflow_id: &str,
    action: &str,
    tooltip: &str,
    glyph: &'static rune::Icon,
    ink: Color,
    on_press: impl Fn(&mut AppState) + 'static,
    k: &Kit,
) -> El {
    row()
        .key(format!("workflow-{workflow_id}-{action}"))
        .shrink()
        .padding(2.0)
        .radius(4.0)
        .role("button")
        .label(tooltip.to_owned())
        .help(tooltip.to_owned())
        .focusable(true)
        .hover(k.c.hover)
        .child(icon(glyph, 12.0).color(ink))
        .on_press(move |state: &mut AppState| on_press(state))
}

// ── the main column ─────────────────────────────────────────────────────────

/// The main column — the panel tab bar, the variables + execute row, the run
/// status and steps bars, and the active panel's content.
fn workflow_main(state: &AppState, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
    let active = state.active_workflow_panel();
    let current_result = active.as_ref().and_then(|panel| panel.execution_result());
    let current_workflow = active
        .as_ref()
        .and_then(|panel| match panel {
            WorkflowPanel::Run(run) => state
                .workflow_list
                .iter()
                .find(|workflow| workflow.id == run.workflow_id)
                .cloned(),
            _ => None,
        });

    let mut main = col().key("workflow-main").grow(1.0).min_h(0.0).min_w(0.0);

    if !state.workflow_panels.is_empty() {
        main = main.child(panel_tab_bar(state, k, dark));
    }
    if current_workflow.is_some() {
        main = main.child(variables_row(state, k, dark, layer));
    }
    if let Some(result) = current_result {
        main = main.child(run_status_row(state, result, k));
        main = main.child(steps_bar(state, result, k));
    }
    main = main.child(panel_content(
        state,
        active,
        current_workflow.is_some(),
        k,
        dark,
        layer,
    ));
    main
}

/// The panel tab bar — one tab per open panel (the kind's glyph, the label,
/// the ✓/✗ status mark, the executing spinner, the close X).
fn panel_tab_bar(state: &AppState, k: &Kit, dark: bool) -> El {
    let mut bar = row()
        .key("workflow-panel-tabs")
        .w_full()
        .shrink()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.surface_weak);
    for panel in &state.workflow_panels {
        bar = bar.child(panel_tab(state, panel, k, dark));
    }
    bar
}

/// One panel tab (React's tab `div` + its close button).
fn panel_tab(state: &AppState, panel: &WorkflowPanel, k: &Kit, dark: bool) -> El {
    let panel_id = panel.id().to_owned();
    let is_active = state.workflow_active_panel.as_deref() == Some(panel.id());
    let (glyph, glyph_ink) = match panel {
        WorkflowPanel::AiCreate { .. } => (&ICON_SPARKLES, Some(k.c.text_accent)),
        WorkflowPanel::Edit(_) => (&ICON_PENCIL, None),
        WorkflowPanel::Run(_) => (&ICON_WAND_SPARKLES, None),
        WorkflowPanel::History(_) => (&ICON_HISTORY, None),
    };
    let status = panel.execution_result().map(|result| result.success);
    let executing = matches!(panel, WorkflowPanel::Run(run) if run.is_executing);

    let mut tab_button = row()
        .key(format!("workflow-tab-{}", panel.id()))
        .items_center()
        .gap(6.0)
        .role("button")
        .label(panel_tab_label(state, panel))
        .focusable(true)
        .on_press(move |state: &mut AppState| state.select_workflow_panel(&panel_id))
        .child(icon(glyph, 14.0).color(glyph_ink.unwrap_or(k.c.text_muted)))
        .child(
            text(panel_tab_label(state, panel))
                .key(format!("workflow-tab-{}-label", panel.id()))
                .font(k.ty.text_xsmall)
                .color(if is_active { k.c.text_base } else { k.c.text_muted })
                .max_w(140.0)
                .shrink()
                .truncate(),
        );
    if let Some(success) = status {
        let (mark, ink) = if success {
            ("✓", k.c.success.fg)
        } else {
            ("✗", k.c.danger.fg)
        };
        tab_button = tab_button.child(
            text(mark.to_owned())
                .key(format!("workflow-tab-{}-status", panel.id()))
                .font(TextStyle {
                    size: 10.0,
                    line_height: 13.0,
                    ..k.ty.text_xsmall
                })
                .color(ink)
                .shrink(),
        );
    }
    if executing {
        tab_button = tab_button.child(
            Spinner::new(format!("workflow-tab-{}-spinner", panel.id()))
                .size(SpinnerSize::Sm)
                .tone(SpinnerTone::Accent)
                .build(dark),
        );
    }

    col()
        .key(format!("workflow-tab-cell-{}", panel.id()))
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
        .child(
            row()
                .items_center()
                .gap(2.0)
                .child(tab_button)
                .child(
                    row()
                        .key(format!("workflow-tab-{}-close", panel.id()))
                        .padding(2.0)
                        .radius(4.0)
                        .role("button")
                        .label(t("common.close"))
                        .focusable(true)
                        .hover(k.c.hover)
                        .child(icon(&ICON_X, 12.0).color(k.c.text_faint))
                        .on_press({
                            let panel_id = panel.id().to_owned();
                            move |state: &mut AppState| state.close_workflow_panel(&panel_id)
                        }),
                ),
        )
        // The active tab's accent underline.
        .child(
            el()
                .key(format!("workflow-tab-{}-underline", panel.id()))
                .h(2.0)
                .w_full()
                .fill(if is_active { k.c.bg_accent } else { Color::TRANSPARENT }),
        )
}

/// The variables + execute row (React's `shrink-0 border-b px-4 py-3`): the
/// workflow's variable inputs (the current projection carries none — React's
/// own empty-variables case), the Execute button (disabled while executing,
/// the spinner then), and the add-to-dashboard press on a successful run.
fn variables_row(state: &AppState, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
    let active = state.active_workflow_run_panel();
    let executing = active.as_ref().is_some_and(|run| run.is_executing);
    let success = active
        .as_ref()
        .and_then(|run| run.result.as_ref())
        .is_some_and(|result| result.success);

    let mut body = col()
        .key("workflow-variables")
        .w_full()
        .shrink()
        .px(VARIABLES_PAD_X)
        .py(VARIABLES_PAD_Y)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.page);

    if let Some(run) = active.filter(|run| !run.variables.is_empty()
        && state.workflow_list.iter().any(|flow| flow.id == run.workflow_id)) {
        let mut fields = row()
            .key("workflow-variables-fields")
            .w_full()
            .mb(VARIABLES_GAP)
            .gap(VARIABLES_GAP)
            .flex_wrap()
            .items_end();
        for (variable, edit) in &run.variables {
            fields = fields.child(variable_field(state, &run.id, variable, edit, k, dark, layer));
        }
        body = body.child(fields);
    }

    let execute = Button::new(if executing {
        t("workflows.executing").to_owned()
    } else {
        t("workflows.execute").to_owned()
    })
    .variant(ButtonVariant::Submit)
    .control_size(ControlSize::Regular)
    .icon(&ICON_PLAY)
    .disabled(executing)
    .on_press(|state: &mut AppState| state.execute_active_workflow())
    .build(k)
    .key("workflow-execute-button")
    .h(32.0);
    // Use compact intrinsic-width controls. They sit on the same line when
    // space permits (React's inline buttons) and wrap on narrow workspaces.
    let mut actions = row()
        .key("workflow-run-actions")
        .w_full()
        .shrink()
        .gap(8.0)
        .items_center()
        .flex_wrap()
        .child(execute);
    if success {
        actions = actions.child(
            Button::new(t("workflows.addToDashboard").to_owned())
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Regular)
                .on_press(|state: &mut AppState| state.open_add_to_dashboard())
                .build(k)
                .key("workflow-add-to-dashboard")
                .h(32.0),
        );
    }
    body.child(actions)
}

/// One variable input — the label row (name, the required mark, the
/// description) and the field: a connection-type variable selects the saved
/// connections, the rest are text inputs bound into the variables map.
fn variable_field(
    state: &AppState,
    panel_id: &str,
    variable: &WorkflowVariableItem,
    edit: &TextEdit,
    k: &Kit,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let name = variable.name.clone();
    let panel_id = panel_id.to_owned();
    let mut label = row().items_center().gap(2.0);
    label = label.child(
        text(variable.name.clone())
            .key(format!("workflow-variable-{}-label", variable.name))
            .font(TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.text_xsmall
            })
            .color(k.c.text_faint),
    );
    if variable.required {
        label = label.child(
            text("*".to_owned())
                .key(format!("workflow-variable-{}-required", variable.name))
                .color(k.c.danger.fg),
        );
    }
    if let Some(description) = variable.description.as_deref().filter(|d| !d.is_empty()) {
        label = label.child(
            text(format!("({description})"))
                .font(TextStyle {
                    size: 11.0,
                    line_height: 14.0,
                    ..k.ty.text_xsmall
                })
                .color(k.c.text_faint),
        );
    }
    let input = if variable.var_type == "connection" {
        // The connection variable's Select — the same listbox pattern the
        // form's connection select takes; the open state rides the page's
        // own list (`workflow_variable_select`), one open at a time.
        let mut labels: Vec<String> = state
            .workflow_form_connections
            .iter()
            .map(|connection| connection.name.clone())
            .collect();
        let mut values: Vec<String> = state
            .workflow_form_connections
            .iter()
            .map(|connection| connection.id.clone())
            .collect();
        labels.insert(0, String::new());
        values.insert(0, String::new());
        let chosen = state
            .workflow_form_connections
            .iter()
            .find(|connection| connection.id == edit.text)
            .map(|connection| connection.name.clone());
        let select_name = variable.name.clone();
        crate::ui::select::Select::new(format!("workflow-variable-{}", variable.name), &labels)
            .values(&values)
            .current(chosen.as_deref())
            .label(variable.name.clone())
            .open(state.workflow_variable_select.as_deref() == Some(variable.name.as_str()))
            .on_toggle(move |state: &mut AppState| {
                state.workflow_variable_select =
                    if state.workflow_variable_select.as_deref() == Some(select_name.as_str()) {
                        None
                    } else {
                        Some(select_name.clone())
                    };
            })
            .on_close(|state: &mut AppState| state.workflow_variable_select = None)
            .on_choose(move |state: &mut AppState, value: &str, _: &mut rune::Tx| {
                if let Some(edit) = state.edit_workflow_variable(&panel_id, &name) {
                    *edit = TextEdit::new(value.to_owned());
                }
                state.workflow_variable_select = None;
            })
            .build(k, layer)
    } else {
        let field_name = variable.name.clone();
        TextField::new(format!("workflow-variable-{}", variable.name), edit)
            .height(VARIABLE_INPUT_H)
            .placeholder(variable.description.clone().unwrap_or_default())
            .on_edit(move |state: &mut AppState| {
                // The field edits a variable the run panel carried at open;
                // the panel owns the name for its lifetime (the family's
                // row-binding precedent).
                state.edit_workflow_variable(&panel_id, &field_name)
                    .expect("workflow variable edits a live run panel")
            })
            .build(dark)
    };
    col()
        .key(format!("workflow-variable-cell-{}", variable.name))
        .grow(1.0)
        .min_w(160.0)
        .max_w(260.0)
        .gap(2.0)
        .child(label)
        .child(input)
}

/// The execution status row — React's `✓/✗ {totalTimeMs}ms` span and the
/// step-result order toggle (both React testids).
fn run_status_row(state: &AppState, result: &WorkflowExecutionResult, k: &Kit) -> El {
    let (mark, ink) = if result.success {
        ("✓", k.c.success.fg)
    } else {
        ("✗", k.c.danger.fg)
    };
    row()
        .key("workflow-run-status-row")
        .w_full()
        .shrink()
        .items_center()
        .justify_between()
        .gap(8.0)
        .px(12.0)
        .py(4.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.surface_weak)
        .child(
            text(format!("{mark} {}ms", result.total_time_ms))
                .key("workflow-run-status")
                .font(k.ty.text_xsmall_medium)
                .color(ink)
                .shrink(),
        )
        .child(
            row()
                .key("workflow-step-order-toggle")
                .shrink()
                .items_center()
                .gap(4.0)
                .px(6.0)
                .py(2.0)
                .radius(k.radius.sm)
                .role("button")
                .label(t("workflows.stepOrder.toggle"))
                .help(t("workflows.stepOrder.toggle"))
                .focusable(true)
                .hover(k.alpha(k.c.layer_02, 0.5))
                .child(icon(&ICON_ARROW_DOWN_UP, 12.0).color(k.c.text_faint))
                .child(
                    text(step_order_label(state.workflow_step_order()).to_owned())
                        .key("workflow-step-order-toggle-label")
                        .font(TextStyle {
                            size: 11.0,
                            line_height: 14.0,
                            ..k.ty.text_xsmall
                        })
                        .color(k.c.text_faint),
                )
                .on_press(|state: &mut AppState| state.toggle_workflow_step_order()),
        )
}

/// The steps sub-tab bar — one tab per step in the result order (React's
/// `stepIndices`), each with the status icon ladder and the `[{stepType}]`
/// chip.
fn steps_bar(state: &AppState, result: &WorkflowExecutionResult, k: &Kit) -> El {
    let mut bar = row()
        .key("workflow-steps")
        .w_full()
        .shrink()
        .items_center()
        .gap(4.0)
        .px(12.0)
        .py(4.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.surface_weak);
    let indices = step_indices(result, state.workflow_step_order());
    for index in indices {
        let Some(step) = result.steps.get(index) else { continue };
        let active = state.workflow_active_step == Some(index);
        bar = bar.child(step_tab(step, index, active, k));
    }
    bar
}

/// React's `stepIndices` memo: every index, reversed for `desc`.
fn step_indices(result: &WorkflowExecutionResult, order: WorkflowStepOrder) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..result.steps.len()).collect();
    if order == WorkflowStepOrder::Desc {
        indices.reverse();
    }
    indices
}

/// One step tab (React's `relative flex items-center gap-1 rounded px-2 py-1
/// text-[11px]` button; the active one accent-tinted).
fn step_tab(step: &StepExecutionResult, index: usize, active: bool, k: &Kit) -> El {
    let index_for_press = index;
    let mut tab = row()
        .key(format!("workflow-step-{}", step.step_id))
        .shrink()
        .items_center()
        .gap(4.0)
        .px(8.0)
        .py(4.0)
        .radius(k.radius.sm)
        .role("button")
        .label(step.step_id.clone())
        .focusable(true)
        .fill(if active {
            k.alpha(k.c.bg_accent, 0.1)
        } else {
            Color::TRANSPARENT
        })
        .hover(if active {
            k.alpha(k.c.bg_accent, 0.1)
        } else {
            k.alpha(k.c.layer_02, 0.5)
        })
        .on_press(move |state: &mut AppState| state.set_workflow_active_step(index_for_press));
    // React's `StepStatusIcon`: the text glyphs the font pool shapes stay
    // text; ⏭/⏱ fall back to their lucide counterparts (the panel's ruling).
    tab = match step.status {
        WorkflowStepStatus::Success => tab.child(
            text("✓".to_owned())
                .key(format!("workflow-step-{}-status", step.step_id))
                .font(k.ty.text_xsmall)
                .color(k.c.success.fg)
                .shrink(),
        ),
        WorkflowStepStatus::Failed => tab.child(
            text("✗".to_owned())
                .key(format!("workflow-step-{}-status", step.step_id))
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .shrink(),
        ),
        WorkflowStepStatus::Skipped => tab.child(
            icon(&crate::icons::ICON_SKIP_FORWARD, 12.0)
                .key(format!("workflow-step-{}-status", step.step_id))
                .color(k.c.warning.fg)
                .shrink(),
        ),
        WorkflowStepStatus::TimedOut => tab.child(
            icon(&ICON_CLOCK, 12.0)
                .key(format!("workflow-step-{}-status", step.step_id))
                .color(k.c.danger.fg)
                .shrink(),
        ),
    };
    tab = tab.child(
        text(step.step_id.clone())
            .key(format!("workflow-step-{}-id", step.step_id))
            .font(TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.text_xsmall
            })
            .color(if active { k.c.text_accent } else { k.c.text_faint })
            .shrink(),
    );
    if let Some(step_type) = step.step_type.as_deref() {
        tab = tab.child(
            text(format!("[{step_type}]"))
                .key(format!("workflow-step-{}-type", step.step_id))
                .font(TextStyle {
                    size: 11.0,
                    line_height: 14.0,
                    ..k.ty.text_xsmall
                })
                .color(k.c.text_faint)
                .shrink(),
        );
    }
    tab
}

// ── the panel content branches ──────────────────────────────────────────────

/// React's panel-content ladder, order preserved: the ai-create pane, the
/// edit pane, the current step's detail, the select-step sentence, the store
/// error, the empty hint. The selected detail owns the remaining space;
/// the aggregate execution report belongs to the history detail component.
fn panel_content(
    state: &AppState,
    active: Option<&WorkflowPanel>,
    has_current_workflow: bool,
    k: &Kit,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let current_result = active.and_then(|panel| panel.execution_result());
    let current_step = current_result
        .zip(state.workflow_active_step)
        .and_then(|(result, index)| result.steps.get(index));

    // React's ladder, order preserved: the panes first, then the current
    // step's detail, the select-step sentence, the store's error, and the
    // empty hint (an active panel without a result renders nothing).
    if matches!(active, Some(WorkflowPanel::AiCreate { .. })) {
        ai_create_pane(k)
    } else if let Some(WorkflowPanel::Edit(edit)) = active {
        edit_pane(state, edit, k, dark, layer)
    } else if let Some(step) = current_step {
        step_detail_view(state, step, k, dark, layer)
    } else if current_result.is_some() {
        select_step_sentence(k)
    } else if state.workflow_store_error.is_some() {
        store_error_pane(state, k)
    } else if active.is_some() {
        col().key("workflow-panel-empty").grow(1.0).min_h(0.0)
    } else if !has_current_workflow {
        empty_hint_pane(k)
    } else {
        select_step_sentence(k)
    }
}

/// React's `workflows.selectStep` sentence — a panel with a result but no
/// step picked.
fn select_step_sentence(k: &Kit) -> El {
    col()
        .key("workflow-select-step")
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(
            text(t("workflows.selectStep").to_owned())
                .key("workflow-select-step-text")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        )
}

/// React's `workflowError` pane (`select-text text-xs text-red-400`).
fn store_error_pane(state: &AppState, k: &Kit) -> El {
    col()
        .key("workflow-store-error")
        .grow(1.0)
        .min_h(0.0)
        .center()
        .px(16.0)
        .child(
            text(state.workflow_store_error.clone().unwrap_or_default())
                .key("workflow-store-error-text")
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .selectable(true)
                .wrap_anywhere(),
        )
}

/// React's empty hint — the dim wand over the sentence.
fn empty_hint_pane(k: &Kit) -> El {
    col()
        .key("workflow-empty-hint")
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(
            col()
                .items_center()
                .child(
                    icon(&ICON_WAND_SPARKLES, 48.0)
                        .key("workflow-empty-hint-icon")
                        .color(k.alpha(k.c.text_faint, 0.2)),
                )
                .child(
                    text(t("workflows.emptyHint").to_owned())
                        .key("workflow-empty-hint-text")
                        .font(k.ty.body)
                        .color(k.c.text_faint)
                        .mt(12.0),
                ),
        )
}

/// The ai-create pane — React's `WorkflowChatPanel` mount with the pane's
/// geometry and an honest sentence (components/ai is the AI panel's family).
fn ai_create_pane(k: &Kit) -> El {
    col()
        .key("workflow-ai-create-panel")
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(
            text(
                "the AI workflow chat panel is not ported yet — components/ai (T334's family) \
                 will render it here"
                    .to_owned(),
            )
            .key("workflow-ai-create-panel-pending")
            .font(k.ty.text_xsmall)
            .color(k.c.text_faint)
            .wrap()
            .max_w(420.0),
        )
}

/// The edit pane — React's mode tabs, the visual form or the yaml document,
/// and the shared bottom action bar (its `saveTestId` slot carries React's
/// page key `workflow-yaml-save`).
fn edit_pane(
    state: &AppState,
    edit: &WorkflowEditPanel,
    k: &Kit,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let visual = state.workflow_editor_mode == WorkflowEditorMode::Visual;
    let mut pane = col()
        .key("workflow-edit-panel")
        .grow(1.0)
        .min_h(0.0)
        .child(
            row()
                .key("workflow-editor-mode-tabs")
                .w_full()
                .shrink()
                .items_center()
                .gap(4.0)
                .px(12.0)
                .py(6.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .child(mode_tab(
                    "workflow-mode-visual",
                    t("workflows.editor.visual"),
                    WorkflowEditorMode::Visual,
                    visual,
                    k,
                ))
                .child(mode_tab(
                    "workflow-mode-yaml",
                    t("workflows.editor.yaml"),
                    WorkflowEditorMode::Yaml,
                    !visual,
                    k,
                )),
        );
    if visual {
        pane = pane.child(
            col()
                .key("workflow-edit-visual")
                .grow(1.0)
                .min_h(0.0)
                // The bounded edit pane lends its remaining height to
                // this scroll body, not to a fixed-height form or footer.
                .scroll(ScrollSpec { stick_bottom: false })
                .child(
                    WorkflowForm::new(
                        &state.workflow_form_connections,
                        state.workflow_form_slots(),
                    )
                    .editing_id(edit.editing_id.as_deref())
                    .variant(WorkflowFormVariant::Page)
                    .show_action_bar(false)
                    .build_into_layer(state, dark, layer),
                ),
        );
    } else {
        // The yaml mode's real editor: the document surface T200 mounts
        // over the same editing model (the highlighting, the line-number
        // and lint gutters) — the lite panel's stand-in years are over;
        // the document model itself stays.
        pane = pane.child(
            col()
                .key("workflow-edit-yaml")
                .grow(1.0)
                .min_h(0.0)
                .padding(12.0)
                .child(super::workflow_yaml_editor::workflow_yaml_editor(
                    state, dark,
                )),
        );
    }
    // React's Save-disabled rule: the visual mode's draft validity (id, name,
    // steps, the multi-db database need); the yaml mode's parse is the
    // command's own.
    let save_disabled = visual
        && (state.workflow_draft.id_trimmed().is_empty()
            || state.workflow_draft.name_trimmed().is_empty()
            || state.workflow_draft.steps.is_empty()
            || step_needs_database(&state.workflow_draft, &state.workflow_form_connections));
    pane.child(workflow_editor_action_bar::<AppState>(
        "workflow-yaml-save",
        save_disabled,
        |state: &mut AppState| state.save_workflow_edit_panel(),
        |state: &mut AppState| state.close_edit_panel(),
        dark,
    ))
}

/// One editor-mode tab (React's `rounded px-2 py-1 text-xs` buttons; the
/// active one accent-tinted).
fn mode_tab(key: &'static str, label: &str, mode: WorkflowEditorMode, active: bool, k: &Kit) -> El {
    row()
        .key(key)
        .shrink()
        .px(8.0)
        .py(4.0)
        .radius(k.radius.sm)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .fill(if active {
            k.alpha(k.c.bg_accent, 0.15)
        } else {
            Color::TRANSPARENT
        })
        .hover(if active {
            k.alpha(k.c.bg_accent, 0.15)
        } else {
            k.c.hover
        })
        .child(
            text(label.to_owned())
                .key(format!("{key}-text"))
                .font(k.ty.text_xsmall)
                .color(if active { k.c.text_accent } else { k.c.text_faint }),
        )
        .on_press(move |state: &mut AppState| state.set_workflow_editor_mode(mode))
}

// ── the step detail view ─────────────────────────────────────────────────────

/// React's `StepDetailView` — the header (id, type, status, connection, time,
/// rows), the executed SQL, the step error, the AI step's result text, and the
/// table/chart body with its view-mode toggle (rendered only when the step's
/// result is chartable).
fn step_detail_view(
    state: &AppState,
    step: &StepExecutionResult,
    k: &Kit,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let statement = step_to_statement_result(step);
    let preview = step_to_statement_preview(step);
    let chartable = preview.as_ref().is_some_and(is_chartable_preview);
    let has_data = statement
        .as_ref()
        .is_some_and(|result| !result.columns.is_empty() && !result.rows.is_empty());
    let rows_count = preview.as_ref().map_or(0, |preview| preview.rows_count);
    let chart_mode = state.workflow_step_chart.contains(&step.step_id);

    let mut view = col().key("workflow-step-detail").grow(1.0).min_h(0.0);

    // The header row.
    let mut header = row()
        .key("workflow-step-detail-header")
        .w_full()
        .shrink()
        .items_center()
        .gap(12.0)
        .px(12.0)
        .py(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(k.c.layer_01)
        .child(
            text(step.step_id.clone())
                .key("workflow-step-detail-id")
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_base)
                .shrink(),
        );
    if let Some(step_type) = step.step_type.as_deref() {
        header = header.child(
            text(format!("[{step_type}]"))
                .key("workflow-step-detail-type")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .shrink(),
        );
    }
    header = header.child(step_status_mark(step, k));
    if let Some(connection) = step.connection_name.as_deref() {
        header = header.child(
            text(connection.to_owned())
                .key("workflow-step-detail-connection")
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent)
                .shrink(),
        );
    }
    header = header.child(
        text(format!("{}ms", step.execution_time_ms))
            .key("workflow-step-detail-time")
            .font(k.ty.text_xsmall)
            .color(k.c.text_faint)
            .shrink(),
    );
    if rows_count > 0 {
        header = header.child(
            text(format!("{} {}", rows_count, t("common.rows")))
                .key("workflow-step-detail-rows")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .shrink(),
        );
    }
    if has_data && chartable {
        header = header.child(view_toggle(step, chart_mode, k));
    }
    view = view.child(header);

    // The executed SQL block.
    if let Some(sql) = step.sql_executed.as_deref().filter(|sql| !sql.is_empty()) {
        view = view.child(
            col()
                .key("workflow-step-sql")
                .w_full()
                .shrink()
                .px(12.0)
                .py(8.0)
                .fill(k.c.bg_base)
                .child(
                    text(sql.to_owned())
                        .key("workflow-step-sql-text")
                        .font(TextStyle {
                            size: 11.0,
                            line_height: 14.0,
                            ..k.ty.mono
                        })
                        .color(k.c.text_muted)
                        .w_full()
                        .max_h(80.0)
                        .wrap_anywhere()
                        .selectable(true),
                ),
        );
    }

    // The step error block.
    if let Some(error) = step.error.as_deref().filter(|error| !error.is_empty()) {
        view = view.child(
            col()
                .key("workflow-step-error")
                .w_full()
                .shrink()
                .px(12.0)
                .py(8.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .fill(k.alpha(k.c.danger.fg, 0.05))
                .child(
                    text(error.to_owned())
                        .key("workflow-step-error-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.danger.fg)
                        .w_full()
                        .wrap_anywhere()
                        .selectable(true),
                ),
        );
    }

    // The AI step's result text.
    if step.step_type.as_deref() == Some("ai") {
        if let Some(result) = step
            .result
            .as_ref()
            .and_then(|result| result.get("result"))
            .filter(|value| !value.is_null())
        {
            view = view.child(
                col()
                    .key("workflow-step-ai-result")
                    .w_full()
                    .shrink()
                    .px(12.0)
                    .py(8.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                    .child(
                        text(js_string(result))
                            .key("workflow-step-ai-result-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .w_full()
                            .max_h(240.0)
                            .wrap_anywhere()
                            .selectable(true),
                    ),
            );
        }
    }

    // The body: the chart view or the lean table (or the no-result sentence).
    if chart_mode {
        if let Some(statement) = statement.as_ref() {
            if statement.rows.len() > SAMPLE_LIMIT {
                let warning = t("workflows.sampledWarning").replace("{limit}", "1000");
                view = view.child(
                    row()
                        .key("workflow-step-sampled-warning")
                        .w_full()
                        .shrink()
                        .items_center()
                        .gap(4.0)
                        .px(12.0)
                        .py(4.0)
                        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                        .fill(k.c.layer_01)
                        .child(
                            icon(&ICON_ALERT_TRIANGLE, 12.0)
                                .key("workflow-step-sampled-warning-icon")
                                .color(k.c.warning.fg),
                        )
                        .child(
                            text(warning)
                                .key("workflow-step-sampled-warning-text")
                                .font(TextStyle {
                                    size: 11.0,
                                    line_height: 14.0,
                                    ..k.ty.text_xsmall
                                })
                                .color(k.c.warning.fg)
                                .wrap()
                                .min_w(0.0)
                                .shrink(),
                        ),
                );
            }
            view = view.child(chart_view_mount(state, step, statement, dark, layer));
        }
    } else if let Some(statement) = statement.as_ref().filter(|_| has_data) {
        view = view.child(step_table(step, statement, k));
    } else if step.step_type.as_deref() == Some("query") && step.error.is_none() {
        view = view.child(
            col()
                .key("workflow-step-no-result")
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(
                    text(t("workflows.noQueryResult").to_owned())
                        .key("workflow-step-no-result-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint),
                ),
        );
    }
    view
}

/// The step's status mark (the steps bar's own ladder, in the header's ink).
fn step_status_mark(step: &StepExecutionResult, k: &Kit) -> El {
    match step.status {
        WorkflowStepStatus::Success => text("✓".to_owned())
            .key("workflow-step-detail-status")
            .font(k.ty.text_xsmall)
            .color(k.c.success.fg)
            .shrink(),
        WorkflowStepStatus::Failed => text("✗".to_owned())
            .key("workflow-step-detail-status")
            .font(k.ty.text_xsmall)
            .color(k.c.danger.fg)
            .shrink(),
        WorkflowStepStatus::Skipped => icon(&crate::icons::ICON_SKIP_FORWARD, 12.0)
            .key("workflow-step-detail-status")
            .color(k.c.warning.fg)
            .shrink(),
        WorkflowStepStatus::TimedOut => icon(&ICON_CLOCK, 12.0)
            .key("workflow-step-detail-status")
            .color(k.c.danger.fg)
            .shrink(),
    }
}

/// The table/chart view toggle (React's `ml-auto` segmented pair, rendered
/// only when the result is chartable).
fn view_toggle(step: &StepExecutionResult, chart_mode: bool, k: &Kit) -> El {
    let step_id = step.step_id.clone();
    row()
        .key("workflow-step-view-toggle")
        .grow(1.0)
        .justify_end()
        .items_center()
        .gap(2.0)
        .radius(k.radius.md)
        .fill(k.c.bg_base)
        .padding(2.0)
        .child(view_toggle_entry(
            "workflow-view-table",
            t("workflows.viewTable"),
            &ICON_TABLE_PROPERTIES,
            !chart_mode,
            false,
            step_id.clone(),
            k,
        ))
        .child(view_toggle_entry(
            "workflow-view-chart",
            t("workflows.viewChart"),
            &ICON_BAR_CHART_3,
            chart_mode,
            true,
            step_id,
            k,
        ))
}

/// One view-toggle entry (React's segmented pair; the active one
/// accent-tinted). `chart` is the entry's target view mode.
fn view_toggle_entry(
    key: &'static str,
    label: &str,
    glyph: &'static rune::Icon,
    active: bool,
    chart: bool,
    step_id: String,
    k: &Kit,
) -> El {
    let ink = if active { k.c.text_accent } else { k.c.text_faint };
    row()
        .key(key)
        .shrink()
        .items_center()
        .gap(4.0)
        .px(8.0)
        .py(2.0)
        .radius(k.radius.sm)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .fill(if active {
            k.alpha(k.c.bg_accent, 0.2)
        } else {
            Color::TRANSPARENT
        })
        .hover(if active {
            k.alpha(k.c.bg_accent, 0.2)
        } else {
            k.c.hover
        })
        .child(icon(glyph, 12.0).color(ink))
        .child(text(label.to_owned()).font(k.ty.text_xsmall).color(ink))
        .on_press(move |state: &mut AppState| {
            state.set_workflow_step_chart(&step_id, chart);
        })
}

/// The `ChartView` mount — the T102 container wired to the page state: the
/// config slot (`workflow_chart_configs`, keyed by step id), the split /
/// expand / export-open flags, the toolbar's change seam, and the data-point
/// click that flips back to the table view (React's `setViewMode('table')`).
fn chart_view_mount(
    state: &AppState,
    step: &StepExecutionResult,
    statement: &StatementResult,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    use crate::components::chart::chart_view::{
        ChartView, StatementCell, StatementResult as ChartStatementResult,
    };
    use serde_json::Value;

    // The chart mirror of the step's statement result (the columns and rows
    // the pipeline reads; non-scalar cells ride as the chart value space's
    // null arm).
    let chart_result = ChartStatementResult {
        columns: statement
            .columns
            .iter()
            .map(|column| crate::components::chart::chart_view::ColumnInfo {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
            })
            .collect(),
        rows: statement
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell.as_ref() {
                        Some(Value::Number(number)) => {
                            StatementCell::Number(number.as_f64().unwrap_or_default())
                        }
                        Some(Value::String(text)) => StatementCell::Text(text.clone()),
                        Some(Value::Bool(flag)) => StatementCell::Bool(*flag),
                        _ => StatementCell::Null,
                    })
                    .collect()
            })
            .collect(),
    };

    let pipeline = WorkflowChartPipeline;
    let fields = pipeline.infer_fields(&chart_result);
    let recommendation = pipeline.recommend(&fields, chart_result.rows.len());
    // React's config derive: the saved config wins, else the recommendation,
    // else the first numeric column (the notify side rides the toolbar's
    // change seam — the first edit lands the derived config in the slot).
    let saved = state.workflow_chart_configs.get(&step.step_id);
    let sync = crate::components::chart::chart_view::sync_config(
        saved,
        recommendation.as_ref(),
        &fields,
    );

    let step_id = step.step_id.clone();
    let expanded = state.workflow_chart_expanded.contains(&step_id);
    let split = state.workflow_chart_split.contains(&step_id);
    let export_open = state.workflow_chart_export_open.contains(&step_id);

    let mut view = ChartView::<AppState>::new(&chart_result, &pipeline, &sync.config)
        .expanded(expanded)
        .split_view(split)
        .export_open(export_open)
        .on_change(move |state, config| {
            state.workflow_chart_configs.insert(step_id.clone(), config);
        });
    {
        let step_id = step.step_id.clone();
        view = view.on_toggle_split(move |state| {
            if state.workflow_chart_split.contains(&step_id) {
                state.workflow_chart_split.remove(&step_id);
            } else {
                state.workflow_chart_split.insert(step_id.clone());
            }
        });
    }
    {
        let step_id = step.step_id.clone();
        view = view.on_expand(move |state| {
            state.workflow_chart_expanded.insert(step_id.clone());
        });
    }
    {
        let step_id = step.step_id.clone();
        view = view.on_close(move |state| {
            state.workflow_chart_expanded.remove(&step_id);
        });
    }
    {
        let step_id = step.step_id.clone();
        view = view.on_export_toggle(move |state| {
            if state.workflow_chart_export_open.contains(&step_id) {
                state.workflow_chart_export_open.remove(&step_id);
            } else {
                state.workflow_chart_export_open.insert(step_id.clone());
            }
        });
    }
    {
        let step_id = step.step_id.clone();
        view = view.on_export_close(move |state| {
            state.workflow_chart_export_open.remove(&step_id);
        });
    }
    // The export request seam (`lib/chart/export.ts` unported): the press
    // says so in the page's feedback slot — no phantom download.
    view = view.on_export(|state, format| {
        state.workflow_feedback = format!(
            "chart export is not ported yet — lib/chart/export.ts will write the {} file",
            format.menu_label()
        );
    });
    {
        let step_id = step.step_id.clone();
        view = view.on_data_point_click(move |state, _index| {
            state.set_workflow_step_chart(&step_id, false);
        });
    }
    view.build(dark, layer)
}

/// Read-only preview of the real result, with the original DataTable's
/// 40pt index lane/header and the workflow's 32pt body rows. The grid does
/// not expose the unported export, virtualisation, or column-menu controls.
fn step_table(step: &StepExecutionResult, statement: &StatementResult, k: &Kit) -> El {
    // The statement converter supplies "text" for chart compatibility when
    // the source carries no type. Do not present that fallback as a declared
    // driver type or guess a type from one sampled value.
    let metadata = step.result.as_ref().map(|result| {
        let view = result.get("rows").filter(|rows| rows.is_object()).unwrap_or(result);
        extract_step_column_names(view)
    }).unwrap_or_default();
    let cell = |element: El| element.w(0.0).grow(1.0).shrink().min_w(0.0);
    let index_cell = |label: String, key: String, height: f32| {
        row().w(40.0).min_w(40.0).h(height).shrink()
            .center()
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(text(label).key(key).font(k.ty.text_xsmall).color(k.c.text_muted))
    };
    let mut head = row()
        .key("workflow-step-table-head")
        .w_full().h(40.0).shrink().items_center()
        .fill(k.c.layer_01)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(index_cell("#".to_owned(), "workflow-step-table-index-header".to_owned(), 40.0));
    for (column_index, column) in statement.columns.iter().enumerate() {
        let mut caption = col().h(40.0).px(12.0).justify_center()
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                text(column.name.clone())
                    .key(format!("workflow-step-table-col-{column_index}"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base)
                    .shrink()
                    .truncate(),
            );
        if let Some(kind) = metadata.get(column_index)
            .filter(|field| field.name == column.name)
            .and_then(|field| field.data_type.as_deref())
            .filter(|kind| !kind.trim().is_empty())
        {
            caption = caption.child(
                text(kind.to_owned())
                    .key(format!("workflow-step-table-col-type-{column_index}"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .truncate(),
            );
        }
        head = head.child(cell(caption));
    }
    let mut table = col()
        .key("workflow-step-table")
        .w_full().grow(1.0).min_h(0.0).min_w(0.0).pr(4.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(head);
    for (row_index, cells) in statement.rows.iter().enumerate() {
        let mut row_el = row()
            .key(format!("workflow-step-table-row-{row_index}"))
            .w_full().h(32.0).shrink().items_center()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(index_cell(
                (row_index + 1).to_string(),
                format!("workflow-step-table-row-index-{row_index}"),
                32.0,
            ));
        for (column_index, value) in cells.iter().enumerate() {
            let display = value.as_ref().map(js_string).unwrap_or_default();
            row_el = row_el.child(
                cell(row().h(32.0).px(12.0).center_y()
                    .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
                    .child(text(display)
                        .key(format!("workflow-step-table-cell-{row_index}-{column_index}"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_base)
                        .min_w(0.0)
                        .shrink()
                        .truncate())),
            );
        }
        table = table.child(row_el);
    }
    table
}
