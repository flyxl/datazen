//! Frozen driver proof recipes, verified under the retained transaction locks.
use datazen_driver_api::{
    namespace::NamespaceTarget, workflow_transaction::WorkflowTransactionPlan,
};
use datazen_feature_core::workflow::{
    context::WorkflowContext,
    model::{WorkflowDefinition, WorkflowStep},
    resource_execution::{execute_with_session, ResourceExecutionHost},
    session_block::RetainedSession,
    WorkflowCommandStep, WorkflowError,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Clone)]
pub(super) struct Proof {
    pub connection: String,
    pub revision: u64,
    pub namespace: NamespaceTarget,
    pub source: Vec<String>,
    pub recipe: WorkflowTransactionPlan,
    pub fingerprint: String,
}
fn unsupported(detail: &str) -> WorkflowError {
    WorkflowError::CapabilityUnsupported {
        detail: format!("CapabilityUnsupported: {detail}"),
    }
}
pub(super) async fn freeze(
    definition: &mut WorkflowDefinition,
    context: &WorkflowContext,
    host: &dyn ResourceExecutionHost,
) -> Result<BTreeMap<String, Proof>, WorkflowError> {
    let executable = definition.clone().into_executable()?;
    let mut proofs = BTreeMap::new();
    for block in &executable.blocks {
        if !block.kind.is_transactional() {
            continue;
        }
        if proofs.len() >= 8 || block.steps.len() > 128 {
            return Err(unsupported("transaction plan quota exceeded"));
        }
        let connection = block
            .target
            .connection
            .as_deref()
            .or(executable.default_target.connection.as_deref())
            .ok_or_else(|| unsupported("explicit connection required"))?;
        let database = block
            .target
            .database
            .clone()
            .or(executable.default_target.database.clone())
            .ok_or_else(|| unsupported("explicit database required"))?;
        let (config, revision, enabled) = host.profile(connection).await?;
        if !enabled || !matches!(config.database_type.as_str(), "postgresql" | "mysql") {
            return Err(unsupported("transaction driver unavailable"));
        }
        let mut steps =
            datazen_feature_core::workflow::transaction_block::resolve_steps(block, context)?;
        let source: Vec<String> = steps
            .iter()
            .map(|step| match step {
                WorkflowStep::Query { sql, .. } => sql.clone(),
                WorkflowStep::Command { input, .. } => {
                    input["sql"].as_str().unwrap_or_default().to_owned()
                }
                _ => String::new(),
            })
            .collect();
        if source
            .iter()
            .any(|sql| sql.is_empty() || sql.contains("{{") || sql.contains("}}"))
        {
            return Err(unsupported("transaction SQL must be completely frozen"));
        }
        let namespace = NamespaceTarget {
            database: Some(database),
            catalog: None,
            schema: config.schema.clone(),
            path: vec![],
        };
        let driver = host.driver(&config).await?;
        let recipe = driver.workflow_transaction_plan(&source, &namespace).map_err(|_| WorkflowError::UnsupportedPlan { offending_steps: block.steps.iter().map(|step| step.step.step_id().to_owned()).collect(), detail: "UnsupportedPlan: driver could not prove the transaction plan".into() })?.ok_or_else(|| unsupported("driver transaction plan proof unavailable (MySQL engine/trigger/FK visibility and metadata locking must be proven)"))?;
        if recipe.statements.len() != steps.len() {
            return Err(unsupported("driver proof shape mismatch"));
        }
        for (step, sql) in steps.iter_mut().zip(&recipe.statements) {
            match step {
                WorkflowStep::Query { sql: target, .. } => *target = sql.clone(),
                WorkflowStep::Command { input, .. } => input["sql"] = sql.clone().into(),
                _ => return Err(unsupported("transaction step shape mismatch")),
            }
        }
        let material = serde_json::json!({"connection":connection,"revision":revision,"namespace":namespace,"proofProfile":recipe.proof_profile,"sql":recipe.statements,"locks":recipe.lock_sql,"probe":recipe.metadata_probe_sql});
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&material)
                    .map_err(|_| unsupported("proof digest unavailable"))?
            )
        );
        let target = definition
            .blocks
            .iter_mut()
            .find(|target| target.id == block.id)
            .ok_or_else(|| unsupported("transaction block lost"))?;
        target.steps = steps;
        proofs.insert(
            block.id.clone(),
            Proof {
                connection: connection.into(),
                revision,
                namespace,
                source,
                recipe,
                fingerprint,
            },
        );
    }
    Ok(proofs)
}
pub(super) async fn verify(
    host: &dyn ResourceExecutionHost,
    block: &str,
    proof: &Proof,
    session: &RetainedSession,
    deadline: tokio::time::Instant,
) -> Result<(), WorkflowError> {
    let (config, revision, enabled) = host.profile(&proof.connection).await?;
    if !enabled || revision != proof.revision || config.schema != proof.namespace.schema {
        return Err(unsupported("frozen transaction profile changed"));
    }
    let driver = host.driver(&config).await?;
    if driver
        .workflow_transaction_plan(&proof.source, &proof.namespace)
        .map_err(|_| unsupported("transaction capability changed"))?
        .as_ref()
        != Some(&proof.recipe)
    {
        return Err(unsupported("frozen driver proof changed"));
    }
    for (index, sql) in proof.recipe.lock_sql.iter().enumerate() {
        dispatch(
            host,
            &format!("{block}__proof_lock_{index}"),
            proof,
            session,
            deadline,
            sql,
        )
        .await?;
    }
    let result = dispatch(
        host,
        &format!("{block}__proof_catalog"),
        proof,
        session,
        deadline,
        &proof.recipe.metadata_probe_sql,
    )
    .await?;
    let safe = metadata_safe(&result.data);
    if !safe {
        return Err(WorkflowError::UnsupportedPlan {
            offending_steps: vec![],
            detail: "UnsupportedPlan: locked transaction relation metadata is not proven safe"
                .into(),
        });
    }
    Ok(())
}
async fn dispatch(
    host: &dyn ResourceExecutionHost,
    id: &str,
    proof: &Proof,
    session: &RetainedSession,
    deadline: tokio::time::Instant,
    sql: &str,
) -> Result<datazen_driver_api::CommandResult, WorkflowError> {
    let step = WorkflowCommandStep::new(
        id,
        "query",
        Some(proof.connection.clone()),
        serde_json::json!({"sql":sql,"database":proof.namespace.database,"schema":proof.namespace.schema}),
    );
    execute_with_session(
        host,
        &step,
        &proof.connection,
        None,
        Some(deadline),
        true,
        Some(session),
    )
    .await
}

fn metadata_safe(data: &serde_json::Value) -> bool {
    data["results"]
        .as_array()
        .filter(|results| {
            results.len() == 1 && results[0]["truncated"] == serde_json::Value::Bool(false)
        })
        .and_then(|results| results[0]["rows"].as_array())
        .filter(|rows| rows.len() == 1)
        .and_then(|rows| rows[0].as_array())
        .is_some_and(|row| row.len() == 1 && row[0] == serde_json::Value::Bool(true))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_complete_boolean_true_is_a_metadata_proof() {
        assert!(metadata_safe(
            &serde_json::json!({"results":[{"rows":[[true]],"truncated":false}]})
        ));
        for data in [
            serde_json::json!({"results":[{"rows":[[false]],"truncated":false}]}),
            serde_json::json!({"results":[{"rows":[[true]],"truncated":true}]}),
            serde_json::json!({"results":[{"rows":[[null]],"truncated":false}]}),
            serde_json::json!({"results":[{"rows":[["true"]],"truncated":false}]}),
            serde_json::json!({"results":[{"rows":[[true],[true]],"truncated":false}]}),
            serde_json::json!({"results":[{"rows":[[true,true]],"truncated":false}]}),
        ] {
            assert!(!metadata_safe(&data));
        }
    }
}
