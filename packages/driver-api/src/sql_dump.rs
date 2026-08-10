//! Generic SQL dump helpers used by [`crate::DatabaseDriver`] defaults.
//!
//! Does **not** emit `CREATE DATABASE` — drivers that support that option
//! prepend their dialect preamble in `dump_database` before calling
//! [`dump_sql_database`].

use crate::traits::DatabaseDriver;
use crate::types::*;

/// Build `CREATE TABLE IF NOT EXISTS` DDL from a table schema (host-compatible).
pub fn build_create_table_sql(
    quote_ident: &dyn Fn(&str) -> String,
    schema: &TableSchema,
) -> String {
    let tname = &schema.table_name;
    let cols_sql: Vec<String> = schema
        .columns
        .iter()
        .map(|c| {
            let mut def = format!("  {} {}", quote_ident(&c.name), c.data_type);
            if !c.nullable {
                def.push_str(" NOT NULL");
            }
            if let Some(ref dv) = c.default_value {
                def.push_str(&format!(" DEFAULT {}", dv));
            }
            def
        })
        .collect();

    let mut create = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}",
        quote_ident(tname),
        cols_sql.join(",\n")
    );
    if !schema.primary_keys.is_empty() {
        let pks: Vec<String> = schema.primary_keys.iter().map(|k| quote_ident(k)).collect();
        create.push_str(&format!(",\n  PRIMARY KEY ({})", pks.join(", ")));
    }
    create.push_str("\n);\n");
    create
}

/// Default `dump_table_ddl`: load schema then build CREATE TABLE.
pub async fn dump_table_ddl_from_schema<D>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let schema = driver.get_table_schema(handle, table).await?;
    Ok(build_create_table_sql(&|n| driver.quote_ident(n), &schema))
}

/// Dump a database to SQL (header, optional DROP, DDL via `dump_table_ddl`, INSERTs).
///
/// Does not emit `CREATE DATABASE`.
pub async fn dump_sql_database<D>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let tables = driver.get_tables(handle, database).await?;

    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {}\n", database));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    let mut opt_flags = Vec::new();
    if opts.schema_only {
        opt_flags.push("schema-only");
    }
    if opts.data_only {
        opt_flags.push("data-only");
    }
    if opts.clean {
        opt_flags.push("clean");
    }
    if opts.create_database {
        opt_flags.push("create");
    }
    if opts.no_owner {
        opt_flags.push("no-owner");
    }
    if opts.single_transaction {
        opt_flags.push("single-transaction");
    }
    if opts.routines {
        opt_flags.push("routines");
    }
    if opts.triggers {
        opt_flags.push("triggers");
    }
    if !opt_flags.is_empty() {
        out.push_str(&format!("-- Options: {}\n", opt_flags.join(", ")));
    }
    if opts.no_owner {
        out.push_str("-- no-owner: OWNER clauses omitted\n");
    }
    out.push('\n');

    for table in &tables {
        let tname = &table.name;
        out.push_str(&format!("-- Table: {}\n", tname));

        if opts.clean {
            out.push_str(&format!(
                "DROP TABLE IF EXISTS {};\n",
                driver.quote_ident(tname)
            ));
        }

        if !opts.data_only {
            let ddl = driver.dump_table_ddl(handle, tname).await?;
            out.push_str(&ddl);
            if !ddl.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }

        if !opts.schema_only {
            let schema = driver.get_table_schema(handle, tname).await?;
            let col_names: Vec<String> = schema
                .columns
                .iter()
                .map(|c| driver.quote_ident(&c.name))
                .collect();
            let select_sql = format!(
                "SELECT {} FROM {}",
                col_names.join(", "),
                driver.quote_ident(tname)
            );

            match driver.query(handle, &select_sql).await {
                Ok(result) => {
                    for row in &result.rows {
                        let vals: Vec<String> = row
                            .iter()
                            .map(|v| driver.format_sql_literal(v))
                            .collect();
                        out.push_str(&format!(
                            "INSERT INTO {} ({}) VALUES ({});\n",
                            driver.quote_ident(tname),
                            col_names.join(", "),
                            vals.join(", ")
                        ));
                    }
                    out.push('\n');
                }
                Err(e) => {
                    out.push_str(&format!("-- Error dumping data for {tname}: {e}\n\n"));
                }
            }
        }
    }

    Ok(out)
}

/// Split SQL text into individual statements, respecting single-quoted strings,
/// double-quoted identifiers, dollar-quoted strings, `--` line comments, and
/// `/* */` block comments so semicolons inside those constructs are not treated
/// as statement terminators.
pub fn split_sql_statements(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut stmts: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1; // escaped ''
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'"' {
                        i += 1;
                        if i < len && bytes[i] == b'"' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'$' => {
                if let Some(tag_end) = find_dollar_tag(bytes, i) {
                    let tag = &input[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len {
                            break;
                        }
                        if bytes[i] == b'$' && input[i..].starts_with(tag) {
                            i += tag.len();
                            break;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                let mut depth = 1u32;
                while i + 1 < len && depth > 0 {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b';' => {
                let fragment = input[start..i].trim();
                if !fragment.is_empty() {
                    stmts.push(fragment.to_string());
                }
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let tail = input[start..].trim();
    if !tail.is_empty() {
        stmts.push(tail.to_string());
    }
    stmts
}

/// Try to match a `$tag$` dollar-quote opener starting at position `pos`.
/// Returns `Some(end)` where `end` is the byte index past the closing `$`.
pub fn find_dollar_tag(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() || bytes[pos] != b'$' {
        return None;
    }
    let mut j = pos + 1;
    while j < bytes.len() {
        if bytes[j] == b'$' {
            return Some(j + 1);
        }
        if !bytes[j].is_ascii_alphanumeric() && bytes[j] != b'_' {
            return None;
        }
        j += 1;
    }
    None
}

/// Returns true when `stmt` is empty or contains only SQL comments / whitespace.
fn is_comment_only_or_empty(stmt: &str) -> bool {
    let bytes = stmt.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            return true;
        }
        if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        return false;
    }

    true
}

/// Parse `-- Options:` header line from a DataZen dump for `single-transaction`.
pub fn dump_header_requests_single_transaction(sql: &str) -> bool {
    for line in sql.lines().take(20) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("-- Options:") {
            return rest.split(',').any(|part| part.trim() == "single-transaction");
        }
    }
    false
}

/// Default restore: split statements intelligently, execute each non-empty one.
pub async fn restore_sql_statements<D>(
    driver: &D,
    handle: &ConnectionHandle,
    sql: &str,
    opts: Option<&BackupRestoreOptions>,
) -> Result<(), DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let statements: Vec<String> = split_sql_statements(sql)
        .into_iter()
        .filter(|s| !is_comment_only_or_empty(s))
        .collect();

    let use_tx = opts
        .map(|o| o.single_transaction)
        .unwrap_or(false)
        || dump_header_requests_single_transaction(sql);

    if use_tx {
        driver.execute(handle, "BEGIN").await?;
    }

    let mut errors = Vec::new();
    for stmt in &statements {
        let full = format!("{};", stmt);
        if let Err(e) = driver.execute(handle, &full).await {
            if use_tx {
                let _ = driver.execute(handle, "ROLLBACK").await;
            }
            let max = 80;
            let end = if stmt.len() <= max {
                stmt.len()
            } else {
                let mut e = max;
                while e > 0 && !stmt.is_char_boundary(e) {
                    e -= 1;
                }
                e
            };
            errors.push(format!("Error executing: {}... -> {e}", &stmt[..end]));
        }
    }

    if !errors.is_empty() {
        return Err(DriverError::QueryFailed(format!(
            "Partial restore failure ({}/{} statements failed):\n{}",
            errors.len(),
            statements.len(),
            errors.join("\n")
        )));
    }

    if use_tx {
        driver.execute(handle, "COMMIT").await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_create_table_sql_includes_pk_and_not_null() {
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "name".into(),
                    data_type: "text".into(),
                    nullable: true,
                    default_value: Some("'anon'".into()),
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
        };
        let sql = build_create_table_sql(&|n| format!("\"{}\"", n), &schema);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"users\""));
        assert!(sql.contains("\"id\" integer NOT NULL"));
        assert!(sql.contains("DEFAULT 'anon'"));
        assert!(sql.contains("PRIMARY KEY (\"id\")"));
        assert!(sql.ends_with(";\n"));
    }

    #[test]
    fn split_sql_respects_semicolon_in_single_quotes() {
        let stmts = split_sql_statements("SELECT 'a;b'; SELECT 1;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 'a;b'");
        assert_eq!(stmts[1], "SELECT 1");
    }

    #[test]
    fn split_sql_respects_dollar_quoted_body_with_semicolon() {
        let stmts = split_sql_statements("SELECT $$foo;bar$$; SELECT 1;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT $$foo;bar$$");
        assert_eq!(stmts[1], "SELECT 1");
    }

    #[test]
    fn split_sql_respects_line_comment_with_semicolon() {
        let stmts = split_sql_statements("SELECT 1; -- trailing; comment\nSELECT 2;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "-- trailing; comment\nSELECT 2");
    }

    #[test]
    fn split_sql_respects_block_comment_with_semicolon() {
        let stmts = split_sql_statements("SELECT 1; /* block; comment */ SELECT 2;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "/* block; comment */ SELECT 2");
    }

    #[test]
    fn is_comment_only_or_empty_detects_comment_statements() {
        assert!(is_comment_only_or_empty("-- only a comment"));
        assert!(is_comment_only_or_empty("/* block */"));
        assert!(!is_comment_only_or_empty("SELECT 1"));
    }

    #[test]
    fn dump_header_single_transaction_flag() {
        let sql = "-- DataZen backup: app\n-- Options: clean, single-transaction\n";
        assert!(dump_header_requests_single_transaction(sql));
        assert!(!dump_header_requests_single_transaction("-- Options: clean\n"));
    }
}
