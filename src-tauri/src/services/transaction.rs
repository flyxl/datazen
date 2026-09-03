//! Dialect-aware transaction scope for DDL and DML operations.

use crate::db::{ConnectionHandle, DatabaseDriver, TransactionHandle};

/// DDL atomicity semantics for a database dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlAtomicity {
    /// DDL is transactional (e.g. PostgreSQL, SQLite).
    Transactional,
    /// Each DDL statement auto-commits (e.g. MySQL).
    AutoCommitPerStatement,
    /// Unknown semantics.
    Unknown,
}

/// Determine DDL atomicity for a given dialect.
pub fn ddl_atomicity(dialect: &str) -> DdlAtomicity {
    match dialect.to_ascii_lowercase().as_str() {
        "postgresql" | "postgres" | "sqlite" => DdlAtomicity::Transactional,
        "mysql" | "mariadb" | "tidb" | "oceanbase" => DdlAtomicity::AutoCommitPerStatement,
        _ => DdlAtomicity::Unknown,
    }
}

/// Dialect-aware transaction scope.
///
/// Manages BEGIN/COMMIT/ROLLBACK lifecycle based on dialect's DDL atomicity.
/// For transactional dialects (PG), wraps operations in a real transaction.
/// For auto-commit dialects (MySQL), operations execute without wrapping.
pub struct TransactionScope<'a> {
    driver: &'a dyn DatabaseDriver,
    handle: &'a ConnectionHandle,
    atomicity: DdlAtomicity,
    tx: Option<TransactionHandle>,
}

impl<'a> TransactionScope<'a> {
    /// Begin a transaction scope. For transactional dialects, calls BEGIN.
    pub async fn begin(
        driver: &'a dyn DatabaseDriver,
        handle: &'a ConnectionHandle,
        dialect: &str,
    ) -> Result<Self, String> {
        let atomicity = ddl_atomicity(dialect);
        let tx = if matches!(atomicity, DdlAtomicity::Transactional) {
            Some(
                driver
                    .begin_transaction(handle)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        Ok(Self {
            driver,
            handle,
            atomicity,
            tx,
        })
    }

    /// Commit the transaction. No-op for auto-commit dialects.
    pub async fn commit(mut self) -> Result<(), String> {
        if let Some(tx) = self.tx.take() {
            self.driver.commit(tx).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Rollback the transaction. No-op for auto-commit dialects.
    pub async fn rollback(mut self) -> Result<(), String> {
        if let Some(tx) = self.tx.take() {
            let _ = self.driver.rollback(tx).await;
        }
        Ok(())
    }

    /// Whether this scope is actually transactional.
    pub fn is_transactional(&self) -> bool {
        self.tx.is_some()
    }

    /// Get the atomicity mode.
    pub fn atomicity(&self) -> DdlAtomicity {
        self.atomicity
    }

    /// Execute a SQL statement within this transaction scope.
    pub async fn execute(&self, sql: &str) -> Result<u64, String> {
        self.driver
            .execute(self.handle, sql)
            .await
            .map_err(|e| e.to_string())
    }
}

impl Drop for TransactionScope<'_> {
    fn drop(&mut self) {
        if self.tx.is_some() {
            tracing::warn!("TransactionScope dropped without commit/rollback");
        }
    }
}
