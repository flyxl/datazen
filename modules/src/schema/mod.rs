//! Schema vertical slice: metadata vocabulary, caching mechanism, and the
//! application services for schema reads (§5.3, D4).
//!
//! D4-a (2026-10-26) starts with the metadata cache (column/table two-layer
//! cache with generation gating, moved from `datazen-kernel::schema_cache`)
//! and the DDL-prefix classifier. Application services and the schema-scoped
//! vocabulary follow in later D4 slices.

/// 私有元数据缓存机制（列级/表级两层 + schema 世代号失效闸门）。
pub mod cache;
/// DDL 前缀判定（`sql_may_mutate_schema`）。
pub mod mutate;

pub use cache::{CachedColumns, CachedSchema, DatabaseCache, SchemaCache};
pub use mutate::sql_may_mutate_schema;
