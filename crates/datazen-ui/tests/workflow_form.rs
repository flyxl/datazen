//! T070: the `WorkflowForm` port — both layout variants mounted over the
//! real `AppState` wiring (`workflow_form_slots()`), the step/variable/
//! command interaction paths, and React's exported pure rules (the
//! multi-db registry slice, the auto-fill, `defaultInput`, the compact
//! Save gate, `Number(value) || 3600`).
//!
//! Every observation is the a11y tree (`rt.elements()`: rect / role /
//! label / text) plus the host state a real view would own — never the
//! form's own reports. Geometry pins: the page variant's `max-w-3xl`
//! centered column with `px-6 pt-6` and the `grid-cols-2 gap-4` header,
//! the compact box's `p-3`, the `h-36` SQL frames and the `gap-2` footer.

use std::collections::BTreeMap;

use rune::{Cx, El, Env, Runtime, TextEdit};
use serde_json::{json, Map, Value};

use datazen_ui::i18n::t;
use datazen_ui::views::{
    auto_fill_single_databases, command_option_label, compact_save_disabled,
    connection_allows_multi_db, db_registry_meta, default_input, effective_connection,
    multi_db_connections, seed_json_text, workflow_definitions, WorkflowCommandDefinition,
    WorkflowCommandMetadata, WorkflowConnectionOption, WorkflowDraft, WorkflowForm,
    WorkflowFormSelect, WorkflowFormVariant, WorkflowStepDraft, WorkflowStepKind,
};
use datazen_ui::AppState;

// ---- harness ----------------------------------------------------------------

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    // An element playing its exit transition is still listed for tools, but
    // it is not mounted (the repo-wide `!e.exiting` convention).
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// Focus the field at `key` and paste `text` into it, the way a real editor
/// enters text (the kit's change handlers fire on the committed edit).
fn type_into(rt: &mut Runtime<AppState>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn page_view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    WorkflowForm::new(&state.workflow_form_connections, state.workflow_form_slots())
        .variant(WorkflowFormVariant::Page)
        .build(state, true)
}

fn compact_view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    WorkflowForm::new(&state.workflow_form_connections, state.workflow_form_slots())
        .variant(WorkflowFormVariant::Compact)
        .build(state, true)
}

fn compact_editing_view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    WorkflowForm::new(&state.workflow_form_connections, state.workflow_form_slots())
        .variant(WorkflowFormVariant::Compact)
        .editing_id(Some(state.workflow_draft.id.text.as_str()))
        .build(state, true)
}

/// The page form at the DoD window's width with headroom for the whole
/// column (the `max-w-3xl` pin reads the 960pt window).
fn page_runtime(state: AppState) -> Runtime<AppState> {
    let mut rt = Runtime::new(state, page_view, (960.0, 1400.0), env());
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn compact_runtime(state: AppState) -> Runtime<AppState> {
    let mut rt = Runtime::new(state, compact_view, (480.0, 720.0), env());
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

// ---- fixtures ----------------------------------------------------------------

fn pg_connection() -> WorkflowConnectionOption {
    WorkflowConnectionOption {
        id: "c1".to_owned(),
        name: "pg-main".to_owned(),
        database_type: "postgresql".to_owned(),
        database: None,
    }
}

fn sqlite_connection() -> WorkflowConnectionOption {
    WorkflowConnectionOption {
        id: "c3".to_owned(),
        name: "lite-dev".to_owned(),
        database_type: "sqlite".to_owned(),
        database: None,
    }
}

/// Two saved connections (one multi-db postgres, one single-db sqlite) and
/// the postgres database list a loaded host would have served.
fn state_with_connections() -> AppState {
    let mut state = AppState::default();
    state.workflow_form_connections = vec![pg_connection(), sqlite_connection()];
    state
        .workflow_form_databases
        .insert("c1".to_owned(), vec!["app".to_owned()]);
    state
}

fn command_def(id: &str, name: &str, workflow: bool, deprecated: bool) -> WorkflowCommandDefinition {
    WorkflowCommandDefinition {
        id: id.to_owned(),
        name: name.to_owned(),
        description: Some("检查连接是否可用".to_owned()),
        input_schema: json!({}),
        metadata: WorkflowCommandMetadata {
            category: None,
            risk: None,
            workflow,
            deprecated,
        },
    }
}

/// A workflow-capable command whose input schema exercises every editor
/// branch: the `sql` frame, a number, a boolean and a JSON array.
fn ping_definition() -> WorkflowCommandDefinition {
    WorkflowCommandDefinition {
        id: "ping".to_owned(),
        name: "Ping".to_owned(),
        description: Some("检查连接是否可用".to_owned()),
        input_schema: json!({
            "properties": {
                "sql": {"type": "string", "title": "SQL"},
                "retries": {"type": "integer", "title": "重试次数"},
                "enabled": {"type": "boolean"},
                "targets": {"type": "array"},
            },
            "required": ["sql"],
        }),
        metadata: WorkflowCommandMetadata {
            category: None,
            risk: Some("highRisk".to_owned()),
            workflow: true,
            deprecated: false,
        },
    }
}

/// A command step on `c1` with the host's loaded command list for it.
fn command_state() -> AppState {
    let mut state = state_with_connections();
    let mut draft = WorkflowDraft::empty();
    let mut step = WorkflowStepDraft::command("step1");
    step.connection = Some("c1".to_owned());
    draft.steps = vec![step];
    state.workflow_draft = draft;
    state.workflow_form_commands.insert(
        "c1".to_owned(),
        vec![ping_definition(), command_def("drop", "Drop", true, true)],
    );
    state
}

/// A page runtime whose command step has chosen `ping` (option1 of the
/// command listbox, `drop` being filtered out as deprecated).
fn chosen_command_runtime(state: AppState) -> Runtime<AppState> {
    let mut rt = page_runtime(state);
    click(&mut rt, "workflow-form-step-0-command-trigger");
    click(&mut rt, "option1");
    rt.frame(0.0);
    rt
}

// ---- the page variant ---------------------------------------------------------

#[test]
fn the_page_form_mounts_its_keyed_parts_over_the_real_state() {
    let mut rt = page_runtime(state_with_connections());

    // The root and every keyed section, in React's vertical order.
    let order = [
        "workflow-form-id-name",
        "workflow-form-description",
        "workflow-form-connection-trigger",
        "workflow-form-schedule",
        "workflow-form-variables-head",
        "workflow-form-steps-head",
        "workflow-form-save",
    ];
    let mut previous_bottom = 0.0;
    for key in order {
        let e = element(&rt, key);
        assert!(
            e.rect[1] >= previous_bottom - 0.5,
            "{key} at {:?} sits below {previous_bottom}",
            e.rect
        );
        previous_bottom = e.rect[1] + e.rect[3];
    }

    // The controls speak their roles.
    assert_eq!(element(&rt, "workflow-form-id").role, "textbox");
    assert_eq!(element(&rt, "workflow-form-name").role, "textbox");
    assert_eq!(
        element(&rt, "workflow-form-connection-trigger").role,
        "combobox"
    );
    assert_eq!(element(&rt, "workflow-form-schedule-enabled").role, "checkbox");
    assert_eq!(element(&rt, "workflow-form-add-variable").role, "button");
    assert_eq!(element(&rt, "workflow-form-add-step").role, "button");
    // The section headers read the zh-CN table.
    assert_eq!(
        element(&rt, "workflow-form-variables-label").text.as_deref(),
        Some(t("workflows.form.variables")),
        "变量 label from the zh table"
    );
    assert_eq!(
        element(&rt, "workflow-form-steps-label").text.as_deref(),
        Some(t("workflows.steps")),
        "步骤 label from the zh table"
    );
    // React's hard-coded connection hint rides under the select.
    assert_eq!(
        element(&rt, "workflow-form-connection-hint").text.as_deref(),
        Some("Data-operation steps inherit this connection unless they override it.")
    );

    // The empty draft's query step renders its card, head and SQL frame.
    assert!(has_key(&rt, "workflow-form-step-0"));
    assert!(has_key(&rt, "workflow-form-step-0-head"));
    assert!(has_key(&rt, "workflow-form-step-0-id"));
    assert!(has_key(&rt, "workflow-form-step-0-type-trigger"));
    assert!(has_key(&rt, "workflow-form-step-0-sql"));

    // The shared footer bar carries the form's own Save slot; its gate is
    // the compact rule (id/name/steps), so an empty draft keeps it closed.
    let save = element(&rt, "workflow-form-save");
    assert_eq!(save.role, "button");
    assert!(!save.focusable, "an empty id/name keeps the shared Save disabled");
    type_into(&mut rt, "workflow-form-id", "wf1");
    type_into(&mut rt, "workflow-form-name", "夜跑");
    assert!(
        element(&rt, "workflow-form-save").focusable,
        "id + name + the seeded step enable the shared Save"
    );

    // Choosing the workflow connection mounts the multi-db database picker
    // and both choices land in the draft through the real slots.
    click(&mut rt, "workflow-form-connection-trigger");
    assert!(has_key(&rt, "option1"), "the connection listbox opened");
    click(&mut rt, "option1");
    match rt.state.workflow_draft.connection.as_deref() {
        Some("c1") => {}
        other => panic!("draft.connection is {other:?}, want Some(\"c1\")"),
    }
    assert!(
        has_key(&rt, "workflow-form-database-trigger"),
        "a multi-db connection offers the database picker"
    );
    click(&mut rt, "workflow-form-database-trigger");
    click(&mut rt, "option1");
    match rt.state.workflow_draft.database.as_deref() {
        Some("app") => {}
        other => panic!("draft.database is {other:?}, want Some(\"app\")"),
    }
    assert!(
        !has_key(&rt, "workflow-form-database-loading"),
        "the loaded list replaces the loading line"
    );

    // Without saved connections the step renders neither select (React's
    // `connections.length === 0` guard), while the card itself stays.
    let bare = page_runtime(AppState::default());
    assert!(has_key(&bare, "workflow-form-step-0"));
    assert!(!has_key(&bare, "workflow-form-step-0-connection-trigger"));
    assert!(!has_key(&bare, "workflow-form-step-0-database-trigger"));
}

#[test]
fn the_page_root_keeps_reacts_max_w_3xl_centered_column_geometry() {
    let rt = page_runtime(state_with_connections());
    let root = element(&rt, "workflow-form-root");
    // max-w-3xl (768pt): the root caps its width even inside the 960pt
    // window. (React's mx-auto centring depends on the page mount's
    // container — the kit root the form builds under is a row, where the
    // form's self-centring is inert — so T147's page pins the x offset.)
    assert!((root.rect[2] - 768.0).abs() < 0.5, "max-w-3xl: {:?}", root.rect);
    // pt-6: the first section starts 24pt below the root's top edge.
    let grid = element(&rt, "workflow-form-id-name");
    assert!(
        (grid.rect[1] - root.rect[1] - 24.0).abs() < 0.5,
        "pt-6: root {:?}, grid {:?}",
        root.rect,
        grid.rect
    );
    // px-6: the grid spans the root minus 2 × 24pt.
    assert!(
        (grid.rect[0] - (root.rect[0] + 24.0)).abs() < 0.5,
        "px-6 left: {:?}",
        grid.rect
    );
    assert!(
        (grid.rect[2] - (root.rect[2] - 48.0)).abs() < 0.5,
        "px-6 width: {:?}",
        grid.rect
    );
    // grid grid-cols-2 gap-4: two equal halves in one band, 16pt apart.
    let id = element(&rt, "workflow-form-id");
    let name = element(&rt, "workflow-form-name");
    assert!(
        (id.rect[1] - name.rect[1]).abs() < 0.5,
        "the columns share a band: id {:?}, name {:?}",
        id.rect,
        name.rect
    );
    assert!(
        (name.rect[0] - id.rect[0] - id.rect[2] - 16.0).abs() < 0.5,
        "gap-4: id {:?}, name {:?}",
        id.rect,
        name.rect
    );
    assert!(
        (id.rect[2] - name.rect[2]).abs() < 0.5,
        "equal halves: id {:?}, name {:?}",
        id.rect,
        name.rect
    );
    // space-y-4 between the connection section's hint and the schedule box.
    let hint = element(&rt, "workflow-form-connection-hint");
    let schedule = element(&rt, "workflow-form-schedule");
    assert!(
        (schedule.rect[1] - hint.rect[1] - hint.rect[3] - 16.0).abs() < 0.5,
        "space-y-4: hint {:?}, schedule {:?}",
        hint.rect,
        schedule.rect
    );
    // The schedule box is rounded-lg border p-3: its checkbox sits 12pt in.
    let checkbox = element(&rt, "workflow-form-schedule-enabled");
    assert!(
        (checkbox.rect[1] - schedule.rect[1] - 12.0).abs() < 0.5,
        "p-3 box: schedule {:?}, checkbox {:?}",
        schedule.rect,
        checkbox.rect
    );
}

// ---- the compact variant ------------------------------------------------------

#[test]
fn the_compact_variant_boxes_in_p3_and_gates_its_save() {
    let mut rt = compact_runtime(AppState::default());
    let root = element(&rt, "workflow-form-root");
    // w-full of the sidebar-sized probe window.
    assert!((root.rect[2] - 480.0).abs() < 0.5, "w-full: {:?}", root.rect);
    // The compact title reads the zh-CN create key.
    let title = element(&rt, "workflow-form-compact-title");
    assert_eq!(title.text.as_deref(), Some("新建工作流"), "zh create title");
    // border + p-3: the first child sits 12pt below the box's top edge.
    assert!(
        (title.rect[1] - root.rect[1] - 12.0).abs() < 0.5,
        "p-3: root {:?}, title {:?}",
        root.rect,
        title.rect
    );
    // The compact footer: `flex justify-end gap-2 pt-1`.
    let footer = element(&rt, "workflow-form-footer");
    let save = element(&rt, "workflow-form-save");
    let cancel = element(&rt, "workflow-form-cancel");
    assert_eq!(save.label.as_deref(), Some("保存"), "zh save");
    assert_eq!(cancel.label.as_deref(), Some("取消"), "zh cancel");
    assert_eq!(save.role, "button");
    assert!(
        save.rect[0] < cancel.rect[0],
        "Save precedes Cancel (React order)"
    );
    assert!(
        (cancel.rect[0] - save.rect[0] - save.rect[2] - 8.0).abs() < 0.5,
        "gap-2: save {:?}, cancel {:?}",
        save.rect,
        cancel.rect
    );
    assert!(
        (save.rect[1] - footer.rect[1] - 4.0).abs() < 0.5,
        "pt-1: footer {:?}, save {:?}",
        footer.rect,
        save.rect
    );
    assert!(
        (footer.rect[0] + footer.rect[2] - cancel.rect[0] - cancel.rect[2]).abs() < 0.5,
        "justify-end: footer {:?}, cancel {:?}",
        footer.rect,
        cancel.rect
    );
    assert!(
        (save.rect[3] - 20.0).abs() < 0.5,
        "the px-3 py-1 small button is 20pt: {:?}",
        save.rect
    );
    // React's disabled prop: no id/name yet → inert and unpressable.
    assert!(!save.focusable, "an empty draft disables the compact Save");
    assert!(cancel.focusable, "Cancel stays enabled beside it");

    // A disabled Save is inert: its press never reaches the host save path
    // (the feedback line the save would write stays silent).
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "workflow-form-save");
    assert!(
        rt.state.workflow_feedback.is_empty(),
        "a disabled Save press does nothing"
    );

    // Filling the id and name opens the gate.
    type_into(&mut rt, "workflow-form-id", "wf1");
    type_into(&mut rt, "workflow-form-name", "夜跑");
    assert!(element(&rt, "workflow-form-save").focusable, "the gate opened");
    click(&mut rt, "workflow-form-save");
    assert_eq!(
        rt.state.workflow_feedback,
        datazen_ui::i18n::t("workflows.saved"),
        "the enabled Save ran save_workflow_form over the real slots"
    );
    // The draft's steps reached the memory registry: the empty first step's
    // sql rides as its id (the lean projection workflowDraftToDefinition
    // carries, T225's full conversion is ahead).
    use datazen_ui_api::WorkspaceApi;
    let saved = rt
        .state
        .workspace
        .workflows()
        .into_iter()
        .find(|flow| flow.name == "夜跑")
        .expect("the registry saved the draft");
    assert_eq!(saved.steps, vec!["step1".to_owned()]);

    // The compact Cancel routes through the real slot wiring too.
    rt.state
        .workflow_results
        .push(datazen_ui_api::WorkflowStepResult {
            step: "SELECT 1".to_owned(),
            ok: true,
            summary: "ok".to_owned(),
        });
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "workflow-form-cancel");
    assert!(
        rt.state.workflow_results.is_empty(),
        "on_cancel ran close_workflow_editor over the real state"
    );

    // The editing form (React's editingId): the title flips to the zh edit
    // key and the id input refuses focus.
    let mut state = AppState::default();
    state.workflow_draft.id = TextEdit::new("existing".to_owned());
    let mut editing = Runtime::new(state, compact_editing_view, (480.0, 720.0), env());
    editing.frame(0.0);
    editing.frame(0.0);
    assert_eq!(
        element(&editing, "workflow-form-compact-title").text.as_deref(),
        Some("编辑"),
        "zh edit title"
    );
    assert!(
        !element(&editing, "workflow-form-id").focusable,
        "editingId disables the id input"
    );
}

// ---- steps -------------------------------------------------------------------

#[test]
fn steps_add_remove_and_rekey_while_the_query_sql_frame_holds_h36() {
    let mut rt = page_runtime(state_with_connections());
    // emptyDraft seeds one query step: card, head, h-36 SQL frame.
    let frame = element(&rt, "workflow-form-step-0-sql");
    assert!((frame.rect[3] - 144.0).abs() < 0.5, "h-36: {:?}", frame.rect);
    assert!(
        !frame.focusable,
        "the SQL engine seam (T243) renders read-only"
    );
    let card = element(&rt, "workflow-form-step-0");
    let head = element(&rt, "workflow-form-step-0-head");
    assert!(
        (head.rect[1] - card.rect[1] - 12.0).abs() < 0.5,
        "the card's p-3: card {:?}, head {:?}",
        card.rect,
        head.rect
    );
    assert!(
        (frame.rect[1] - head.rect[1] - head.rect[3] - 8.0).abs() < 0.5,
        "the card's space-y-2: head {:?}, frame {:?}",
        head.rect,
        frame.rect
    );
    // The empty SQL document shows React's placeholder line.
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("SELECT ...")),
        "the empty frame carries the SELECT ... placeholder"
    );
    // One step shows no remove button (React: steps.length > 1).
    assert!(!has_key(&rt, "workflow-form-step-0-remove"));

    // + 添加步骤 pushes a command step named step{len+1}.
    click(&mut rt, "workflow-form-add-step");
    assert!(has_key(&rt, "workflow-form-step-1"));
    assert!(has_key(&rt, "workflow-form-step-1-command-trigger"));
    assert!(
        !has_key(&rt, "workflow-form-step-1-sql"),
        "the added step is a command step, not a query step"
    );
    match rt.state.workflow_draft.steps.get(1) {
        Some(step) => {
            assert_eq!(step.kind, WorkflowStepKind::Command);
            assert_eq!(step.id.text, "step2", "React's step{{len+1}} naming");
        }
        None => panic!("the add button pushed a second step"),
    }
    // Both cards grow their remove buttons now.
    assert!(has_key(&rt, "workflow-form-step-0-remove"));
    assert!(has_key(&rt, "workflow-form-step-1-remove"));

    // Typing into step 1's id edits exactly that step.
    type_into(&mut rt, "workflow-form-step-1-id", "X");
    match rt.state.workflow_draft.steps.get(1) {
        Some(step) => assert_eq!(step.id.text, "step2X", "the edit landed on step 1"),
        None => panic!("step 1 still exists"),
    }
    match rt.state.workflow_draft.steps.first() {
        Some(step) => assert_eq!(step.id.text, "step1", "step 0 untouched"),
        None => panic!("step 0 exists"),
    }

    // Removing step 0 keeps the edited step, re-keyed to index 0.
    click(&mut rt, "workflow-form-step-0-remove");
    assert!(!has_key(&rt, "workflow-form-step-1"));
    assert_eq!(rt.state.workflow_draft.steps.len(), 1);
    match rt.state.workflow_draft.steps.first() {
        Some(step) => {
            assert_eq!(step.id.text, "step2X", "the survivor kept its edit");
            assert_eq!(step.kind, WorkflowStepKind::Command);
        }
        None => panic!("one step remains"),
    }
    // One step again: its remove button is gone.
    assert!(!has_key(&rt, "workflow-form-step-0-remove"));
}

// ---- variables ----------------------------------------------------------------

#[test]
fn variable_rows_add_choose_a_type_toggle_and_remove() {
    let mut rt = page_runtime(state_with_connections());
    assert!(has_key(&rt, "workflow-form-variables-label"));
    assert!(!has_key(&rt, "workflow-form-variable-0"));

    // + 添加变量 pushes React's empty row (string type, not required).
    click(&mut rt, "workflow-form-add-variable");
    assert!(has_key(&rt, "workflow-form-variable-0"));
    assert!(has_key(&rt, "workflow-form-variable-0-name"));
    assert!(has_key(&rt, "workflow-form-variable-0-type-trigger"));
    assert!(has_key(&rt, "workflow-form-variable-0-description"));
    assert!(has_key(&rt, "workflow-form-variable-0-required"));
    assert!(has_key(&rt, "workflow-form-variable-0-remove"));
    match rt.state.workflow_draft.variables.first() {
        Some(var) => {
            assert_eq!(var.var_type, "string");
            assert!(!var.required);
        }
        None => panic!("the row landed in the draft"),
    }

    // The type listbox: string / number / 连接 (zh).
    click(&mut rt, "workflow-form-variable-0-type-trigger");
    assert!(has_key(&rt, "option2"), "three type options");
    click(&mut rt, "option2");
    match rt.state.workflow_draft.variables.first() {
        Some(var) => assert_eq!(var.var_type, "connection"),
        None => panic!("the row exists"),
    }

    // The required checkbox toggles the row.
    click(&mut rt, "workflow-form-variable-0-required");
    match rt.state.workflow_draft.variables.first() {
        Some(var) => assert!(var.required, "the checkbox toggled required"),
        None => panic!("the row exists"),
    }

    // The remove button drops exactly its row.
    click(&mut rt, "workflow-form-add-variable");
    type_into(&mut rt, "workflow-form-variable-0-name", "首");
    click(&mut rt, "workflow-form-variable-1-remove");
    assert_eq!(rt.state.workflow_draft.variables.len(), 1);
    match rt.state.workflow_draft.variables.first() {
        Some(var) => assert_eq!(var.name.text, "首", "row 0 survived row 1's removal"),
        None => panic!("one row remains"),
    }
    assert!(!has_key(&rt, "workflow-form-variable-1"));
}

// ---- the command step ---------------------------------------------------------

#[test]
fn the_command_step_builds_default_input_and_its_schema_editor() {
    // The command listbox offers the workflow-capable definitions; a
    // deprecated one stays hidden until it is the chosen (React's filter).
    let mut rt = page_runtime(command_state());
    click(&mut rt, "workflow-form-step-0-command-trigger");
    assert!(has_key(&rt, "option0"), "the placeholder row");
    assert!(has_key(&rt, "option1"), "Ping is offered");
    assert!(!has_key(&rt, "option2"), "deprecated Drop is filtered out");
    click(&mut rt, "option1");
    {
        let step = match rt.state.workflow_draft.steps.first() {
            Some(step) => step,
            None => panic!("the command step exists"),
        };
        assert_eq!(step.command.as_deref(), Some("ping"));
        // React's defaultInput: boolean false, array [], object {}, else ''.
        assert_eq!(
            step.input.get("sql"),
            Some(&Value::String(String::new())),
            "string fields default to ''"
        );
        assert_eq!(
            step.input.get("retries"),
            Some(&Value::String(String::new())),
            "numbers default to '' until typed"
        );
        assert_eq!(step.input.get("enabled"), Some(&Value::Bool(false)));
        assert_eq!(step.input.get("targets"), Some(&Value::Array(Vec::new())));
    }

    // The schema editor: the sql frame (React testid command-sql-editor),
    // the number field, the boolean checkbox, the JSON textarea, and the
    // description line with its risk suffix.
    let sql_frame = element(&rt, "workflow-form-step-0-input-sql-editor");
    assert!(
        (sql_frame.rect[3] - 144.0).abs() < 0.5,
        "the command sql frame is h-36: {:?}",
        sql_frame.rect
    );
    assert_eq!(
        element(&rt, "workflow-form-step-0-input-retries").role,
        "textbox"
    );
    assert_eq!(
        element(&rt, "workflow-form-step-0-input-enabled").role,
        "checkbox"
    );
    assert!(has_key(&rt, "workflow-form-step-0-input-targets"));
    assert_eq!(
        element(&rt, "workflow-form-step-0-command-description")
            .text
            .as_deref(),
        Some("检查连接是否可用 · highRisk"),
        "description · risk, as React renders it"
    );

    // Number coercion: typing 5 lands a JSON number in the input object
    // (the f64 Number React's Number() would produce).
    type_into(&mut rt, "workflow-form-step-0-input-retries", "5");
    {
        let step = match rt.state.workflow_draft.steps.first() {
            Some(step) => step,
            None => panic!("the command step exists"),
        };
        assert_eq!(
            step.input.get("retries").and_then(Value::as_f64),
            Some(5.0),
            "Number(value) when numeric"
        );
    }
    // Typing a non-number keeps the raw string (React's NaN || raw rule).
    type_into(&mut rt, "workflow-form-step-0-input-retries", "x");
    {
        let step = match rt.state.workflow_draft.steps.first() {
            Some(step) => step,
            None => panic!("the command step exists"),
        };
        assert_eq!(
            step.input.get("retries"),
            Some(&Value::String("5x".to_owned())),
            "a failed Number() keeps the raw string"
        );
    }

    // A fresh editor: the JSON textarea writes what parses and keeps the
    // last good value on a broken edit (React's try/catch keep-editing).
    let mut good = chosen_command_runtime(command_state());
    type_into(&mut good, "workflow-form-step-0-input-targets", "[\"a\"]");
    {
        let step = match good.state.workflow_draft.steps.first() {
            Some(step) => step,
            None => panic!("the command step exists"),
        };
        assert_eq!(
            step.input.get("targets"),
            Some(&json!(["a"])),
            "a parseable array replaces the default"
        );
    }
    let mut bad = chosen_command_runtime(command_state());
    type_into(&mut bad, "workflow-form-step-0-input-targets", "not json");
    {
        let step = match bad.state.workflow_draft.steps.first() {
            Some(step) => step,
            None => panic!("the command step exists"),
        };
        assert_eq!(
            step.input.get("targets"),
            Some(&Value::Array(Vec::new())),
            "a broken edit leaves the input untouched"
        );
    }
    match bad.state.workflow_form_input_edits.get("step0.targets") {
        Some(edit) => assert_eq!(edit.text, "not json", "the raw text stays visible"),
        None => panic!("the text area kept its edit"),
    }
}

// ---- the pure, exported rules -------------------------------------------------

#[test]
fn the_exported_rules_pin_react_semantics() {
    // The multi-db registry slice (drivers' hasMultiDatabase / domain type).
    assert_eq!(db_registry_meta("postgresql"), Some((true, false)));
    assert_eq!(db_registry_meta("kiwi"), Some((true, true)));
    assert_eq!(db_registry_meta("sqlite"), None);

    let pg = pg_connection();
    let locked = WorkflowConnectionOption {
        database: Some("db0".to_owned()),
        ..pg.clone()
    };
    let kiwi = WorkflowConnectionOption {
        id: "kiwi".to_owned(),
        name: "kiwi".to_owned(),
        database_type: "kiwi".to_owned(),
        database: Some("domain".to_owned()),
    };
    let lite = sqlite_connection();
    assert!(!connection_allows_multi_db(None));
    assert!(connection_allows_multi_db(Some(&pg)));
    assert!(
        !connection_allows_multi_db(Some(&locked)),
        "a pinned database locks a non-domain driver"
    );
    assert!(
        connection_allows_multi_db(Some(&kiwi)),
        "domain-type drivers stay multi-db"
    );
    assert!(!connection_allows_multi_db(Some(&lite)));

    // multiDbConnections: the distinct multi-db connections the draft uses
    // (workflow level or query/command steps; ai steps never ask).
    let mut draft = WorkflowDraft::empty();
    draft.connection = Some("c1".to_owned());
    let mut own = WorkflowStepDraft::command("s1");
    own.connection = Some("kiwi".to_owned());
    let ai = WorkflowStepDraft {
        kind: WorkflowStepKind::Ai,
        id: TextEdit::new("s3".to_owned()),
        sql: String::new(),
        prompt: TextEdit::new(String::new()),
        connection: Some("c3".to_owned()),
        database: None,
        command: None,
        input: Map::new(),
    };
    draft.steps = vec![own, WorkflowStepDraft::query("s2"), ai];
    assert_eq!(
        multi_db_connections(&draft, &[pg, kiwi, lite]),
        vec!["c1".to_owned(), "kiwi".to_owned()]
    );

    // The auto-fill rule: exactly one database fills the workflow default
    // and every query/command step on that connection.
    let mut databases = BTreeMap::new();
    databases.insert("c1".to_owned(), vec!["app".to_owned()]);
    databases.insert("kiwi".to_owned(), vec!["a".to_owned(), "b".to_owned()]);
    let mut fill = WorkflowDraft::empty();
    fill.connection = Some("c1".to_owned());
    let mut on_kiwi = WorkflowStepDraft::query("on-kiwi");
    on_kiwi.connection = Some("kiwi".to_owned());
    fill.steps = vec![WorkflowStepDraft::query("inherits"), on_kiwi];
    assert!(auto_fill_single_databases(&mut fill, &databases));
    assert_eq!(fill.database.as_deref(), Some("app"));
    assert_eq!(fill.steps[0].database.as_deref(), Some("app"));
    assert_eq!(
        fill.steps[1].database,
        None,
        "two databases offer a picker, no fill"
    );
    assert!(
        !auto_fill_single_databases(&mut fill, &databases),
        "the fill is idempotent"
    );

    // The compact Save gate (React's disabled prop).
    let mut gate = WorkflowDraft::empty();
    assert!(compact_save_disabled(&gate));
    gate.id = TextEdit::new("wf".to_owned());
    assert!(compact_save_disabled(&gate), "the name is still empty");
    gate.name = TextEdit::new("n".to_owned());
    assert!(!compact_save_disabled(&gate));
    gate.steps.clear();
    assert!(compact_save_disabled(&gate), "no steps disables Save");

    // scheduleIntervalSecs: Number(value) || 3600.
    gate.steps = vec![WorkflowStepDraft::query("step1")];
    gate.schedule_interval = TextEdit::new("90".to_owned());
    assert_eq!(gate.interval_secs(), 90);
    gate.schedule_interval = TextEdit::new("abc".to_owned());
    assert_eq!(gate.interval_secs(), 3600, "NaN falls back to 3600");
    gate.schedule_interval = TextEdit::new("  42 ".to_owned());
    assert_eq!(gate.interval_secs(), 42, "trim then parse");

    // effectiveConnection: the step's override, the draft's fallback, ''.
    let mut effective = WorkflowDraft::empty();
    effective.connection = Some("wf".to_owned());
    assert_eq!(effective_connection(&effective, &effective.steps[0]), "wf");
    effective.steps[0].connection = Some("own".to_owned());
    assert_eq!(effective_connection(&effective, &effective.steps[0]), "own");

    // The command option label and the workflow filter.
    let backup = WorkflowCommandDefinition {
        id: "backup".to_owned(),
        name: "Backup".to_owned(),
        description: None,
        input_schema: json!({}),
        metadata: WorkflowCommandMetadata {
            category: Some("维护".to_owned()),
            risk: None,
            workflow: true,
            deprecated: true,
        },
    };
    assert_eq!(command_option_label(&backup), "Backup · 维护 (deprecated)");
    let defs = vec![ping_definition(), command_def("internal", "Internal", false, false), backup];
    assert_eq!(
        workflow_definitions(&defs, None).len(),
        1,
        "workflow !== false, and not deprecated"
    );
    assert_eq!(
        workflow_definitions(&defs, Some("backup")).len(),
        2,
        "a deprecated command stays while it is the chosen one"
    );

    // defaultInput per schema type.
    let defaults = default_input(Some(&ping_definition()));
    assert_eq!(defaults.get("sql"), Some(&Value::String(String::new())));
    assert_eq!(defaults.get("retries"), Some(&Value::String(String::new())));
    assert_eq!(defaults.get("enabled"), Some(&Value::Bool(false)));
    assert_eq!(defaults.get("targets"), Some(&Value::Array(Vec::new())));
    assert!(default_input(None).is_empty());

    // seed_json_text: strings pass through, others pretty-print at 2.
    assert_eq!(seed_json_text(Some(&Value::String("raw".to_owned()))), "raw");
    assert_eq!(seed_json_text(Some(&json!({"a": 1}))), "{\n  \"a\": 1\n}");
    assert_eq!(seed_json_text(None), "");
}

// ---- the schedule box ---------------------------------------------------------

#[test]
fn the_schedule_box_toggles_revealing_the_interval_and_hint() {
    let mut rt = page_runtime(state_with_connections());
    let checkbox = element(&rt, "workflow-form-schedule-enabled");
    assert_eq!(checkbox.role, "checkbox");
    assert_eq!(
        checkbox.label.as_deref(),
        Some("定时运行"),
        "the zh-CN schedule label"
    );
    // Off: React's conditional block is unmounted.
    assert!(!has_key(&rt, "workflow-form-schedule-interval"));
    assert!(!has_key(&rt, "workflow-form-schedule-hint"));

    // Toggling mounts the interval input and its hint.
    click(&mut rt, "workflow-form-schedule-enabled");
    assert!(rt.state.workflow_draft.schedule_enabled);
    assert_eq!(
        element(&rt, "workflow-form-schedule-interval").role,
        "textbox",
        "the interval input appeared"
    );
    let hint = element(&rt, "workflow-form-schedule-hint");
    assert_eq!(
        hint.text.as_deref(),
        Some(t("workflows.schedule.hint")),
        "the hint reads the zh table"
    );
    assert_eq!(
        hint.text.as_deref(),
        Some("最短 30 秒。定时备份请使用含 backup/command 的工作流。"),
        "the zh-CN hint literal"
    );
    assert_eq!(
        rt.state.workflow_draft.schedule_interval.text, "3600",
        "emptyDraft seeds the interval field at 3600"
    );

    // Toggling back unmounts the block again.
    click(&mut rt, "workflow-form-schedule-enabled");
    assert!(!rt.state.workflow_draft.schedule_enabled);
    assert!(!has_key(&rt, "workflow-form-schedule-interval"));
    assert!(!has_key(&rt, "workflow-form-schedule-hint"));
}

// ---- the wiring seam ----------------------------------------------------------

#[test]
fn reset_workflow_form_caches_clears_ephemeral_state_but_keeps_the_draft() {
    let mut state = AppState::default();
    state.workflow_form_open = Some(WorkflowFormSelect::Connection);
    state
        .workflow_form_input_edits
        .insert("step0.sql".to_owned(), TextEdit::new("x".to_owned()));
    state.workflow_draft.name = TextEdit::new("kept".to_owned());
    state.reset_workflow_form_caches();
    assert_eq!(
        state.workflow_form_open,
        None,
        "the one open listbox closes (React unmounts)"
    );
    assert!(
        state.workflow_form_input_edits.is_empty(),
        "the command-input text areas reset"
    );
    assert_eq!(
        state.workflow_draft.name.text, "kept",
        "the draft itself survives the reset"
    );
}
