//! Execute schema diff deploy plans with dialect-aware atomicity.

use super::types::{
    DdlAtomicity, DeployStatus, SchemaDiffDeployResult, SchemaDiffPlan, StatementExecResult,
    StatementRisk,
};
use crate::db::{ConnectionHandle, DatabaseDriver};
use crate::services::transaction::TransactionScope;
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
                if can_tx {
                    let _ = executor.exec("ROLLBACK").await;
                }
                return SchemaDiffDeployResult {
                    status: DeployStatus::Cancelled,
                    executed_count: ok_count,
                    statement_count: n,
                    errors: vec!["Deploy cancelled by user".into()],
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
            let _ = executor.exec("ROLLBACK").await;
            return SchemaDiffDeployResult {
                status: DeployStatus::RolledBack,
                executed_count: 0,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        if let Err(e) = executor.exec("COMMIT").await {
            let _ = executor.exec("ROLLBACK").await;
            errors.push(format!("COMMIT failed: {e}"));
            return SchemaDiffDeployResult {
                status: DeployStatus::Failed,
                executed_count: 0,
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

    let tx_scope = if can_tx {
        match TransactionScope::begin(driver, handle).await {
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

    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut ok_count = 0usize;
    let mut failed = false;

    for (index, stmt) in plan.statements.iter().enumerate() {
        if let Some(ref flag) = cancelled {
            if flag.load(Ordering::SeqCst) {
                tracing::info!(index, "schema diff deploy cancelled");
                if let Some(scope) = tx_scope {
                    let _ = scope.rollback().await;
                }
                return SchemaDiffDeployResult {
                    status: DeployStatus::Cancelled,
                    executed_count: ok_count,
                    statement_count: n,
                    errors: vec!["Deploy cancelled by user".into()],
                    statement_results: results,
                };
            }
        }

        let exec_result = if let Some(ref scope) = tx_scope {
            scope.execute(&stmt.sql).await.map(|_| ())
        } else {
            driver
                .execute(handle, &stmt.sql)
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
            let _ = scope.rollback().await;
            return SchemaDiffDeployResult {
                status: DeployStatus::RolledBack,
                executed_count: 0,
                statement_count: n,
                errors,
                statement_results: results,
            };
        }
        if let Err(e) = scope.commit().await {
            errors.push(format!("COMMIT failed: {e}"));
            return SchemaDiffDeployResult {
                status: DeployStatus::Failed,
                executed_count: 0,
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
    use crate::schema_diff::types::{PlanStatement, RollbackCompleteness, StatementRisk};
    use std::sync::Mutex;

    struct ScriptedExecutor {
        outcomes: Mutex<Vec<Result<(), String>>>,
        log: Mutex<Vec<String>>,
    }

    impl ScriptedExecutor {
        fn new(outcomes: Vec<Result<(), String>>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes),
                log: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait::async_trait]
    impl StatementExecutor for ScriptedExecutor {
        async fn exec(&self, sql: &str) -> Result<(), String> {
            self.log.lock().unwrap().push(sql.to_string());
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
            })
            .collect();
        SchemaDiffPlan {
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
}
