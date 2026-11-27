//! Port of `packages/frontend/src/windows/workflow/WorkflowForm.tsx` (T070's
//! split scope: the visual workflow editor form — the draft model it exports,
//! the field/variable/step editors, the command-input schema editor and the
//! two footer variants).
//!
//! React renders one controlled form over a [`WorkflowDraft`]: every edit
//! rebuilds the draft and calls `onDraftChange`. In immediate mode the draft
//! lives in the host's `S` and every control mutates it through
//! [`WorkflowFormSlots::draft_mut`] — the whole-draft replacement and the
//! partial mutation are the same single-owner edit.
//!
//! # What landed with which task (honest seams)
//!
//! - **The page mount** (`WorkflowPage.tsx`) is T147. This task wires the
//!   state (`AppState::workflow_draft` + the form caches + the slots
//!   constructor in `schema_workflows_state.rs`) and proves it with probes
//!   that drive the real `AppState`. The pre-parity lite editor keeps the
//!   `workflow-form` / `step-up-*` / `step-down-*` keys the workspace-tabs
//!   tests pin, so this form roots at `workflow-form-root` — T147 swaps the
//!   whole edit pane and may reclaim the shorter key then.
//! - **The editable SQL engine** (`components/SqlEditor.tsx`, T243): the
//!   query-step and command-`sql` frames render React's geometry (`h-36`
//!   bordered frame, the `workflow-sql-editor` / `command-sql-editor`
//!   testids, per-step keys) with the draft's SQL as read-only mono text.
//!   The context menu (T155) and `formatSql` (T615) ride along with it.
//! - **The loaders** React drives through effects — `driverCommands` per
//!   command connection, `listDatabasesDedicated` per multi-db connection
//!   (T467), the `aiCommands.workflowGet` hydrate for editing a saved
//!   workflow — are host-side seams: the host fills
//!   `workflow_form_commands` / `workflow_form_databases` and the form
//!   renders them. The pure halves the effects compute live here and are
//!   tested: [`multi_db_connections`] (the memo) and
//!   [`auto_fill_single_databases`] (the single-database fill rule).
//! - **`rememberWorkflowDraft`** (`draftBridge.ts`, T484) is a page-level
//!   cross-window bridge; not ported.
//! - **JSON textareas**: kit `TextArea` has no font slot, so the
//!   `font-mono` typeface of React's JSON boxes is not reproduced (the
//!   geometry — `min-h-[80px]`, border, radius — is). React's array/object
//!   textareas are controlled and snap back to the last valid
//!   serialization on a failed parse; the port keeps the raw text visible
//!   (like React's own raw fallback textarea) while the *data* path stays
//!   identical: only JSON that parses lands in the draft's `input`.
//! - **The step-header selects' `!h-7`** override (28pt) happens to be the
//!   kit field trigger's own height, so no override is needed; the
//!   `!h-8` selects (32pt) are set explicitly after the build.
//! - **The icon-only remove buttons** are unnamed in React; the port names
//!   them for the screen reader with the existing `common.delete` /
//!   `workflows.form.removeStep` keys.
//!
//! # Immediate-mode state
//!
//! React keeps per-row `useState`s; the port's `S` holds: the draft, the one
//! open listbox ([`WorkflowFormSelect`] — React's `ui/Select` keeps its own
//! open state, and closing on a choice is the form's handler's job, which
//! every `on_choose` here reproduces), the command-input text areas
//! ([`WorkflowFormSlots::input_edits`], keyed `step<i>.<field>` /
//! `step<i>.raw` — React's `CommandInputEditor` locals, reset by the
//! command/connection choice and the type switch exactly where React's
//! `useEffect([definition?.id])` resets them), and the loaded command /
//! database caches.
//!
//! ## Keys (prefix `workflow-form`)
//!
//! | part | key |
//! |------|-----|
//! | root | `workflow-form-root` |
//! | compact title | `workflow-form-compact-title` |
//! | id / name / description fields | `workflow-form-id` / `-name` / `-description` |
//! | connection select + hint | `workflow-form-connection` / `-connection-hint` |
//! | database select / loading / hint | `workflow-form-database` / `-database-loading` / `-database-hint` |
//! | schedule box / checkbox / interval / hint | `workflow-form-schedule` / `-schedule-enabled` / `-schedule-interval` / `-schedule-hint` |
//! | variables label / add | `workflow-form-variables-label` / `workflow-form-add-variable` |
//! | variable row parts | `workflow-form-variable-<i>` + `-name` / `-type` / `-description` / `-required` / `-remove` |
//! | steps label / add | `workflow-form-steps-label` / `workflow-form-add-step` |
//! | step card / head / id / type / connection / database / remove | `workflow-form-step-<i>` + `-head` / `-id` / `-type` / `-connection` / `-database` / `-remove` |
//! | command select / description | `workflow-form-step-<i>-command` / `-command-description` |
//! | command input editor | `workflow-form-step-<i>-input` (+ `-input-<field>`, `-input-raw`) |
//! | query SQL frame (React `workflow-sql-editor`) | `workflow-form-step-<i>-sql` |
//! | command SQL frame (React `command-sql-editor`) | `workflow-form-step-<i>-input-sql-editor` |
//! | AI prompt | `workflow-form-step-<i>-prompt` |
//! | compact footer | `workflow-form-save` / `workflow-form-cancel` |
//!
//! The kit appends `-trigger` to a select's key for its combobox and keys
//! the listbox rows `option<i>`.

use std::collections::{BTreeMap, BTreeSet};

use rune::{TextEdit, col, icon, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, TextArea};
use serde_json::{Map, Value};

use crate::i18n::t;
use crate::icons::ICON_TRASH;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;
use crate::views::workflow::editor_action_bar::workflow_editor_action_bar;

// ── geometry constants (Tailwind → points) ─────────────────────────────────

/// The page variant's `space-y-4`.
const PAGE_GAP: f32 = 16.0;
/// The compact variant's `space-y-2`.
const COMPACT_GAP: f32 = 8.0;
/// `p-3` (compact box, schedule box, step cards).
const PAD_3: f32 = 12.0;
/// `px-6` / `pt-6` (page variant).
const PAGE_PAD_X: f32 = 24.0;
const PAGE_PAD_TOP: f32 = 24.0;
/// `max-w-3xl`.
const PAGE_MAX_W: f32 = 768.0;
/// The `h-8` text input.
const INPUT_H: f32 = 32.0;
/// The SQL frame's `h-36`.
const SQL_FRAME_H: f32 = 144.0;

/// React's `text-xs` label rows (`text-fg-muted`).
fn label_style(k: &Kit) -> TextStyle {
    k.ty.text_xsmall
}

/// React's `text-[11px]` hint lines.
fn hint_style(k: &Kit) -> TextStyle {
    TextStyle { size: 11.0, ..k.ty.ui }
}

// ── the draft model (React's own exports) ─────────────────────────────────

/// The step kinds the form's type select offers (React's `WorkflowStepType`
/// has seven; the form creates and edits these three — the rest round-trip
/// through the YAML editor, T202).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowStepKind {
    Query,
    Command,
    Ai,
}

impl WorkflowStepKind {
    /// React's select value.
    pub fn as_str(self) -> &'static str {
        match self {
            WorkflowStepKind::Query => "query",
            WorkflowStepKind::Command => "command",
            WorkflowStepKind::Ai => "ai",
        }
    }
    /// React's select label.
    pub fn label(self) -> &'static str {
        match self {
            WorkflowStepKind::Query => "Query (legacy)",
            WorkflowStepKind::Command => "Command",
            WorkflowStepKind::Ai => "AI",
        }
    }
}

/// React's variable row `{ name, varType, description, required }`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowVariableDraft {
    pub name: TextEdit,
    /// `'string' | 'number' | 'connection'`.
    pub var_type: String,
    pub description: TextEdit,
    pub required: bool,
}

impl WorkflowVariableDraft {
    /// React's add-variable row.
    pub fn empty() -> Self {
        Self {
            name: TextEdit::new(String::new()),
            var_type: "string".to_owned(),
            description: TextEdit::new(String::new()),
            required: false,
        }
    }
}

/// React's `WorkflowStepDraft`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowStepDraft {
    pub kind: WorkflowStepKind,
    pub id: TextEdit,
    /// The query step's SQL document — the data path the SQL engine (T243)
    /// binds its editor to.
    pub sql: String,
    /// The AI step's prompt (React's textarea).
    pub prompt: TextEdit,
    pub connection: Option<String>,
    pub database: Option<String>,
    /// The command step's chosen command id.
    pub command: Option<String>,
    /// The command step's input object (React's `Record<string, unknown>`).
    pub input: Map<String, Value>,
}

impl WorkflowStepDraft {
    /// React's `emptyDraft` seeds `{ type: 'query', id: 'step1', sql: '' }`.
    pub fn query(id: &str) -> Self {
        Self {
            kind: WorkflowStepKind::Query,
            id: TextEdit::new(id.to_owned()),
            sql: String::new(),
            prompt: TextEdit::new(String::new()),
            connection: None,
            database: None,
            command: None,
            input: Map::new(),
        }
    }
    /// React's add-step button pushes `{ type: 'command', id, command: '', input: {} }`.
    pub fn command(id: &str) -> Self {
        Self {
            kind: WorkflowStepKind::Command,
            id: TextEdit::new(id.to_owned()),
            sql: String::new(),
            prompt: TextEdit::new(String::new()),
            connection: None,
            database: None,
            command: None,
            input: Map::new(),
        }
    }
}

/// React's `WorkflowDraft`. The text fields are `TextEdit`s (the immediate
/// mode editing model, the same idiom as `AppState::widget_title`); the
/// interval keeps its typed text and [`WorkflowDraft::interval_secs`]
/// applies React's `Number(value) || 3600`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowDraft {
    pub id: TextEdit,
    pub name: TextEdit,
    pub description: TextEdit,
    pub connection: Option<String>,
    /// The default database multi-db data steps inherit.
    pub database: Option<String>,
    pub variables: Vec<WorkflowVariableDraft>,
    pub steps: Vec<WorkflowStepDraft>,
    pub schedule_enabled: bool,
    /// React's `scheduleIntervalSecs ?? 3600`, held as the field's text.
    pub schedule_interval: TextEdit,
}

impl WorkflowDraft {
    /// React's `emptyDraft()`.
    pub fn empty() -> Self {
        Self {
            id: TextEdit::new(String::new()),
            name: TextEdit::new(String::new()),
            description: TextEdit::new(String::new()),
            connection: None,
            database: None,
            variables: Vec::new(),
            steps: vec![WorkflowStepDraft::query("step1")],
            schedule_enabled: false,
            schedule_interval: TextEdit::new("3600".to_owned()),
        }
    }

    /// React's `Number(e.target.value) || 3600` on the interval field.
    pub fn interval_secs(&self) -> u64 {
        self.schedule_interval
            .text
            .trim()
            .parse::<u64>()
            .unwrap_or(3600)
    }

    /// `draft.id.trim()` — the compact Save's disabled rule reads it trimmed.
    pub fn id_trimmed(&self) -> &str {
        self.id.text.trim()
    }

    /// `draft.name.trim()`.
    pub fn name_trimmed(&self) -> &str {
        self.name.text.trim()
    }
}

/// React's compact footer's `disabled` prop.
pub fn compact_save_disabled(draft: &WorkflowDraft) -> bool {
    draft.id_trimmed().is_empty()
        || draft.name_trimmed().is_empty()
        || draft.steps.is_empty()
}

// ── the form's inputs ─────────────────────────────────────────────────────

/// One row of the `connections` prop: React's
/// `{ id, name, databaseType, database? }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowConnectionOption {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}

/// The slice of `DriverCommandMetadata` the form reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowCommandMetadata {
    pub category: Option<String>,
    /// `'read' | 'write' | 'highRisk'` when present.
    pub risk: Option<String>,
    /// React filters `d.metadata?.workflow !== false`.
    pub workflow: bool,
    pub deprecated: bool,
}

/// The slice of `DriverCommandDefinition` the form consumes. The full
/// contract lives in `packages/backend-client` (its Rust twin lands with the
/// commands domain); the form only labels, describes and schema-edits.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowCommandDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// The JSON-schema `inputSchema` object.
    pub input_schema: Value,
    pub metadata: WorkflowCommandMetadata,
}

// ── pure helpers (React's exported/derived rules) ──────────────────────────

/// The DB registry slice the multi-db rule consults —
/// `DB_REGISTRY[type].hasMultiDatabase` + `databaseFieldType === 'domain'`.
/// Sourced from `crates/drivers/*/ui/meta.ts` (`hasMultiDatabase: true` on
/// the protocol-reuse families; `kiwi` is the domain-type one). The full
/// registry port is the `lib/databaseTypes` layer task.
pub fn db_registry_meta(database_type: &str) -> Option<(bool, bool)> {
    // (hasMultiDatabase, databaseFieldType === 'domain')
    match database_type {
        "postgresql" | "questdb" | "cloudberry" | "mysql" | "mariadb" | "doris" | "starrocks"
        | "manticore" | "ob_oracle" | "mongodb" | "sqlserver" | "clickhouse" => Some((true, false)),
        "kiwi" => Some((true, true)),
        _ => None,
    }
}

/// React's exported `connectionAllowsMultiDb`: a connection offers a
/// database picker when its driver has multi-db capability and is not locked
/// to a single real database — a configured `database` is only that lock
/// for non-domain drivers (domain-type drivers store an instance domain
/// separately and always stay multi-db).
pub fn connection_allows_multi_db(conn: Option<&WorkflowConnectionOption>) -> bool {
    let Some(conn) = conn else { return false };
    let Some((multi, domain)) = db_registry_meta(&conn.database_type) else {
        return false;
    };
    if !multi {
        return false;
    }
    conn.database.is_none() || domain
}

/// `step.connection || draft.connection || ''` — the connection a step acts
/// through (React's `effectiveConnection`).
pub fn effective_connection(draft: &WorkflowDraft, step: &WorkflowStepDraft) -> String {
    step.connection
        .clone()
        .or_else(|| draft.connection.clone())
        .unwrap_or_default()
}

/// React's `multiDbConnections` memo: the distinct multi-db connections used
/// at workflow level or by query/command steps (the ones whose database list
/// the form asks for).
pub fn multi_db_connections(
    draft: &WorkflowDraft,
    connections: &[WorkflowConnectionOption],
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut push = |id: &str| {
        let conn = connections.iter().find(|c| c.id == id);
        if connection_allows_multi_db(conn) && seen.insert(id.to_owned()) {
            out.push(id.to_owned());
        }
    };
    if let Some(conn) = &draft.connection {
        push(conn);
    }
    for step in &draft.steps {
        if matches!(step.kind, WorkflowStepKind::Query | WorkflowStepKind::Command) {
            push(&effective_connection(draft, step));
        }
    }
    out
}

/// React's auto-fill effect (the rule half): when a multi-db connection
/// offers exactly one database, that database fills the workflow default and
/// every query/command step's `database` unless the step overrides its own.
/// Returns whether anything changed. The *caller* is the host's database
/// loader resolution (the async half, T467-seam).
pub fn auto_fill_single_databases(
    draft: &mut WorkflowDraft,
    databases: &BTreeMap<String, Vec<String>>,
) -> bool {
    let mut changed = false;
    if draft.connection.is_some() && draft.database.is_none() {
        if let Some(conn) = &draft.connection {
            if let Some(dbs) = databases.get(conn) {
                if dbs.len() == 1 && draft.database.as_deref() != Some(dbs[0].as_str()) {
                    draft.database = Some(dbs[0].clone());
                    changed = true;
                }
            }
        }
    }
    for step in &mut draft.steps {
        if !matches!(step.kind, WorkflowStepKind::Query | WorkflowStepKind::Command) {
            continue;
        }
        let conn = step
            .connection
            .clone()
            .or_else(|| draft.connection.clone());
        let Some(conn) = conn else { continue };
        if let Some(dbs) = databases.get(&conn) {
            if dbs.len() == 1 && step.database.as_deref() != Some(dbs[0].as_str()) {
                step.database = Some(dbs[0].clone());
                changed = true;
            }
        }
    }
    changed
}

/// React's `commandOptionLabel`: `name` + ` · category` + ` (deprecated)`.
pub fn command_option_label(def: &WorkflowCommandDefinition) -> String {
    let category = def
        .metadata
        .category
        .as_deref()
        .map(|c| format!(" · {c}"))
        .unwrap_or_default();
    let deprecated = if def.metadata.deprecated {
        " (deprecated)"
    } else {
        ""
    };
    format!("{}{category}{deprecated}", def.name)
}

/// One property of the command's input schema (React's `schemaProperties`).
pub struct CommandSchemaField {
    pub name: String,
    pub schema: Value,
    pub required: bool,
}

/// React's `schemaProperties`: the input schema's `properties` entries with
/// their required flags. `serde_json`'s map iterates sorted; React keeps the
/// JSON's insertion order — a documented ordering seam.
pub fn schema_properties(
    def: Option<&WorkflowCommandDefinition>,
) -> Vec<CommandSchemaField> {
    let Some(def) = def else { return Vec::new() };
    let Some(schema) = def.input_schema.as_object() else {
        return Vec::new();
    };
    let properties: Vec<(String, Value)> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    properties
        .into_iter()
        .map(|(name, raw)| CommandSchemaField {
            required: required.contains(name.as_str()),
            name,
            schema: raw,
        })
        .collect()
}

/// React's `defaultInput`: `false` / `[]` / `{}` / `''` per schema type.
pub fn default_input(def: Option<&WorkflowCommandDefinition>) -> Map<String, Value> {
    let mut input = Map::new();
    for field in schema_properties(def) {
        let value = match field.schema.get("type").and_then(Value::as_str) {
            Some("boolean") => Value::Bool(false),
            Some("array") => Value::Array(Vec::new()),
            Some("object") => Value::Object(Map::new()),
            _ => Value::String(String::new()),
        };
        input.insert(field.name, value);
    }
    input
}

/// React's `getDefinitions` filter: workflow-capable, and a deprecated
/// command stays only while it is the chosen one.
pub fn workflow_definitions<'a>(
    defs: &'a [WorkflowCommandDefinition],
    chosen: Option<&str>,
) -> Vec<&'a WorkflowCommandDefinition> {
    defs.iter()
        .filter(|d| {
            d.metadata.workflow
                && (!d.metadata.deprecated || Some(d.id.as_str()) == chosen)
        })
        .collect()
}

/// The command-input value a JSON text area seeds from: a string passes
/// through, anything else pretty-prints at two spaces (React's
/// `JSON.stringify(value, null, 2)`).
pub fn seed_json_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string_pretty(other).unwrap_or_default(),
        None => String::new(),
    }
}

// ── the form's `S`-side state contract ────────────────────────────────────

/// Which of the form's listboxes is open (one at a time — React's
/// `ui/Select` keeps its own open state; closing on a choice is the form's
/// handler's job, reproduced by every `on_choose` here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowFormSelect {
    Connection,
    Database,
    VariableType(usize),
    StepType(usize),
    StepConnection(usize),
    StepDatabase(usize),
    StepCommand(usize),
}

/// Where the form's state lives in the host's `S` (function pointers, the
/// [`crate::views::workflow::execution_result_panel::StepUiSlots`] pattern
/// scaled to the form's interaction count). `Copy` without an `S` bound —
/// every field is a function pointer.
impl<S: 'static> Clone for WorkflowFormSlots<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: 'static> Copy for WorkflowFormSlots<S> {}

pub struct WorkflowFormSlots<S: 'static> {
    /// The controlled draft (React's `draft` prop).
    pub draft: fn(&S) -> &WorkflowDraft,
    /// Every edit (React's `onDraftChange`).
    pub draft_mut: fn(&mut S) -> &mut WorkflowDraft,
    /// The one open listbox.
    pub open: fn(&S) -> Option<WorkflowFormSelect>,
    pub set_open: fn(&mut S, Option<WorkflowFormSelect>),
    /// The command-input text areas' editing state (React's
    /// `CommandInputEditor` locals), keyed `step<i>.<field>` / `step<i>.raw`
    /// — the write side (an `entry().or_default()` while editing).
    pub input_edits: fn(&mut S) -> &mut BTreeMap<String, TextEdit>,
    /// The same map's read side: the render seeds each text area from the
    /// entry it finds (the build only holds `&S`).
    pub input_edits_read: fn(&S) -> &BTreeMap<String, TextEdit>,
    /// React's `commandsByConnection` / `loadingCommands` (the loaded
    /// command lists — the loader itself is a host-side seam).
    pub commands: fn(&S) -> &BTreeMap<String, Vec<WorkflowCommandDefinition>>,
    pub commands_loading: fn(&S) -> &BTreeSet<String>,
    /// React's `databasesByConn` / `loadingDatabases`.
    pub databases: fn(&S) -> &BTreeMap<String, Vec<String>>,
    pub databases_loading: fn(&S) -> &BTreeSet<String>,
    /// React's `onSave` / `onCancel` props.
    pub on_save: fn(&mut S),
    pub on_cancel: fn(&mut S),
}

/// The two React layout variants: `compact` fits the AI panel sidebar, `page`
/// is the full workflow window editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowFormVariant {
    Page,
    Compact,
}

/// The workflow form.
pub struct WorkflowForm<'a, S: 'static> {
    editing_id: Option<&'a str>,
    connections: &'a [WorkflowConnectionOption],
    variant: WorkflowFormVariant,
    show_action_bar: bool,
    slots: WorkflowFormSlots<S>,
}

impl<'a, S: 'static> WorkflowForm<'a, S> {
    /// The form over `connections` (React's `connections` prop; the draft
    /// itself is `S`-side via the slots).
    pub fn new(
        connections: &'a [WorkflowConnectionOption],
        slots: WorkflowFormSlots<S>,
    ) -> Self {
        Self {
            editing_id: None,
            connections,
            variant: WorkflowFormVariant::Page,
            show_action_bar: true,
            slots,
        }
    }

    /// React's `editingId` prop (disables the id field, titles the compact
    /// header).
    pub fn editing_id(mut self, id: Option<&'a str>) -> Self {
        self.editing_id = id;
        self
    }

    /// React's `variant` prop.
    pub fn variant(mut self, variant: WorkflowFormVariant) -> Self {
        self.variant = variant;
        self
    }

    /// React's `showActionBar` prop: `false` when the page-level shared bar
    /// (a sibling, T019's port) is used instead.
    pub fn show_action_bar(mut self, show: bool) -> Self {
        self.show_action_bar = show;
        self
    }

    /// Builds a standalone form with its own overlay layer.
    pub fn build(&self, s: &S, dark: bool) -> El {
        let mut layer = Vec::new();
        let form = self.build_into_layer(s, dark, &mut layer);
        rune_kit::root(form, layer)
    }

    /// Builds intrinsic-height form content inside an existing scroll viewport.
    /// Mount its anchored selects in the caller's layer outside that viewport.
    pub(super) fn build_into_layer(&self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let draft = (self.slots.draft)(s);
        let compact = self.variant == WorkflowFormVariant::Compact;
        let mut root = if compact {
            // `space-y-2 border border-edge rounded-md p-3 bg-surface w-full`
            col()
                .key("workflow-form-root")
                .w_full()
                .shrink()
                .gap(COMPACT_GAP)
                .border(1.0, k.c.border_base)
                .radius(k.radius.md)
                .padding(PAD_3)
                .fill(k.c.bg_base)
        } else {
            // `w-full max-w-3xl mx-auto px-6 pt-6 space-y-4`
            col()
                .key("workflow-form-root")
                .w_full()
                .max_w(PAGE_MAX_W)
                .gap(PAGE_GAP)
                .px(PAGE_PAD_X)
                .pt(PAGE_PAD_TOP)
                .self_center()
        };

        if compact {
            root = root.child(
                text(if self.editing_id.is_some() {
                    t("workflows.edit")
                } else {
                    t("workflows.create")
                })
                .key("workflow-form-compact-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
            );
        }

        // ── id / name (a two-column grid on the page variant) ──
        let id_label = if compact {
            t("workflows.form.id")
        } else {
            "ID"
        };
        let name_label = if compact {
            t("workflows.form.name")
        } else {
            t("workflows.name")
        };
        let slots = self.slots;
        let id_field = col()
            .gap(4.0)
            .grow(1.0)
            .min_w(0.0)
            .child(muted_label(k, id_label))
            .child(
                TextField::new("workflow-form-id", &draft.id)
                    .height(INPUT_H)
                    .disabled(self.editing_id.is_some())
                    .placeholder(if compact {
                        t("workflows.form.idPlaceholder")
                    } else {
                        ""
                    })
                    .on_edit(move |s: &mut S| &mut (slots.draft_mut)(s).id)
                    .build(dark),
            );
        let name_field = col()
            .gap(4.0)
            .grow(1.0)
            .min_w(0.0)
            .child(muted_label(k, name_label))
            .child(
                TextField::new("workflow-form-name", &draft.name)
                    .height(INPUT_H)
                    .placeholder(if compact {
                        t("workflows.form.namePlaceholder")
                    } else {
                        ""
                    })
                    .on_edit(move |s: &mut S| &mut (slots.draft_mut)(s).name)
                    .build(dark),
            );
        if compact {
            root = root.child(id_field).child(name_field);
        } else {
            // `grid grid-cols-2 gap-4`
            root = root.child(
                row()
                    .key("workflow-form-id-name")
                    .w_full()
                    .gap(PAGE_GAP)
                    .child(id_field)
                    .child(name_field),
            );
        }

        // ── description ──
        root = root.child(
            col().gap(4.0).child(muted_label(
                k,
                if compact {
                    t("workflows.form.description")
                } else {
                    t("workflows.description")
                },
            ))
            .child(
                TextField::new("workflow-form-description", &draft.description)
                    .height(INPUT_H)
                    .placeholder(if compact {
                        t("workflows.form.descriptionPlaceholder")
                    } else {
                        ""
                    })
                    .on_edit(move |s: &mut S| &mut (slots.draft_mut)(s).description)
                    .build(dark),
            ),
        );

        if !compact {
            root = root.child(self.page_connection_section(s, k, layer));
            root = root.child(self.schedule_box(s, k, dark));
        }

        root = root.child(self.variables_section(s, k, dark, layer));
        root = root.child(self.steps_section(s, k, dark, layer));

        if compact {
            // React's compact footer: `flex justify-end gap-2 pt-1`.
            root = root.child(self.compact_footer(s, k));
        } else if self.show_action_bar {
            // React's default: the shared bottom bar (T019's port) with the
            // form's own callbacks. The disabled rule is the compact
            // footer's (id/name/steps presence) — the lite page's use of
            // the bar passes its own.
            root = root.child(workflow_editor_action_bar::<S>(
                "workflow-form-save",
                compact_save_disabled(draft),
                self.slots.on_save,
                self.slots.on_cancel,
                dark,
            ));
        }

        root
    }

    // ── the page-only connection + database section ────────────────────────

    fn page_connection_section(&self, s: &S, k: &Kit, layer: &mut Vec<El>) -> El {
        let draft = (self.slots.draft)(s);
        let slots = self.slots;
        let mut section = col().gap(PAGE_GAP);

        // Workflow Connection select.
        let mut labels = vec!["No default connection".to_owned()];
        let mut values = vec![String::new()];
        let mut current = None;
        for conn in self.connections {
            labels.push(conn.name.clone());
            values.push(conn.id.clone());
            if draft.connection.as_deref() == Some(conn.id.as_str()) {
                current = Some(conn.name.clone());
            }
        }
        section = section.child(
            col().gap(4.0)
                .child(muted_label(k, "Workflow Connection"))
                .child(
                    Select::new("workflow-form-connection", &labels)
                        .values(&values)
                        .current(current.as_deref())
                        .label("Workflow Connection")
                        .open((slots.open)(s) == Some(WorkflowFormSelect::Connection))
                        .on_toggle(move |s: &mut S| {
                            let next = toggle(s, slots, WorkflowFormSelect::Connection);
                            (slots.set_open)(s, next)
                        })
                        .on_close(move |s: &mut S| (slots.set_open)(s, None))
                        .on_choose(move |s: &mut S, value: &str, _tx| {
                            let draft = (slots.draft_mut)(s);
                            draft.connection = (!value.is_empty()).then(|| value.to_owned());
                            (slots.set_open)(s, None);
                        })
                        .build(k, layer)
                        .h(INPUT_H),
                )
                .child(
                    text(
                        "Data-operation steps inherit this connection unless they override it."
                            .to_owned(),
                    )
                    .key("workflow-form-connection-hint")
                    .font(hint_style(k))
                    .color(k.c.text_muted),
                ),
        );

        // The workflow-level database picker (multi-db connections only).
        if connection_allows_multi_db(
            self.connections
                .iter()
                .find(|c| Some(c.id.as_str()) == draft.connection.as_deref()),
        ) {
            let conn_id = draft.connection.clone().unwrap_or_default();
            let databases = (slots.databases)(s).get(&conn_id).cloned().unwrap_or_default();
            let loading = (slots.databases_loading)(s).contains(&conn_id);
            let mut db_labels = vec![t("workflows.form.databaseRequired").to_owned()];
            let mut db_values = vec![String::new()];
            let mut db_current = None;
            for db in &databases {
                db_labels.push(db.clone());
                db_values.push(db.clone());
                if draft.database.as_deref() == Some(db.as_str()) {
                    db_current = Some(db.clone());
                }
            }
            let mut db_col = col().gap(4.0).child(muted_label(k, t("workflows.form.database")));
            if !databases.is_empty() {
                db_col = db_col.child(
                    Select::new("workflow-form-database", &db_labels)
                        .values(&db_values)
                        .current(db_current.as_deref())
                        .label(t("workflows.form.database"))
                        .open((slots.open)(s) == Some(WorkflowFormSelect::Database))
                        .on_toggle(move |s: &mut S| {
                            let next = toggle(s, slots, WorkflowFormSelect::Database);
                            (slots.set_open)(s, next)
                        })
                        .on_close(move |s: &mut S| (slots.set_open)(s, None))
                        .on_choose(move |s: &mut S, value: &str, _tx| {
                            let draft = (slots.draft_mut)(s);
                            draft.database = (!value.is_empty()).then(|| value.to_owned());
                            (slots.set_open)(s, None);
                        })
                        .build(k, layer)
                        .h(INPUT_H),
                );
            } else if loading {
                db_col = db_col.child(
                    text(t("workflows.form.databaseLoading").to_owned())
                        .key("workflow-form-database-loading")
                        .font(hint_style(k))
                        .color(k.c.text_muted),
                );
            }
            db_col = db_col.child(
                text(
                    "Data-operation steps inherit this database unless they override it."
                        .to_owned(),
                )
                .key("workflow-form-database-hint")
                .font(hint_style(k))
                .color(k.c.text_muted),
            );
            section = section.child(db_col);
        }
        section
    }

    // ── the page-only schedule box ────────────────────────────────────────

    fn schedule_box(&self, s: &S, k: &Kit, dark: bool) -> El {
        let draft = (self.slots.draft)(s);
        let slots = self.slots;
        // `rounded-lg border p-3 space-y-2`
        let mut schedule = col()
            .key("workflow-form-schedule")
            .w_full()
            .shrink()
            .gap(COMPACT_GAP)
            .border(1.0, k.c.border_base)
            .radius(k.radius.lg)
            .padding(PAD_3)
            .child(
                Checkbox::new("workflow-form-schedule-enabled", draft.schedule_enabled)
                    .label(t("workflows.schedule.enabled").to_owned())
                    .on_toggle(move |s: &mut S| {
                        let draft = (slots.draft_mut)(s);
                        draft.schedule_enabled = !draft.schedule_enabled;
                    })
                    .build(k),
            );
        if draft.schedule_enabled {
            schedule = schedule.child(
                col().gap(4.0)
                    .child(muted_label(k, t("workflows.schedule.interval")))
                    .child(
                        TextField::new("workflow-form-schedule-interval", &draft.schedule_interval)
                            .height(INPUT_H)
                            .on_edit(move |s: &mut S| {
                                &mut (slots.draft_mut)(s).schedule_interval
                            })
                            .build(dark),
                    )
                    .child(
                        text(t("workflows.schedule.hint").to_owned())
                            .key("workflow-form-schedule-hint")
                            .font(hint_style(k))
                            .color(k.c.text_muted),
                    ),
            );
        }
        schedule
    }

    // ── variables ────────────────────────────────────────────────────────

    fn variables_section(&self, s: &S, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let draft = (self.slots.draft)(s);
        let slots = self.slots;
        // Header: `flex items-center justify-between mb-2` with the accent
        // `+ 添加变量` text button.
        let header = row()
            .key("workflow-form-variables-head")
            .w_full()
            .shrink()
            .center_y()
            .justify_between()
            .child(
                text(t("workflows.form.variables").to_owned())
                    .key("workflow-form-variables-label")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted),
            )
            .child(accent_text_button(
                k,
                "workflow-form-add-variable",
                &format!("+ {}", t("workflows.form.addVariable")),
                move |s: &mut S| {
                    (slots.draft_mut)(s)
                        .variables
                        .push(WorkflowVariableDraft::empty());
                },
            ));
        let mut section = col().gap(8.0).child(header);
        for (i, var) in draft.variables.iter().enumerate() {
            section = section.child(variable_row(self.slots, s, k, dark, i, var, layer));
        }
        section
    }

    // ── steps ─────────────────────────────────────────────────────────────

    fn steps_section(&self, s: &S, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let draft = (self.slots.draft)(s);
        let slots = self.slots;
        let header = row()
            .key("workflow-form-steps-head")
            .w_full()
            .shrink()
            .center_y()
            .justify_between()
            .child(
                text(t("workflows.steps").to_owned())
                    .key("workflow-form-steps-label")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted),
            )
            .child(accent_text_button(
                k,
                "workflow-form-add-step",
                &format!("+ {}", t("workflows.addStep")),
                move |s: &mut S| {
                    // React adds a command step named step{len+1}.
                    let draft = (slots.draft_mut)(s);
                    let id = format!("step{}", draft.steps.len() + 1);
                    draft.steps.push(WorkflowStepDraft::command(&id));
                },
            ));
        let section = col().gap(8.0).child(header);
        // The cards carry React's `mb-3` spacing between them.
        let mut cards = col().key("workflow-form-steps").w_full().shrink().gap(12.0);
        for (i, _step) in draft.steps.iter().enumerate() {
            cards = cards.child(step_card(self, s, k, dark, i, layer));
        }
        section.child(cards)
    }

    // ── the compact footer (React's inline Save/Cancel) ────────────────────

    fn compact_footer(&self, s: &S, k: &Kit) -> El {
        let draft = (self.slots.draft)(s);
        let slots = self.slots;
        // `flex justify-end gap-2 pt-1` with a small primary + secondary.
        row()
            .key("workflow-form-footer")
            .w_full()
            .shrink()
            .justify_end()
            .gap(8.0)
            .pt(4.0)
            .child(
                Button::new(t("common.save").to_owned())
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Small)
                    .disabled(compact_save_disabled(draft))
                    .on_press(move |s: &mut S| (slots.on_save)(s))
                    .build(k)
                    .key("workflow-form-save"),
            )
            .child(
                Button::new(t("common.cancel").to_owned())
                    .variant(ButtonVariant::Outline)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| (slots.on_cancel)(s))
                    .build(k)
                    .key("workflow-form-cancel"),
            )
    }
}

// ── row builders (free functions to keep `impl` thin) ─────────────────────

/// React's label rows: `text-xs text-fg-muted block mb-1`.
fn muted_label(k: &Kit, label: &str) -> El {
    text(label.to_owned())
        .font(label_style(k))
        .color(k.c.text_muted)
}

/// A two-way toggle for a listbox: same select → close, other → it.
fn toggle<S: 'static>(
    s: &mut S,
    slots: WorkflowFormSlots<S>,
    which: WorkflowFormSelect,
) -> Option<WorkflowFormSelect> {
    if (slots.open)(s) == Some(which) {
        None
    } else {
        Some(which)
    }
}

/// React's `text-accent text-xs hover:underline` button (an add-variable /
/// add-step link): a small named button row in the accent ink.
fn accent_text_button<S: 'static>(
    k: &Kit,
    key: &'static str,
    label: &str,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .focus_ring(2.0, 2.0, k.c.border_focus)
        .hover(k.c.hover)
        .px(2.0)
        .on_press(on_press)
        .child(
            text(label.to_owned())
                .font(label_style(k))
                .color(k.c.text_accent),
        )
}

/// One variable row: `mb-2 flex items-center gap-2` with name (25%), type
/// select (20%), description (35%), required checkbox and the trash button.
fn variable_row<S: 'static>(
    slots: WorkflowFormSlots<S>,
    s: &S,
    k: &Kit,
    dark: bool,
    i: usize,
    var: &WorkflowVariableDraft,
    layer: &mut Vec<El>,
) -> El {
    let key = format!("workflow-form-variable-{i}");
    let mut var_row = row()
        .key(key.clone())
        .w_full()
        .shrink()
        .gap(8.0)
        .center_y();

    // name input, w-[25%] → a 5/16 grow share of the row.
    var_row = var_row.child(
        TextField::new(format!("{key}-name"), &var.name)
            .height(INPUT_H)
            .placeholder(t("workflows.form.varName"))
            .on_edit(move |s: &mut S| {
                match (slots.draft_mut)(s).variables.get_mut(i) {
                    Some(var) => &mut var.name,
                    None => panic!("variable row {i} edits a draft that rebuilt"),
                }
            })
            .build(dark)
            .grow(5.0),
    );

    // type select, w-[20%]: string / number / connection.
    let type_labels: Vec<String> = vec![
        "string".to_owned(),
        "number".to_owned(),
        t("workflows.form.varTypeConnection").to_owned(),
    ];
    let type_values: Vec<&str> = vec!["string", "number", "connection"];
    let current = match var.var_type.as_str() {
        "string" => Some(type_labels[0].clone()),
        "number" => Some(type_labels[1].clone()),
        "connection" => Some(type_labels[2].clone()),
        _ => None,
    };
    var_row = var_row.child(
        Select::new(format!("{key}-type"), &type_labels)
            .values(&type_values)
            .current(current.as_deref())
            .label(t("workflows.form.varType"))
            .open((slots.open)(s) == Some(WorkflowFormSelect::VariableType(i)))
            .on_toggle(move |s: &mut S| {
                let next = toggle(s, slots, WorkflowFormSelect::VariableType(i));
                (slots.set_open)(s, next)
            })
            .on_close(move |s: &mut S| (slots.set_open)(s, None))
            .on_choose(move |s: &mut S, value: &str, _tx| {
                let draft = (slots.draft_mut)(s);
                if let Some(var) = draft.variables.get_mut(i) {
                    var.var_type = value.to_owned();
                }
                (slots.set_open)(s, None);
            })
            .build(k, layer)
            .grow(4.0),
    );

    // description input, w-[35%].
    var_row = var_row.child(
        TextField::new(format!("{key}-description"), &var.description)
            .height(INPUT_H)
            .placeholder(t("workflows.form.varDesc"))
            .on_edit(move |s: &mut S| {
                match (slots.draft_mut)(s).variables.get_mut(i) {
                    Some(var) => &mut var.description,
                    None => panic!("variable row {i} edits a draft that rebuilt"),
                }
            })
            .build(dark)
            .grow(7.0),
    );

    // required checkbox, whitespace-nowrap.
    var_row = var_row.child(
        Checkbox::new(format!("{key}-required"), var.required)
            .label(t("workflows.form.varRequired").to_owned())
            .on_toggle(move |s: &mut S| {
                let draft = (slots.draft_mut)(s);
                if let Some(var) = draft.variables.get_mut(i) {
                    var.required = !var.required;
                }
            })
            .build(k),
    );

    // remove: lucide Trash2 (the registry's ICON_TRASH) at h-3.5 (14pt).
    var_row = var_row.child(
        trash_button(
            k,
            format!("{key}-remove"),
            t("common.delete"),
            move |s: &mut S| {
                let draft = (slots.draft_mut)(s);
                if i < draft.variables.len() {
                    draft.variables.remove(i);
                }
            },
        ),
    );
    var_row
}

/// One step card: `mb-3 rounded-lg border p-3 space-y-2`.
fn step_card<S: 'static>(
    form: &WorkflowForm<'_, S>,
    s: &S,
    k: &Kit,
    dark: bool,
    i: usize,
    layer: &mut Vec<El>,
) -> El {
    let slots = form.slots;
    let connections = form.connections;
    let draft = (slots.draft)(s);
    let Some(step) = draft.steps.get(i) else {
        return col().key(format!("workflow-form-step-{i}"));
    };
    let key = format!("workflow-form-step-{i}");
    let mut card = col()
        .key(key.clone())
        .w_full()
        .shrink()
        .gap(COMPACT_GAP)
        .border(1.0, k.c.border_base)
        .radius(k.radius.lg)
        .padding(PAD_3);

    // ── header row: `flex items-center gap-2` ──
    let mut head = row()
        .key(format!("{key}-head"))
        .w_full()
        .shrink()
        .gap(8.0)
        .center_y();

    // step id input, `h-7 w-28`.
    head = head.child(
        TextField::new(format!("{key}-id"), &step.id)
            .height(28.0)
            .width(112.0)
            .placeholder("step_id")
            .on_edit(move |s: &mut S| {
                match (slots.draft_mut)(s).steps.get_mut(i) {
                    Some(step) => &mut step.id,
                    None => panic!("step row {i} edits a draft that rebuilt"),
                }
            })
            .build(dark),
    );

    // type select, `!h-7 !text-xs w-32` (the kit trigger is 28pt already).
    let type_labels: Vec<&str> = vec![
        WorkflowStepKind::Query.label(),
        WorkflowStepKind::Command.label(),
        WorkflowStepKind::Ai.label(),
    ];
    let type_values: Vec<&str> = vec![
        WorkflowStepKind::Query.as_str(),
        WorkflowStepKind::Command.as_str(),
        WorkflowStepKind::Ai.as_str(),
    ];
    head = head.child(
        Select::new(format!("{key}-type"), &type_labels)
            .values(&type_values)
            .current(Some(step.kind.label()))
            .label("Type")
            .open((slots.open)(s) == Some(WorkflowFormSelect::StepType(i)))
            .on_toggle(move |s: &mut S| {
                let next = toggle(s, slots, WorkflowFormSelect::StepType(i));
                (slots.set_open)(s, next)
            })
            .on_close(move |s: &mut S| (slots.set_open)(s, None))
            .on_choose(move |s: &mut S, value: &str, _tx| {
                let draft = (slots.draft_mut)(s);
                if let Some(step) = draft.steps.get_mut(i) {
                    let next = match value {
                        "command" => WorkflowStepKind::Command,
                        "ai" => WorkflowStepKind::Ai,
                        _ => WorkflowStepKind::Query,
                    };
                    // React's patch: switching to command keeps the (already
                    // cleared) command and defaults the input to `{}`;
                    // switching away clears both.
                    if next == WorkflowStepKind::Command {
                        step.kind = WorkflowStepKind::Command;
                    } else {
                        step.kind = next;
                        step.command = None;
                        step.input.clear();
                    }
                }
                clear_step_input_edits(s, slots, i);
                (slots.set_open)(s, None);
            })
            .build(k, layer)
            .w(128.0),
    );

    // connection + database selects (query/command steps with connections).
    if matches!(step.kind, WorkflowStepKind::Query | WorkflowStepKind::Command)
        && !connections.is_empty()
    {
        let conn_id = effective_connection(draft, step);
        let needs_database =
            connection_allows_multi_db(connections.iter().find(|c| c.id == conn_id));
        let databases = (slots.databases)(s).get(&conn_id).cloned().unwrap_or_default();
        let loading_db = (slots.databases_loading)(s).contains(&conn_id);

        // connection select, `flex-1`.
        let inherited = draft.connection.is_some();
        let mut labels = vec![
            if inherited { "Inherited" } else { "Select connection" }.to_owned(),
        ];
        let mut values = vec![String::new()];
        let mut current = None;
        for conn in connections {
            labels.push(conn.name.clone());
            values.push(conn.id.clone());
            if step.connection.as_deref() == Some(conn.id.as_str()) {
                current = Some(conn.name.clone());
            }
        }
        head = head.child(
            Select::new(format!("{key}-connection"), &labels)
                .values(&values)
                .current(current.as_deref())
                .label("Connection")
                .open((slots.open)(s) == Some(WorkflowFormSelect::StepConnection(i)))
                .on_toggle(move |s: &mut S| {
                    let next = toggle(s, slots, WorkflowFormSelect::StepConnection(i));
                    (slots.set_open)(s, next)
                })
                .on_close(move |s: &mut S| (slots.set_open)(s, None))
                .on_choose(move |s: &mut S, value: &str, _tx| {
                    // React: choosing a connection clears the chosen command
                    // (the command list is per connection) and resets the
                    // input editor's text state.
                    let draft = (slots.draft_mut)(s);
                    if let Some(step) = draft.steps.get_mut(i) {
                        step.connection = (!value.is_empty()).then(|| value.to_owned());
                        if !value.is_empty() {
                            step.command = None;
                            step.input.clear();
                        }
                    }
                    clear_step_input_edits(s, slots, i);
                    (slots.set_open)(s, None);
                })
                .build(k, layer)
                .grow(1.0),
        );

        // database select, `flex-1` (multi-db connections only).
        if needs_database {
            if !databases.is_empty() {
                let mut db_labels = vec![if draft.database.is_some() {
                    t("workflows.form.databaseInherit")
                } else {
                    t("workflows.form.databaseRequired")
                }
                .to_owned()];
                let mut db_values = vec![String::new()];
                let mut db_current = None;
                for db in &databases {
                    db_labels.push(db.clone());
                    db_values.push(db.clone());
                    if step.database.as_deref() == Some(db.as_str()) {
                        db_current = Some(db.clone());
                    }
                }
                head = head.child(
                    Select::new(format!("{key}-database"), &db_labels)
                        .values(&db_values)
                        .current(db_current.as_deref())
                        .label(t("workflows.form.database"))
                        .open((slots.open)(s) == Some(WorkflowFormSelect::StepDatabase(i)))
                        .on_toggle(move |s: &mut S| {
                            let next = toggle(s, slots, WorkflowFormSelect::StepDatabase(i));
                            (slots.set_open)(s, next)
                        })
                        .on_close(move |s: &mut S| (slots.set_open)(s, None))
                        .on_choose(move |s: &mut S, value: &str, _tx| {
                            let draft = (slots.draft_mut)(s);
                            if let Some(step) = draft.steps.get_mut(i) {
                                step.database = (!value.is_empty()).then(|| value.to_owned());
                            }
                            (slots.set_open)(s, None);
                        })
                        .build(k, layer)
                        .grow(1.0),
                );
            } else if loading_db {
                head = head.child(
                    text(t("workflows.form.databaseLoading").to_owned())
                        .key(format!("{key}-database-loading"))
                        .font(hint_style(k))
                        .color(k.c.text_muted)
                        .grow(1.0),
                );
            }
        }
    }

    // remove button (only with more than one step).
    if draft.steps.len() > 1 {
        head = head.child(
            trash_button(
                k,
                format!("{key}-remove"),
                t("workflows.form.removeStep"),
                move |s: &mut S| {
                    let draft = (slots.draft_mut)(s);
                    if i < draft.steps.len() {
                        draft.steps.remove(i);
                    }
                },
            ),
        );
    }
    card = card.child(head);

    // ── the command body ──
    if step.kind == WorkflowStepKind::Command {
        card = card
            .child(command_select(slots, s, k, i, layer))
            .child(command_input_editor(slots, s, k, dark, i));
    }

    // ── the query body: the h-36 SQL frame (React's `workflow-sql-editor`) ──
    if step.kind == WorkflowStepKind::Query {
        card = card.child(sql_frame(
            k,
            format!("{key}-sql"),
            &step.sql,
            "SELECT ...",
        ));
    }

    // ── the AI body: the prompt textarea (rows=4) ──
    if step.kind == WorkflowStepKind::Ai {
        card = card.child(
            TextArea::new(&step.prompt)
                .placeholder("AI prompt...")
                .rows(4)
                .on_edit(move |s: &mut S| {
                    match (slots.draft_mut)(s).steps.get_mut(i) {
                        Some(step) => &mut step.prompt,
                        None => panic!("step row {i} edits a draft that rebuilt"),
                    }
                })
                .build(k)
                .key(format!("{key}-prompt")),
        );
    }
    card
}

/// The command select with its state-dependent placeholder and the chosen
/// command's description line.
fn command_select<S: 'static>(
    slots: WorkflowFormSlots<S>,
    s: &S,
    k: &Kit,
    i: usize,
    layer: &mut Vec<El>,
) -> El {
    let draft = (slots.draft)(s);
    let Some(step) = draft.steps.get(i) else {
        return col();
    };
    let key = format!("workflow-form-step-{i}-command");
    let conn_id = effective_connection(draft, step);
    let loading = (slots.commands_loading)(s).contains(&conn_id);
    let all_defs = (slots.commands)(s).get(&conn_id).cloned().unwrap_or_default();
    let definitions = workflow_definitions(&all_defs, step.command.as_deref());
    let definition = definitions
        .iter()
        .find(|d| Some(d.id.as_str()) == step.command.as_deref())
        .map(|d| *d);

    let mut labels = vec![if loading {
        "Loading commands…".to_owned()
    } else if conn_id.is_empty() {
        "Select a connection first".to_owned()
    } else {
        "Select command".to_owned()
    }];
    let mut values = vec![String::new()];
    let mut current = None;
    for def in &definitions {
        labels.push(command_option_label(def));
        values.push(def.id.clone());
        if step.command.as_deref() == Some(def.id.as_str()) {
            current = Some(command_option_label(def));
        }
    }
    let mut select_col = col().gap(4.0).child(muted_label(k, "Command"));
    select_col = select_col.child(
        Select::new(key.clone(), &labels)
            .values(&values)
            .current(current.as_deref())
            .label("Command")
            .open((slots.open)(s) == Some(WorkflowFormSelect::StepCommand(i)))
            .on_toggle(move |s: &mut S| {
                let next = toggle(s, slots, WorkflowFormSelect::StepCommand(i));
                (slots.set_open)(s, next)
            })
            .on_close(move |s: &mut S| (slots.set_open)(s, None))
            .on_choose(move |s: &mut S, value: &str, _tx| {
                // React: `input: next ? defaultInput(next) : {}` — and the
                // input editor's text state resets with the definition.
                let next = {
                    let draft = (slots.draft)(s);
                    let conn = match draft.steps.get(i) {
                        Some(step) => effective_connection(draft, step),
                        None => String::new(),
                    };
                    let chosen = draft.steps.get(i).and_then(|st| st.command.clone());
                    let all = (slots.commands)(s)
                        .get(&conn)
                        .cloned()
                        .unwrap_or_default();
                    workflow_definitions(&all, chosen.as_deref())
                        .into_iter()
                        .find(|d| d.id == value)
                        .cloned()
                };
                let draft = (slots.draft_mut)(s);
                if let Some(step) = draft.steps.get_mut(i) {
                    step.command = (!value.is_empty()).then(|| value.to_owned());
                    step.input = next.as_ref().map(|d| default_input(Some(d))).unwrap_or_default();
                }
                clear_step_input_edits(s, slots, i);
                (slots.set_open)(s, None);
            })
            .build(k, layer)
            .h(INPUT_H),
    );
    if let Some(definition) = &definition {
        // React renders the description line only when the description is
        // present, with the risk suffix inside it.
        if let Some(description) = &definition.description {
            let risk = definition
                .metadata
                .risk
                .as_deref()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default();
            select_col = select_col.child(
                text(format!("{description}{risk}"))
                    .key(format!("{key}-description"))
                    .font(hint_style(k))
                    .color(k.c.text_muted),
            );
        }
    }
    select_col
}

/// The command-input editor: schema-driven fields (boolean → checkbox,
/// `sql` → the SQL frame, array/object → JSON text area, number → numeric
/// coercion, string → text field) with the raw-JSON fallback when the schema
/// has no properties.
fn command_input_editor<S: 'static>(
    slots: WorkflowFormSlots<S>,
    s: &S,
    k: &Kit,
    dark: bool,
    i: usize,
) -> El {
    let draft = (slots.draft)(s);
    let Some(step) = draft.steps.get(i) else {
        return col();
    };
    let key = format!("workflow-form-step-{i}-input");
    let conn_id = effective_connection(draft, step);
    let all_defs = (slots.commands)(s).get(&conn_id).cloned().unwrap_or_default();
    let definition = workflow_definitions(&all_defs, step.command.as_deref())
        .into_iter()
        .find(|d| Some(d.id.as_str()) == step.command.as_deref());

    if definition.is_none() {
        // React's no-definition hint line.
        return col().key(key).child(
            text("Select a command to configure its input.".to_owned())
                .font(label_style(k))
                .color(k.c.text_muted),
        );
    }

    let fields = schema_properties(definition);
    let mut editor = col().key(key.clone()).w_full().shrink().gap(8.0);
    for field in &fields {
        let field_name = field.name.clone();
        let fkey = format!("{key}-{field_name}");
        let label = field
            .schema
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| field_name.clone());
        let required_mark = if field.required { " *" } else { "" };
        let field_type = field.schema.get("type").and_then(Value::as_str);

        if field_type == Some("boolean") {
            editor = editor.child(
                Checkbox::new(
                    fkey,
                    step.input.get(&field_name).and_then(Value::as_bool).unwrap_or(false),
                )
                .label(format!("{label}{required_mark}"))
                .on_toggle(move |s: &mut S| {
                    let draft = (slots.draft_mut)(s);
                    if let Some(step) = draft.steps.get_mut(i) {
                        let next = !step
                            .input
                            .get(&field_name)
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        step.input.insert(field_name.clone(), Value::Bool(next));
                    }
                })
                .build(k),
            );
            continue;
        }

        if field_name == "sql" && (field_type == Some("string") || field_type.is_none()) {
            let sql = step
                .input
                .get(&field_name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            editor = editor.child(
                col().gap(4.0)
                    .child(muted_label(k, &format!("{label}{required_mark}")))
                    // React's `command-sql-editor` testid names this frame.
                    .child(sql_frame(k, format!("{fkey}-editor"), &sql, "SELECT ...")),
            );
            continue;
        }

        if matches!(field_type, Some("array") | Some("object")) {
            // JSON text area: the seed is the current value's serialization;
            // a parse writes back into the draft (invalid text keeps editing,
            // see the module doc).
            let entry_key = format!("step{i}.{field_name}");
            let seed = seed_json_text(step.input.get(&field_name));
            let entry = (slots.input_edits_read)(s)
                .get(&entry_key)
                .cloned()
                .unwrap_or_else(|| TextEdit::new(seed));
            let edit_key = entry_key.clone();
            editor = editor.child(
                col().gap(4.0)
                    .child(muted_label(k, &format!("{label}{required_mark}")))
                    .child(
                        TextArea::new(&entry)
                            .on_edit(move |s: &mut S| {
                                input_edit_at(s, slots, &edit_key)
                            })
                            .on_change(move |s: &mut S, _tx| {
                                let text = (slots.input_edits_read)(s)
                                    .get(&entry_key)
                                    .map(|e| e.text.clone())
                                    .unwrap_or_default();
                                // React inserts whatever parses.
                                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                                    let draft = (slots.draft_mut)(s);
                                    if let Some(step) = draft.steps.get_mut(i) {
                                        step.input.insert(field_name.clone(), parsed);
                                    }
                                }
                            })
                            .build(k)
                            .key(fkey),
                    ),
            );
            continue;
        }

        // number / integer / string / unspecified scalar fields.
        let is_number = matches!(field_type, Some("number") | Some("integer"));
        let entry_key = format!("step{i}.{field_name}");
        let seed = match step.input.get(&field_name) {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(other) => other.to_string(),
            None => String::new(),
        };
        let entry = (slots.input_edits_read)(s)
            .get(&entry_key)
            .cloned()
            .unwrap_or_else(|| TextEdit::new(seed));
        let placeholder = field
            .schema
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let edit_key = entry_key.clone();
        let mut field_editor = TextField::new(fkey.clone(), &entry)
            .height(INPUT_H)
            .placeholder(placeholder.clone())
            .on_edit(move |s: &mut S| input_edit_at(s, slots, &edit_key));
        if is_number {
            field_editor = field_editor.on_change(move |s: &mut S, _tx| {
                // React coerces on change: `Number(value)` when numeric and
                // non-empty, else the raw string.
                let text = (slots.input_edits_read)(s)
                    .get(&entry_key)
                    .map(|e| e.text.clone())
                    .unwrap_or_default();
                let value = coerce_number(&text);
                let draft = (slots.draft_mut)(s);
                if let Some(step) = draft.steps.get_mut(i) {
                    step.input.insert(field_name.clone(), value);
                }
            });
        } else {
            field_editor = field_editor.on_change(move |s: &mut S, _tx| {
                let text = (slots.input_edits_read)(s)
                    .get(&entry_key)
                    .map(|e| e.text.clone())
                    .unwrap_or_default();
                let draft = (slots.draft_mut)(s);
                if let Some(step) = draft.steps.get_mut(i) {
                    step.input.insert(field_name.clone(), Value::String(text));
                }
            });
        }
        editor = editor.child(
            col().gap(4.0)
                .child(muted_label(k, &format!("{label}{required_mark}")))
                .child(field_editor.build(dark)),
        );
    }

    if fields.is_empty() {
        // The raw JSON fallback over the whole input object.
        let entry_key = format!("step{i}.raw");
        let seed = serde_json::to_string_pretty(&Value::Object(step.input.clone()))
            .unwrap_or_default();
        let entry = (slots.input_edits_read)(s)
            .get(&entry_key)
            .cloned()
            .unwrap_or_else(|| TextEdit::new(seed));
        let edit_key = entry_key.clone();
        editor = editor.child(
            TextArea::new(&entry)
                .label("Command input JSON")
                .on_edit(move |s: &mut S| input_edit_at(s, slots, &edit_key))
                .on_change(move |s: &mut S, _tx| {
                    let text = (slots.input_edits_read)(s)
                        .get(&entry_key)
                        .map(|e| e.text.clone())
                        .unwrap_or_default();
                    // React accepts objects only.
                    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                        let draft = (slots.draft_mut)(s);
                        if let Some(step) = draft.steps.get_mut(i) {
                            step.input = map;
                        }
                    }
                })
                .build(k)
                .key(format!("{key}-raw")),
        );
    }
    editor
}

/// React's numeric coercion `Number(value) || raw` on the command-input
/// number fields: a parseable float becomes its number, anything else keeps
/// the raw string (React's `NaN` poison is not reproduced — see the module
/// doc's JSON notes; the save path re-coerces).
fn coerce_number(text: &str) -> Value {
    if text.is_empty() {
        return Value::String(text.to_owned());
    }
    match text.parse::<f64>() {
        Ok(n) => match serde_json::Number::from_f64(n) {
            Some(number) => Value::Number(number),
            None => Value::String(text.to_owned()),
        },
        Err(_) => Value::String(text.to_owned()),
    }
}

/// The command-input text area at `key`, created empty on first edit (the
/// build seeds the model from the draft's serialization instead).
fn input_edit_at<'a, S: 'static>(
    s: &'a mut S,
    slots: WorkflowFormSlots<S>,
    key: &str,
) -> &'a mut TextEdit {
    let map = (slots.input_edits)(s);
    match map.entry(key.to_owned()) {
        std::collections::btree_map::Entry::Occupied(occupied) => occupied.into_mut(),
        std::collections::btree_map::Entry::Vacant(vacant) => {
            vacant.insert(TextEdit::new(String::new()))
        }
    }
}

/// Drops the step's input-editor text state — the port's equivalent of the
/// `useEffect([definition?.id])` reset in React's `CommandInputEditor`,
/// called wherever React changes the definition the editor shows (command
/// choice, connection choice, type switch, step removal).
fn clear_step_input_edits<S: 'static>(s: &mut S, slots: WorkflowFormSlots<S>, i: usize) {
    let prefix = format!("step{i}.");
    let keys: Vec<String> = (slots.input_edits)(s)
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    for key in keys {
        (slots.input_edits)(s).remove(&key);
    }
}

/// The `h-36` SQL frame: React's bordered box around the SQL editor
/// (testids `workflow-sql-editor` / `command-sql-editor`). The editable
/// engine lands with the SqlEditor port (T243); this renders the frame with
/// the draft's SQL as mono text plus the placeholder line when empty.
fn sql_frame(k: &Kit, key: String, sql: &str, placeholder: &str) -> El {
    let mut frame = col()
        .key(key)
        .w_full()
        .h(SQL_FRAME_H)
        .radius(k.radius.sm)
        .border(1.0, k.c.border_base)
        .padding(8.0);
    if sql.trim().is_empty() {
        frame = frame.child(
            text(placeholder.to_owned())
                .font(k.ty.mono)
                .color(k.c.text_faint),
        );
    } else {
        for line in sql.lines() {
            frame = frame.child(
                text(line.to_owned())
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }
    }
    frame
}

/// The lucide Trash2 remove button (`h-3.5 w-3.5` = 14pt icon): a small
/// icon-only button named for the screen reader (React leaves it unnamed —
/// the port names it with the existing remove/delete keys).
fn trash_button<S: 'static>(
    k: &Kit,
    key: String,
    label: &str,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .focus_ring(2.0, 2.0, k.c.border_focus)
        .hover(k.c.hover)
        .padding(4.0)
        .center_y()
        .on_press(on_press)
        .child(icon(&ICON_TRASH, 14.0).color(k.c.danger.fg))
}
