//! Schema 元数据缓存：列级 / 表级两层缓存，加 schema 世代号失效闸门。
//!
//! 实现自 `src-tauri/src/cache/{schema_cache,schema_mutate}.rs` 迁入；宿主侧只保留
//! re-export 门面（`src-tauri/src/cache/mod.rs`），使 `crate::cache::*` 既有调用点零改动。

pub mod schema_cache;
pub mod schema_mutate;

pub use schema_cache::{CachedColumns, CachedSchema, DatabaseCache, SchemaCache};
pub use schema_mutate::sql_may_mutate_schema;
