//! T223 (the ledger's older numbering says T225): the workflow draft
//! conversion pair and the panel guards — the definition → draft field map
//! (the variable kind and required defaults, the command legs, the schedule's
//! dual-key read), the draft → definition save (the trims, the command step's
//! `input.database` carry, the 30-second floor and the `Number() || 3600`
//! parse fallback), the round trip over the form's three kinds, the guards'
//! narrowing and result read, and the tab label's arms with its clock reads.
//!
//! The conversion is pure data — asserted directly. The clock's `HH:MM` is
//! local-zone output, so the pins read its shape (two digits, colon, two
//! digits) and the unparseable fallback verbatim, not a fixed clock time.

use std::collections::BTreeMap;

use datazen_ui_api::{
    WorkflowDefinition, WorkflowSchedule, WorkflowStep, WorkflowStepType, WorkflowVariable,
};
use rune::TextEdit;
use serde_json::{Map, Value};

use datazen_ui::views::workflow_draft_convert::{
    workflow_definition_to_draft, workflow_draft_to_definition,
};
use datazen_ui::views::workflow_form::{WorkflowDraft, WorkflowStepKind, WorkflowVariableDraft};
use datazen_ui::views::workflow_page::{
    WorkflowEditPanel, WorkflowHistoryPanel, WorkflowPanel, WorkflowRunPanel,
};
use datazen_ui::views::workflow_panel_guards::{
    is_ai_create_panel, is_edit_panel, is_history_detail_panel, is_workflow_run_panel,
    panel_execution_result, panel_tab_label, tab_clock, PanelTabLabels,
};
use datazen_ui::views::WorkflowExecutionResult;

// ---- Fixtures ----------------------------------------------------------------

fn definition() -> WorkflowDefinition {
    let command_input = BTreeMap::from([
        ("path".to_owned(), Value::String("/tmp/x.csv".to_owned())),
        ("mode".to_owned(), Value::String("append".to_owned())),
    ]);
    WorkflowDefinition {
        id: "wf-1".to_owned(),
        name: "nightly-etl".to_owned(),
        description: "the nightly load".to_owned(),
        version: Some("2".to_owned()),
        author: Some("ops".to_owned()),
        variables: vec![
            WorkflowVariable {
                name: "env".to_owned(),
                r#type: "string".to_owned(),
                description: "target env".to_owned(),
                required: Some(true),
                default: None,
            },
            WorkflowVariable {
                name: "loose".to_owned(),
                r#type: String::new(),
                description: String::new(),
                required: None,
                default: None,
            },
        ],
        connection: Some("pg-main".to_owned()),
        database: Some("analytics".to_owned()),
        steps: vec![
            WorkflowStep {
                r#type: WorkflowStepType::Query,
                id: "q1".to_owned(),
                sql: Some("SELECT 1".to_owned()),
                connection: Some("pg-replica".to_owned()),
                database: Some("raw".to_owned()),
                ..zeroed_step()
            },
            WorkflowStep {
                r#type: WorkflowStepType::Command,
                id: "c1".to_owned(),
                command: Some("fs.copy".to_owned()),
                input: Some(command_input),
                ..zeroed_step()
            },
            WorkflowStep {
                r#type: WorkflowStepType::Ai,
                id: "a1".to_owned(),
                prompt: Some("summarise".to_owned()),
                ..zeroed_step()
            },
        ],
        blocks: None,
        output: None,
        timeout_secs: None,
        error_handling: None,
        schedule: Some(WorkflowSchedule {
            enabled: true,
            interval_secs: Some(120),
        }),
        visibility: None,
    }
}

fn zeroed_step() -> WorkflowStep {
    WorkflowStep {
        r#type: WorkflowStepType::Query,
        id: String::new(),
        sql: None,
        connection: None,
        database: None,
        command: None,
        input: None,
        prompt: None,
        timeout_secs: None,
        on_error: None,
        r#if: None,
        then_steps: None,
        else_steps: None,
        items: None,
        as_var: None,
        steps: None,
        max_iterations: None,
        sources: None,
        columns: None,
        from: None,
        add_columns: None,
        filter: None,
        sort_by: None,
        offset: None,
        limit: None,
    }
}

/// A draft with the three step kinds and text to trim.
fn draft() -> WorkflowDraft {
    let mut d = WorkflowDraft::empty();
    d.id = TextEdit::new("  wf-1  ".to_owned());
    d.name = TextEdit::new("  nightly  ".to_owned());
    d.description = TextEdit::new("  the load  ".to_owned());
    d.connection = Some("pg-main".to_owned());
    d.database = Some("analytics".to_owned());
    d.schedule_enabled = true;
    d.schedule_interval = TextEdit::new("90".to_owned());
    d
}

fn labels() -> PanelTabLabels {
    PanelTabLabels {
        ai_create: "AI Create".to_owned(),
        edit: "Edit".to_owned(),
        create: "Create".to_owned(),
    }
}

fn result() -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success: true,
        final_output: "done".to_owned(),
        steps: Vec::new(),
        total_time_ms: 3,
        error: None,
    }
}

// ---- definition → draft -------------------------------------------------------

#[test]
fn the_definition_rounds_into_the_draft_field_by_field() {
    let draft = workflow_definition_to_draft(&definition());

    assert_eq!(draft.id.text, "wf-1");
    assert_eq!(draft.name.text, "nightly-etl");
    assert_eq!(draft.description.text, "the nightly load");
    assert_eq!(draft.connection.as_deref(), Some("pg-main"));
    assert_eq!(draft.database.as_deref(), Some("analytics"));

    // Variables: `v.type || 'string'` and `v.required ?? false`.
    assert_eq!(draft.variables.len(), 2);
    let env = &draft.variables[0];
    assert_eq!(env.name.text, "env");
    assert_eq!(env.var_type, "string");
    assert_eq!(env.description.text, "target env");
    assert!(env.required);
    let loose = &draft.variables[1];
    assert_eq!(loose.var_type, "string", "the empty kind falls back");
    assert!(!loose.required, "the absent required defaults false");
    assert_eq!(loose.description.text, "");

    // Schedule: `enabled ?? false` and the interval read.
    assert!(draft.schedule_enabled);
    assert_eq!(draft.schedule_interval.text, "120");

    // The definition's non-form legs (version/author/…) are not draft state —
    // nothing of them appears.
}

#[test]
fn the_steps_carry_their_kind_legs_and_the_exotic_kinds_fold_to_query() {
    let mut def = definition();
    def.steps.push(WorkflowStep {
        r#type: WorkflowStepType::Condition,
        id: "cond".to_owned(),
        sql: Some("SELECT 2".to_owned()),
        ..zeroed_step()
    });
    let draft = workflow_definition_to_draft(&def);
    assert_eq!(draft.steps.len(), 4);

    // Query step: sql/connection/database carried; command/input absent.
    let q = &draft.steps[0];
    assert_eq!(q.kind, WorkflowStepKind::Query);
    assert_eq!(q.id.text, "q1");
    assert_eq!(q.sql, "SELECT 1");
    assert_eq!(q.connection.as_deref(), Some("pg-replica"));
    assert_eq!(q.database.as_deref(), Some("raw"));
    assert!(q.command.is_none() && q.input.is_empty());

    // Command step: command/input carried; sql absent.
    let c = &draft.steps[1];
    assert_eq!(c.kind, WorkflowStepKind::Command);
    assert_eq!(c.command.as_deref(), Some("fs.copy"));
    assert_eq!(c.input.get("path"), Some(&Value::String("/tmp/x.csv".into())));
    assert_eq!(c.sql, "");

    // Ai step: prompt carried.
    let a = &draft.steps[2];
    assert_eq!(a.kind, WorkflowStepKind::Ai);
    assert_eq!(a.prompt.text, "summarise");
    assert_eq!(a.sql, "");

    // The exotic kind folds to a Query draft step (the module docs'
    // deviation — React's save degrades it the same way one boundary later).
    let folded = &draft.steps[3];
    assert_eq!(folded.kind, WorkflowStepKind::Query);
    assert_eq!(folded.sql, "SELECT 2");
    assert!(folded.command.is_none() && folded.input.is_empty());
}

#[test]
fn the_schedule_arms_read_the_dual_key_and_the_defaults() {
    // No schedule: enabled ?? false, interval ?? 3600.
    let mut def = definition();
    def.schedule = None;
    let draft = workflow_definition_to_draft(&def);
    assert!(!draft.schedule_enabled);
    assert_eq!(draft.schedule_interval.text, "3600");

    // A schedule without an interval: the same 3600.
    def.schedule = Some(WorkflowSchedule {
        enabled: false,
        interval_secs: None,
    });
    let draft = workflow_definition_to_draft(&def);
    assert!(!draft.schedule_enabled);
    assert_eq!(draft.schedule_interval.text, "3600");

    // The camel key `intervalSecs` reads the same field (the api's serde
    // alias — the React chain `interval_secs ?? intervalSecs` collapses).
    let json = r#"{"enabled": true, "intervalSecs": 45}"#;
    let schedule: WorkflowSchedule = serde_json::from_str(json).expect("the alias reads the camel key");
    assert_eq!(schedule.interval_secs, Some(45));
}

// ---- draft → definition --------------------------------------------------------

#[test]
fn the_draft_saves_back_trimmed_with_the_three_step_kinds() {
    let mut d = draft();
    d.variables = vec![datazen_ui::views::workflow_form::WorkflowVariableDraft {
        name: TextEdit::new(" env ".to_owned()),
        var_type: "number".to_owned(),
        description: TextEdit::new("the env".to_owned()),
        required: true,
    }];
    // Replace the seeded query step with the three kinds.
    d.steps.clear();
    let mut q = datazen_ui::views::workflow_form::WorkflowStepDraft::query("q1");
    q.id = TextEdit::new(" q1 ".to_owned());
    q.sql = "SELECT 1".to_owned();
    q.connection = Some("pg-replica".to_owned());
    q.database = Some("raw".to_owned());
    let mut a = datazen_ui::views::workflow_form::WorkflowStepDraft::query("a1");
    a.kind = WorkflowStepKind::Ai;
    a.prompt = TextEdit::new("summarise".to_owned());
    d.steps.push(q);
    d.steps.push(a);

    let def = workflow_draft_to_definition(&d);

    // The three trims.
    assert_eq!(def.id, "wf-1");
    assert_eq!(def.name, "nightly");
    assert_eq!(def.description, "the load");
    assert_eq!(def.connection.as_deref(), Some("pg-main"));
    assert_eq!(def.database.as_deref(), Some("analytics"));

    // Variables: the fields ride untrimmed (React does not trim these), the
    // required boolean carries, no default is written.
    assert_eq!(def.variables.len(), 1);
    assert_eq!(def.variables[0].name, " env ");
    assert_eq!(def.variables[0].r#type, "number");
    assert_eq!(def.variables[0].description, "the env");
    assert_eq!(def.variables[0].required, Some(true));
    assert_eq!(def.variables[0].default, None);

    // The query step: sql/connection/database, id trimmed by the draft's own
    // text (the id arm reads `s.id` — the trim is the outer id/name/description
    // only, the step id rides as typed).
    assert_eq!(def.steps[0].r#type, WorkflowStepType::Query);
    assert_eq!(def.steps[0].id, " q1 ");
    assert_eq!(def.steps[0].sql.as_deref(), Some("SELECT 1"));
    assert_eq!(def.steps[0].connection.as_deref(), Some("pg-replica"));
    assert_eq!(def.steps[0].database.as_deref(), Some("raw"));
    assert_eq!(def.steps[0].command, None);
    assert_eq!(def.steps[0].input, None);

    // The ai step: prompt ?? ''.
    assert_eq!(def.steps[1].r#type, WorkflowStepType::Ai);
    assert_eq!(def.steps[1].prompt.as_deref(), Some("summarise"));
    assert_eq!(def.steps[1].sql, None);

    // The schedule: enabled with the interval.
    let schedule = def.schedule.as_ref().unwrap();
    assert!(schedule.enabled);
    assert_eq!(schedule.interval_secs, Some(90));

    // The object literal sets nothing else.
    assert_eq!(def.version, None);
    assert_eq!(def.author, None);
    assert_eq!(def.blocks, None);
    assert_eq!(def.output, None);
    assert_eq!(def.timeout_secs, None);
    assert_eq!(def.error_handling, None);
    assert_eq!(def.visibility, None);
}

#[test]
fn the_command_step_carries_its_database_into_input_database() {
    fn command_draft(database: Option<&str>, input: Map<String, Value>) -> WorkflowDraft {
        let mut d = WorkflowDraft::empty();
        let mut c = datazen_ui::views::workflow_form::WorkflowStepDraft::command("c1");
        c.kind = WorkflowStepKind::Command;
        c.command = Some("fs.copy".to_owned());
        c.connection = Some("pg-main".to_owned());
        c.database = database.map(str::to_owned);
        c.input = input;
        d.steps = vec![c];
        d
    }

    // Arm 1: a database and no input.database → carried.
    let def = workflow_draft_to_definition(&command_draft(Some("raw"), Map::new()));
    let step = &def.steps[0];
    assert_eq!(step.r#type, WorkflowStepType::Command);
    assert_eq!(step.command.as_deref(), Some("fs.copy"));
    assert_eq!(step.connection.as_deref(), Some("pg-main"));
    assert_eq!(step.database, None, "the command step keeps no database field");
    assert_eq!(
        step.input.as_ref().unwrap().get("database"),
        Some(&Value::String("raw".into()))
    );

    // Arm 2: an existing input.database wins — not overwritten.
    let mut input = Map::new();
    input.insert("database".to_owned(), Value::String("chosen".to_owned()));
    let def = workflow_draft_to_definition(&command_draft(Some("raw"), input));
    assert_eq!(
        def.steps[0].input.as_ref().unwrap().get("database"),
        Some(&Value::String("chosen".into()))
    );

    // Arm 3: a null input.database is `== null` — carried (the TS loose arm).
    let mut input = Map::new();
    input.insert("database".to_owned(), Value::Null);
    let def = workflow_draft_to_definition(&command_draft(Some("raw"), input));
    assert_eq!(
        def.steps[0].input.as_ref().unwrap().get("database"),
        Some(&Value::String("raw".into()))
    );

    // Arm 4: an empty database string is falsy — not carried.
    let def = workflow_draft_to_definition(&command_draft(Some(""), Map::new()));
    assert!(def.steps[0].input.as_ref().unwrap().get("database").is_none());

    // Arm 5: no database — nothing carried, the input rides as-is.
    let mut input = Map::new();
    input.insert("path".to_owned(), Value::String("/tmp/x".into()));
    let def = workflow_draft_to_definition(&command_draft(None, input));
    let saved = def.steps[0].input.as_ref().unwrap();
    assert_eq!(saved.get("path"), Some(&Value::String("/tmp/x".into())));
    assert!(saved.get("database").is_none());

    // The command step's own `?? ''` arm: a command-less draft saves "".
    let mut d = command_draft(None, Map::new());
    d.steps[0].command = None;
    let def = workflow_draft_to_definition(&d);
    assert_eq!(def.steps[0].command.as_deref(), Some(""));
}

#[test]
fn the_save_path_floors_the_interval_at_thirty_and_parses_the_draft_text() {
    // `Math.max(30, ...)`: a below-floor interval clamps.
    let mut d = draft();
    d.schedule_interval = TextEdit::new("10".to_owned());
    let def = workflow_draft_to_definition(&d);
    assert_eq!(def.schedule.unwrap().interval_secs, Some(30));

    // An above-floor interval rides.
    d.schedule_interval = TextEdit::new("90".to_owned());
    let def = workflow_draft_to_definition(&d);
    assert_eq!(def.schedule.unwrap().interval_secs, Some(90));

    // The `Number(value) || 3600` parse: non-numeric text falls back, then
    // the floor applies to the fallback too.
    d.schedule_interval = TextEdit::new("soon".to_owned());
    let def = workflow_draft_to_definition(&d);
    assert_eq!(def.schedule.unwrap().interval_secs, Some(3600));

    // The disabled arm: enabled false, no interval.
    d.schedule_enabled = false;
    d.schedule_interval = TextEdit::new("90".to_owned());
    let schedule = workflow_draft_to_definition(&d).schedule.unwrap();
    assert!(!schedule.enabled);
    assert_eq!(schedule.interval_secs, None);
}

#[test]
fn the_round_trip_is_stable_for_the_three_form_kinds() {
    // draft → definition → draft over the form's three kinds is the identity
    // the edit-then-save flow relies on (the exotic kinds' fold is one-way —
    // the definition of a folded step comes back a query step).
    let mut d = draft();
    d.steps.clear();
    let mut q = datazen_ui::views::workflow_form::WorkflowStepDraft::query("q1");
    q.sql = "SELECT 1".to_owned();
    q.connection = Some("pg-replica".to_owned());
    q.database = Some("raw".to_owned());
    let mut c = datazen_ui::views::workflow_form::WorkflowStepDraft::command("c1");
    c.command = Some("fs.copy".to_owned());
    let mut c_input = Map::new();
    c_input.insert("path".to_owned(), Value::String("/tmp/x".to_owned()));
    c.input = c_input;
    c.database = None;
    let mut a = datazen_ui::views::workflow_form::WorkflowStepDraft::query("a1");
    a.kind = WorkflowStepKind::Ai;
    a.prompt = TextEdit::new("summarise".to_owned());
    d.steps = vec![q, c, a];

    let def = workflow_draft_to_definition(&d);
    let back = workflow_definition_to_draft(&def);

    assert_eq!(back.id.text, "wf-1"); // the trimmed id is the new draft text
    assert_eq!(back.name.text, "nightly");
    assert_eq!(back.description.text, "the load");
    assert_eq!(back.schedule_enabled, true);
    assert_eq!(back.schedule_interval.text, "90");
    assert_eq!(back.steps.len(), 3);
    assert_eq!(back.steps[0].kind, WorkflowStepKind::Query);
    assert_eq!(back.steps[0].sql, "SELECT 1");
    assert_eq!(back.steps[0].connection.as_deref(), Some("pg-replica"));
    assert_eq!(back.steps[0].database.as_deref(), Some("raw"));
    assert_eq!(back.steps[1].kind, WorkflowStepKind::Command);
    assert_eq!(back.steps[1].command.as_deref(), Some("fs.copy"));
    assert_eq!(
        back.steps[1].input.get("path"),
        Some(&Value::String("/tmp/x".into()))
    );
    assert_eq!(back.steps[2].kind, WorkflowStepKind::Ai);
    assert_eq!(back.steps[2].prompt.text, "summarise");
}

// ---- The guards (React 36-58) ---------------------------------------------------

#[test]
fn the_panel_guards_narrow_the_union_and_the_result_read() {
    let run = WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-1".to_owned(),
        workflow_id: "etl".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        started_at: "2026-01-02T10:30:00Z".to_owned(),
        result: Some(result()),
        is_executing: false,
    });
    let run_pending = WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-2".to_owned(),
        workflow_id: "etl".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        started_at: "2026-01-02T10:31:00Z".to_owned(),
        result: None,
        is_executing: true,
    });
    let edit = WorkflowPanel::Edit(WorkflowEditPanel {
        id: "edit-1".to_owned(),
        editing_id: Some("etl".to_owned()),
    });
    let history = WorkflowPanel::History(WorkflowHistoryPanel {
        id: "hist-1".to_owned(),
        history_id: "h-9".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        created_at: "2026-01-02T09:15:00Z".to_owned(),
        result: result(),
    });
    let ai_create = WorkflowPanel::AiCreate { id: "ai-1".to_owned() };

    // The four guards, one true arm each.
    assert!(is_workflow_run_panel(&run) && !is_workflow_run_panel(&edit));
    assert!(is_history_detail_panel(&history) && !is_history_detail_panel(&run));
    assert!(is_edit_panel(&edit) && !is_edit_panel(&history));
    assert!(is_ai_create_panel(&ai_create) && !is_ai_create_panel(&edit));

    // panelExecutionResult: the run's answer (null while in flight), the
    // history record; edit and ai-create carry none.
    assert!(panel_execution_result(&run).is_some());
    assert!(panel_execution_result(&run_pending).is_none());
    let read = panel_execution_result(&history).unwrap();
    assert_eq!(read.final_output, "done");
    assert!(panel_execution_result(&edit).is_none());
    assert!(panel_execution_result(&ai_create).is_none());
}

// ---- panelTabLabel (React 60-75) ---------------------------------------------------

#[test]
fn the_tab_label_reads_the_labels_bag_the_draft_name_and_the_clocks() {
    let labels = labels();

    // The ai-create arm.
    let ai = WorkflowPanel::AiCreate { id: "ai-1".to_owned() };
    assert_eq!(panel_tab_label(&ai, &labels, ""), "AI Create");

    // The edit arms: the editing tab reads the draft's name, the empty name
    // falls back to the edit label, the create tab is the create label.
    let editing = WorkflowPanel::Edit(WorkflowEditPanel {
        id: "edit-1".to_owned(),
        editing_id: Some("etl".to_owned()),
    });
    assert_eq!(panel_tab_label(&editing, &labels, "nightly"), "nightly");
    assert_eq!(panel_tab_label(&editing, &labels, ""), "Edit");
    let creating = WorkflowPanel::Edit(WorkflowEditPanel {
        id: "edit-2".to_owned(),
        editing_id: None,
    });
    assert_eq!(panel_tab_label(&creating, &labels, "nightly"), "Create");

    // The run/history arms: the name and the clock read.
    let run = WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-1".to_owned(),
        workflow_id: "etl".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        started_at: "2026-01-02T10:30:00Z".to_owned(),
        result: None,
        is_executing: true,
    });
    let label = panel_tab_label(&run, &labels, "");
    assert!(label.starts_with("nightly-etl "), "{label}");
    let clock = label.trim_start_matches("nightly-etl ");
    assert!(
        clock.len() == 5
            && clock.as_bytes()[2] == b':'
            && clock[..2].bytes().all(|b| b.is_ascii_digit())
            && clock[3..].bytes().all(|b| b.is_ascii_digit()),
        "the 2-digit:2-digit clock shape, got {clock:?}"
    );

    // An unparseable clock input stays verbatim.
    let run_odd = WorkflowPanel::Run(WorkflowRunPanel {
        id: "wf-2".to_owned(),
        workflow_id: "etl".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        started_at: "not-a-clock".to_owned(),
        result: None,
        is_executing: false,
    });
    assert_eq!(panel_tab_label(&run_odd, &labels, ""), "nightly-etl not-a-clock");
    assert_eq!(tab_clock("not-a-clock"), "not-a-clock");

    // The history arm reads its own clock field.
    let history = WorkflowPanel::History(WorkflowHistoryPanel {
        id: "hist-1".to_owned(),
        history_id: "h-9".to_owned(),
        workflow_name: "nightly-etl".to_owned(),
        created_at: "not-a-clock-either".to_owned(),
        result: result(),
    });
    assert_eq!(
        panel_tab_label(&history, &labels, ""),
        "nightly-etl not-a-clock-either"
    );
}


// ==== T223 audit round ==========================================================
//
// The pins below close mutation survivors: every probe mutation survived the
// port's first 9 tests red-handed and now dies on its pin. The React
// positions cite `packages/frontend/src/windows/workflow/workflowDraftConvert.ts`.

#[test]
fn the_step_fields_carry_regardless_of_their_kind() {
    // React 17-25 maps every field of every step with no kind gate — sql,
    // prompt, connection and database ride on the command and ai steps the
    // same as the query ones (only command/input are command-gated, React
    // 24-25). Three probe mutations gating sql/prompt/connection to their
    // "own" kinds survived the suite; these pins close them (and database
    // with them).
    let mut def = definition();
    def.steps[1].sql = Some("SELECT 0".to_owned());
    def.steps[1].prompt = Some("unused".to_owned());
    def.steps[1].connection = Some("pg-alt".to_owned());
    def.steps[1].database = Some("alt-db".to_owned());
    def.steps[2].sql = Some("SELECT 9".to_owned());
    def.steps[2].connection = Some("pg-alt2".to_owned());
    def.steps[2].database = Some("alt-db2".to_owned());
    let draft = workflow_definition_to_draft(&def);

    let c = &draft.steps[1];
    assert_eq!(c.kind, WorkflowStepKind::Command);
    assert_eq!(c.sql, "SELECT 0", "sql rides a command step ungated");
    assert_eq!(c.prompt.text, "unused", "prompt rides a command step ungated");
    assert_eq!(c.connection.as_deref(), Some("pg-alt"));
    assert_eq!(c.database.as_deref(), Some("alt-db"));
    assert_eq!(c.command.as_deref(), Some("fs.copy"));

    let a = &draft.steps[2];
    assert_eq!(a.kind, WorkflowStepKind::Ai);
    assert_eq!(a.sql, "SELECT 9");
    assert_eq!(a.prompt.text, "summarise");
    assert_eq!(a.connection.as_deref(), Some("pg-alt2"));
    assert_eq!(a.database.as_deref(), Some("alt-db2"));
    assert!(a.command.is_none() && a.input.is_empty(), "the command legs stay gated");
}
