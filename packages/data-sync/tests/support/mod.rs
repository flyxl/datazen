//! CM-43 / CM-44 / CM-45 的共用测试夹具。
//!
//! 每个测试二进制以 `mod support;` 引入本模块；夹具只通过公开端口
//! （`DataSyncHost` / `TargetExecutor` / `JobHandler`）驱动 handler，不借用 crate 私有项。

#![allow(dead_code, unused_imports)]

pub mod executor;
pub mod host;
pub mod page_source;
pub mod rows;

pub use executor::{ExecHook, FakeExecutor, SharedStore, StatementRows, TargetStore};
pub use host::{source_endpoint, target_endpoint, FakeHost, SchemaMode, SOURCE_CONN, TARGET_CONN};
pub use page_source::FakePageSource;
pub use rows::{
    drifted_schema, endpoint, int, key_repr, relation, row, rows_equal, schema, text, FAMILY,
    PLAN_ID, SOURCE_TABLE, TABLE,
};
