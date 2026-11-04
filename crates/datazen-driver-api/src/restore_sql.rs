//! Opt-in contract for restoring SQL statements on a retained ResourceProvider.
//! It deliberately excludes COPY/client protocols and embedded transaction control.
use crate::{sql_dump::is_comment_only_or_empty, DriverError};

pub const CAPABILITY_KEY: &str = "retainedSqlRestoreV1";
pub const TRANSACTIONAL: &str = "transactionalDdl";
pub const IMPLICIT_COMMIT: &str = "implicitCommitDdl";

/// Preflight the entire script before any external write. The returned statements
/// use the driver's scanner, including dollar quotes and MySQL DELIMITER handling.
///
/// The scanner comes from the [`BackupDialect`](crate::backup_dialect::BackupDialect)
/// file-rule source, not from the whole driver: preflight is a pure text rule
/// and must not depend on the execution surface.
pub fn prepare(
    dialect: &dyn crate::backup_dialect::BackupDialect,
    input: &str,
    profile: &str,
    single_transaction: bool,
    overwrite: bool,
    target: &crate::namespace::NamespaceTarget,
) -> Result<Vec<String>, DriverError> {
    if !matches!(profile, TRANSACTIONAL | IMPLICIT_COMMIT)
        || (single_transaction && profile != TRANSACTIONAL)
    {
        return Err(DriverError::InvalidConfig(
            "Restore transaction mode unsupported by retained resource".into(),
        ));
    }
    let mut scanner = dialect.new_sql_scanner();
    let mut statements = scanner.push(input);
    statements.extend(scanner.finish());
    let mut bodies = Vec::new();
    for sql in statements {
        prepare_statement(&sql, profile, target, overwrite, &mut bodies)
            .map_err(|error| statement_context(&sql, error))?;
    }
    if bodies.is_empty() {
        return Err(DriverError::InvalidConfig(
            "Restore input contains no supported statements".into(),
        ));
    }
    let mut prepared = Vec::new();
    if overwrite {
        // Overwrite drops every object the script is about to create, and it does
        // so **before the first creation**. Dropping inline cannot work: the dump
        // re-creates a table's sequence before the table, and a sequence still
        // owned by the old table can only be removed once that table is gone.
        // Tables go first for exactly that reason.
        for body in &bodies {
            let words = prefix_words(body)?;
            if !words.first().is_some_and(|word| word == "CREATE") {
                continue;
            }
            let kind = words.get(1).map(String::as_str).unwrap_or_default();
            if matches!(kind, "TABLE" | "VIEW") {
                let ident = crate::sql_dump::created_relation_ident(body).ok_or_else(|| {
                    DriverError::InvalidConfig(
                        "Restore overwrite requires an explicit relation identifier".into(),
                    )
                })?;
                prepared.push(format!("DROP {kind} IF EXISTS {ident}"));
            }
        }
        for body in &bodies {
            let words = prefix_words(body)?;
            if !words.first().is_some_and(|word| word == "CREATE") {
                continue;
            }
            if words.get(1).is_some_and(|kind| kind == "SEQUENCE") {
                if let Some(ident) = created_sequence_ident(body) {
                    prepared.push(format!("DROP SEQUENCE IF EXISTS {ident}"));
                }
            }
        }
    }
    prepared.extend(bodies);
    Ok(prepared)
}

/// Validate one scanned statement and append it to the script body.
fn prepare_statement(
    sql: &str,
    profile: &str,
    target: &crate::namespace::NamespaceTarget,
    overwrite: bool,
    bodies: &mut Vec<String>,
) -> Result<(), DriverError> {
    if executable_comment(sql) {
        return Err(DriverError::InvalidConfig(
            "Executable restore comments are unsupported".into(),
        ));
    }
    if is_comment_only_or_empty(sql) {
        return Ok(());
    }
    {
        let body = leading_body(sql)?;
        let words = prefix_words(body)?;
        let first = words.first().map(String::as_str).unwrap_or_default();
        // This is a bounded restore format, not arbitrary client scripting.
        // SET (including executable MySQL comments), USE and transaction control
        // could redirect the frozen namespace or end the owned transaction.
        if !matches!(
            first,
            "CREATE" | "ALTER" | "DROP" | "INSERT" | "UPDATE" | "DELETE" | "TRUNCATE" | "COMMENT"
        ) || words.iter().any(|word| {
            matches!(
                word.as_str(),
                "DATABASE"
                    | "SCHEMA"
                    | "TABLESPACE"
                    | "SERVER"
                    | "ROLE"
                    | "USER"
                    | "SYSTEM"
                    | "EXTENSION"
                    | "PUBLICATION"
                    | "SUBSCRIPTION"
            )
        }) {
            return Err(DriverError::InvalidConfig(
                "Restore input contains unsupported SQL/client protocol or context control".into(),
            ));
        }
        // The drop preambles are assembled by `prepare` once the whole script is
        // known; validation still needs to know that this is an overwrite.
        crate::restore_sql_target::validate(sql, profile, target, overwrite)?;
        bodies.push(sql.to_owned());
    }
    Ok(())
}

/// The identifier of a `CREATE [IF NOT EXISTS] SEQUENCE <ident>` statement.
///
/// Mirrors `created_relation_ident` so both overwrite preambles read names the
/// same way: leading comment lines are skipped, keywords are stripped by the
/// same helpers, and `IF NOT EXISTS` is matched as a whole keyword. The old
/// hand-rolled scanner treated any unqualified name starting with `if`
/// (`ifog`, `if_x`) as the qualifier, and a leading comment line blinded it
/// entirely — both silently skipped the `DROP SEQUENCE` preamble, leaving the
/// old sequence standing under overwrite.
fn created_sequence_ident(sql: &str) -> Option<String> {
    let line = sql
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("--") && !l.starts_with("/*"))
        .unwrap_or("");
    let rest = crate::sql_dump::strip_kw(line, "CREATE")?;
    let rest = crate::sql_dump::strip_kw(rest, "SEQUENCE")?;
    let rest = crate::sql_dump::strip_opt_kw(rest, "IF NOT EXISTS");
    crate::sql_dump::take_qualified_sql_ident(rest)
}

#[cfg(test)]
mod sequence_ident_tests {
    use super::created_sequence_ident;

    #[test]
    fn reads_plain_and_schema_qualified_names() {
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE public.t_id_seq START WITH 2"),
            Some("public.t_id_seq".into())
        );
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE t_id_seq"),
            Some("t_id_seq".into())
        );
    }

    #[test]
    fn strips_the_if_not_exists_qualifier_as_a_whole_keyword() {
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE IF NOT EXISTS public.t_id_seq AS bigint"),
            Some("public.t_id_seq".into())
        );
    }

    #[test]
    fn a_sequence_really_named_if_something_is_a_name_not_a_qualifier() {
        // The old scanner saw `IF…` and bailed, silently skipping the
        // overwrite preamble for exactly these names.
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE ifog START WITH 1"),
            Some("ifog".into())
        );
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE \"IF NOT EXISTS\" START WITH 1"),
            Some("\"IF NOT EXISTS\"".into())
        );
    }

    #[test]
    fn quoted_names_with_spaces_and_skipped_comment_lines_parse_like_the_relation_parser() {
        assert_eq!(
            created_sequence_ident("CREATE SEQUENCE \"my seq\" START WITH 1"),
            Some("\"my seq\"".into())
        );
        assert_eq!(
            created_sequence_ident("-- emitted by the dump\nCREATE SEQUENCE public.orders_id_seq"),
            Some("public.orders_id_seq".into())
        );
    }

    #[test]
    fn other_shapes_stay_unrecognized() {
        assert_eq!(
            created_sequence_ident("CREATE TABLE public.t (id int)"),
            None
        );
        // `CREATE IF NOT EXISTS SEQUENCE s` is not a shape any engine accepts;
        // the old scanner mis-parsed it to the identifier `SEQUENCE`.
        assert_eq!(
            created_sequence_ident("CREATE IF NOT EXISTS SEQUENCE s"),
            None
        );
    }
}

/// Prefix a preflight rejection with a bounded excerpt of the statement.
fn statement_context(sql: &str, error: DriverError) -> DriverError {
    let excerpt: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let excerpt: String = excerpt.chars().take(120).collect();
    DriverError::InvalidConfig(format!(
        "Restore statement rejected ({error}); statement starts with: {excerpt}"
    ))
}

fn leading_body(mut sql: &str) -> Result<&str, DriverError> {
    loop {
        sql = sql.trim_start();
        if let Some(rest) = sql.strip_prefix("--") {
            sql = rest
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or_default();
        } else if let Some(rest) = sql.strip_prefix("/*") {
            if rest.starts_with('!') || rest.starts_with("M!") {
                return Err(DriverError::InvalidConfig(
                    "Executable restore comments are unsupported".into(),
                ));
            }
            sql = rest
                .split_once("*/")
                .map(|(_, rest)| rest)
                .ok_or_else(|| DriverError::InvalidConfig("Unterminated restore comment".into()))?;
        } else {
            return Ok(sql);
        }
    }
}

fn prefix_words(mut sql: &str) -> Result<Vec<String>, DriverError> {
    let mut words = Vec::new();
    for _ in 0..4 {
        sql = leading_body(sql)?;
        let end = sql
            .find(|ch: char| !ch.is_ascii_alphabetic() && ch != '_')
            .unwrap_or(sql.len());
        if end == 0 {
            break;
        }
        words.push(sql[..end].to_ascii_uppercase());
        sql = &sql[end..];
    }
    Ok(words)
}

fn executable_comment(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == quote {
                    index += 1;
                    if bytes.get(index) == Some(&quote) {
                        index += 1;
                    } else {
                        break;
                    }
                } else {
                    index += 1;
                }
            }
        } else if bytes[index] == b'$' {
            if let Some(end) = crate::sql_dump::find_dollar_tag(bytes, index) {
                let tag = &sql[index..end];
                if let Some(close) = sql[end..].find(tag) {
                    index = end + close + tag.len();
                } else {
                    index = bytes.len();
                }
            } else {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"--") {
            index = sql[index..]
                .find('\n')
                .map_or(bytes.len(), |end| index + end + 1);
        } else if bytes[index..].starts_with(b"/*") {
            if bytes[index..].starts_with(b"/*!") || bytes[index..].starts_with(b"/*M!") {
                return true;
            }
            index = sql[index + 2..]
                .find("*/")
                .map_or(bytes.len(), |end| index + end + 4);
        } else {
            index += 1;
        }
    }
    false
}
