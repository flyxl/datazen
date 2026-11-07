//! Fixed target normalization for scheduled monitor definitions.

use crate::workflow::{
    application::context::WorkflowContext,
    domain::executable::TargetDeclaration,
    domain::model::{WorkflowDefinition, WorkflowStep},
    domain::target_plan::{resolve, TargetRequirements},
    WorkflowError,
};

pub fn prepare_definition(
    definition: &WorkflowDefinition,
    context: &WorkflowContext,
) -> Result<WorkflowDefinition, WorkflowError> {
    fn declaration(
        value: &mut Option<String>,
        context: &WorkflowContext,
    ) -> Result<(), WorkflowError> {
        if let Some(raw) = value {
            *raw = context.resolve_template(raw)?;
        }
        Ok(())
    }
    let mut definition = definition.clone();
    declaration(&mut definition.connection, context)?;
    declaration(&mut definition.database, context)?;
    let workflow = TargetDeclaration::from_parts(
        definition.connection.as_deref(),
        definition.database.as_deref(),
    );
    for step in &mut definition.steps {
        match step {
            WorkflowStep::Query {
                connection,
                database,
                ..
            } => {
                declaration(connection, context)?;
                declaration(database, context)?;
            }
            WorkflowStep::Command {
                command,
                connection,
                input,
                ..
            } if command == "query" => {
                declaration(connection, context)?;
                if input
                    .get("database")
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                {
                    return Err(WorkflowError::Validation(
                        "monitor query database must be a string".into(),
                    ));
                }
                if let Some(raw) = input.get("database").and_then(serde_json::Value::as_str) {
                    input["database"] = context.resolve_template(raw)?.into();
                }
            }
            _ => {
                return Err(WorkflowError::Validation(
                    "scheduled monitor supports fixed read-only query steps only".into(),
                ))
            }
        }
        let target = resolve(
            step.step_id(),
            &TargetDeclaration::from_step(step),
            None,
            &workflow,
            None,
            TargetRequirements::NONE,
        )?;
        match step {
            WorkflowStep::Query {
                connection,
                database,
                ..
            } => {
                *connection = Some(target.connection_id);
                *database = target.database;
            }
            WorkflowStep::Command {
                connection, input, ..
            } => {
                *connection = Some(target.connection_id);
                input["database"] = target.database.into();
            }
            _ => {}
        }
    }
    Ok(definition)
}
