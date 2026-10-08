//! Dialect-aware transaction scope for DDL and DML operations.

use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, DdlAtomicity, SqlTarget, TransactionHandle,
};

/// Dialect-aware transaction scope.
///
/// Manages BEGIN/COMMIT/ROLLBACK lifecycle based on the driver's DDL atomicity.
/// For transactional dialects (PG), wraps operations in a real transaction.
/// For auto-commit dialects (MySQL), operations execute without wrapping.
#[allow(dead_code)]
pub struct TransactionScope<'a> {
    driver: &'a dyn DatabaseDriver,
    handle: &'a ConnectionHandle,
    atomicity: DdlAtomicity,
    tx: Option<TransactionHandle>,
    /// The database this scope writes to. PostgreSQL binds a transaction to the
    /// one connection it holds, so a statement aimed at a different database
    /// must be refused rather than run against the wrong catalog.
    target: SqlTarget<'a>,
}

#[allow(dead_code)]
impl<'a> TransactionScope<'a> {
    /// Begin a transaction scope. For transactional drivers, calls BEGIN.
    pub async fn begin(
        driver: &'a dyn DatabaseDriver,
        handle: &'a ConnectionHandle,
    ) -> Result<Self, String> {
        Self::begin_at(driver, handle, SqlTarget::new(None, None)).await
    }

    /// Begin a scope bound to an explicit target.
    pub async fn begin_at(
        driver: &'a dyn DatabaseDriver,
        handle: &'a ConnectionHandle,
        target: SqlTarget<'a>,
    ) -> Result<Self, String> {
        let atomicity = driver.ddl_atomicity();
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
            target,
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
    ///
    /// A driver that cannot confirm the rollback has left the transaction in
    /// an unknown state, so the caller is told — exactly as `commit` does.
    /// Swallowing it reported a clean rollback for a transaction that may still
    /// be open, and `schema_diff/deploy.rs` already treats the failure as
    /// `DeployStatus::Unknown` when it sees one.
    pub async fn rollback(mut self) -> Result<(), String> {
        if let Some(tx) = self.tx.take() {
            self.driver.rollback(tx).await.map_err(|e| e.to_string())?;
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
            .execute_at(self.handle, sql, self.target)
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

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};

    fn mock_handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "conn-1".into(),
            pool_id: "pool-1".into(),
        }
    }

    fn mock_driver(atomicity: DdlAtomicity) -> std::sync::Arc<MockDriver> {
        MockDriver::new(
            "postgres",
            MockDriverOptions {
                ddl_atomicity: Some(atomicity),
                ..Default::default()
            },
        )
    }

    #[tokio::test]
    async fn test_tester_transactional_scope_begins_and_commits() {
        let driver = mock_driver(DdlAtomicity::Transactional);
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        assert_eq!(scope.atomicity(), DdlAtomicity::Transactional);
        assert!(scope.is_transactional());
        scope.commit().await.unwrap();
    }

    #[tokio::test]
    async fn test_tester_auto_commit_scope_skips_begin() {
        let driver = mock_driver(DdlAtomicity::AutoCommitPerStatement);
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        assert_eq!(scope.atomicity(), DdlAtomicity::AutoCommitPerStatement);
        assert!(!scope.is_transactional());
        scope.commit().await.unwrap();
    }

    #[tokio::test]
    async fn test_tester_unknown_atomicity_skips_begin() {
        let driver = mock_driver(DdlAtomicity::Unknown);
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        assert_eq!(scope.atomicity(), DdlAtomicity::Unknown);
        assert!(!scope.is_transactional());
        scope.rollback().await.unwrap();
    }

    /// The happy path stays happy: a driver that accepts the rollback still
    /// yields `Ok`, so the fix cannot be satisfied by failing everything.
    #[tokio::test]
    async fn rollback_reports_ok_when_the_driver_accepts_it() {
        let driver = mock_driver(DdlAtomicity::Transactional);
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        scope.rollback().await.unwrap();
    }

    /// A rollback the driver could not perform leaves the transaction in an
    /// unknown state. `commit` has always reported such a failure; `rollback`
    /// used to swallow it and answer `Ok`, which told the caller — and
    /// `schema_diff/deploy.rs`, which downgrades to `DeployStatus::Unknown` on
    /// a rollback error — that the transaction was closed when it may not be.
    #[tokio::test]
    async fn rollback_reports_a_failed_rollback() {
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                ddl_atomicity: Some(DdlAtomicity::Transactional),
                rollback_error_on_call: Some(1),
                rollback_error: Some("rollback lost its acknowledgement".into()),
                ..Default::default()
            },
        );
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        let err = scope.rollback().await.unwrap_err();
        assert!(
            err.contains("rollback lost its acknowledgement"),
            "the driver failure must reach the caller verbatim, got: {err}"
        );
    }

    /// An auto-commit dialect never opens a transaction, so there is nothing to
    /// roll back and nothing that can fail.
    #[tokio::test]
    async fn rollback_of_a_non_transactional_scope_is_unaffected() {
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                ddl_atomicity: Some(DdlAtomicity::AutoCommitPerStatement),
                rollback_error_on_call: Some(1),
                ..Default::default()
            },
        );
        let handle = mock_handle();

        let scope = TransactionScope::begin(driver.as_ref(), &handle)
            .await
            .unwrap();
        assert!(!scope.is_transactional());
        scope.rollback().await.unwrap();
    }
}
