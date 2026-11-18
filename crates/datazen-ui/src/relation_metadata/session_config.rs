//! Configuration dimension of editor relation-metadata sessions, ported from
//! React `packages/frontend/src/lib/relationMetadata/sessionConfig.ts`.
//!
//! Sessions are isolated by identity **and** configuration **and** runtime
//! session. A rebind moves a session to a different partition address, so no
//! entry, snapshot, debounce deadline or in-flight load survives the change.
//!
//! React publishes the configuration through a process-global resolver. Here the
//! host injects a resolver closure into the cache instead, which keeps this pure
//! layer free of mutable global state (single-threaded immediate mode).

/// The configuration dimension of an editor metadata session.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct EditorSessionConfig {
    pub connection_id: String,
    pub config_revision: u64,
}

/// Partition used before the host publishes a configuration.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn unbound_session_config() -> EditorSessionConfig {
    EditorSessionConfig {
        connection_id: String::new(),
        config_revision: 0,
    }
}

/// `JSON.stringify([connectionId, configRevision, dbSessionId])` — the same
/// session prefix the per-relation key uses. serde's compact output matches JS
/// for these primitive values, so the derivation is byte-for-byte stable.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn metadata_session_key(config: &EditorSessionConfig, db_session_id: &str) -> String {
    serde_json::to_string(&serde_json::json!([
        config.connection_id.clone(),
        config.config_revision,
        db_session_id
    ]))
    .unwrap_or_default()
}

/// Address of one (identity, configuration, session) partition.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn editor_session_cache_key(
    db_session_id: &str,
    config: &EditorSessionConfig,
) -> String {
    metadata_session_key(config, db_session_id)
}

/// A host-injected replacement for React's global config resolver.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) type ConfigResolver = dyn Fn(&str) -> Option<EditorSessionConfig>;

/// Resolve a session's config with an injected resolver (React's
/// `resolveEditorSessionConfig`), falling back to the unbound partition.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn resolve_editor_session_config_with(
    resolver: &ConfigResolver,
    db_session_id: &str,
) -> EditorSessionConfig {
    resolver(db_session_id).unwrap_or_else(unbound_session_config)
}
