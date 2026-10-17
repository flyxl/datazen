use super::{command::WorkflowCommandStep, error::WorkflowError};
pub fn resolve_connection_id<'a>(
    step: &'a WorkflowCommandStep,
    workflow_connection: Option<&'a str>,
) -> Result<&'a str, WorkflowError> {
    step.effective_connection(workflow_connection)
        .ok_or_else(|| WorkflowError::MissingConnection {
            step_id: step.id.clone(),
        })
}

/// Let a command step inherit the workflow-level default database.
///
/// A `Command` step has no `database` field of its own (`WorkflowStep::Command`
/// in `model.rs`); its execution target travels inside `input.database`, which
/// `command_runtime` later reads. So the workflow default has to be injected
/// here to give command steps the same inheritance a query step already gets
/// from `step_database.or(inherited_database)` in `executor.rs`.
///
/// A non-empty explicit `input.database` wins, including malformed values that
/// must be rejected by execution validation rather than silently replaced.
pub fn inject_inherited_database(
    mut input: serde_json::Value,
    workflow_database: &str,
) -> serde_json::Value {
    if !input.is_object() {
        return input;
    }
    let should_inherit = match input.get("database") {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(database)) => database.trim().is_empty(),
        // Keep malformed, non-string values intact so the normal input
        // validator can reject them instead of quietly choosing another DB.
        Some(_) => false,
    };
    if !should_inherit {
        return input;
    }
    // `inject_sql_target_fields` already filters blank/empty values, so a
    // whitespace-only workflow default is a no-op here.
    if let Some(database) = Some(workflow_database)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        input["database"] = serde_json::Value::String(database.into());
    }
    input
}
