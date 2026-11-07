//! Row-count estimate for Data Sync (thin delegate).
//!
//! host-boundary F4c 残余收口：COUNT 装配与宽容解析下沉
//! `data_sync::api::count_table_rows`，本层只保留 `cmd_err` 的 IPC 错误
//! 映射（驱动错误原样透传，文本逐字节不变）。

use crate::commands::error::{CmdExt, CommandError};

/// Count rows in a table on a given connection.
pub(crate) async fn count_rows(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    family: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> Result<u64, CommandError> {
    crate::data_sync::api::count_table_rows(driver, handle, family, database, schema, table)
        .await
        .cmd_err("count_rows")
}
