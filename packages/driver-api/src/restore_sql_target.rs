//! Fail-closed AST validation for the frozen restore namespace.
use crate::{namespace::NamespaceTarget, restore_sql, DriverError};
use sqlparser::{
    ast::{ObjectName, Statement, Visit, Visitor},
    dialect::{Dialect, MySqlDialect, PostgreSqlDialect},
    parser::Parser,
    tokenizer::{Token, Tokenizer},
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
    // `ALTER SEQUENCE ... OWNED BY <table>.<column>` is emitted by this
    // product's own PostgreSQL dump to defer sequence ownership until after the
    // owning table exists, and the pinned sqlparser cannot parse it at all — so
    // every backup containing a serial/identity column produced a dump that its
    // own restore refused. The shape is small and closed: verify it here and
    // keep rejecting every other `ALTER SEQUENCE` form.
    if let Some(verdict) = validate_alter_sequence_owned_by(sql, dialect, target) {
        return verdict;
    }
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

/// Verify `ALTER SEQUENCE <schema>.<sequence> OWNED BY <schema>.<table>.<column>`.
///
/// Returns `None` when the statement is not an `ALTER SEQUENCE` at all, so the
/// regular AST path keeps handling everything else. Any `ALTER SEQUENCE` form
/// other than `OWNED BY` stays rejected: it could change a sequence the frozen
/// target does not own, and the pinned parser cannot prove its shape.
fn validate_alter_sequence_owned_by(
    sql: &str,
    dialect: &dyn Dialect,
    target: &NamespaceTarget,
) -> Option<Result<(), DriverError>> {
    let tokens: Vec<Token> = Tokenizer::new(dialect, sql)
        .tokenize()
        .ok()?
        .into_iter()
        .filter(|token| !matches!(token, Token::Whitespace(_) | Token::SemiColon))
        .collect();
    let word = |token: &Token, expected: &str| matches!(token, Token::Word(w) if w.value.eq_ignore_ascii_case(expected));
    if tokens.len() < 3 || !word(&tokens[0], "ALTER") || !word(&tokens[1], "SEQUENCE") {
        return None;
    }
    let mut cursor = 2;
    let mut sequence = Vec::new();
    while cursor < tokens.len() {
        match &tokens[cursor] {
            Token::Word(w) if w.value.eq_ignore_ascii_case("OWNED") => break,
            // An identifier is a word here: sqlparser folds quoted identifiers
            // into `Word`. A single-quoted string is never an identifier in any
            // engine this gates — accepting it made this fail-closed verifier
            // looser than the engines it sits in front of.
            Token::Word(w) => sequence.push(w.value.clone()),
            Token::Period => {}
            _ => return Some(Err(invalid())),
        }
        cursor += 1;
    }
    if cursor + 2 >= tokens.len()
        || !word(&tokens[cursor], "OWNED")
        || !word(&tokens[cursor + 1], "BY")
    {
        return Some(Err(invalid()));
    }
    let mut owner = Vec::new();
    for token in &tokens[cursor + 2..] {
        match token {
            Token::Word(w) => owner.push(w.value.clone()),
            Token::Period => {}
            _ => return Some(Err(invalid())),
        }
    }
    // Both names are `schema.object` (the table additionally carries its column).
    let schema_matches = |parts: &[String]| {
        matches!(parts.len(), 2 | 3) && target.schema.as_deref() == Some(parts[0].as_str())
    };
    if sequence.len() != 2 || !schema_matches(&sequence) {
        return Some(Err(invalid()));
    }
    if owner.len() != 3 || !schema_matches(&owner) {
        return Some(Err(invalid()));
    }
    Some(Ok(()))
}
