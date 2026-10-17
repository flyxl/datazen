//! Fail-closed AST validation for the frozen restore namespace.
use crate::{namespace::NamespaceTarget, restore_sql, DriverError};
use sqlparser::{
    ast::{ObjectName, Statement, Visit, Visitor},
    dialect::{Dialect, MySqlDialect, PostgreSqlDialect},
    parser::Parser,
};
use std::ops::ControlFlow;

fn invalid() -> DriverError {
    DriverError::InvalidConfig(
        "Restore SQL object is outside the frozen namespace or cannot be verified".into(),
    )
}

pub(super) fn validate(
    sql: &str,
    profile: &str,
    target: &NamespaceTarget,
    overwrite: bool,
) -> Result<(), DriverError> {
    let dialect: &dyn Dialect = match profile {
        restore_sql::TRANSACTIONAL => &PostgreSqlDialect {},
        restore_sql::IMPLICIT_COMMIT => &MySqlDialect {},
        _ => return Err(invalid()),
    };
    let statements = Parser::parse_sql(dialect, sql).map_err(|_| invalid())?;
    if statements.len() != 1 {
        return Err(invalid());
    }
    let mut guard = TargetGuard {
        target,
        postgres: profile == restore_sql::TRANSACTIONAL,
    };
    let statement = &statements[0];
    // Some DDL names are not annotated as visitor relations upstream. Inspect
    // those explicitly; unknown statements cannot silently escape the guard.
    match statement {
        Statement::CreateTable(table) => {
            guard.name(&table.name)?;
            // These carry additional object names without relation annotations.
            // They are outside the verified restore format until individually supported.
            if table.like.is_some() || table.clone.is_some() || table.inherits.is_some() {
                return Err(invalid());
            }
        }
        Statement::CreateView {
            name, materialized, ..
        } => {
            guard.name(name)?;
            // No partial overwrite: reject before the first script write.
            if overwrite && *materialized {
                return Err(DriverError::InvalidConfig(
                    "Restore overwrite of materialized views is unsupported".into(),
                ));
            }
        }
        Statement::CreateFunction(function) => guard.name(&function.name)?,
        Statement::CreateIndex(index) => {
            if let Some(name) = &index.name {
                // PostgreSQL creates an index in its table's schema; the new
                // unqualified index name is not a search_path lookup.
                if !guard.postgres || name.0.len() != 1 {
                    guard.name(name)?;
                }
            }
            guard.name(&index.table_name)?;
        }
        Statement::Drop {
            names,
            table,
            cascade,
            ..
        } => {
            if guard.postgres && *cascade {
                return Err(invalid());
            }
            for name in names {
                guard.name(name)?;
            }
            if let Some(table) = table {
                guard.name(table)?;
            }
        }
        Statement::CreateSequence { name, owned_by, .. } => {
            guard.name(name)?;
            if let Some(owner) = owned_by {
                guard.column(owner)?;
            }
        }
        Statement::CreateType { name, .. } => guard.name(name)?,
        Statement::Comment {
            object_type,
            object_name,
            ..
        } => {
            if object_type.to_string().eq_ignore_ascii_case("COLUMN") {
                guard.column(object_name)?;
            } else {
                guard.name(object_name)?;
            }
        }
        Statement::Delete(delete) => {
            for table in &delete.tables {
                guard.name(table)?;
            }
        }
        Statement::AlterTable { operations, .. } => {
            // Namespace/rename clauses have additional destinations that the
            // relation visitor does not cover. Refuse these forms up front.
            if guard.postgres
                && operations.iter().any(|operation| {
                    matches!(
                        operation,
                        sqlparser::ast::AlterTableOperation::DropColumn {
                            drop_behavior: Some(sqlparser::ast::DropBehavior::Cascade),
                            ..
                        } | sqlparser::ast::AlterTableOperation::DropConstraint {
                            drop_behavior: Some(sqlparser::ast::DropBehavior::Cascade),
                            ..
                        } | sqlparser::ast::AlterTableOperation::DropPrimaryKey {
                            drop_behavior: Some(sqlparser::ast::DropBehavior::Cascade),
                            ..
                        } | sqlparser::ast::AlterTableOperation::DropForeignKey {
                            drop_behavior: Some(sqlparser::ast::DropBehavior::Cascade),
                            ..
                        }
                    )
                })
            {
                return Err(invalid());
            }
            if operations.iter().any(|operation| {
                matches!(
                    operation,
                    sqlparser::ast::AlterTableOperation::RenameTable { .. }
                        | sqlparser::ast::AlterTableOperation::SwapWith { .. }
                )
            }) {
                return Err(invalid());
            }
        }
        Statement::Truncate { cascade, .. } => {
            if guard.postgres && matches!(cascade, Some(sqlparser::ast::CascadeOption::Cascade)) {
                return Err(invalid());
            }
        }
        Statement::Insert(_) | Statement::Update { .. } => {}
        _ => return Err(invalid()),
    }
    match statement.visit(&mut guard) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(invalid()),
    }
}

struct TargetGuard<'a> {
    target: &'a NamespaceTarget,
    postgres: bool,
}
impl TargetGuard<'_> {
    fn name(&self, name: &ObjectName) -> Result<(), DriverError> {
        let parts = name
            .0
            .iter()
            .map(|part| {
                part.as_ident()
                    .map(|ident| {
                        if self.postgres && ident.quote_style.is_none() {
                            ident.value.to_ascii_lowercase()
                        } else {
                            ident.value.clone()
                        }
                    })
                    .ok_or_else(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let matches = match parts.as_slice() {
            // The connection initializes search_path as chosen_schema,public;
            // current_schema only proves the first usable schema. An unqualified
            // lookup could therefore mutate a public object when the target lacks it.
            [_] => !self.postgres,
            [namespace, _] if self.postgres => self.target.schema.as_ref() == Some(namespace),
            // PostgreSQL restore accepts schema.object only. In particular,
            // overwrite's relation parser must never truncate a three-part name
            // into database.schema and DROP a different object.
            [database, _] => self.target.database.as_ref() == Some(database),
            _ => false,
        };
        if matches {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    fn column(&self, name: &ObjectName) -> Result<(), DriverError> {
        if name.0.len() < 2 {
            return Err(invalid());
        }
        self.name(&ObjectName(name.0[..name.0.len() - 1].to_vec()))
    }
}
impl Visitor for TargetGuard<'_> {
    type Break = ();
    fn pre_visit_relation(&mut self, name: &ObjectName) -> ControlFlow<()> {
        if self.name(name).is_ok() {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
}
