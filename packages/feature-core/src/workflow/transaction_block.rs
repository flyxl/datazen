//! Transaction blocks use one retained session and accept only statements whose
//! effects PostgreSQL and MySQL can keep inside an explicit transaction.
use super::{
    error::WorkflowError,
    executable::ExecutableBlock,
    host::WorkflowHost,
    model::{BlockKind, ErrorStrategy, WorkflowStep},
    session_block::BlockHost,
    target_plan::ExecutionTarget,
    WorkflowCommandStep,
};

pub(super) fn validate(block: &ExecutableBlock) -> Result<(), WorkflowError> {
    if block.kind != BlockKind::TransactionBlock {
        return Err(WorkflowError::Validation(
            "transactionBlock kind required".into(),
        ));
    }
    if block.retry.block > 0 {
        return Err(WorkflowError::Validation(
            "transactionBlock retry requires verified rollback evidence and is unsupported".into(),
        ));
    }
    for step in &block.steps {
        if step
            .step
            .on_error_strategy()
            .is_some_and(|strategy| !matches!(strategy, ErrorStrategy::Abort))
        {
            return Err(WorkflowError::Validation(
                "transactionBlock steps must abort on error".into(),
            ));
        }
        let sql = match &step.step {
            WorkflowStep::Query { sql, .. } => sql.as_str(),
            WorkflowStep::Command { command, input, .. }
                if matches!(command.as_str(), "query" | "execute") =>
            {
                input
                    .get("sql")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        WorkflowError::Validation(
                            "transactionBlock SQL command requires a string sql".into(),
                        )
                    })?
            }
            WorkflowStep::Command { .. } => {
                return Err(WorkflowError::Validation(
                    "transactionBlock accepts query and execute commands only".into(),
                ));
            }
            _ => {
                return Err(WorkflowError::Validation(
                    "transactionBlock accepts SQL query/execute steps only".into(),
                ));
            }
        };
        validate_transaction_sql(&mask_templates(sql)?)?;
    }
    Ok(())
}

pub fn resolve_steps(
    block: &ExecutableBlock,
    context: &super::context::WorkflowContext,
) -> Result<Vec<WorkflowStep>, WorkflowError> {
    let mut steps = Vec::with_capacity(block.steps.len());
    for executable_step in &block.steps {
        let mut step = executable_step.step.clone();
        match &mut step {
            WorkflowStep::Query { sql, .. } => {
                *sql = context
                    .resolve_template(sql)
                    .map_err(WorkflowError::Validation)?;
                validate_transaction_sql(sql)?;
            }
            WorkflowStep::Command { command, input, .. }
                if matches!(command.as_str(), "query" | "execute") =>
            {
                let sql = input
                    .get("sql")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        WorkflowError::Validation(
                            "transactionBlock SQL command requires a string sql".into(),
                        )
                    })?;
                let sql = context
                    .resolve_template(sql)
                    .map_err(WorkflowError::Validation)?;
                validate_transaction_sql(&sql)?;
                input["sql"] = sql.into();
            }
            _ => {
                return Err(WorkflowError::Validation(
                    "transactionBlock accepts SQL query/execute steps only".into(),
                ));
            }
        }
        steps.push(step);
    }
    Ok(steps)
}

fn mask_templates(sql: &str) -> Result<String, WorkflowError> {
    let mut result = String::with_capacity(sql.len());
    let mut remaining = sql;
    loop {
        match remaining.find("{{") {
            Some(start) => {
                result.push_str(&remaining[..start]);
                let expression = &remaining[start + 2..];
                let end = expression.find("}}").ok_or_else(|| {
                    WorkflowError::Validation("transactionBlock template is incomplete".into())
                })?;
                result.push_str("datazen_template_value");
                remaining = &expression[end + 2..];
            }
            None => {
                if remaining.contains("}}") {
                    return Err(WorkflowError::Validation(
                        "transactionBlock template is incomplete".into(),
                    ));
                }
                result.push_str(remaining);
                return Ok(result);
            }
        }
    }
}

fn validate_transaction_sql(sql: &str) -> Result<(), WorkflowError> {
    use sqlparser::{
        ast::{Expr, ObjectNamePart, Query, SetExpr, Statement, TableFactor, Visit, Visitor},
        dialect::GenericDialect,
        parser::Parser,
    };
    use std::ops::ControlFlow;

    fn read_only_query_body(body: &SetExpr) -> bool {
        match body {
            SetExpr::Select(select) => select.into.is_none(),
            SetExpr::Query(query) => query.with.is_none() && read_only_query_body(&query.body),
            SetExpr::SetOperation { left, right, .. } => {
                read_only_query_body(left) && read_only_query_body(right)
            }
            SetExpr::Values(_) => true,
            _ => false,
        }
    }

    struct SafeTransactionSql;
    impl Visitor for SafeTransactionSql {
        type Break = ();

        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            if let Expr::Function(function) = expr {
                let [ObjectNamePart::Identifier(name)] = function.name.0.as_slice() else {
                    return ControlFlow::Break(());
                };
                if name.quote_style.is_some()
                    || !matches!(
                        name.value.to_ascii_lowercase().as_str(),
                        "count"
                            | "sum"
                            | "avg"
                            | "min"
                            | "max"
                            | "abs"
                            | "round"
                            | "ceil"
                            | "ceiling"
                            | "floor"
                            | "sqrt"
                            | "lower"
                            | "upper"
                            | "length"
                            | "char_length"
                            | "octet_length"
                            | "trim"
                            | "ltrim"
                            | "rtrim"
                            | "replace"
                            | "substring"
                            | "substr"
                            | "concat"
                            | "coalesce"
                            | "nullif"
                            | "greatest"
                            | "least"
                            | "now"
                            | "current_date"
                            | "current_timestamp"
                    )
                {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        }

        fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<()> {
            match table {
                TableFactor::Table { args: None, .. }
                | TableFactor::Derived { .. }
                | TableFactor::NestedJoin { .. } => ControlFlow::Continue(()),
                _ => ControlFlow::Break(()),
            }
        }

        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
            if query.with.is_none() && query.locks.is_empty() && read_only_query_body(&query.body) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        }
    }

    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|_| WorkflowError::Validation("transactionBlock SQL is invalid".into()))?;
    let dml_only = statements.len() == 1
        && statements.iter().all(|statement| {
            matches!(
                statement,
                Statement::Query(_)
                    | Statement::Insert { .. }
                    | Statement::Update { .. }
                    | Statement::Delete { .. }
                    | Statement::Merge { .. }
            )
        });
    if !dml_only || statements.visit(&mut SafeTransactionSql).is_break() {
        return Err(WorkflowError::Validation(
            "transactionBlock permits effect-bounded SELECT/DML only; transaction control, CTEs, locks, table functions, unapproved functions and implicit-commit statements are rejected".into(),
        ));
    }
    Ok(())
}

pub(super) async fn begin(
    host: &BlockHost<'_>,
    block_id: &str,
    target: &ExecutionTarget,
    deadline: tokio::time::Instant,
) -> Result<(), WorkflowError> {
    control(host, block_id, target, "BEGIN", deadline).await
}

pub(super) async fn finish(
    host: &BlockHost<'_>,
    block: &ExecutableBlock,
    target: &ExecutionTarget,
    deadline: tokio::time::Instant,
    result: Result<(), WorkflowError>,
) -> Result<(), WorkflowError> {
    match result {
        Ok(()) => control(host, &block.id, target, "COMMIT", deadline)
            .await
            .map_err(|_| WorkflowError::SqlUnconfirmed {
                execution_id: format!("transactionBlock:{}", block.id),
                state: "Unknown".into(),
                effects: "Unknown".into(),
                code: "TransactionCommitUnconfirmed".into(),
            }),
        Err(error) => match control(host, &block.id, target, "ROLLBACK", deadline).await {
            Ok(()) => Err(error),
            Err(_) => Err(WorkflowError::SqlUnconfirmed {
                execution_id: format!("transactionBlock:{}", block.id),
                state: "Unknown".into(),
                effects: "Unknown".into(),
                code: "TransactionRollbackUnconfirmed".into(),
            }),
        },
    }
}

async fn control(
    host: &BlockHost<'_>,
    block_id: &str,
    target: &ExecutionTarget,
    sql: &str,
    deadline: tokio::time::Instant,
) -> Result<(), WorkflowError> {
    let step = WorkflowCommandStep::new(
        format!("{block_id}__transaction_control"),
        "query",
        Some(target.connection_id.clone()),
        serde_json::json!({
            "sql": sql,
            "database": target.database,
        }),
    );
    host.execute_command(&step, Some(&target.connection_id), None, Some(deadline))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, validate_transaction_sql};
    use crate::workflow::{model::WorkflowDefinition, WorkflowError};

    fn executable_block(block: serde_json::Value) -> super::ExecutableBlock {
        let definition: WorkflowDefinition = serde_json::from_value(serde_json::json!({
            "id": "workflow",
            "name": "workflow",
            "description": "",
            "version": "2",
            "steps": [],
            "blocks": [block],
        }))
        .expect("workflow definition");
        definition
            .into_executable()
            .expect("executable workflow")
            .blocks
            .into_iter()
            .next()
            .expect("block")
    }

    #[test]
    fn transaction_sql_allows_queries_and_dml_only() {
        for sql in [
            "SELECT id FROM items",
            "INSERT INTO items(id) VALUES (1)",
            "UPDATE items SET id = 2 WHERE id = 1",
            "DELETE FROM items WHERE id = 2",
        ] {
            assert!(validate_transaction_sql(sql).is_ok(), "{sql}");
        }
    }

    #[test]
    fn transaction_sql_rejects_controls_ddl_ctes_and_invalid_sql() {
        for sql in [
            "BEGIN",
            "COMMIT",
            "ROLLBACK",
            "CREATE TABLE items(id INT)",
            "WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed",
            "SELECT 1; UPDATE items SET id = 2",
            "SELECT nextval('items_id_seq')",
            "SELECT pg_catalog.set_config('search_path', '', false)",
            "SELECT * FROM generate_series(1, 10)",
            "not sql",
        ] {
            assert!(validate_transaction_sql(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn transaction_block_accepts_dml_and_rejects_retry_or_skip_policy() {
        let safe = executable_block(serde_json::json!({
            "id": "tx",
            "type": "transactionBlock",
            "connection": "pg",
            "database": "app",
            "steps": [{"type": "query", "id": "write", "sql": "UPDATE items SET value = 1"}],
        }));
        assert!(validate(&safe).is_ok());

        let retry = executable_block(serde_json::json!({
            "id": "tx",
            "type": "transactionBlock",
            "connection": "pg",
            "database": "app",
            "retry": {"block": 1},
            "steps": [{"type": "query", "id": "write", "sql": "UPDATE items SET value = 1"}],
        }));
        assert!(matches!(
            validate(&retry),
            Err(WorkflowError::Validation(_))
        ));

        let skip = executable_block(serde_json::json!({
            "id": "tx",
            "type": "transactionBlock",
            "connection": "pg",
            "database": "app",
            "steps": [{"type": "query", "id": "write", "sql": "UPDATE items SET value = 1", "on_error": {"strategy": "skip"}}],
        }));
        assert!(matches!(validate(&skip), Err(WorkflowError::Validation(_))));
    }

    #[test]
    fn transaction_sql_templates_are_checked_again_after_resolution() {
        let block = executable_block(serde_json::json!({
            "id": "tx",
            "type": "transactionBlock",
            "connection": "pg",
            "database": "app",
            "steps": [{"type": "query", "id": "write", "sql": "UPDATE items SET value = {{value}}"}],
        }));
        assert!(validate(&block).is_ok());

        let safe_context =
            super::super::context::WorkflowContext::new(&serde_json::json!({"value": "1"}));
        assert!(super::resolve_steps(&block, &safe_context).is_ok());

        let unsafe_context = super::super::context::WorkflowContext::new(
            &serde_json::json!({"value": "1; DROP TABLE items"}),
        );
        assert!(matches!(
            super::resolve_steps(&block, &unsafe_context),
            Err(WorkflowError::Validation(_))
        ));
    }
}
