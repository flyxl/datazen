//! Generic SQL dump helpers used by [`crate::DatabaseDriver`] defaults.
//!
//! Does **not** emit `CREATE DATABASE` — drivers that support that option
//! prepend their dialect preamble in `dump_database` before calling
//! [`dump_sql_database`].

use crate::traits::DatabaseDriver;
use crate::types::*;

pub use crate::sql_split::{
    find_dollar_tag, is_comment_only_or_empty, split_sql_statements, SqlStatementScanner,
    Utf8ChunkDecoder,
};

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

pub fn is_view_like(table_type: &TableType) -> bool {
    matches!(table_type, TableType::View | TableType::MaterializedView)
}

pub fn drop_object_sql(
    quote_ident: &dyn Fn(&str) -> String,
    name: &str,
    table_type: &TableType,
) -> String {
    let keyword = if is_view_like(table_type) {
        "VIEW"
    } else {
        "TABLE"
    };
    format!("DROP {keyword} IF EXISTS {};\n", quote_ident(name))
}

/// Dump a database to SQL (header, optional DROP, DDL via `dump_table_ddl`, INSERTs).
///
/// Views / materialized views emit `CREATE VIEW` (when available) and **never**
/// `INSERT INTO`. Does not emit `CREATE DATABASE`.
pub async fn dump_sql_database<D>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    dump_sql_database_with_progress(driver, handle, database, opts, |_| {}).await
}

pub async fn dump_sql_database_with_progress<D, F>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
    mut on_progress: F,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
    F: FnMut(DumpProgress),
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

    let total = tables.len() as u32;
    for (i, table) in tables.iter().enumerate() {
        let tname = &table.name;
        on_progress(DumpProgress {
            current: (i as u32) + 1,
            total,
            object_name: tname.clone(),
            phase: DumpPhase::Object,
        });
        let view_like = is_view_like(&table.table_type);
        out.push_str(&format!(
            "-- {}: {}\n",
            if view_like { "View" } else { "Table" },
            tname
        ));

        if opts.clean {
            out.push_str(&drop_object_sql(
                &|n| driver.quote_ident(n),
                tname,
                &table.table_type,
            ));
        }

        if !opts.data_only {
            let ddl = if view_like {
                match driver.dump_view_ddl(handle, tname).await {
                    Ok(sql) => sql,
                    Err(e) => format!("-- View {tname}: skipped DDL ({e})\n"),
                }
            } else {
                driver.dump_table_ddl(handle, tname).await?
            };
            out.push_str(&ddl);
            if !ddl.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }

        if view_like {
            continue;
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
                        let vals: Vec<String> =
                            row.iter().map(|v| driver.format_sql_literal(v)).collect();
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

    if !opts.data_only && opts.routines {
        on_progress(DumpProgress {
            current: total.saturating_add(1),
            total: total.saturating_add(1),
            object_name: "routines".into(),
            phase: DumpPhase::Object,
        });
        let routines = driver.dump_routines(handle, database).await?;
        if !routines.trim().is_empty() {
            out.push_str("-- Routines (functions / procedures)\n");
            out.push_str(&routines);
            if !routines.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
    }

    if !opts.data_only && opts.triggers {
        on_progress(DumpProgress {
            current: total.saturating_add(2),
            total: total.saturating_add(2),
            object_name: "triggers".into(),
            phase: DumpPhase::Object,
        });
        let triggers = driver.dump_triggers(handle, database).await?;
        if !triggers.trim().is_empty() {
            out.push_str("-- Triggers\n");
            out.push_str(&triggers);
            if !triggers.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
    }

    Ok(out)
}

/// Parse `-- Options:` header line from a DataZen dump for `single-transaction`.
pub fn dump_header_requests_single_transaction(sql: &str) -> bool {
    for line in sql.lines().take(20) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("-- Options:") {
            return rest
                .split(',')
                .any(|part| part.trim() == "single-transaction");
        }
    }
    false
}

/// Short preview of a SQL statement for restore progress UI.
pub fn restore_statement_label(stmt: &str) -> String {
    let line = stmt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("--") && !l.starts_with("/*"))
        .unwrap_or_else(|| stmt.trim());
    const MAX: usize = 200;
    if line.chars().count() <= MAX {
        line.to_string()
    } else {
        let truncated: String = line.chars().take(MAX).collect();
        format!("{truncated}…")
    }
}

/// Default restore pipeline: incremental split + execute.
///
/// Host feeds file chunks via [`RestoreSession::feed`]; drivers that want a
/// different splitter pass their own [`SqlStatementScanner`]. Override
/// [`DatabaseDriver::restore_sql_with_progress`] to replace the whole pipeline.
pub struct RestoreSession<'a, D: DatabaseDriver + ?Sized> {
    driver: &'a D,
    handle: &'a ConnectionHandle,
    scanner: SqlStatementScanner,
    header: String,
    header_done: bool,
    use_tx: bool,
    tx_open: bool,
    executed: u32,
    errors: Vec<String>,
}

impl<'a, D: DatabaseDriver + ?Sized> RestoreSession<'a, D> {
    pub fn new(
        driver: &'a D,
        handle: &'a ConnectionHandle,
        scanner: SqlStatementScanner,
        opts: Option<&BackupRestoreOptions>,
    ) -> Self {
        Self {
            driver,
            handle,
            scanner,
            header: String::new(),
            header_done: false,
            use_tx: opts.map(|o| o.single_transaction).unwrap_or(false),
            tx_open: false,
            executed: 0,
            errors: Vec::new(),
        }
    }

    pub async fn feed(
        &mut self,
        chunk: &str,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        self.note_header(chunk);
        let stmts = self.scanner.push(chunk);
        self.exec_all(stmts, on_progress).await
    }

    pub async fn finish(
        &mut self,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        if !self.header_done {
            self.use_tx = self.use_tx || dump_header_requests_single_transaction(&self.header);
            self.header_done = true;
            self.header.clear();
        }
        let stmts = self.scanner.finish();
        self.exec_all(stmts, on_progress).await?;
        if self.tx_open {
            self.driver.execute(self.handle, "COMMIT").await?;
            self.tx_open = false;
        }
        if !self.errors.is_empty() {
            return Err(DriverError::QueryFailed(format!(
                "Partial restore failure ({}/{} statements failed):\n{}",
                self.errors.len(),
                self.executed,
                self.errors.join("\n")
            )));
        }
        on_progress(DumpProgress {
            current: self.executed,
            total: self.executed,
            object_name: String::new(),
            phase: DumpPhase::Done,
        });
        Ok(())
    }

    fn note_header(&mut self, chunk: &str) {
        if self.header_done {
            return;
        }
        self.header.push_str(chunk);
        if self.header.lines().count() >= 20 || chunk.is_empty() {
            self.use_tx = self.use_tx || dump_header_requests_single_transaction(&self.header);
            self.header_done = true;
            self.header.clear();
        }
    }

    async fn exec_all(
        &mut self,
        stmts: Vec<String>,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        for stmt in stmts {
            if is_comment_only_or_empty(&stmt) {
                continue;
            }
            if self.use_tx && !self.tx_open {
                self.driver.execute(self.handle, "BEGIN").await?;
                self.tx_open = true;
            }
            self.executed += 1;
            on_progress(DumpProgress {
                current: self.executed,
                total: 0,
                object_name: restore_statement_label(&stmt),
                phase: DumpPhase::Object,
            });
            let full = format!("{};", stmt);
            if let Err(e) = self.driver.execute(self.handle, &full).await {
                if self.tx_open {
                    let _ = self.driver.execute(self.handle, "ROLLBACK").await;
                    self.tx_open = false;
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
                self.errors
                    .push(format!("Error executing: {}... -> {e}", &stmt[..end]));
                if self.use_tx {
                    break;
                }
            }
        }
        Ok(())
    }
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
    restore_sql_statements_with_progress(driver, handle, sql, opts, |_| {}).await
}

/// Same as [`restore_sql_statements`] with per-statement progress callbacks.
pub async fn restore_sql_statements_with_progress<D, F>(
    driver: &D,
    handle: &ConnectionHandle,
    sql: &str,
    opts: Option<&BackupRestoreOptions>,
    mut on_progress: F,
) -> Result<(), DriverError>
where
    D: DatabaseDriver + ?Sized,
    F: FnMut(DumpProgress) + Send,
{
    let mut session = RestoreSession::new(driver, handle, driver.new_sql_scanner(), opts);
    session.feed(sql, &mut on_progress).await?;
    session.finish(&mut on_progress).await
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
    fn dump_header_single_transaction_flag() {
        let sql = "-- DataZen backup: app\n-- Options: clean, single-transaction\n";
        assert!(dump_header_requests_single_transaction(sql));
        assert!(!dump_header_requests_single_transaction(
            "-- Options: clean\n"
        ));
    }

    #[test]
    fn view_like_skips_insert_and_uses_drop_view() {
        assert!(is_view_like(&TableType::View));
        assert!(is_view_like(&TableType::MaterializedView));
        assert!(!is_view_like(&TableType::Table));
        assert_eq!(
            drop_object_sql(&|n| format!("\"{n}\""), "active_users", &TableType::View),
            "DROP VIEW IF EXISTS \"active_users\";\n"
        );
        assert_eq!(
            drop_object_sql(&|n| format!("\"{n}\""), "users", &TableType::Table),
            "DROP TABLE IF EXISTS \"users\";\n"
        );
    }

    #[test]
    fn restore_statement_label_truncates_and_skips_comments() {
        assert_eq!(
            restore_statement_label("CREATE TABLE users (id int)"),
            "CREATE TABLE users (id int)"
        );
        assert_eq!(
            restore_statement_label("-- comment\nINSERT INTO t VALUES (1)"),
            "INSERT INTO t VALUES (1)"
        );
        let long = format!("INSERT INTO t VALUES ({})", "x".repeat(250));
        let label = restore_statement_label(&long);
        assert!(label.ends_with('…'));
        assert!(label.chars().count() <= 201);
    }
}
