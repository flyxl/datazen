//! Portable table ChangeSet logic: plan, guards, fingerprint and execution.
//!
//! Both hosts (Desktop today, a web transport later) share this crate. A host
//! is responsible only for its own I/O: reading the connection profile,
//! resolving the driver, fetching the table's columns and handing over the
//! runtime engine that owns the session. Everything that decides *what* is
//! written — canonicalization, primary-key/column guards, the fingerprint, the
//! statement plan and the transaction ordering — lives here and produces
//! identical output on every transport.

pub mod change_set;
pub mod insert_plan;
pub mod row_identity;
pub mod write_exec;

pub use change_set::{
    bound_statement_for_plan, build_row_change_plan, canonicalize_changes, changes_fingerprint,
    plan_changes, validate_table_context, ChangeWarning, CommitPendingChangesRequest,
    CommitPendingChangesResponse, PendingRowChange, PlannedStatement, RowChangePlan,
    RowChangeTableContext, RowCommitStatementResult,
};
pub use insert_plan::bound_insert_statement_for_plan;
pub use write_exec::execute_plan;

use crate::error::FeatureError;
use async_trait::async_trait;
use datazen_driver_api::DatabaseDriver;
use datazen_kernel::schema_cache::CachedColumns;
use datazen_platform_api::{dto::session::SessionHandle, id::Counter};
use datazen_runtime::application::RuntimeConnectionUseCases;
use std::sync::Arc;

/// Catalog port: the host answers with the table's columns and primary key.
///
/// The plan builder needs the real key before it renders SQL, so a host that
/// cannot answer must return an explicit error rather than an empty schema —
/// an empty answer is indistinguishable from a table without a primary key.
#[async_trait]
pub trait TableColumnSource: Send + Sync {
    async fn table_columns(
        &self,
        table: &RowChangeTableContext,
    ) -> Result<CachedColumns, FeatureError>;
}

/// The connection facts the plan and the commit are validated against.
///
/// The host reads these from its own profile store; the rules about what they
/// mean stay here so both transports reject the same stale context.
#[derive(Debug, Clone)]
pub struct ConnectionFacts {
    pub database_type: String,
    pub read_only: bool,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub config_revision: u64,
}

/// Build a preview plan from column metadata supplied by the host's catalog.
pub async fn preview_changes(
    driver: &dyn DatabaseDriver,
    table: RowChangeTableContext,
    changes: &[PendingRowChange],
    columns: &dyn TableColumnSource,
    config_revision: u64,
) -> Result<RowChangePlan, FeatureError> {
    let loaded = columns.table_columns(&table).await?;
    let mut plan = build_row_change_plan(driver, table, changes, &loaded)?;
    plan.config_revision = Some(Counter::new(config_revision));
    Ok(plan)
}

/// The shared half of the "the context still matches the connection" rule.
///
/// A host reads these facts from its own profile store; only this layer knows
/// that a driver-type drift invalidates the plan, so a second transport cannot
/// forget the check.
pub fn validate_connection_facts(
    facts: &ConnectionFacts,
    table: &RowChangeTableContext,
) -> Result<(), FeatureError> {
    if facts.database_type != table.driver_type {
        return Err(FeatureError::Validation(
            "Table connection context changed".into(),
        ));
    }
    Ok(())
}

/// Validate a confirmed plan against current facts and execute it.
///
/// The client-provided SQL/summary fields are display-only; the plan is rebuilt
/// from its immutable change metadata before anything runs, so a tampered plan
/// cannot reach the database.
#[allow(clippy::too_many_arguments)]
pub async fn commit_changes(
    driver: &dyn DatabaseDriver,
    request: CommitPendingChangesRequest,
    facts: &ConnectionFacts,
    columns: &dyn TableColumnSource,
    manual_handle: Option<SessionHandle>,
    runtime: Arc<RuntimeConnectionUseCases>,
    context: datazen_platform_api::context::RequestContext,
) -> Result<CommitPendingChangesResponse, FeatureError> {
    validate_table_context(&request.plan.table)?;
    validate_connection_facts(facts, &request.plan.table)?;
    if request.plan.config_revision != Some(Counter::new(facts.config_revision)) {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Connection configuration changed; preview the changes again"
                .into(),
        ));
    }
    if facts.read_only {
        return Err(FeatureError::Validation(
            "grid.cell.readOnly: Connection is read-only; row changes are not allowed".into(),
        ));
    }
    if facts
        .database
        .as_deref()
        .filter(|database| !database.trim().is_empty())
        .is_some_and(|database| request.plan.table.database.as_deref() != Some(database))
    {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Database context changed; preview the changes again".into(),
        ));
    }
    if facts
        .schema
        .as_deref()
        .filter(|schema| !schema.trim().is_empty())
        .is_some_and(|schema| request.plan.table.schema.as_deref() != Some(schema))
    {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Schema context changed; preview the changes again".into(),
        ));
    }
    let loaded = columns.table_columns(&request.plan.table).await?;
    let plan = change_set::validate_immutable_plan(
        driver,
        &request.db_session_id,
        &request.plan,
        &request.fingerprint,
        &loaded,
    )?;
    execute_plan(driver, &plan, manual_handle, runtime, context).await
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
