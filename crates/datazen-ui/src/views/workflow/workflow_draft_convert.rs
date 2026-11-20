//! Port of `packages/frontend/src/windows/workflow/workflowDraftConvert.ts` (T223; the
//! ledger's older numbering says T225).
//!
//! The React file is the conversion pair between the persisted
//! `WorkflowDefinition` and the form's editing `WorkflowDraft`:
//! `workflowDefinitionToDraft` (what `handleEdit` feeds the edit panel) and
//! `workflowDraftToDefinition` (what the save path hands
//! `aiCommands.workflowSave`). Both halves are line-by-line over the
//! crate's own mirrors: the definition is `datazen_ui_api::WorkflowDefinition`
//! (`types/workflow.rs`, the T-213 family), the draft is
//! `views/workflow/workflow_form.rs`'s `WorkflowDraft` (T070).
//!
//! ## The seams
//!
//! | React | shape here |
//! |-------|------------|
//! | `workflow.id/name/description` | `TextEdit`s — the form's editing model; `draft.name.trim()` reads [`WorkflowDraft::name_trimmed`] |
//! | `v.type \|\| 'string'` | the empty-string fallback on the variable kind |
//! | `v.required ?? false` | `Option<bool>`'s `unwrap_or(false)` |
//! | `s.type === 'command' ? s.command : undefined` | the command legs only on the command kind |
//! | `workflow.schedule?.interval_secs ?? workflow.schedule?.intervalSecs ?? 3600` | the api's `WorkflowSchedule` already unifies the dual key (one field, the snake key on the wire, the camel key an serde alias) — the chain collapses to `and_then(interval_secs)` |
//! | `Math.max(30, draft.scheduleIntervalSecs ?? 3600)` | [`WorkflowDraft::interval_secs`] (React's `Number(value) \|\| 3600`) then the 30-second floor |
//! | the command step's `input.database` carry | the TS comment in the source: the Rust-side command step has no `database` field — the form's step-level choice rides inside `input.database` |
//!
//! ## Deliberate deviations
//!
//! * The draft model carries the form's three step kinds (`WorkflowStepKind`,
//! T070's own documented split — the four exotic kinds round-trip through the
//! YAML editor, T202). React's draft would carry `type: 'condition'` (and its
//! form select renders blank for it, the select offering three); this
//! converter folds the four exotic kinds to a Query draft step — the same
//! degradation React's own save path applies one boundary later (`switch` on
//! `'command'`/`'ai'`/else-query), so the round trip exotic → query-step is
//! identical end to end.
//! * The draft's `sql`/`prompt`/`command`/`input` legs are `Option`/present
//! in React and plain `String`/`Map` in the Rust draft model (T070's
//! editing-model choice); the undefined arms surface as `""`/empty — the
//! save path's `?? ''` arms then read the same values.
//! * The exotic step's sub-structure (`if`/`thenSteps`/`items`/`sources`/…)
//! is lost by the fold, exactly as React's own save path loses it when an
//! exotic step is edited through the visual form (the YAML editor is the
//! lossless path in both).
//! * `String(n)` for the schedule interval is `u64::to_string` — the integer
//! spellings agree.

use std::collections::BTreeMap;

use datazen_ui_api::{WorkflowDefinition, WorkflowSchedule, WorkflowStep, WorkflowStepType};
use rune::TextEdit;
use serde_json::{Map, Value};

use super::workflow_form::{WorkflowDraft, WorkflowStepDraft, WorkflowStepKind, WorkflowVariableDraft};
use datazen_ui_api::WorkflowVariable;

// ---- definition → draft -------------------------------------------------------

/// React's `workflowDefinitionToDraft` — the edit panel's draft over a
/// persisted definition.
pub(crate) fn workflow_definition_to_draft(workflow: &WorkflowDefinition) -> WorkflowDraft {
    WorkflowDraft {
        id: TextEdit::new(workflow.id.clone()),
        name: TextEdit::new(workflow.name.clone()),
        description: TextEdit::new(workflow.description.clone()),
        connection: workflow.connection.clone(),
        database: workflow.database.clone(),
        variables: workflow
            .variables
            .iter()
            .map(|v| WorkflowVariableDraft {
                // `v.type || 'string'`: the empty kind falls back.
                name: TextEdit::new(v.name.clone()),
                var_type: if v.r#type.is_empty() {
                    "string".to_owned()
                } else {
                    v.r#type.clone()
                },
                description: TextEdit::new(v.description.clone()),
                // `v.required ?? false`.
                required: v.required.unwrap_or(false),
            })
            .collect(),
        steps: workflow
            .steps
            .iter()
            .map(|s| {
                // The form's three kinds; the four exotic kinds fold to Query
                // (the module docs' deviation — React's save degrades them the
                // same way one boundary later).
                let kind = match s.r#type {
                    WorkflowStepType::Command => WorkflowStepKind::Command,
                    WorkflowStepType::Ai => WorkflowStepKind::Ai,
                    WorkflowStepType::Query
                    | WorkflowStepType::Condition
                    | WorkflowStepType::Foreach
                    | WorkflowStepType::Merge
                    | WorkflowStepType::Transform => WorkflowStepKind::Query,
                };
                WorkflowStepDraft {
                    kind,
                    id: TextEdit::new(s.id.clone()),
                    sql: s.sql.clone().unwrap_or_default(),
                    prompt: TextEdit::new(s.prompt.clone().unwrap_or_default()),
                    connection: s.connection.clone(),
                    database: s.database.clone(),
                    // `s.type === 'command' ? s.command : undefined` — the
                    // command legs only on the command kind.
                    command: (kind == WorkflowStepKind::Command)
                        .then(|| s.command.clone().unwrap_or_default()),
                    // `s.type === 'command' ? (s.input ?? {}) : undefined`.
                    input: (kind == WorkflowStepKind::Command)
                        .then(|| {
                            s.input
                                .clone()
                                .unwrap_or_default()
                                .into_iter()
                                .collect::<Map<String, Value>>()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect(),
        // `workflow.schedule?.enabled ?? false`.
        schedule_enabled: workflow.schedule.map(|s| s.enabled).unwrap_or(false),
        // `interval_secs ?? intervalSecs ?? 3600` — the api's dual-key read is
        // one field (the serde alias), so the chain is one `and_then`.
        schedule_interval: TextEdit::new(
            workflow
                .schedule
                .and_then(|s| s.interval_secs)
                .unwrap_or(3600)
                .to_string(),
        ),
    }
}

// ---- draft → definition ---------------------------------------------------------

/// A zeroed step — every `?:` leg starts absent, the arms below fill theirs.
fn base_step(kind: WorkflowStepType, id: String) -> WorkflowStep {
    WorkflowStep {
        r#type: kind,
        id,
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

/// React's `workflowDraftToDefinition` — the save path's definition over the
/// editing draft.
pub(crate) fn workflow_draft_to_definition(draft: &WorkflowDraft) -> WorkflowDefinition {
    let steps = draft
        .steps
        .iter()
        .map(|s| match s.kind {
            WorkflowStepKind::Command => {
                // The TS comment: `WorkflowStep::Command` has no `database`
                // field — the target lives in `input.database`. The form still
                // offers a step-level database dropdown, so the choice carries
                // across instead of dropping on the floor.
                let mut input: BTreeMap<String, Value> = s
                    .input
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                // `if (s.database && input.database == null)`: a truthy (non-
                // empty) database and the key absent or null (`undefined ==
                // null` is true, a present value is not).
                if let Some(database) = s.database.as_deref().filter(|d| !d.is_empty()) {
                    if matches!(input.get("database"), None | Some(Value::Null)) {
                        input.insert("database".to_owned(), Value::String(database.to_owned()));
                    }
                }
                let mut step = base_step(WorkflowStepType::Command, s.id.text.clone());
                step.command = Some(s.command.clone().unwrap_or_default()); // `s.command ?? ''`
                step.input = Some(input);
                step.connection = s.connection.clone();
                step
            }
            WorkflowStepKind::Ai => {
                let mut step = base_step(WorkflowStepType::Ai, s.id.text.clone());
                step.prompt = Some(s.prompt.text.clone()); // `s.prompt ?? ''`
                step
            }
            // The else arm — every non-command, non-ai draft step saves as a
            // query step (the exotic kinds already folded at the definition →
            // draft boundary).
            WorkflowStepKind::Query => {
                let mut step = base_step(WorkflowStepType::Query, s.id.text.clone());
                step.sql = Some(s.sql.clone()); // `s.sql ?? ''`
                step.connection = s.connection.clone();
                step.database = s.database.clone();
                step
            }
        })
        .collect();

    WorkflowDefinition {
        id: draft.id_trimmed().to_owned(),        // `draft.id.trim()`
        name: draft.name_trimmed().to_owned(),    // `draft.name.trim()`
        description: draft.description.text.trim().to_owned(),
        connection: draft.connection.clone(),
        database: draft.database.clone(),
        variables: draft
            .variables
            .iter()
            .map(|v| WorkflowVariable {
                name: v.name.text.clone(), // no trim — React doesn't trim these
                r#type: v.var_type.clone(),
                description: v.description.text.clone(),
                required: Some(v.required),
                default: None, // the conversion never writes a default
            })
            .collect(),
        steps,
        // The object literal React writes sets nothing else — the definition's
        // other legs stay absent.
        version: None,
        author: None,
        blocks: None,
        output: None,
        timeout_secs: None,
        error_handling: None,
        schedule: Some(if draft.schedule_enabled {
            WorkflowSchedule {
                enabled: true,
                // `Math.max(30, draft.scheduleIntervalSecs ?? 3600)`.
                interval_secs: Some(draft.interval_secs().max(30)),
            }
        } else {
            WorkflowSchedule {
                enabled: false,
                interval_secs: None,
            }
        }),
        visibility: None,
    }
}
