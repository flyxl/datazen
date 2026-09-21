//! KeyValueDriver trait implementation for RedisDriver.

use async_trait::async_trait;
use datazen_driver_api::*;

use crate::redis_driver::RedisDriver;

#[async_trait]
impl KeyValueDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        "redis".to_string()
    }

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError> {
        // Trait path: no type filter, logical size (not MEMORY USAGE).
        // Full options (keyType / withMemory) are available via the scan_keys command.
        RedisDriver::scan_keys_with_info(
            self, handle, db_index, pattern, cursor, count, None, false, false,
        )
        .await
    }
}
