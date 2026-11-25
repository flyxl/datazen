//! Store-side metadata identity keys, ported from React
//! `packages/frontend/src/stores/schemaMetadataKeys.ts` (tasklist T412).
//!
//! The schema store addresses its per-relation metadata cache through the
//! identity/config/target keys defined in `lib/metadata` — never through
//! ad-hoc strings — so two reads can only share a cache entry when every
//! isolation dimension matches: the persistent connection config, that
//! config's session-local revision, the runtime session, and the target
//! relation.
//!
//! Type mirrors: React imports `ExecutionTarget` / `NamespaceTarget` from
//! `@datazen/backend-client` and `RelationRef` from `@datazen/driver-sdk`.
//! The api crate's `metadata::identity` (T079) already mirrors the
//! backend-client pair field-for-field, so this file adds only the
//! driver-sdk `RelationRef` mirror plus the two store-side derivations on
//! top. React's `identityForSession` exists to cast branded strings into
//! `MetadataIdentityInput`'s brands field-by-field; Rust's `String`/`u64`
//! carry no brands, so the cast collapses to construction — kept as a
//! function so the store-side derivation stays single-sourced instead of
//! every caller re-spelling the struct.

#![allow(dead_code)] // consumers: the schema-store slice (schemaStore.ts port, not landed yet)

use datazen_ui_api::{
    metadata_cache_key, ExecutionTarget, MetadataIdentityInput, MetadataTarget, NamespaceTarget,
    ObjectTarget,
};

/// Driver-sdk `RelationRef` mirror: the exact catalog names of one relation.
///
/// Catalog names are exact names, never quoted SQL text. `database` is a
/// required string; `schema` is `string | null` in the SDK. React's
/// `ref.database ?? null` / `ref.schema ?? null` guards therefore degrade
/// to "database always present, schema optional" — mirrored by
/// `String` / `Option<String>`. (The relation target below fixes `catalog`
/// to `null` and `path` to empty, exactly as the React key does.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRef {
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
}

/// Build the identity input that isolates one metadata request.
///
/// The four dimensions of [`MetadataIdentityInput`], one constructor: this
/// is React's `identityForSession` with the brand casts collapsed (see the
/// module docs). A bare [`MetadataTarget::Namespace`] passes through
/// unchanged, exactly as the `ExecutionTarget | NamespaceTarget` union does.
pub fn identity_for_session(
    connection_id: String,
    config_revision: u64,
    db_session_id: String,
    target: MetadataTarget,
) -> MetadataIdentityInput {
    MetadataIdentityInput {
        connection_id,
        config_revision,
        db_session_id,
        target,
    }
}

/// The identity dimensions one schema-store session entry carries.
///
/// React's `SessionMetadataIdentity` interface: the persistent connection
/// config the session's schema belongs to, plus the session-local revision
/// that is bumped whenever that identity changes. The runtime session id
/// is deliberately not part of this struct — React passes `dbSessionId`
/// alongside at call time and so does the key below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMetadataIdentity {
    /// Persistent connection config id this session's schema belongs to.
    pub connection_id: String,
    /// Session-local config revision; bumped whenever that identity changes.
    pub metadata_revision: u64,
}

/// Cache key for one relation's columns inside a schema-store session.
///
/// Identity (connection config + its revision) and runtime session are part
/// of the key, so columns read for one connection config can never be served
/// to another, and the target carries the relation itself. The returned
/// string is the canonical `metadataCacheKey` spelling, so the schema
/// store's column cache and the `lib/metadata` batcher deduplicate through
/// one derivation rather than two spellings of the same address.
pub fn relation_columns_cache_key(
    session: &SessionMetadataIdentity,
    db_session_id: &str,
    relation: &RelationRef,
) -> String {
    metadata_cache_key(&identity_for_session(
        session.connection_id.clone(),
        session.metadata_revision,
        db_session_id.to_string(),
        MetadataTarget::Execution(ExecutionTarget {
            // React sets the target's own connectionId from the session's;
            // the key derivation never reads it (the identity-level
            // `connectionId` is the dimension that enters the key), so this
            // field is mirrored for contract parity only.
            connection_id: session.connection_id.clone(),
            namespace: NamespaceTarget::new(
                Some(relation.database.clone()),
                None,
                relation.schema.clone(),
                Vec::new(),
            ),
            object: Some(ObjectTarget::new("relation", relation.name.clone())),
        }),
    ))
}
