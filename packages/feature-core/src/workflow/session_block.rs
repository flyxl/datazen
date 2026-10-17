//! Retained block resources. Ownership is created from the trusted adapter context.
use super::{
    host::WorkflowHost,
    resource_execution::{PreparedExecution, ResourceExecutionHost},
    WorkflowError,
};
use datazen_application::{
    dto::requests::{CloseMode, CloseSessionRequest},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::dto::session::SessionHandle;

pub struct RetainedSession {
    pub prepared: PreparedExecution,
    pub handle: SessionHandle,
    closed: bool,
}
impl RetainedSession {
    /// Only a trusted Job adapter may construct this after Runtime has accepted
    /// an authorized WorkflowBlock owner. Definitions never provide ownership.
    pub fn from_opened(prepared: PreparedExecution, handle: SessionHandle) -> Self {
        Self {
            prepared,
            handle,
            closed: false,
        }
    }
    pub async fn close(&mut self) -> Result<(), WorkflowError> {
        let receipt = self
            .prepared
            .runtime
            .close_session(
                &self.prepared.context,
                CloseSessionRequest::new(self.handle.clone(), CloseMode::RollbackAndClose),
            )
            .await
            .map_err(api)?;
        self.closed = true;
        if receipt.resource_release
            != datazen_platform_api::dto::session::ResourceRelease::Confirmed
        {
            return Err(WorkflowError::Validation(
                "block close resource release unconfirmed; effects Unknown".into(),
            ));
        }
        Ok(())
    }
}
fn api(error: datazen_application::error::ApiError) -> WorkflowError {
    WorkflowError::Validation(format!("block resource: {:?}", error.code))
}
impl Drop for RetainedSession {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let runtime = self.prepared.runtime.clone();
        let context = self.prepared.context.clone();
        let handle = self.handle.clone();
        if let Ok(task) = tokio::runtime::Handle::try_current() {
            task.spawn(async move {
                if runtime
                    .close_session(
                        &context,
                        CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                    )
                    .await
                    .is_err()
                {
                    tracing::warn!("workflow block cleanup unconfirmed; effects Unknown");
                }
            });
        }
    }
}

pub struct BlockHost<'a> {
    pub parent: &'a dyn WorkflowHost,
    pub resource: &'a dyn ResourceExecutionHost,
    pub session: &'a RetainedSession,
}
#[async_trait::async_trait]
impl WorkflowHost for BlockHost<'_> {
    async fn connection_name(&self, id: &str) -> Result<String, WorkflowError> {
        self.parent.connection_name(id).await
    }
    async fn execute_command(
        &self,
        step: &super::WorkflowCommandStep,
        connection: Option<&str>,
        mode: Option<crate::mcp::permission::McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<datazen_driver_api::CommandResult, WorkflowError> {
        let id = step
            .connection
            .as_deref()
            .or(connection)
            .ok_or_else(|| WorkflowError::Validation("block target required".into()))?;
        super::resource_execution::execute_with_session(
            self.resource,
            step,
            id,
            mode,
            deadline,
            true,
            Some(self.session),
        )
        .await
    }
    async fn complete_ai(&self, prompt: String) -> Result<String, WorkflowError> {
        self.parent.complete_ai(prompt).await
    }
    async fn execute_profile_step(
        &self,
        _: &str,
        _: super::model::WorkflowMigrationOperation,
        _: &str,
        _: Option<&str>,
        _: super::model::UnattendedDestructivePolicy,
        _: Option<&str>,
        _: &mut super::context::WorkflowContext,
    ) -> Result<super::model::StepExecutionResult, WorkflowError> {
        Err(WorkflowError::Validation(
            "migration is not supported in sessionBlock".into(),
        ))
    }
}

/// Resolve v2 targets before any dispatch. Dynamic SQL remains resolved per step;
/// resource identities must be fixed by the run's declared input variables.
pub fn prepare_definition(
    definition: &super::model::WorkflowDefinition,
    context: &super::context::WorkflowContext,
) -> Result<super::model::WorkflowDefinition, WorkflowError> {
    let mut definition = definition.clone();
    fn declaration(
        value: &mut Option<String>,
        context: &super::context::WorkflowContext,
    ) -> Result<(), WorkflowError> {
        if let Some(raw) = value {
            *raw = context.resolve_template(raw)?;
        }
        Ok(())
    }
    declaration(&mut definition.connection, context)?;
    declaration(&mut definition.database, context)?;
    let workflow = super::executable::TargetDeclaration::from_parts(
        definition.connection.as_deref(),
        definition.database.as_deref(),
    );
    fn steps(
        items: &mut [super::model::WorkflowStep],
        workflow: &super::executable::TargetDeclaration,
        block: Option<&super::executable::TargetDeclaration>,
        context: &super::context::WorkflowContext,
    ) -> Result<(), WorkflowError> {
        use super::model::WorkflowStep;
        for step in items {
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
                    connection, input, ..
                } => {
                    declaration(connection, context)?;
                    if input
                        .get("database")
                        .is_some_and(|value| !value.is_null() && !value.is_string())
                    {
                        return Err(WorkflowError::Validation(
                            "v2 command database must be a string".into(),
                        ));
                    }
                    if let Some(raw) = input.get("database").and_then(serde_json::Value::as_str) {
                        input["database"] = context.resolve_template(raw)?.into();
                    }
                }
                WorkflowStep::Condition {
                    then_steps,
                    else_steps,
                    ..
                } => {
                    steps(then_steps, workflow, block, context)?;
                    if let Some(other) = else_steps {
                        steps(other, workflow, block, context)?;
                    }
                    continue;
                }
                WorkflowStep::ForEach { steps: nested, .. } => {
                    steps(nested, workflow, block, context)?;
                    continue;
                }
                _ => continue,
            }
            let selected = super::target_plan::resolve(
                step.step_id(),
                &super::executable::TargetDeclaration::from_step(step),
                block,
                workflow,
                None,
                super::target_plan::TargetRequirements::NONE,
            )?;
            // Materialize the complete selected level so legacy per-field
            // inheritance cannot combine identities from separate levels.
            match step {
                WorkflowStep::Query {
                    connection,
                    database,
                    ..
                } => {
                    *connection = Some(selected.connection_id);
                    *database = selected.database;
                }
                WorkflowStep::Command {
                    connection, input, ..
                } => {
                    *connection = Some(selected.connection_id);
                    input["database"] = selected.database.into();
                }
                _ => {}
            }
        }
        Ok(())
    }
    steps(&mut definition.steps, &workflow, None, context)?;
    for block in &mut definition.blocks {
        declaration(&mut block.connection, context)?;
        declaration(&mut block.database, context)?;
        let target = super::executable::TargetDeclaration::from_parts(
            block.connection.as_deref(),
            block.database.as_deref(),
        );
        steps(&mut block.steps, &workflow, Some(&target), context)?;
    }
    Ok(definition)
}

pub fn plan_blocks<'a>(
    executable: &'a super::executable::ExecutableWorkflow,
    host: &dyn WorkflowHost,
) -> Result<
    Vec<(
        &'a super::executable::ExecutableBlock,
        super::target_plan::ExecutionTarget,
    )>,
    WorkflowError,
> {
    use super::model::WorkflowStep;
    let mut blocks = Vec::new();
    for block in &executable.blocks {
        if block.kind != super::model::BlockKind::SessionBlock {
            return Err(WorkflowError::Validation(
                "transactionBlock execution is unsupported".into(),
            ));
        }
        if block.steps.iter().any(|s| !(matches!(&s.step, WorkflowStep::Query { .. }) || matches!(&s.step, WorkflowStep::Command { command, .. } if matches!(command.as_str(), "query" | "execute" | "explain")))) {
            return Err(WorkflowError::Validation("sessionBlock currently accepts SQL query/execute/explain steps only".into()));
        }
        let target = super::target_plan::resolve_block_targets(
            &block.id,
            block.kind,
            &block.target,
            &executable.default_target,
            &block.steps,
            super::target_plan::TargetRequirements::NONE,
            super::target_plan::NamespaceSwitch::RequiresNewConnection,
        )?;
        host.validate_session_block()?;
        blocks.push((block, target));
    }
    Ok(blocks)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_blocks(
    blocks: Vec<(
        &super::executable::ExecutableBlock,
        super::target_plan::ExecutionTarget,
    )>,
    app_state: &dyn WorkflowHost,
    context: &mut super::context::WorkflowContext,
    step_results: &mut Vec<super::model::StepExecutionResult>,
    default_strategy: &super::model::ErrorStrategy,
    global_timeout: u64,
    start: &std::time::Instant,
    options: &super::executor::WorkflowExecuteOptions,
) -> Result<(), WorkflowError> {
    for (block, target) in blocks {
        let resources = app_state
            .resources()
            .ok_or_else(|| WorkflowError::Validation("sessionBlock runtime unavailable".into()))?;
        let deadline =
            tokio::time::Instant::from_std(*start) + std::time::Duration::from_secs(global_timeout);
        let mut session = acquire_block(app_state, &target, deadline).await?;
        let host = super::session_block::BlockHost {
            parent: app_state,
            resource: resources,
            session: &session,
        };
        let steps = block
            .steps
            .iter()
            .map(|s| s.step.clone())
            .collect::<Vec<_>>();
        let result = super::executor::WorkflowExecutor::execute_steps(
            &steps,
            &host,
            Some(&target.connection_id),
            target.database.as_deref(),
            context,
            step_results,
            default_strategy,
            global_timeout,
            start,
            options,
        )
        .await;
        let cleanup = session.close().await;
        if let Err(cleanup) = cleanup {
            return Err(WorkflowError::Step(format!(
                "{cleanup}; execution: {:?}",
                result.err()
            )));
        }
        result?;
    }
    Ok(())
}

/// Dropping timed-out admission is safe only when the trusted adapter keeps its
/// own candidate cleanup guard. No receipt means the admission result is Unknown.
pub async fn acquire_block(
    host: &dyn WorkflowHost,
    target: &super::target_plan::ExecutionTarget,
    deadline: tokio::time::Instant,
) -> Result<RetainedSession, WorkflowError> {
    if tokio::time::Instant::now() >= deadline {
        return Err(WorkflowError::GlobalTimeout(0));
    }
    let mut session = tokio::time::timeout_at(deadline, host.open_session_block(target, deadline))
        .await
        .map_err(|_| WorkflowError::SqlUnconfirmed {
            execution_id: "unavailable".into(),
            state: "Unknown".into(),
            effects: "Unknown".into(),
            code: "BlockAcquisitionTimeout".into(),
        })??;
    if tokio::time::Instant::now() >= deadline {
        let cleanup = session.close().await;
        return Err(WorkflowError::Step(format!("block acquisition exceeded workflow deadline; zero steps dispatched; cleanup: {cleanup:?}")));
    }
    Ok(session)
}

pub async fn validate_definition_targets(
    definition: &super::model::WorkflowDefinition,
    host: &dyn WorkflowHost,
) -> Result<(), WorkflowError> {
    fn collect<'a>(
        steps: &'a [super::model::WorkflowStep],
        targets: &mut Vec<&'a super::model::WorkflowStep>,
    ) {
        use super::model::WorkflowStep;
        for step in steps {
            match step {
                WorkflowStep::Query { .. } | WorkflowStep::Command { .. } => targets.push(step),
                WorkflowStep::Condition {
                    then_steps,
                    else_steps,
                    ..
                } => {
                    collect(then_steps, targets);
                    if let Some(other) = else_steps {
                        collect(other, targets);
                    }
                }
                WorkflowStep::ForEach { steps, .. } => collect(steps, targets),
                _ => {}
            }
        }
    }
    let mut targets = Vec::new();
    collect(&definition.steps, &mut targets);
    for block in &definition.blocks {
        collect(&block.steps, &mut targets);
    }
    for step in targets {
        let declared = super::executable::TargetDeclaration::from_step(step);
        let command = match step {
            super::model::WorkflowStep::Command { command, .. } => command.as_str(),
            _ => "query",
        };
        let requirements = host
            .target_requirements(
                declared.connection.as_deref().ok_or_else(|| {
                    WorkflowError::Validation("target connection unavailable".into())
                })?,
                command,
            )
            .await?;
        super::target_plan::resolve(
            step.step_id(),
            &declared,
            None,
            &super::executable::TargetDeclaration::empty(),
            None,
            requirements,
        )?;
    }
    Ok(())
}

pub async fn prepare_v2(
    definition: &super::model::WorkflowDefinition,
    context: &super::context::WorkflowContext,
    host: &dyn WorkflowHost,
    deadline: tokio::time::Instant,
) -> Result<super::model::WorkflowDefinition, WorkflowError> {
    let prepared = prepare_definition(definition, context)?;
    if tokio::time::Instant::now() >= deadline {
        return Err(WorkflowError::Step(
            "workflow deadline expired before target preflight; no resource acquired".into(),
        ));
    }
    tokio::time::timeout_at(deadline, validate_definition_targets(&prepared, host))
        .await
        .map_err(|_| {
            WorkflowError::Step(
                "workflow target metadata preflight timed out; no resource acquired".into(),
            )
        })??;
    Ok(prepared)
}
