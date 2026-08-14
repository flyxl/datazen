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

/// Tables first, then views — restore must create base relations before
/// `CREATE VIEW ... AS SELECT FROM <table>`.
pub fn partition_dump_objects(tables: Vec<TableInfo>) -> (Vec<TableInfo>, Vec<TableInfo>) {
    let mut base = Vec::new();
    let mut views = Vec::new();
    for table in tables {
        if is_view_like(&table.table_type) {
            views.push(table);
        } else {
            base.push(table);
        }
    }
    (base, views)
}

/// `schema.name` when `schema` is a real grouping label; otherwise just `name`.
pub fn qualified_ident(
    quote_ident: &dyn Fn(&str) -> String,
    schema: Option<&str>,
    name: &str,
) -> String {
    match schema
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "CATALOG" && *s != "SCHEMA")
    {
        Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(name)),
        None => quote_ident(name),
    }
}

pub fn drop_object_sql(
    quote_ident: &dyn Fn(&str) -> String,
    name: &str,
    table_type: &TableType,
    schema: Option<&str>,
) -> String {
    let keyword = if is_view_like(table_type) {
        "VIEW"
    } else {
        "TABLE"
    };
    format!(
        "DROP {keyword} IF EXISTS {};\n",
        qualified_ident(quote_ident, schema, name)
    )
}

/// Relation name from `CREATE TABLE` / `CREATE VIEW` (quoted, maybe schema-qualified).
pub fn created_relation_ident(stmt: &str) -> Option<String> {
    let line = stmt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("--") && !l.starts_with("/*"))
        .unwrap_or("");
    let rest = strip_kw(line, "CREATE")?;
    let rest = strip_opt_kw(rest, "OR REPLACE");
    let rest = if let Some(after) = strip_kw(rest, "MATERIALIZED") {
        strip_kw(after, "VIEW")?
    } else if let Some(after) = strip_kw(rest, "TABLE") {
        after
    } else {
        strip_kw(rest, "VIEW")?
    };
    let rest = strip_opt_kw(rest, "IF NOT EXISTS");
    take_qualified_sql_ident(rest)
}

fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < kw.len() || !s[..kw.len()].eq_ignore_ascii_case(kw) {
        return None;
    }
    Some(s[kw.len()..].trim_start())
}

fn strip_opt_kw<'a>(s: &'a str, kw: &str) -> &'a str {
    strip_kw(s, kw).unwrap_or(s)
}

fn take_sql_ident(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    if bytes.first().copied() == Some(b'"') || bytes.first().copied() == Some(b'`') {
        let q = bytes[0];
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == q {
                if i + 1 < bytes.len() && bytes[i + 1] == q {
                    i += 2;
                    continue;
                }
                return Some((&s[..=i], &s[i + 1..]));
            }
            i += 1;
        }
        return None;
    }
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((&s[..end], &s[end..]))
}

fn take_qualified_sql_ident(s: &str) -> Option<String> {
    let (first, rest) = take_sql_ident(s)?;
    let rest = rest.trim_start();
    if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, _) = take_sql_ident(after_dot)?;
        Some(format!("{first}.{second}"))
    } else {
        Some(first.to_string())
    }
}

fn relation_already_exists(error: &str) -> bool {
    let msg = error.to_lowercase();
    msg.contains("already exists")
        || msg.contains("duplicate")
        || msg.contains("1050")
        || msg.contains("42p07")
}

/// Sequence names from `nextval('categories_id_seq'::regclass)` defaults.
pub fn extract_nextval_sequence_names(sql: &str) -> Vec<String> {
    let mut names = Vec::new();
    let lower = sql.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("nextval") {
        let abs = from + rel + 7;
        let after = sql[abs..].trim_start();
        from = abs;
        let Some(rest) = after.strip_prefix('(') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(name) = take_sql_string_literal(rest) else {
            continue;
        };
        if !names.iter().any(|n| n == &name) {
            names.push(name);
        }
    }
    names
}

fn take_sql_string_literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('\'') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    let mut out = String::new();
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                out.push('\'');
                i += 2;
                continue;
            }
            return Some(out);
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    None
}

/// Quote a dump/restore sequence ident (`categories_id_seq` or `public.foo_seq`).
pub fn quote_sequence_ident(name: &str) -> String {
    name.split('.')
        .map(|part| {
            let part = part.trim().trim_matches('"');
            format!("\"{}\"", part.replace('"', "\"\""))
        })
        .collect::<Vec<_>>()
        .join(".")
}

async fn dump_one_object<D, F>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &TableInfo,
    opts: &BackupDumpOptions,
    current: u32,
    total: u32,
    out: &mut String,
    on_progress: &mut F,
) -> Result<(), DriverError>
where
    D: DatabaseDriver + ?Sized,
    F: FnMut(DumpProgress),
{
    let tname = &table.name;
    on_progress(DumpProgress {
        current,
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

    if view_like || opts.schema_only {
        return Ok(());
    }

    let schema = driver.get_table_schema(handle, tname).await?;
    let col_names: Vec<String> = schema
        .columns
        .iter()
        .map(|c| driver.quote_ident(&c.name))
        .collect();
    let rel = qualified_ident(&|n| driver.quote_ident(n), table.schema.as_deref(), tname);
    let select_sql = format!("SELECT {} FROM {}", col_names.join(", "), rel);

    match driver.query(handle, &select_sql).await {
        Ok(result) => {
            let tuples: Vec<String> = result
                .rows
                .iter()
                .map(|row| {
                    let vals: Vec<String> =
                        row.iter().map(|v| driver.format_sql_literal(v)).collect();
                    format!("({})", vals.join(", "))
                })
                .collect();
            append_batched_inserts(
                out,
                &rel,
                &col_names.join(", "),
                &tuples,
                INSERT_BATCH_MAX_ROWS,
                INSERT_BATCH_MAX_BYTES,
            );
            if !tuples.is_empty() {
                out.push('\n');
            }
        }
        Err(e) => {
            out.push_str(&format!("-- Error dumping data for {tname}: {e}\n\n"));
        }
    }
    Ok(())
}

/// Rows per multi-value `INSERT` (mysqldump-style extended insert).
pub const INSERT_BATCH_MAX_ROWS: usize = 250;
/// Flush a batch before the statement grows past this (packet / parser safety).
pub const INSERT_BATCH_MAX_BYTES: usize = 512 * 1024;

/// `INSERT INTO rel (cols) VALUES (...), (...);` in row/size-limited batches.
pub fn append_batched_inserts(
    out: &mut String,
    rel: &str,
    columns_sql: &str,
    value_tuples: &[String],
    max_rows: usize,
    max_bytes: usize,
) {
    if value_tuples.is_empty() {
        return;
    }
    let prefix = format!("INSERT INTO {rel} ({columns_sql}) VALUES ");
    let max_rows = max_rows.max(1);
    let max_bytes = max_bytes.max(prefix.len() + 2);
    let mut batch = String::new();
    let mut count = 0usize;
    let flush = |out: &mut String, batch: &mut String, count: &mut usize| {
        if *count == 0 {
            return;
        }
        out.push_str(&prefix);
        out.push_str(batch);
        out.push_str(";\n");
        batch.clear();
        *count = 0;
    };
    for tuple in value_tuples {
        let extra = if count == 0 {
            tuple.len()
        } else {
            2 + tuple.len()
        };
        if count > 0 && (count >= max_rows || prefix.len() + batch.len() + extra + 2 > max_bytes) {
            flush(out, &mut batch, &mut count);
        }
        if count > 0 {
            batch.push_str(", ");
        }
        batch.push_str(tuple);
        count += 1;
    }
    flush(out, &mut batch, &mut count);
}

/// Dump a database to SQL (header, optional DROP, DDL via `dump_table_ddl`, INSERTs).
///
/// Views / materialized views emit `CREATE VIEW` (when available) and **never**
/// `INSERT INTO`. Does not emit `CREATE DATABASE`. Dump order is tables then
/// views so restore can create base relations before dependent views.
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
    let (base_tables, views) = partition_dump_objects(tables);

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

    // DROP VIEW before DROP TABLE so dependent views do not block table drops.
    if opts.clean {
        for table in views.iter().chain(base_tables.iter()) {
            out.push_str(&drop_object_sql(
                &|n| driver.quote_ident(n),
                &table.name,
                &table.table_type,
                table.schema.as_deref(),
            ));
        }
        if !views.is_empty() || !base_tables.is_empty() {
            out.push('\n');
        }
    }

    let total = (base_tables.len() + views.len()) as u32;
    let mut current = 0u32;
    for table in base_tables.iter().chain(views.iter()) {
        current += 1;
        dump_one_object(
            driver,
            handle,
            table,
            opts,
            current,
            total,
            &mut out,
            &mut on_progress,
        )
        .await?;
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

/// Parse dump-header `-- Options: …, single-transaction`.
///
/// This flag means a **consistent snapshot while dumping** (mysqldump
/// `--single-transaction`). Restore must **not** wrap the whole file in
/// `BEGIN` just because this appears in the header — that aborts every
/// subsequent object after the first error (and MySQL rejects `BEGIN` on
/// the prepared protocol).
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

/// Clear an aborted session, then (if overwrite + already-exists CREATE) drop
/// the leftover relation so the pipeline can retry `CREATE`.
pub async fn recover_restore_statement_default<D>(
    driver: &D,
    handle: &ConnectionHandle,
    stmt: &str,
    error: &DriverError,
    overwrite: bool,
) -> Result<bool, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let _ = driver.execute(handle, "ROLLBACK").await;
    let err_s = error.to_string();
    // Old PG dumps emit `DEFAULT nextval('foo_id_seq')` without CREATE SEQUENCE.
    if err_s.to_lowercase().contains("does not exist") {
        let seqs = extract_nextval_sequence_names(stmt);
        let mut created_any = false;
        for seq in seqs {
            let ident = quote_sequence_ident(&seq);
            if driver
                .execute(handle, &format!("CREATE SEQUENCE IF NOT EXISTS {ident}"))
                .await
                .is_ok()
            {
                created_any = true;
            }
        }
        if created_any {
            return Ok(true);
        }
    }
    if !overwrite || !relation_already_exists(&err_s) {
        return Ok(false);
    }
    let Some(ident) = created_relation_ident(stmt) else {
        return Ok(false);
    };
    for sql in [
        format!("DROP VIEW IF EXISTS {ident} CASCADE"),
        format!("DROP MATERIALIZED VIEW IF EXISTS {ident} CASCADE"),
        format!("DROP TABLE IF EXISTS {ident} CASCADE"),
    ] {
        let _ = driver.execute(handle, &sql).await;
        let _ = driver.execute(handle, "ROLLBACK").await;
    }
    Ok(true)
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
    use_tx: bool,
    overwrite: bool,
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
            // Only the explicit restore option. Dump-header `single-transaction`
            // is snapshot isolation while dumping, not restore atomicity.
            use_tx: opts.map(|o| o.single_transaction).unwrap_or(false),
            overwrite: opts.map(|o| o.overwrite).unwrap_or(false),
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
        let stmts = self.scanner.push(chunk);
        self.exec_all(stmts, on_progress).await
    }

    pub async fn finish(
        &mut self,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
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
            let mut err = self.driver.execute(self.handle, &full).await.err();
            if let Some(ref e) = err {
                if !self.use_tx {
                    match self
                        .driver
                        .recover_restore_statement(self.handle, &stmt, e, self.overwrite)
                        .await
                    {
                        Ok(true) => {
                            err = self.driver.execute(self.handle, &full).await.err();
                        }
                        Ok(false) => {}
                        Err(recover_err) => err = Some(recover_err),
                    }
                }
            }
            if let Some(e) = err {
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
    fn partition_dump_objects_puts_views_after_tables() {
        let (base, views) = partition_dump_objects(vec![
            TableInfo {
                schema: Some("public".into()),
                name: "active_users".into(),
                table_type: TableType::View,
                row_count: None,
            },
            TableInfo {
                schema: Some("public".into()),
                name: "orders".into(),
                table_type: TableType::Table,
                row_count: None,
            },
        ]);
        assert_eq!(
            base.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["orders"]
        );
        assert_eq!(
            views.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["active_users"]
        );
    }

    #[test]
    fn view_like_skips_insert_and_uses_drop_view() {
        assert!(is_view_like(&TableType::View));
        assert!(is_view_like(&TableType::MaterializedView));
        assert!(!is_view_like(&TableType::Table));
        assert_eq!(
            drop_object_sql(
                &|n| format!("\"{n}\""),
                "active_users",
                &TableType::View,
                None
            ),
            "DROP VIEW IF EXISTS \"active_users\";\n"
        );
        assert_eq!(
            drop_object_sql(
                &|n| format!("\"{n}\""),
                "users",
                &TableType::Table,
                Some("public"),
            ),
            "DROP TABLE IF EXISTS \"public\".\"users\";\n"
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

    #[test]
    fn created_relation_ident_reads_schema_qualified_create() {
        assert_eq!(
            created_relation_ident("CREATE TABLE \"public\".\"users\" (\n  id int\n)"),
            Some("\"public\".\"users\"".into())
        );
        assert_eq!(
            created_relation_ident("CREATE OR REPLACE VIEW active_users AS SELECT 1"),
            Some("active_users".into())
        );
        assert_eq!(
            created_relation_ident("CREATE TABLE IF NOT EXISTS foo (id int)"),
            Some("foo".into())
        );
        assert_eq!(created_relation_ident("INSERT INTO t VALUES (1)"), None);
    }

    #[test]
    fn relation_already_exists_detects_pg_and_mysql() {
        assert!(relation_already_exists(
            "error returned from database: 42P07 duplicate_table: relation \"users\" already exists"
        ));
        assert!(relation_already_exists(
            "1050 (42S01): Table 'users' already exists"
        ));
        assert!(!relation_already_exists("syntax error"));
    }

    #[test]
    fn extract_nextval_sequence_names_from_create_table() {
        let sql = r#"CREATE TABLE public.categories (
  "id" integer NOT NULL DEFAULT nextval('categories_id_seq'::regclass)
)"#;
        assert_eq!(extract_nextval_sequence_names(sql), ["categories_id_seq"]);
        assert_eq!(
            extract_nextval_sequence_names(r#"DEFAULT nextval('public.orders_id_seq'::regclass)"#),
            ["public.orders_id_seq"]
        );
        assert!(extract_nextval_sequence_names("CREATE TABLE t (id int)").is_empty());
        assert_eq!(
            quote_sequence_ident("categories_id_seq"),
            "\"categories_id_seq\""
        );
        assert_eq!(
            quote_sequence_ident("public.categories_id_seq"),
            "\"public\".\"categories_id_seq\""
        );
    }

    #[test]
    fn append_batched_inserts_groups_rows_and_respects_limits() {
        let rows: Vec<String> = (1..=5).map(|i| format!("({i})")).collect();
        let mut out = String::new();
        append_batched_inserts(&mut out, "t", "id", &rows, 2, 10_000);
        assert_eq!(
            out,
            "INSERT INTO t (id) VALUES (1), (2);\nINSERT INTO t (id) VALUES (3), (4);\nINSERT INTO t (id) VALUES (5);\n"
        );

        let mut small = String::new();
        append_batched_inserts(
            &mut small,
            "t",
            "id",
            &["(1)".into(), "(2)".into(), "(3)".into()],
            100,
            40,
        );
        assert!(small.matches("INSERT INTO").count() >= 2);
        assert!(small.contains("VALUES (1)"));
    }
}
