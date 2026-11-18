//! Session-scoped relation-metadata cache, ported from React
//! `packages/frontend/src/lib/relationMetadata/metadataCache.ts` (T032).
//!
//! The React module is a single 563-line file; here it is split by responsibility
//! to respect the 800-line limit:
//!
//! * [`identity`] — `QualifiedRelationId`, dialect folding/quoting and the
//!   canonical `dbSessionId::identity` key.
//! * [`session_config`] — the identity/config/session partition address.
//! * [`schema`] — `TableSchema` and snapshot/metadata contracts.
//! * [`invalidation`] — the explicit schema/DDL invalidation callback list.
//! * [`ordered_map`] — insertion-ordered working map (JS `Map` semantics).
//! * [`metadata_cache`] — the cache itself (debounce, dedup, TTL, epochs).
//!
//! Everything is pure logic: no Rune tree, no `oklch`, no i18n, no IPC. See
//! `metadata_cache`'s module docs for the immediate-mode adaptations.
#![allow(dead_code)] // consumers: the SQL editor / VQB completion layer (not landed yet)

pub(crate) mod identity;
pub(crate) mod invalidation;
pub(crate) mod metadata_cache;
pub(crate) mod ordered_map;
pub(crate) mod schema;
pub(crate) mod session_config;

pub(crate) use identity::*;
pub(crate) use invalidation::*;
pub(crate) use metadata_cache::*;
pub(crate) use schema::*;
pub(crate) use session_config::*;

// The test file lives in `src/tests/` per migration convention but is attached
// here so it can reach the public surface through these re-exports; it is not
// registered in the shared `tests/mod.rs`.
#[cfg(test)]
#[path = "../tests/metadata_cache.rs"]
mod cache_tests;
