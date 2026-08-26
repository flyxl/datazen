//! F7: dialect-aware SQL target qualification (AST-level rewrite).
//!
//! Every SQL command can carry targeting information (MySQL family:
//! database; PG family: database + schema). The **driver** rewrites
//! unqualified table references into dialect-qualified names — e.g.
//! `select * from users` → `select * from \`mydb\`.\`users\`` for MySQL —
//! so the statement lands on the caller-selected target without any
//! session-level `USE`/switch. The host stays dialect-agnostic; legacy
//! drivers without the [`crate::DatabaseDriver::qualify_sql_target`]
//! override keep executing SQL as-is and rely on the host
//! `ensure_session_database` pin as fallback.
//!
//! Rewrite rules (shared by every dialect binding):
//! - Only *targeting contexts* are touched: table references reached through
//!   FROM / JOIN / INSERT INTO / UPDATE / DELETE FROM / TRUNCATE /
//!   CREATE TABLE / ALTER TABLE / CREATE INDEX ON (sqlparser dispatches these
//!   as "relations"), plus DROP TABLE handled explicitly (its name list is not
//!   visitor-annotated upstream).
//! - Skipped: already multi-part (qualified) references, CTE names, derived
//!   table aliases (they carry no `ObjectName`, so they are never visited),
//!   string literals (values, never relations).
//! - Idempotent: a second pass sees qualified names and changes nothing.
//! - Parse failure passes the original SQL through unchanged (callers log).

use std::collections::HashSet;
use std::ops::ControlFlow;

use sqlparser::ast::{
    ObjectName, ObjectNamePart, ObjectType, Query, Statement, VisitMut, VisitorMut,
};
use sqlparser::dialect::Dialect;
use sqlparser::parser::Parser;

/// Targeting context carried alongside one SQL command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SqlTarget<'a> {
    /// Logical database (MySQL family: `` `db`.t `` inline; PG family: pool
    /// switch dimension — inlining is only valid per dialect).
    pub database: Option<&'a str>,
    /// Schema (PG family: `"schema"."t"` inline). Ignored by dialects that
    /// have no schema dimension (MySQL family, ClickHouse).
    pub schema: Option<&'a str>,
}

impl<'a> SqlTarget<'a> {
    fn nonblank(value: Option<&'a str>) -> Option<&'a str> {
        value.map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn database(&self) -> Option<&'a str> {
        Self::nonblank(self.database)
    }

    pub fn schema(&self) -> Option<&'a str> {
        Self::nonblank(self.schema)
    }

    /// True when at least one usable qualifier part is present.
    pub fn is_present(&self) -> bool {
        self.database().is_some() || self.schema().is_some()
    }
}

/// Result of a qualification attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualifiedSql {
    /// SQL to execute: rewritten when qualification applied, otherwise the
    /// original text untouched (parse failure / no-op target).
    pub sql: String,
    /// Whether the AST was actually rewritten.
    pub rewritten: bool,
}

/// Quote style used for the injected qualifier identifiers.
///
/// The original table identifier keeps its own source form (quoting and case);
/// only the *newly injected* parts are quoted with this char.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualifierQuote {
    Backtick,
    DoubleQuote,
    Bracket,
}

impl QualifierQuote {
    fn quote_char(self) -> char {
        match self {
            QualifierQuote::Backtick => '`',
            QualifierQuote::DoubleQuote => '"',
            // sqlparser renders bracket idents verbatim inside [], so escape
            // a closing bracket ourselves (T-SQL doubling).
            QualifierQuote::Bracket => '[',
        }
    }

    /// Quote char recorded onto the rewritten table identifier. For brackets
    /// sqlparser expects `quote_style = '['`.
    fn table_quote_char(self) -> char {
        self.quote_char()
    }

    fn ident(self, value: &str) -> sqlparser::ast::Ident {
        let value = if self == QualifierQuote::Bracket {
            value.replace(']', "]]")
        } else {
            value.to_string()
        };
        sqlparser::ast::Ident::with_quote(self.quote_char(), value)
    }
}

struct RelationQualifier {
    /// Qualifier identifiers to prepend (already quoted).
    prefix: Vec<sqlparser::ast::Ident>,
    /// Quote style applied to the rewritten table identifier itself.
    table_quote: char,
    cte_names: HashSet<String>,
    changed: bool,
}

impl RelationQualifier {
    /// CTE names are collected globally instead of per-scope on purpose: an
    /// unqualified reference shadowed by an outer CTE resolves to that CTE,
    /// so skipping it is always the safe direction (a missed rewrite never
    /// corrupts semantics, a wrong rewrite would).
    fn collect_cte_names(&mut self, query: &Query) {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.cte_names.insert(cte.alias.name.value.clone());
            }
        }
    }

    fn qualify_relation(&mut self, relation: &mut ObjectName) {
        // Only bare single-part references are rewritten; anything already
        // qualified keeps its explicit resolution (idempotency).
        if relation.0.len() != 1 {
            return;
        }
        let Some(ident) = relation.0[0].as_ident() else {
            return;
        };
        if self.cte_names.contains(&ident.value) {
            return;
        }
        // T-SQL temp tables (#temp) must never leave tempdb scope.
        if ident.value.starts_with('#') {
            return;
        }

        // Quote the table identifier in the dialect's style so the output
        // matches the canonical qualified shape (`db`.`t` / "schema"."t") —
        // except when quoting would change name resolution: an unquoted
        // mixed-case identifier folds to lowercase on engines like Postgres,
        // and forcing quotes would break that, so it stays as written.
        let mut table_ident = ident.clone();
        if table_ident.quote_style.is_none()
            && table_ident.value == table_ident.value.to_lowercase()
        {
            table_ident.quote_style = Some(self.table_quote);
        }

        let mut parts: Vec<ObjectNamePart> = self
            .prefix
            .iter()
            .map(|i| ObjectNamePart::Identifier(i.clone()))
            .collect();
        parts.push(ObjectNamePart::Identifier(table_ident));
        *relation = ObjectName(parts);
        self.changed = true;
    }
}

impl VisitorMut for RelationQualifier {
    type Break = std::convert::Infallible;

    fn pre_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        self.collect_cte_names(query);
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &mut ObjectName) -> ControlFlow<Self::Break> {
        self.qualify_relation(relation);
        ControlFlow::Continue(())
    }
}

fn serialize(statements: &[Statement], original_ends_with_semicolon: bool) -> String {
    let mut out = String::new();
    for (idx, statement) in statements.iter().enumerate() {
        if idx > 0 {
            out.push_str(";\n");
        }
        out.push_str(&statement.to_string());
    }
    if original_ends_with_semicolon && !out.trim_end().ends_with(';') {
        out.push(';');
    }
    out
}

/// Rewrite unqualified table references in `sql` to be qualified by the given
/// target parts, using `dialect` for parsing/serialization and `quote` for the
/// injected qualifiers.
///
/// `prefix_parts` are the pre-built qualifier identifiers in display order
/// (e.g. `[db]`, `[schema]`) after dialect mapping — see the per-dialect
/// bindings in the driver crates. With an empty prefix this returns the input
/// unchanged. Parse failure returns the input unchanged (`rewritten == false`);
/// callers log the parse error via [`QualifiedSql::rewritten`] being false.
pub fn qualify_sql_with(
    dialect: &dyn Dialect,
    quote: QualifierQuote,
    prefix_parts: &[&str],
    sql: &str,
) -> QualifiedSql {
    if prefix_parts.is_empty() || sql.trim().is_empty() {
        return QualifiedSql {
            sql: sql.to_string(),
            rewritten: false,
        };
    }

    let statements = match Parser::parse_sql(dialect, sql) {
        Ok(statements) => statements,
        Err(error) => {
            tracing::warn!(
                error = %error,
                sql_len = sql.len(),
                "SQL target qualification skipped: statement failed to parse; executing as-is"
            );
            return QualifiedSql {
                sql: sql.to_string(),
                rewritten: false,
            };
        }
    };

    let ends_with_semicolon = sql.trim_end().ends_with(';');
    let prefix: Vec<sqlparser::ast::Ident> =
        prefix_parts.iter().map(|part| quote.ident(part)).collect();
    let mut qualifier = RelationQualifier {
        prefix,
        table_quote: quote.table_quote_char(),
        cte_names: HashSet::new(),
        changed: false,
    };
    let mut statements = statements;
    // DROP TABLE names are plain `Vec<ObjectName>` upstream (not annotated as
    // visitor relations), so qualify them explicitly here.
    for statement in &mut statements {
        if let Statement::Drop {
            object_type: ObjectType::Table,
            names,
            ..
        } = statement
        {
            for name in names.iter_mut() {
                qualifier.qualify_relation(name);
            }
        }
    }
    let _ = statements.visit(&mut qualifier);

    QualifiedSql {
        sql: serialize(&statements, ends_with_semicolon),
        rewritten: qualifier.changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::{GenericDialect, MySqlDialect, PostgreSqlDialect};

    fn qualify(dialect: &dyn Dialect, quote: QualifierQuote, prefix: &[&str], sql: &str) -> String {
        qualify_sql_with(dialect, quote, prefix, sql).sql
    }

    #[test]
    fn rewrites_from_join_and_subqueries() {
        let out = qualify(
            &MySqlDialect {},
            QualifierQuote::Backtick,
            &["mydb"],
            "SELECT * FROM users u JOIN orders o ON o.uid = u.id WHERE EXISTS (SELECT 1 FROM audit)",
        );
        assert!(out.contains("FROM `mydb`.`users` AS u"), "{out}");
        assert!(out.contains("JOIN `mydb`.`orders` AS o"), "{out}");
        assert!(out.contains("FROM `mydb`.`audit`"), "{out}");
    }

    #[test]
    fn skips_cte_names_and_already_qualified() {
        let out = qualify(
            &PostgreSqlDialect {},
            QualifierQuote::DoubleQuote,
            &["app"],
            "WITH t AS (SELECT 1) SELECT * FROM t JOIN public.real r ON true",
        );
        // CTE name untouched; already-qualified reference untouched.
        assert!(out.contains("FROM t"), "{out}");
        assert!(out.contains("public.real"), "{out}");
        assert!(!out.contains("\"app\""), "{out}");
    }

    #[test]
    fn covers_insert_update_delete_ddl_and_index() {
        for (sql, expect) in [
            ("INSERT INTO users (id) VALUES (1)", "INTO \"s\".\"users\""),
            ("UPDATE users SET a = 1", "UPDATE \"s\".\"users\""),
            ("DELETE FROM users", "DELETE FROM \"s\".\"users\""),
            ("TRUNCATE TABLE users", "TRUNCATE TABLE \"s\".\"users\""),
            (
                "CREATE TABLE users (id int)",
                "CREATE TABLE \"s\".\"users\"",
            ),
            (
                "ALTER TABLE users ADD COLUMN c int",
                "ALTER TABLE \"s\".\"users\"",
            ),
            ("CREATE INDEX ix ON users (id)", "ON \"s\".\"users\""),
            ("DROP TABLE users", "DROP TABLE \"s\".\"users\""),
        ] {
            let out = qualify(
                &PostgreSqlDialect {},
                QualifierQuote::DoubleQuote,
                &["s"],
                sql,
            );
            assert!(out.contains(expect), "{sql} -> {out}");
        }
    }

    #[test]
    fn is_idempotent() {
        let dialect = MySqlDialect {};
        let once = qualify(
            &dialect,
            QualifierQuote::Backtick,
            &["db"],
            "select * from users where name = 'x'",
        );
        let twice = qualify(&dialect, QualifierQuote::Backtick, &["db"], &once);
        assert_eq!(once, twice);
        assert!(!qualify_sql_with(&dialect, QualifierQuote::Backtick, &["db"], &once).rewritten);
    }

    #[test]
    fn parse_failure_passes_through_unchanged() {
        let broken = "SELEC * FORM users";
        let outcome = qualify_sql_with(
            &GenericDialect {},
            QualifierQuote::DoubleQuote,
            &["db"],
            broken,
        );
        assert_eq!(outcome.sql, broken);
        assert!(!outcome.rewritten);
    }

    #[test]
    fn empty_target_is_noop_without_parsing() {
        let outcome = qualify_sql_with(
            &GenericDialect {},
            QualifierQuote::DoubleQuote,
            &[],
            "SELECT",
        );
        assert_eq!(outcome.sql, "SELECT");
        assert!(!outcome.rewritten);
    }

    #[test]
    fn string_literals_and_multi_statement_are_preserved() {
        let out = qualify(
            &MySqlDialect {},
            QualifierQuote::Backtick,
            &["db"],
            "SELECT 'users' FROM t1; DELETE FROM t2;",
        );
        assert!(out.contains("'users'"), "{out}");
        assert!(out.contains("`db`.`t1`"), "{out}");
        assert!(out.contains("`db`.`t2`"), "{out}");
    }

    #[test]
    fn mssql_temp_tables_are_skipped() {
        let out = qualify(
            &sqlparser::dialect::MsSqlDialect {},
            QualifierQuote::Bracket,
            &["db", "dbo"],
            "SELECT * FROM #tmp JOIN real_table r ON 1 = 1",
        );
        assert!(out.contains("#tmp"), "{out}");
        assert!(out.contains("[db].[dbo].[real_table]"), "{out}");
    }

    #[test]
    fn bracket_quoting_escapes_closing_bracket() {
        let quote = QualifierQuote::Bracket;
        assert_eq!(quote.ident("we]ird").to_string(), "[we]]ird]");
        assert_eq!(quote.ident("plain").to_string(), "[plain]");
    }
}
