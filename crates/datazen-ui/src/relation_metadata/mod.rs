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
//! * [`relation_prediction_types`] — prediction contracts (T081).
//! * [`from_relation_metadata`] — the metadata → prediction input bridge (T081).
//!
//! Everything is pure logic: no Rune tree, no `oklch`, no i18n, no IPC. See
//! `metadata_cache`'s module docs for the immediate-mode adaptations.
#![allow(dead_code)] // consumers: the SQL editor / VQB completion layer (not landed yet)

pub mod from_relation_metadata;
pub mod identity;
pub mod invalidation;
pub mod metadata_cache;
pub mod ordered_map;
pub mod relation_prediction_types;
pub mod schema;
pub mod session_config;

pub use from_relation_metadata::*;
pub use identity::*;
pub use invalidation::*;
pub use metadata_cache::*;
pub use relation_prediction_types::*;
pub use schema::*;
pub use session_config::*;
