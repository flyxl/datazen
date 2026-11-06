//! Collect SQL templates from a workflow definition.
//!
//! Authorization and execution must agree on *which* strings are SQL and what
//! connection they target. This module is the single walker: the desktop
//! prevalidation/preview path and the shared executor both consume it.

use crate::workflow::domain::model::{ErrorHandlingConfig, WorkflowDefinition, WorkflowStep};
use crate::workflow::WorkflowError;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub struct WorkflowSqlTemplate<'a> {
    pub step_id: &'a str,
    pub sql: &'a str,
    /// Effective connection for this SQL string (step override or workflow).
    pub connection: Option<&'a str>,
}

/// Return every SQL template in execution order, including condition branches,
/// loop bodies and error fallbacks. Non-SQL strings are deliberately absent.
pub fn workflow_sql_templates(
    workflow: &WorkflowDefinition,
) -> Result<Vec<WorkflowSqlTemplate<'_>>, WorkflowError> {
    let mut templates = Vec::new();
    collect_steps(
        &workflow.steps,
        workflow.connection.as_deref(),
        &mut templates,
    )?;
    Ok(templates)
}

/// Names introduced by foreach steps. Authorization uses this to distinguish a
/// loop value (unknown until execution) from a genuinely missing variable.
pub fn workflow_loop_variables(workflow: &WorkflowDefinition) -> HashSet<&str> {
    let mut names = HashSet::new();
    collect_loop_variables(&workflow.steps, &mut names);
    names
}

/// Every step id in the workflow graph, including condition branches, foreach
/// bodies and error fallbacks. A `steps.<id>` template may only name one of
/// these ids; existence is static even though the step result is not.
pub fn workflow_step_ids(workflow: &WorkflowDefinition) -> HashSet<&str> {
    let mut ids = HashSet::new();
    collect_step_ids(&workflow.steps, &mut ids);
    ids
}

fn collect_step_ids<'a>(steps: &'a [WorkflowStep], ids: &mut HashSet<&'a str>) {
    for step in steps {
        ids.insert(step_id(step));
        match step {
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                collect_step_ids(then_steps, ids);
                if let Some(else_steps) = else_steps {
                    collect_step_ids(else_steps, ids);
                }
            }
            WorkflowStep::ForEach { steps, .. } => collect_step_ids(steps, ids),
            _ => {}
        }
        if let Some(config) = step_on_error(step) {
            if let Some(steps) = config.fallback_steps.as_deref() {
                collect_step_ids(steps, ids);
            }
        }
    }
}

fn step_id(step: &WorkflowStep) -> &str {
    match step {
        WorkflowStep::Query { id, .. }
        | WorkflowStep::Command { id, .. }
        | WorkflowStep::Ai { id, .. }
        | WorkflowStep::Condition { id, .. }
        | WorkflowStep::ForEach { id, .. }
        | WorkflowStep::Merge { id, .. }
        | WorkflowStep::Transform { id, .. } => id,
    }
}

fn collect_loop_variables<'a>(steps: &'a [WorkflowStep], names: &mut HashSet<&'a str>) {
    for step in steps {
        match step {
            WorkflowStep::ForEach { as_var, steps, .. } => {
                names.insert(as_var.as_str());
                collect_loop_variables(steps, names);
            }
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                collect_loop_variables(then_steps, names);
                if let Some(else_steps) = else_steps {
                    collect_loop_variables(else_steps, names);
                }
            }
            _ => {}
        }
        if let Some(config) = step_on_error(step) {
            if let Some(steps) = config.fallback_steps.as_deref() {
                collect_loop_variables(steps, names);
            }
        }
    }
}

fn step_on_error(step: &WorkflowStep) -> Option<&ErrorHandlingConfig> {
    match step {
        WorkflowStep::Query { on_error, .. }
        | WorkflowStep::Command { on_error, .. }
        | WorkflowStep::Ai { on_error, .. }
        | WorkflowStep::Merge { on_error, .. }
        | WorkflowStep::Transform { on_error, .. } => on_error.as_ref(),
        WorkflowStep::Condition { .. } | WorkflowStep::ForEach { .. } => None,
    }
}

fn collect_steps<'a>(
    steps: &'a [WorkflowStep],
    inherited_connection: Option<&'a str>,
    out: &mut Vec<WorkflowSqlTemplate<'a>>,
) -> Result<(), WorkflowError> {
    for step in steps {
        match step {
            WorkflowStep::Query {
                id,
                sql,
                connection,
                on_error,
                ..
            } => {
                out.push(WorkflowSqlTemplate {
                    step_id: id,
                    sql,
                    connection: connection.as_deref().or(inherited_connection),
                });
                collect_fallback(on_error.as_ref(), inherited_connection, out)?;
            }
            WorkflowStep::Command {
                id,
                connection,
                input,
                on_error,
                ..
            } => {
                let connection = connection.as_deref().or(inherited_connection);
                collect_input_sql(id, input, connection, out)?;
                collect_fallback(on_error.as_ref(), inherited_connection, out)?;
            }
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                collect_steps(then_steps, inherited_connection, out)?;
                if let Some(else_steps) = else_steps {
                    collect_steps(else_steps, inherited_connection, out)?;
                }
            }
            WorkflowStep::ForEach { steps, .. } => {
                collect_steps(steps, inherited_connection, out)?;
            }
            WorkflowStep::Ai { on_error, .. }
            | WorkflowStep::Merge { on_error, .. }
            | WorkflowStep::Transform { on_error, .. } => {
                collect_fallback(on_error.as_ref(), inherited_connection, out)?;
            }
        }
    }
    Ok(())
}

fn collect_fallback<'a>(
    config: Option<&'a ErrorHandlingConfig>,
    inherited_connection: Option<&'a str>,
    out: &mut Vec<WorkflowSqlTemplate<'a>>,
) -> Result<(), WorkflowError> {
    if let Some(steps) = config.and_then(|config| config.fallback_steps.as_deref()) {
        collect_steps(steps, inherited_connection, out)?;
    }
    Ok(())
}

fn collect_input_sql<'a>(
    step_id: &'a str,
    input: &'a serde_json::Value,
    connection: Option<&'a str>,
    out: &mut Vec<WorkflowSqlTemplate<'a>>,
) -> Result<(), WorkflowError> {
    match input {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if crate::sql_template::is_sql_template_key(key) {
                    let sql = value.as_str().ok_or_else(|| {
                        WorkflowError::Validation(format!(
                            "workflow step '{step_id}' has a non-string SQL template"
                        ))
                    })?;
                    out.push(WorkflowSqlTemplate {
                        step_id,
                        sql,
                        connection,
                    });
                } else {
                    collect_input_sql(step_id, value, connection, out)?;
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_input_sql(step_id, item, connection, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}
