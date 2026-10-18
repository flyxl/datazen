//! Conservative, locked plain-heap DML proof. Catalog probes run inside the
//! retained transaction after relation locks, never on a separate connection.
use datazen_driver_api::{
    namespace::NamespaceTarget, workflow_transaction::WorkflowTransactionPlan, DriverError,
};
use sqlparser::{ast::*, dialect::PostgreSqlDialect, parser::Parser};
use std::{collections::BTreeSet, ops::ControlFlow};
fn unsupported() -> DriverError {
    DriverError::Unsupported("UnsupportedPlan: PostgreSQL transaction requires plain heap literal DML and null predicates".into())
}
fn ident(name: &Ident) -> Result<String, DriverError> {
    if name.quote_style.is_none() && !name.value.is_ascii() {
        return Err(unsupported());
    }
    let value = if name.quote_style.is_some() {
        name.value.clone()
    } else {
        name.value.to_lowercase()
    };
    if value.is_empty() || value.len() > 63 || value.contains('\0') {
        return Err(unsupported());
    }
    Ok(value)
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn scalar(expr: &Expr) -> bool {
    matches!(expr, Expr::Value(value) if matches!(value.value, Value::Number(_, _) | Value::SingleQuotedString(_) | Value::Boolean(_) | Value::Null))
}
struct Bounded<'a> {
    schema: &'a str,
    relations: BTreeSet<String>,
}
impl VisitorMut for Bounded<'_> {
    type Break = ();
    fn pre_visit_relation(&mut self, name: &mut ObjectName) -> ControlFlow<()> {
        let [ObjectNamePart::Identifier(raw)] = name.0.as_slice() else {
            return ControlFlow::Break(());
        };
        let Ok(table) = ident(raw) else {
            return ControlFlow::Break(());
        };
        self.relations.insert(table.clone());
        name.0 = vec![
            ObjectNamePart::Identifier(Ident::with_quote('"', self.schema)),
            ObjectNamePart::Identifier(Ident::with_quote('"', table)),
        ];
        ControlFlow::Continue(())
    }
    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<()> {
        // No operator/function lookup can invoke user code. Boolean AND/OR and
        // IS NULL are syntax primitives. Assignments/VALUES are literals only.
        if scalar(expr)
            || matches!(
                expr,
                Expr::Identifier(_)
                    | Expr::Nested(_)
                    | Expr::IsNull(_)
                    | Expr::IsNotNull(_)
                    | Expr::BinaryOp {
                        op: BinaryOperator::And | BinaryOperator::Or,
                        ..
                    }
            )
        {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
    fn pre_visit_query(&mut self, query: &mut Query) -> ControlFlow<()> {
        if query.with.is_some() || !query.locks.is_empty() {
            return ControlFlow::Break(());
        }
        match query.body.as_ref() {
            SetExpr::Values(_) => ControlFlow::Continue(()),
            SetExpr::Select(select)
                if select.into.is_none()
                    && select.from.len() <= 1
                    && select.from.iter().all(|table| table.joins.is_empty())
                    && select.projection.iter().all(|item| {
                        matches!(
                            item,
                            SelectItem::UnnamedExpr(_)
                                | SelectItem::ExprWithAlias { .. }
                                | SelectItem::Wildcard(_)
                        )
                    }) =>
            {
                ControlFlow::Continue(())
            }
            _ => ControlFlow::Break(()),
        }
    }
    fn pre_visit_table_factor(&mut self, table: &mut TableFactor) -> ControlFlow<()> {
        if matches!(
            table,
            TableFactor::Table {
                args: None,
                alias: None,
                ..
            }
        ) {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
}
pub(crate) fn prepare(
    sql: &[String],
    target: &NamespaceTarget,
) -> Result<WorkflowTransactionPlan, DriverError> {
    let schema = target
        .schema
        .as_deref()
        .filter(|s| !s.is_empty() && s.len() <= 63 && !s.contains('\0') && !s.contains('\\'))
        .ok_or_else(unsupported)?;
    if schema.starts_with("pg_")
        || schema == "information_schema"
        || target.database.as_deref().is_none_or(str::is_empty)
        || target.catalog.is_some()
        || !target.path.is_empty()
        || sql.is_empty()
    {
        return Err(unsupported());
    }
    let mut visitor = Bounded {
        schema,
        relations: BTreeSet::new(),
    };
    let mut statements = Vec::new();
    for raw in sql {
        // PostgreSQL has no executable comments, but refusing comments also
        // prevents accidental use of this proof contract for another dialect.
        if raw.contains("/*") || raw.contains("--") || raw.contains('\\') || raw.contains('\0') {
            return Err(unsupported());
        }
        let mut parsed =
            Parser::parse_sql(&PostgreSqlDialect {}, raw).map_err(|_| unsupported())?;
        if parsed.len() != 1 {
            return Err(unsupported());
        }
        let safe = match &parsed[0] {
            Statement::Query(query) => matches!(query.body.as_ref(), SetExpr::Select(_)),
            Statement::Insert(insert) => insert.on.is_none() && insert.returning.is_none() && insert.table_alias.is_none()
                && !insert.columns.is_empty() && insert.source.as_ref().is_some_and(|source| matches!(source.body.as_ref(), SetExpr::Values(values) if values.rows.iter().all(|row| row.iter().all(scalar)))),
            Statement::Update { table, assignments, from: None, returning: None, or: None, limit: None, .. } => table.joins.is_empty()
                && assignments.iter().all(|assignment| scalar(&assignment.value) && matches!(&assignment.target, AssignmentTarget::ColumnName(name) if name.0.len() == 1)),
            Statement::Delete(delete) => delete.tables.is_empty() && delete.using.is_none() && delete.returning.is_none() && delete.limit.is_none() && delete.order_by.is_empty(),
            _ => false,
        };
        if !safe || VisitMut::visit(&mut parsed, &mut visitor).is_break() {
            return Err(unsupported());
        }
        statements.push(parsed[0].to_string());
    }
    if visitor.relations.is_empty() || visitor.relations.len() > 32 {
        return Err(unsupported());
    }
    let lock_sql = visitor
        .relations
        .iter()
        .map(|table| {
            format!(
                "LOCK TABLE ONLY {}.{} IN SHARE ROW EXCLUSIVE MODE",
                quote(schema),
                quote(table)
            )
        })
        .collect();
    let names = visitor
        .relations
        .iter()
        .map(|table| format!("c.relname OPERATOR(pg_catalog.=) {}", literal(table)))
        .collect::<Vec<_>>()
        .join(" OR ");
    // Explicit pg_catalog operators/functions avoid search_path-dependent
    // resolution while preserving the frozen Runtime namespace.
    let metadata_probe_sql = format!(
        r#"SELECT pg_catalog.count(*) OPERATOR(pg_catalog.=) {count}
AND pg_catalog.bool_and(c.relkind OPERATOR(pg_catalog.=) 'r' AND c.relpersistence OPERATOR(pg_catalog.=) 'p'
 AND NOT c.relispartition AND NOT c.relrowsecurity AND NOT c.relforcerowsecurity
 AND NOT c.relhasrules AND NOT c.relhastriggers AND NOT c.relhasindex
 AND EXISTS (SELECT 1 FROM pg_catalog.pg_am am WHERE am.oid OPERATOR(pg_catalog.=) c.relam AND am.amname OPERATOR(pg_catalog.=) 'heap')
 AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_inherits i WHERE i.inhrelid OPERATOR(pg_catalog.=) c.oid OR i.inhparent OPERATOR(pg_catalog.=) c.oid)
 AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_constraint k WHERE k.conrelid OPERATOR(pg_catalog.=) c.oid OR k.confrelid OPERATOR(pg_catalog.=) c.oid)
 AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_attribute a WHERE a.attrelid OPERATOR(pg_catalog.=) c.oid AND a.attnum OPERATOR(pg_catalog.>) 0 AND NOT a.attisdropped AND
  (a.atthasdef OR NOT (a.attgenerated OPERATOR(pg_catalog.=) '') OR NOT (a.attidentity OPERATOR(pg_catalog.=) '') OR NOT
   (a.atttypid OPERATOR(pg_catalog.=) 16::pg_catalog.oid OR a.atttypid OPERATOR(pg_catalog.=) 20::pg_catalog.oid OR a.atttypid OPERATOR(pg_catalog.=) 21::pg_catalog.oid OR a.atttypid OPERATOR(pg_catalog.=) 23::pg_catalog.oid OR a.atttypid OPERATOR(pg_catalog.=) 25::pg_catalog.oid OR a.atttypid OPERATOR(pg_catalog.=) 1043::pg_catalog.oid))))
FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid OPERATOR(pg_catalog.=) c.relnamespace
WHERE n.nspname OPERATOR(pg_catalog.=) {schema} AND ({names})"#,
        count = visitor.relations.len(),
        schema = literal(schema)
    );
    Ok(WorkflowTransactionPlan {
        proof_profile: format!(
            "pgLockedPlainHeapLiteralDmlV1:{}",
            env!("CARGO_PKG_VERSION")
        ),
        statements,
        lock_sql,
        metadata_probe_sql,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> NamespaceTarget {
        NamespaceTarget::empty()
            .with_database("app")
            .with_schema("approved")
    }
    #[test]
    fn literal_dml_is_qualified_and_all_relations_are_locked_before_catalog_proof() {
        let plan = prepare(
            &[
                "INSERT INTO items(id) VALUES (1)".into(),
                "UPDATE items SET id = 2 WHERE id IS NOT NULL".into(),
                "DELETE FROM items WHERE id IS NULL".into(),
            ],
            &target(),
        )
        .unwrap();
        assert!(plan
            .statements
            .iter()
            .all(|sql| sql.contains("\"approved\".\"items\"")));
        assert_eq!(
            plan.statements[1],
            "UPDATE \"approved\".\"items\" SET id = 2 WHERE id IS NOT NULL"
        );
        assert_eq!(
            plan.lock_sql,
            vec!["LOCK TABLE ONLY \"approved\".\"items\" IN SHARE ROW EXCLUSIVE MODE"]
        );
        assert!(Parser::parse_sql(&PostgreSqlDialect {}, &plan.metadata_probe_sql).is_ok());
        for catalog in [
            "pg_inherits",
            "pg_constraint",
            "pg_attribute",
            "relhasindex",
            "relhastriggers",
            "relrowsecurity",
        ] {
            assert!(plan.metadata_probe_sql.contains(catalog));
        }
    }
    #[test]
    fn unproven_shapes_and_namespace_escapes_are_rejected() {
        for sql in [
            "INSERT INTO items(id) VALUES (nextval('s'))",
            "UPDATE other.items SET id=1",
            "UPDATE items SET id=id+1",
            "SELECT custom() FROM items",
            "DELETE FROM items USING other",
            "CALL p()",
            "COMMIT",
            "CREATE TABLE x(id int)",
            "SELECT * FROM items i JOIN other o ON true",
            "WITH x AS (SELECT 1) SELECT * FROM items",
            "INSERT INTO items DEFAULT VALUES",
        ] {
            assert!(prepare(&[sql.into()], &target()).is_err(), "{sql}");
        }
    }
}
