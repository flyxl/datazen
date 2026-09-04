//! Dialect-aware transaction scope for DDL and DML operations.

use crate::db::{ConnectionHandle, DatabaseDriver, DdlAtomicity, TransactionHandle};

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
}

#[allow(dead_code)]
impl<'a> TransactionScope<'a> {
    /// Begin a transaction scope. For transactional drivers, calls BEGIN.
    pub async fn begin(
        driver: &'a dyn DatabaseDriver,
        handle: &'a ConnectionHandle,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

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
}
