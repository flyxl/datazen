//! In-memory KeyValueDriver for kv command unit tests.

use std::sync::Arc;

use async_trait::async_trait;

use crate::db::{ConnectionHandle, DatabaseType, DriverError, KeyDetail, KeyEntry, KeyValueDriver};

#[derive(Clone)]
pub struct MockKvDriverOptions {
    pub keys: Vec<KeyEntry>,
    pub detail: KeyDetail,
    pub next_cursor: u64,
    pub db_size: u64,
}

impl Default for MockKvDriverOptions {
    fn default() -> Self {
        Self {
            keys: vec![KeyEntry {
                key: "user:1".into(),
                key_type: "string".into(),
                ttl: -1,
                size: 4,
                preview: "\"alice\"".into(),
            }],
            detail: KeyDetail {
                key: "user:1".into(),
                key_type: "string".into(),
                ttl: -1,
                value: serde_json::json!("alice"),
            },
            next_cursor: 0,
            db_size: 1,
        }
    }
}

pub struct MockKvDriver {
    db_type: DatabaseType,
    opts: MockKvDriverOptions,
}

impl MockKvDriver {
    pub fn new(db_type: impl Into<DatabaseType>, opts: MockKvDriverOptions) -> Arc<Self> {
        Arc::new(Self {
            db_type: db_type.into(),
            opts,
        })
    }
}

#[async_trait]
impl KeyValueDriver for MockKvDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
    }

    async fn scan_keys_with_info(
        &self,
        _handle: &ConnectionHandle,
        _db_index: u32,
        _pattern: &str,
        _cursor: u64,
        _count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError> {
        Ok((
            self.opts.next_cursor,
            self.opts.keys.clone(),
            self.opts.db_size,
        ))
    }

    async fn get_key_detail(
        &self,
        _handle: &ConnectionHandle,
        _db_index: u32,
        _key: &str,
    ) -> Result<KeyDetail, DriverError> {
        Ok(self.opts.detail.clone())
    }
}
