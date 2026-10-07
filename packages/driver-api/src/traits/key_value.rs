//! The key/value driver contract.
//!
//! Only Redis-like drivers implement this trait. It is a separate contract from
//! the relational one, so it is kept in its own module; `traits.rs` re-exports it
//! so that it keeps being reachable under the same public path.

use async_trait::async_trait;

use crate::types::{ConnectionHandle, DatabaseType, DriverError, KeyEntry};

#[async_trait]
pub trait KeyValueDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>;
}
