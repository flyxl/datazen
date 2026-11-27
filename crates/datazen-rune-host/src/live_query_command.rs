//! Receipt-scoped SQL command output for the live Rune workspace.
use super::{ConnectionId, Counter, ExecutionTarget, LiveSession, RuntimeConnectionUseCases};
use datazen::application::ConnectionUseCases;
use std::sync::{Arc, Mutex};

pub(super) async fn explain_query_observed(
    runtime: &Arc<RuntimeConnectionUseCases>,
    profiles: &Arc<crate::backend::HostProfiles>,
    session: &Arc<Mutex<LiveSession>>,
    sql: &str,
    target: Option<&datazen_ui_api::NativeQueryDatabase>,
    stop: &std::sync::atomic::AtomicBool,
    note_execution: impl Fn(bool),
) -> Result<datazen_ui_api::ExplainResult, String> {
    // The opted-in driver also checks its dialect splitter; never accept an EXPLAIN
    // statement as input, so this action cannot request ANALYZE.
    let supported =
        datazen::execution::domain::sql_guard::sql_main_keyword(sql).is_some_and(|keyword| {
            matches!(
                keyword.as_str(),
                "SELECT" | "VALUES" | "TABLE" | "UPDATE" | "INSERT" | "DELETE" | "MERGE"
            )
        });
    if !supported || datazen_driver_api::sql_dump::split_sql_statements(sql).len() != 1 {
        return Err("Explain requires one SELECT or data-change statement.".into());
    }
    let (connection, database) = {
        let session = session
            .lock()
            .map_err(|_| "The live session is unavailable.")?;
        if !session.supports_query_explain() {
            return Err("Explain is not available on this connection.".into());
        }
        let database = match target {
            Some(target) if session.allows_query_database(target) => Some(target.database.clone()),
            Some(_) => return Err("The selected query database is no longer available.".into()),
            None => session.database.clone(),
        };
        (
            session
                .connection
                .clone()
                .ok_or("The live connection is unavailable.")?,
            database,
        )
    };
    let mut plan = None;
    consume_query_command(
        runtime,
        profiles,
        connection,
        database,
        "explain",
        sql,
        None,
        stop,
        note_execution,
        |output| {
            if output["kind"] != "commandResult" {
                return Ok(());
            }
            let result: datazen_ui_api::ExplainResult =
                serde_json::from_value(output["data"].clone())
                    .map_err(|_| "The server returned an unreadable query plan.")?;
            if result.plan_text.trim().is_empty() || plan.replace(result).is_some() {
                return Err("The server returned ambiguous query plan output.".into());
            }
            Ok(())
        },
    )
    .await?;
    plan.ok_or_else(|| "The server returned no query plan.".into())
}

/// Submit one command against the validated live target, wait for its exact
/// receipt, and consume its published chunks in order.
pub(super) async fn consume_query_command(
    runtime: &Arc<RuntimeConnectionUseCases>,
    profiles: &Arc<crate::backend::HostProfiles>,
    connection: String,
    database: Option<String>,
    command: &'static str,
    sql: &str,
    published_row_limit: Option<u32>,
    stop: &std::sync::atomic::AtomicBool,
    note_execution: impl Fn(bool),
    mut consume: impl FnMut(serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let ctx = crate::host_request_context().clone();
    let token = runtime
        .issue_submission_token(
            &ctx,
            datazen::execution::IdempotentOperation::ExecuteAtTarget,
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut input = serde_json::json!({ "sql": sql });
    if let Some(rows) = published_row_limit {
        input["publishedRowLimit"] = serde_json::json!(rows);
    }
    let receipt = runtime
        .execute_at_target(
            &ctx,
            datazen::execution::application::requests::ExecuteAtTargetRequest {
                target: ExecutionTarget::new(
                    ConnectionId::new(connection.clone()),
                    datazen::connection::domain::NamespaceTarget::new(
                        database,
                        None,
                        None,
                        Vec::new(),
                    ),
                ),
                expected_config_revision: Counter::new(profiles.revision_of(&connection)),
                call: datazen::execution::application::requests::CommandCall {
                    command: command.into(),
                    input,
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    // The serial host worker is occupied by this query. Read cancellation
    // intent outside its job queue and apply it only to this exact receipt.
    use datazen::execution::CancelDisposition;
    use std::sync::atomic::Ordering;
    let view = loop {
        let view = runtime
            .get_execution(&ctx, receipt.execution_id.clone())
            .await
            .map_err(|error| error.to_string())?;
        if view.state.is_terminal() {
            note_execution(true);
            break view;
        }
        if stop.swap(false, Ordering::AcqRel) {
            let receipt = runtime
                .cancel_execution(&ctx, receipt.execution_id.clone())
                .await;
            if !receipt.is_ok_and(|receipt| {
                matches!(
                    receipt.disposition,
                    CancelDisposition::Requested | CancelDisposition::AlreadyFinished
                )
            }) {
                note_execution(false);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    if view.state != datazen::execution::ExecutionState::Succeeded {
        return Err(match view.error_code {
            Some(code) => format!("the statement failed: {code:?}"),
            None => "the statement failed".to_string(),
        });
    }
    for artifact in &view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(&ctx, artifact.clone(), Counter::ZERO)
            .await
            .map_err(|error| error.to_string())?;
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                runtime
                    .read_artifact_chunk(&ctx, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(|error| error.to_string())?
            };
            if let Some(output) = chunk.output {
                consume(output)?;
            }
        }
    }
    Ok(())
}
