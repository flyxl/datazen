//! SQL restore pipeline: streaming session, statement execution, recovery.

use crate::sql_split::{is_comment_only_or_empty, SqlStatementScanner};
use crate::traits::DatabaseDriver;
use crate::types::*;

use super::dump::{extract_nextval_sequence_names, quote_sequence_ident};
use super::parser::{created_relation_ident, relation_already_exists};

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

/// Optional per-statement policy check before executing restore SQL.
pub type RestoreStatementGuard = Box<dyn Fn(&str) -> Result<(), DriverError> + Send>;

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
    statement_guard: Option<RestoreStatementGuard>,
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
            statement_guard: None,
            tx_open: false,
            executed: 0,
            errors: Vec::new(),
        }
    }

    /// Run `guard` on each non-empty statement (without trailing `;`) before execute.
    pub fn with_statement_guard(mut self, guard: RestoreStatementGuard) -> Self {
        self.statement_guard = Some(guard);
        self
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
            if let Some(guard) = &self.statement_guard {
                if let Err(e) = guard(&stmt) {
                    if self.tx_open {
                        let _ = self.driver.execute(self.handle, "ROLLBACK").await;
                        self.tx_open = false;
                    }
                    return Err(e);
                }
            }
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
