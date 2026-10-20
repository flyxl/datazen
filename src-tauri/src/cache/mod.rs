//! Schema 元数据缓存门面。
//!
//! ## 临时门面
//!
//! `SchemaCache` 与 `sql_may_mutate_schema` 的实现已迁入
//! `packages/runtime/src/schema_cache/`（`datazen_runtime::schema_cache`），
//! 与 DriverRegistry 同处共享 runtime 内核。
//! 本文件只是 **re-export 门面**：具名项 `sql_may_mutate_schema` 与 `SchemaCache`
//! 原样转出，使 `crate::cache::*` 的既有调用点零改动。
//! 等宿主连接层统一改走 runtime 路径后，本文件随之删除。

pub use datazen_runtime::schema_cache::{sql_may_mutate_schema, SchemaCache};
