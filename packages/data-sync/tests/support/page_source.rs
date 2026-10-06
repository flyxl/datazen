//! `KeysetPageSource` 的内存实现（prepare 侧的源/目标行读取）。

use std::cmp::Ordering;

use async_trait::async_trait;
use datazen_data_sync::compare::{cmp_keys, extract_key};
use datazen_data_sync::error::DataSyncError;
use datazen_data_sync::job::KeysetPageSource;
use datazen_data_sync::model::Row;
use datazen_driver_api::{SyncKeyValue, Value};

/// 按主键排序的行切片，行为与 `SliceRowSource` 一致。
pub struct FakePageSource {
    rows: Vec<Row>,
    pk_indexes: Vec<usize>,
}

impl FakePageSource {
    pub fn new(mut rows: Vec<Row>, pk_indexes: Vec<usize>) -> Self {
        rows.sort_by(|a, b| {
            let ka = extract_key(a, &pk_indexes).unwrap_or_default();
            let kb = extract_key(b, &pk_indexes).unwrap_or_default();
            cmp_keys(&ka, &kb)
        });
        Self { rows, pk_indexes }
    }
}

#[async_trait]
impl KeysetPageSource for FakePageSource {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        let limit = limit.max(1) as usize;
        let start = match after_key {
            None => 0,
            Some(after) => {
                let mut idx = 0;
                while idx < self.rows.len() {
                    let key = extract_key(&self.rows[idx], &self.pk_indexes)?;
                    if cmp_keys(&key, after) == Ordering::Greater {
                        break;
                    }
                    idx += 1;
                }
                idx
            }
        };
        let end = (start + limit).min(self.rows.len());
        Ok(self.rows[start..end].to_vec())
    }

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        key.iter()
            .map(|v| SyncKeyValue::from_value(v).map_err(DataSyncError::validation))
            .collect()
    }
}
