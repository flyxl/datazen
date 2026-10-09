//! Execute schema diff deploy plans with dialect-aware atomicity.

use super::types::{
    DdlAtomicity, DeployStatus, SchemaDiffDeployResult, SchemaDiffPlan, StatementExecResult,
    StatementRisk,
};
use crate::transaction::TransactionScope;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, SqlTarget};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct DeployOptions {
    pub use_transaction: bool,
    pub stop_on_error: bool,
}

impl Default for DeployOptions {
    fn default() -> Self {
        Self {
            use_transaction: true,
            stop_on_error: true,
        }
    }
}

/// Trait for unit-testing deploy status classification without a real driver.
#[async_trait::async_trait]
pub trait StatementExecutor: Send + Sync {
    async fn exec(&self, sql: &str) -> Result<(), String>;
}

pub async fn run_deploy_with_executor(
    executor: &dyn StatementExecutor,
    plan: &SchemaDiffPlan,
    opts: &DeployOptions,
    atomicity: DdlAtomicity,
    cancelled: Option<&AtomicBool>,
) -> SchemaDiffDeployResult {
    let can_tx = matches!(atomicity, DdlAtomicity::Transactional) && opts.use_transaction;
    let n = plan.statements.len();

    if n == 0 {
        return SchemaDiffDeployResult {
            status: DeployStatus::Committed,
            executed_count: 0,
            statement_count: 0,
            errors: vec![],
            statement_results: vec![],
        };
    }

    if plan
        .statements
        .iter()
        .any(|statement| statement.requires_transaction)
        && !can_tx
    {
        return SchemaDiffDeployResult {
            status: DeployStatus::Failed,
            executed_count: 0,
            statement_count: n,
            errors: vec!["This reviewed table rebuild requires transactional DDL; enable transaction execution".into()],
            statement_results: vec![],
        };
    }

    if can_tx {
        if let Err(e) = executor.exec("BEGIN").await {
            return SchemaDiffDeployResult {
                status: DeployStatus::Failed,
                executed_count: 0,
                statement_count: n,
                errors: vec![format!("BEGIN failed: {e}")],
                statement_results: vec![],
            };
        }
    }

    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut ok_count = 0usize;
    let mut failed = false;

    for (index, stmt) in plan.statements.iter().enumerate() {
        if let Some(flag) = cancelled {
            if flag.load(Ordering::SeqCst) {
                tracing::info!(index, "schema diff deploy cancelled");
                let mut status = if ok_count > 0 {
                    DeployStatus::Mixed
                } else {
                    DeployStatus::Cancelled
                };
                let mut cancel_errors = vec!["Deploy cancelled by user".into()];
                if can_tx {
                    match executor.exec("ROLLBACK").await {
                        Ok(()) => {
                            ok_count = 0;
                            status = DeployStatus::Cancelled;
                        }
                        Err(e) => {
                            status = DeployStatus::Unknown;
                            cancel_errors.push(format!("ROLLBACK failed: {e}"));
                        }
                    }
                }
                return SchemaDiffDeployResult {
                    status,
                    executed_count: ok_count,
                    statement_count: n,
                    errors: cancel_errors,
                    statement_results: results,
                };
            }
        }
        match executor.exec(&stmt.sql).await {
            Ok(()) => {
                tracing::info!(index, sql = %stmt.sql, "deploy statement OK");
                ok_count += 1;
                results.push(StatementExecResult {
                    index,
                    sql: stmt.sql.clone(),
                    ok: true,
                    error: None,
                });
            }
            Err(e) => {
                failed = true;
                tracing::warn!(index, sql = %stmt.sql, error = %e, "deploy statement failed");
                errors.push(format!("[{index}] {}: {e}", stmt.summary));
                results.push(StatementExecResult {
                    index,
                    sql: stmt.sql.clone(),
                    ok: false,
                    error: Some(e),
                });
                if opts.stop_on_error {
                    break;
                }
            }
        }
    }

    if can_tx {
        if failed {
            let rollback = executor.exec("ROLLBACK").await;
            let status = if let Err(e) = rollback {
                errors.push(format!("ROLLBACK failed: {e}"));
                DeployStatus::Unknown
            } else {
                ok_count = 0;
                DeployStatus::RolledBack
            };
            return SchemaDiffDeployResult {
                status,
                executed_count: ok_count,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        if let Err(e) = executor.exec("COMMIT").await {
            errors.push(format!("COMMIT outcome unknown: {e}"));
            return SchemaDiffDeployResult {
                status: DeployStatus::Unknown,
                executed_count: ok_count,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        return SchemaDiffDeployResult {
            status: DeployStatus::Committed,
            executed_count: ok_count,
            statement_count: n,
            errors,
            statement_results: results,
        };
    }

    // Auto-commit / unknown: never claim RolledBack for prior statements.
    let status = if !failed {
        DeployStatus::Committed
    } else if ok_count == 0 {
        DeployStatus::Failed
    } else {
        DeployStatus::Mixed
    };

    SchemaDiffDeployResult {
        status,
        executed_count: ok_count,
        statement_count: n,
        errors,
        statement_results: results,
    }
}

pub async fn execute_schema_diff_deploy(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    plan: &SchemaDiffPlan,
    opts: DeployOptions,
    cancelled: Option<std::sync::Arc<AtomicBool>>,
) -> SchemaDiffDeployResult {
    execute_schema_diff_deploy_at(
        driver,
        handle,
        plan,
        opts,
        cancelled,
        SqlTarget::new(None, None),
    )
    .await
}

/// [`execute_schema_diff_deploy`] against an explicit target database/schema.
///
/// The deploy statements are already qualified, but PostgreSQL cannot reference
/// another database in one statement, so the target is what routes each
/// statement (and the wrapping transaction) to the right catalog.
pub async fn execute_schema_diff_deploy_at(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    plan: &SchemaDiffPlan,
    opts: DeployOptions,
    cancelled: Option<std::sync::Arc<AtomicBool>>,
    target: SqlTarget<'_>,
) -> SchemaDiffDeployResult {
    let atomicity = driver.ddl_atomicity();
    let can_tx = matches!(atomicity, DdlAtomicity::Transactional) && opts.use_transaction;
    let n = plan.statements.len();

    if n == 0 {
        return SchemaDiffDeployResult {
            status: DeployStatus::Committed,
            executed_count: 0,
            statement_count: 0,
            errors: vec![],
            statement_results: vec![],
        };
    }

    if plan
        .statements
        .iter()
        .any(|statement| statement.requires_transaction)
        && !can_tx
    {
        return SchemaDiffDeployResult {
            status: DeployStatus::Failed,
            executed_count: 0,
            statement_count: n,
            errors: vec!["This reviewed table rebuild requires transactional DDL; enable transaction execution".into()],
            statement_results: vec![],
        };
    }

    let mut tx_scope = if can_tx {
        match TransactionScope::begin_at(driver, handle, target).await {
            Ok(scope) => Some(scope),
            Err(e) => {
                return SchemaDiffDeployResult {
                    status: DeployStatus::Failed,
                    executed_count: 0,
                    statement_count: n,
                    errors: vec![format!("BEGIN failed: {e}")],
                    statement_results: vec![],
                };
            }
        }
    } else {
        None
    };

    if plan
        .statements
        .iter()
        .any(|statement| statement.requires_transaction)
    {
        let validation = driver
            .validate_schema_migration_plan(handle, target, &plan.expected_target_schemas)
            .await;
        if let Err(error) = validation {
            let mut errors = vec![format!(
                "Reviewed target schema changed or could not be validated: {error}"
            )];
            let status = match tx_scope.take() {
                Some(scope) => match scope.rollback().await {
                    Ok(()) => DeployStatus::RolledBack,
                    Err(rollback_error) => {
                        errors.push(format!("ROLLBACK failed: {rollback_error}"));
                        DeployStatus::Unknown
                    }
                },
                None => DeployStatus::Failed,
            };
            return SchemaDiffDeployResult {
                status,
                executed_count: 0,
                statement_count: n,
                errors,
                statement_results: vec![],
            };
        }
    }

    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut ok_count = 0usize;
    let mut failed = false;

    for (index, stmt) in plan.statements.iter().enumerate() {
        if let Some(ref flag) = cancelled {
            if flag.load(Ordering::SeqCst) {
                tracing::info!(index, "schema diff deploy cancelled");
                let mut status = if ok_count > 0 {
                    DeployStatus::Mixed
                } else {
                    DeployStatus::Cancelled
                };
                let mut cancel_errors = vec!["Deploy cancelled by user".into()];
                if let Some(scope) = tx_scope {
                    match scope.rollback().await {
                        Ok(()) => {
                            ok_count = 0;
                            status = DeployStatus::Cancelled;
                        }
                        Err(e) => {
                            status = DeployStatus::Unknown;
                            cancel_errors.push(format!("ROLLBACK failed: {e}"));
                        }
                    }
                }
                return SchemaDiffDeployResult {
                    status,
                    executed_count: ok_count,
                    statement_count: n,
                    errors: cancel_errors,
                    statement_results: results,
                };
            }
        }

        let exec_result = if let Some(ref scope) = tx_scope {
            scope.execute(&stmt.sql).await.map(|_| ())
        } else {
            driver
                .execute_at(handle, &stmt.sql, target)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        };

        match exec_result {
            Ok(()) => {
                tracing::info!(index, sql = %stmt.sql, "deploy statement OK");
                ok_count += 1;
                results.push(StatementExecResult {
                    index,
                    sql: stmt.sql.clone(),
                    ok: true,
                    error: None,
                });
            }
            Err(e) => {
                failed = true;
                tracing::warn!(index, sql = %stmt.sql, error = %e, "deploy statement failed");
                errors.push(format!("[{index}] {}: {e}", stmt.summary));
                results.push(StatementExecResult {
                    index,
                    sql: stmt.sql.clone(),
                    ok: false,
                    error: Some(e),
                });
                if opts.stop_on_error {
                    break;
                }
            }
        }
    }

    if !failed
        && plan
            .statements
            .iter()
            .any(|statement| statement.requires_transaction)
    {
        if let Err(error) = driver.validate_schema_migration(handle, target).await {
            failed = true;
            errors.push(format!("Schema migration validation failed: {error}"));
        }
    }

    if can_tx {
        let Some(scope) = tx_scope else {
            // Should be unreachable: can_tx implies DdlAtomicity::Transactional
            // which guarantees tx_scope is Some. Defend without panicking.
            return SchemaDiffDeployResult {
                status: DeployStatus::Failed,
                executed_count: 0,
                statement_count: n,
                errors: vec!["transaction scope missing despite can_tx=true".into()],
                statement_results: results,
            };
        };
        if failed {
            let rollback = scope.rollback().await;
            let status = if let Err(e) = rollback {
                errors.push(format!("ROLLBACK failed: {e}"));
                DeployStatus::Unknown
            } else {
                ok_count = 0;
                DeployStatus::RolledBack
            };
            return SchemaDiffDeployResult {
                status,
                executed_count: ok_count,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        if let Err(e) = scope.commit().await {
            errors.push(format!("COMMIT outcome unknown: {e}"));
            return SchemaDiffDeployResult {
                status: DeployStatus::Unknown,
                executed_count: ok_count,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        return SchemaDiffDeployResult {
            status: DeployStatus::Committed,
            executed_count: ok_count,
            statement_count: n,
            errors,
            statement_results: results,
        };
    }

    let status = if !failed {
        DeployStatus::Committed
    } else if ok_count == 0 {
        DeployStatus::Failed
    } else {
        DeployStatus::Mixed
    };

    SchemaDiffDeployResult {
        status,
        executed_count: ok_count,
        statement_count: n,
        errors,
        statement_results: results,
    }
}

pub fn plan_has_destructive(plan: &SchemaDiffPlan) -> bool {
    plan.statements
        .iter()
        .any(|s| matches!(s.risk, StatementRisk::Destructive | StatementRisk::Rewrite))
}

pub const DESTRUCTIVE_CONFIRM_TOKEN: &str = "DEPLOY";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PlanStatement, RollbackCompleteness, StatementRisk};
    use std::sync::Mutex;

    struct ScriptedExecutor {
        outcomes: Mutex<Vec<Result<(), String>>>,
        log: Mutex<Vec<String>>,
        control_failure: Option<&'static str>,
    }

    impl ScriptedExecutor {
        fn new(outcomes: Vec<Result<(), String>>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes),
                log: Mutex::new(vec![]),
                control_failure: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl StatementExecutor for ScriptedExecutor {
        async fn exec(&self, sql: &str) -> Result<(), String> {
            self.log.lock().unwrap().push(sql.to_string());
            if self.control_failure == Some(sql) {
                return Err("connection lost".into());
            }
            if sql == "BEGIN" || sql == "COMMIT" || sql == "ROLLBACK" {
                return Ok(());
            }
            let mut outcomes = self.outcomes.lock().unwrap();
            if outcomes.is_empty() {
                return Ok(());
            }
            outcomes.remove(0)
        }
    }

    fn plan_with(dialect: &str, n_ok: usize) -> SchemaDiffPlan {
        let statements = (0..n_ok)
            .map(|i| PlanStatement {
                sql: format!("SQL_{i}"),
                risk: StatementRisk::Additive,
                rollback_sql: Some(format!("RB_{i}")),
                summary: format!("s{i}"),
                requires_transaction: false,
            })
            .collect();
        SchemaDiffPlan {
            plan_id: None,
            table: "t".into(),
            tables: vec!["t".into()],
            source_dialect: dialect.into(),
            target_dialect: dialect.into(),
            same_dialect: true,
            statements,
            warnings: vec![],
            rollback_completeness: RollbackCompleteness {
                complete: true,
                missing: vec![],
            },
            requirements: vec![],
            type_suggestions: vec![],
            expected_target_schemas: vec![],
        }
    }

    #[tokio::test]
    async fn all_ok_committed() {
        let plan = plan_with("postgresql", 3);
        let exec = ScriptedExecutor::new(vec![Ok(()), Ok(()), Ok(())]);
        let result = run_deploy_with_executor(
            &exec,
            &plan,
            &DeployOptions {
                use_transaction: true,
                stop_on_error: true,
            },
            DdlAtomicity::Transactional,
            None,
        )
        .await;
        assert_eq!(result.status, DeployStatus::Committed);
        assert_eq!(result.executed_count, 3);
    }

    #[tokio::test]
    async fn required_transaction_is_enforced_before_any_statement() {
        let mut plan = plan_with("sqlite", 2);
        plan.statements[0].requires_transaction = true;
        let exec = ScriptedExecutor::new(vec![Ok(()), Ok(())]);
        let result = run_deploy_with_executor(
            &exec,
            &plan,
            &DeployOptions {
                use_transaction: false,
                stop_on_error: true,
            },
            DdlAtomicity::Transactional,
            None,
        )
        .await;

        assert_eq!(result.status, DeployStatus::Failed);
        assert_eq!(result.executed_count, 0);
        assert!(exec.log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn mid_fail_without_tx_is_mixed() {
        let plan = plan_with("mysql", 3);
        let exec = ScriptedExecutor::new(vec![Ok(()), Ok(()), Err("boom".into())]);
        let result = run_deploy_with_executor(
            &exec,
            &plan,
            &DeployOptions {
                use_transaction: true, // ignored for mysql
                stop_on_error: true,
            },
            DdlAtomicity::AutoCommitPerStatement,
            None,
        )
        .await;
        assert_eq!(result.status, DeployStatus::Mixed);
        assert_eq!(result.executed_count, 2);
    }

    #[tokio::test]
    async fn postgres_tx_rollback_on_fail() {
        let plan = plan_with("postgresql", 3);
        let exec = ScriptedExecutor::new(vec![Ok(()), Err("fail".into())]);
        let result = run_deploy_with_executor(
            &exec,
            &plan,
            &DeployOptions {
                use_transaction: true,
                stop_on_error: true,
            },
            DdlAtomicity::Transactional,
            None,
        )
        .await;
        assert_eq!(result.status, DeployStatus::RolledBack);
        assert_eq!(result.executed_count, 0);
        let log = exec.log.lock().unwrap();
        assert!(log.iter().any(|s| s == "ROLLBACK"));
    }

    #[tokio::test]
    async fn cancel_during_deploy_rolls_back_pg() {
        let plan = plan_with("postgresql", 5);
        let exec = ScriptedExecutor::new(vec![Ok(()), Ok(()), Ok(()), Ok(()), Ok(())]);
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::SeqCst);

        let result = run_deploy_with_executor(
            &exec,
            &plan,
            &DeployOptions::default(),
            DdlAtomicity::Transactional,
            Some(&flag),
        )
        .await;
        assert_eq!(result.status, DeployStatus::Cancelled);
        let log = exec.log.lock().unwrap();
        assert!(log.iter().any(|s| s == "ROLLBACK"));
    }
    #[tokio::test]
    async fn transaction_control_errors_never_claim_rollback_or_known_commit() {
        for command in ["COMMIT", "ROLLBACK"] {
            let mut exec = ScriptedExecutor::new(if command == "COMMIT" {
                vec![Ok(())]
            } else {
                vec![Err("DDL error".into())]
            });
            exec.control_failure = Some(command);
            let result = run_deploy_with_executor(
                &exec,
                &plan_with("postgresql", 1),
                &DeployOptions::default(),
                DdlAtomicity::Transactional,
                None,
            )
            .await;
            assert_eq!(result.status, DeployStatus::Unknown);
            assert!(result.errors.iter().any(|e| e.contains(command)));
        }
    }
    #[tokio::test]
    async fn cancellation_rollback_failure_is_unknown() {
        let mut exec = ScriptedExecutor::new(vec![]);
        exec.control_failure = Some("ROLLBACK");
        let flag = AtomicBool::new(true);
        let result = run_deploy_with_executor(
            &exec,
            &plan_with("postgresql", 1),
            &DeployOptions::default(),
            DdlAtomicity::Transactional,
            Some(&flag),
        )
        .await;
        assert_eq!(result.status, DeployStatus::Unknown);
    }
}
