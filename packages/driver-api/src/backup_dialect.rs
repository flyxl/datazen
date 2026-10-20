//! The backup/restore file-rule contract.
//!
//! A driver's dump/restore *rules* — whether the shared SQL restore pipeline
//! applies, which statement scanner the pipeline uses, and how a whole SQL
//! buffer is split — are text-level dialect decisions. They sit here, separate
//! from the handle-bound dump/restore execution entry points that stay on the
//! driver's resource lifecycle (`dump_database*`, `restore_sql*`,
//! `recover_restore_statement`). The shared pipeline in `sql_dump` and the
//! backup host flow read only this contract.
//!
//! [`LegacyBackupDialect`] forwards to the `DatabaseDriver` methods of the
//! same name, one way. A driver keeps exactly one implementation of each.

use crate::sql_split::SqlStatementScanner;
use crate::traits::DatabaseDriver;

/// The restore-file rules the shared SQL backup pipeline needs before
/// executing anything.
pub trait BackupDialect: Send + Sync {
    /// When true (SQL drivers), the host streams the dump file into
    /// [`crate::sql_dump::RestoreSession`]. `false` takes over the whole
    /// restore via the driver's `restore_sql_with_progress`.
    fn uses_sql_restore_pipeline(&self) -> bool;

    /// Statement scanner for the default restore pipeline. Override to
    /// disable `DELIMITER`, change quote rules, or swap splitters.
    fn new_sql_scanner(&self) -> SqlStatementScanner;

    /// Split a complete SQL buffer.
    fn split_restore_sql(&self, sql: &str) -> Vec<String>;
}

/// Borrowing adapter from an existing [`DatabaseDriver`] to [`BackupDialect`].
///
/// One-way, like the other migration seams: every method forwards to the
/// driver method of the same name, so a driver override keeps working and
/// there is exactly one implementation of each rule.
#[derive(Debug, Clone, Copy)]
pub struct LegacyBackupDialect<'a, D: DatabaseDriver + ?Sized> {
    driver: &'a D,
}

impl<'a, D: DatabaseDriver + ?Sized> LegacyBackupDialect<'a, D> {
    pub fn new(driver: &'a D) -> Self {
        Self { driver }
    }

    /// The wrapped driver, for callers at the boundary that still need a
    /// driver-only rule.
    pub fn driver(&self) -> &'a D {
        self.driver
    }
}

impl<D: DatabaseDriver + ?Sized> BackupDialect for LegacyBackupDialect<'_, D> {
    fn uses_sql_restore_pipeline(&self) -> bool {
        self.driver.uses_sql_restore_pipeline()
    }

    fn new_sql_scanner(&self) -> SqlStatementScanner {
        self.driver.new_sql_scanner()
    }

    fn split_restore_sql(&self, sql: &str) -> Vec<String> {
        self.driver.split_restore_sql(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};
    use std::sync::Arc;

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    fn mock() -> Arc<MockDriver> {
        MockDriver::new("postgres", MockDriverOptions::default())
    }

    #[test]
    fn contract_is_object_safe_and_thread_safe() {
        assert_send_sync::<dyn BackupDialect>();
    }

    #[test]
    fn sql_driver_routes_to_the_shared_pipeline_with_its_scanner() {
        let driver = mock();
        let dialect: &dyn BackupDialect = &LegacyBackupDialect::new(driver.as_ref());
        assert!(dialect.uses_sql_restore_pipeline());
        // Scanner and splitter must be two views of the same rule source: a
        // whole-buffer split equals feeding the same text through the scanner.
        let sql = "SELECT 1;\nSELECT 2;\n";
        assert_eq!(dialect.split_restore_sql(sql).len(), 2);
        let mut scanner = dialect.new_sql_scanner();
        let mut statements = scanner.push(sql);
        statements.extend(scanner.finish());
        assert_eq!(statements.len(), 2);
    }
}
