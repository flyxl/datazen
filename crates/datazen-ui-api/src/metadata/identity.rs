//! Metadata cache identity and isolation keys, ported from React
//! `packages/frontend/src/lib/metadata/identity.ts` (tasklist T080).
//!
//! P4 contract: metadata batches and cache entries are isolated by
//! *identity* (who requested it), *configuration* (which connection config,
//! at which revision), and *target* (which namespace/object the metadata is
//! about). Two requests that differ in any of these dimensions must never
//! share an entry, a batch, or an in-flight promise.
//!
//! The keys are `JSON.stringify`-canonical strings: a flat JSON array, no
//! whitespace, `null` for absent layers and objects, JavaScript string
//! escaping. Rust builds them through `serde_json::Value`, whose compact
//! `Display` form is byte-identical to `JSON.stringify` for these shapes —
//! both escape exactly `"`, `\` and the C0 control codes (short `\b \t \n \f
//! \r` where defined, otherwise lowercase `\u00xx`), both pass non-ASCII
//! through, both emit arrays as `,`-joined with no padding. The tests pin the
//! literal strings a real Node `JSON.stringify` produced for the same
//! inputs, so the parity is observed, not asserted.
//!
//! One number-form decision worth writing down: React's `Counter` is a
//! branded JS *number* by the time identity sees it (backend-client decodes
//! the wire's decimal string before the UI does), so the revision lands in
//! the key as a bare JSON number. Mirrored here as `u64`, which prints the
//! same digits for every value the frontend can hold — `toCounter` refuses
//! decimal strings above `Number.MAX_SAFE_INTEGER`. A `u64` above 2^53 would
//! print exact digits where JavaScript would already have rounded, which is
//! strictly more precise and unreachable from a frontend that upholds the
//! `toCounter` contract.
//!
//! Type mirrors: React imports `Counter`, `Id`, `ExecutionTarget`,
//! `NamespaceTarget` from `@datazen/backend-client`
//! (`types/identity.ts` / `types/session.ts`). The kernel has field-for-field
//! equivalents (`datazen::connection::domain::NamespaceTarget`,
//! `datazen::execution::target::{ExecutionTarget, ObjectTarget}` and the
//! `datazen::shared::id` newtypes), but `datazen` is a
//! `cfg(not(target_arch = "wasm32"))`-only dependency of this crate while the
//! wasm host compiles these pure modules too — so this module keeps local
//! contract mirrors, the same trade `connection_views` made (T029). A task
//! that wires kernel DTOs to UI contracts should convert *into* these types
//! and leave the key derivation single-sourced here. `Id` mirrors as `String`
//! and `Counter` as `u64` (see the number-form note above).
//!
//! Downstream: `lib/metadata/batch.ts` (tasklist T031) buckets through
//! [`metadata_batch_bucket`] and deduplicates through [`metadata_cache_key`]
//! with [`MetadataIdentityInput`], so it lands next to this file.

use serde_json::Value;

/// Mirror of backend-client `NamespaceTarget` (`types/session.ts`): database
/// / catalog / schema resolution. Absent layers are `null`; only layers the
/// driver declares may carry values. `path` holds driver namespace ids,
/// **not** a filesystem path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamespaceTarget {
    pub database: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub path: Vec<String>,
}

impl NamespaceTarget {
    /// Field-for-field constructor, same argument order the kernel's
    /// `datazen::connection::domain::NamespaceTarget::new` uses.
    pub fn new(
        database: Option<String>,
        catalog: Option<String>,
        schema: Option<String>,
        path: Vec<String>,
    ) -> Self {
        Self {
            database,
            catalog,
            schema,
            path,
        }
    }
}

/// Mirror of backend-client `ObjectTarget`: one addressable object inside a
/// namespace (table, view, …). `signature` is `null` when it cannot be
/// determined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectTarget {
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
}

impl ObjectTarget {
    /// Full object identity without a signature (the undetermined case).
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
            signature: None,
        }
    }

    /// Attach the signature the backend reported (`table|view`, …).
    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }
}

/// Mirror of backend-client `ExecutionTarget`: what an execution runs
/// *against* — connection, namespace, optional object.
///
/// The key derivations below never read this struct's `connection_id`:
/// React's `metadataCacheKey` uses the request-level `connectionId` on
/// [`MetadataIdentityInput`], and the field is mirrored anyway so the
/// contract stays field-for-field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionTarget {
    pub connection_id: String,
    pub namespace: NamespaceTarget,
    pub object: Option<ObjectTarget>,
}

/// React's `ExecutionTarget | NamespaceTarget` union.
///
/// A bare [`NamespaceTarget`] behaves exactly like an execution target with
/// `object: null`: every key derived from either spells the same identity
/// (pinned by a test below), which is what lets a namespace-level request
/// share a batch bucket with the object-level requests inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataTarget {
    Execution(ExecutionTarget),
    Namespace(NamespaceTarget),
}

/// One metadata request's three non-target dimensions.
///
/// `connection_id` (which persistent config) and `db_session_id` (which
/// runtime session) are distinct dimensions and are never collapsed into one
/// optional field — see the ID terminology rule in `AGENTS.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataIdentityInput {
    /// Persistent connection configuration the request is issued against.
    pub connection_id: String,
    /// Revision of that configuration the request saw.
    pub config_revision: u64,
    /// Runtime session the request runs against (never persisted).
    pub db_session_id: String,
    /// Target namespace/object the metadata describes.
    pub target: MetadataTarget,
}

/// The identity/config dimensions of one metadata request, without its
/// target — React's `Omit<MetadataIdentityInput, 'target'>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataSessionIdentity {
    pub connection_id: String,
    pub config_revision: u64,
    pub db_session_id: String,
}

impl MetadataIdentityInput {
    /// The `Omit<…, 'target'>` projection React gets structurally for free;
    /// Rust spells it as a clone into the sibling struct.
    pub fn session_identity(&self) -> MetadataSessionIdentity {
        MetadataSessionIdentity {
            connection_id: self.connection_id.clone(),
            config_revision: self.config_revision,
            db_session_id: self.db_session_id.clone(),
        }
    }
}

/// Canonical, stable cache key isolating identity + config + target.
///
/// Byte-for-byte the string React's `metadataCacheKey` returns: the
/// identity/config/session prefix, then the namespace layers, the path
/// array, and finally `null` (no object) or the
/// `[kind, name, signature ?? null]` triple.
pub fn metadata_cache_key(input: &MetadataIdentityInput) -> String {
    let (namespace, object) = split_target(&input.target);
    let mut parts = session_parts(&input.session_identity());
    push_namespace(namespace, &mut parts);
    parts.push(object_value(object));
    array_key(parts)
}

/// Session address: identity + config + runtime session, no target.
///
/// This is the single derivation of the session-level address. Cache layers
/// that hold *per-session* state (rather than per-relation state) must
/// address it through this function so their partition boundaries match
/// [`metadata_cache_key`] exactly.
pub fn metadata_session_key(session: &MetadataSessionIdentity) -> String {
    array_key(session_parts(session))
}

/// Config-level bucket key: identity + config (no target). React's
/// `metadataConfigBucket` forwards to the session-key derivation; so does
/// this port.
pub fn metadata_config_bucket(input: &MetadataIdentityInput) -> String {
    metadata_session_key(&input.session_identity())
}

/// Batch bucket key: identity + config + target namespace, object excluded.
///
/// `batch.ts` (tasklist T031) groups queued keys by this bucket so a batch
/// never mixes targets, while object-level requests inside one bucket still
/// deduplicate by their full [`metadata_cache_key`].
pub fn metadata_batch_bucket(input: &MetadataIdentityInput) -> String {
    let (namespace, _) = split_target(&input.target);
    let mut parts = session_parts(&input.session_identity());
    push_namespace(namespace, &mut parts);
    array_key(parts)
}

/// `'namespace' in target ? target.namespace : target`, paired with
/// `'object' in target ? target.object : null` — React's union narrowing,
/// extracted once so the cache key and the batch bucket cannot disagree
/// about which half of the union they read.
fn split_target(target: &MetadataTarget) -> (&NamespaceTarget, Option<&ObjectTarget>) {
    match target {
        MetadataTarget::Execution(execution) => {
            (&execution.namespace, execution.object.as_ref())
        }
        MetadataTarget::Namespace(namespace) => (namespace, None),
    }
}

/// The `[connectionId, configRevision, dbSessionId]` prefix every key
/// starts with.
fn session_parts(session: &MetadataSessionIdentity) -> Vec<Value> {
    vec![
        Value::String(session.connection_id.clone()),
        Value::Number(session.config_revision.into()),
        Value::String(session.db_session_id.clone()),
    ]
}

/// `ns.database ?? null` (and catalog/schema, and `obj.signature ?? null`):
/// absent values serialize as JSON `null`, never as the empty string.
fn opt_string(value: &Option<String>) -> Value {
    match value {
        Some(text) => Value::String(text.clone()),
        None => Value::Null,
    }
}

/// `[...ns.path]` — a fresh JSON array of the path segments, so an empty
/// path serializes as `[]` and a one-segment path is still an array.
fn path_value(path: &[String]) -> Value {
    Value::Array(
        path.iter()
            .map(|segment| Value::String(segment.clone()))
            .collect(),
    )
}

/// `obj ? [obj.kind, obj.name, obj.signature ?? null] : null`.
fn object_value(object: Option<&ObjectTarget>) -> Value {
    match object {
        Some(object) => Value::Array(vec![
            Value::String(object.kind.clone()),
            Value::String(object.name.clone()),
            opt_string(&object.signature),
        ]),
        None => Value::Null,
    }
}

/// The namespace half shared by the cache key and the batch bucket:
/// database, catalog, schema, then the path array.
fn push_namespace(namespace: &NamespaceTarget, parts: &mut Vec<Value>) {
    parts.push(opt_string(&namespace.database));
    parts.push(opt_string(&namespace.catalog));
    parts.push(opt_string(&namespace.schema));
    parts.push(path_value(&namespace.path));
}

/// `JSON.stringify(parts)` — `serde_json::Value`'s compact `Display` is that
/// canonical form; see the module docs for the byte-parity argument.
/// `Display` cannot fail, which keeps the key derivations infallible without
/// a silent fallback.
fn array_key(parts: Vec<Value>) -> String {
    Value::Array(parts).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example request used by the pinned-literal tests: identity
    /// `conn-1` at revision 7 in session `sess-9`, an execution target on
    /// namespace `app/public` with path `[a, b]` and object `table.users`
    /// whose signature is `table`.
    fn execution_input() -> MetadataIdentityInput {
        let namespace = NamespaceTarget::new(
            Some("app".to_string()),
            None,
            Some("public".to_string()),
            vec!["a".to_string(), "b".to_string()],
        );
        MetadataIdentityInput {
            connection_id: "conn-1".to_string(),
            config_revision: 7,
            db_session_id: "sess-9".to_string(),
            target: MetadataTarget::Execution(ExecutionTarget {
                connection_id: "conn-1".to_string(),
                namespace,
                object: Some(ObjectTarget::new("table", "users").with_signature("table")),
            }),
        }
    }

    #[test]
    fn the_cache_key_pins_the_json_stringify_form() {
        // Literal captured from Node's JSON.stringify on the same input.
        assert_eq!(
            metadata_cache_key(&execution_input()),
            r#"["conn-1",7,"sess-9","app",null,"public",["a","b"],["table","users","table"]]"#
        );
    }

    #[test]
    fn a_null_object_signature_serializes_as_null() {
        let mut input = execution_input();
        if let MetadataTarget::Execution(target) = &mut input.target {
            target.object = Some(ObjectTarget::new("table", "users"));
        }
        assert_eq!(
            metadata_cache_key(&input),
            r#"["conn-1",7,"sess-9","app",null,"public",["a","b"],["table","users",null]]"#
        );
    }

    #[test]
    fn an_execution_target_without_object_serializes_the_trailing_null() {
        let mut input = execution_input();
        if let MetadataTarget::Execution(target) = &mut input.target {
            target.object = None;
        }
        assert_eq!(
            metadata_cache_key(&input),
            r#"["conn-1",7,"sess-9","app",null,"public",["a","b"],null]"#
        );
    }

    #[test]
    fn a_bare_namespace_target_spells_the_same_key_as_objectless_execution() {
        let namespace = NamespaceTarget::new(
            Some("app".to_string()),
            None,
            Some("public".to_string()),
            vec!["a".to_string(), "b".to_string()],
        );
        // Same namespace, no object — and a *different* connection id on the
        // execution target, pinning that the target's own connection id
        // never enters the key (React reads input.connectionId instead).
        let execution = MetadataIdentityInput {
            connection_id: "conn-1".to_string(),
            config_revision: 7,
            db_session_id: "sess-9".to_string(),
            target: MetadataTarget::Execution(ExecutionTarget {
                connection_id: "other-conn".to_string(),
                namespace: namespace.clone(),
                object: None,
            }),
        };
        let bare = MetadataIdentityInput {
            connection_id: "conn-1".to_string(),
            config_revision: 7,
            db_session_id: "sess-9".to_string(),
            target: MetadataTarget::Namespace(namespace),
        };
        assert_eq!(
            metadata_cache_key(&execution),
            r#"["conn-1",7,"sess-9","app",null,"public",["a","b"],null]"#
        );
        assert_eq!(metadata_cache_key(&bare), metadata_cache_key(&execution));
        assert_eq!(metadata_batch_bucket(&bare), metadata_batch_bucket(&execution));
    }

    #[test]
    fn the_session_key_is_identity_config_session_only() {
        // Literal captured from Node's JSON.stringify on the same input.
        assert_eq!(
            metadata_session_key(&execution_input().session_identity()),
            r#"["conn-1",7,"sess-9"]"#
        );
    }

    #[test]
    fn the_config_bucket_is_the_session_key() {
        let input = execution_input();
        assert_eq!(
            metadata_config_bucket(&input),
            metadata_session_key(&input.session_identity())
        );
        assert_eq!(metadata_config_bucket(&input), r#"["conn-1",7,"sess-9"]"#);
    }

    #[test]
    fn the_batch_bucket_drops_the_object_but_keeps_the_namespace() {
        // Literal captured from Node's JSON.stringify on the same input.
        assert_eq!(
            metadata_batch_bucket(&execution_input()),
            r#"["conn-1",7,"sess-9","app",null,"public",["a","b"]]"#
        );
    }

    #[test]
    fn two_objects_in_one_namespace_share_a_bucket_but_not_a_cache_key() {
        // The T031 contract in miniature: a batch may coalesce the two
        // object requests of one namespace, but each keeps its own cache
        // cell — differing in the object triple only.
        let first = execution_input();
        let mut second = execution_input();
        if let MetadataTarget::Execution(target) = &mut second.target {
            if let Some(object) = &mut target.object {
                object.name = "events".to_string();
            }
        }
        assert_eq!(metadata_batch_bucket(&first), metadata_batch_bucket(&second));
        assert_ne!(metadata_cache_key(&first), metadata_cache_key(&second));
    }

    #[test]
    fn absent_layers_and_an_empty_path_serialize_as_nulls_and_an_empty_array() {
        // Literal captured from Node's JSON.stringify: all-null namespace,
        // empty path (`[]`, not `null`), revision 0.
        let input = MetadataIdentityInput {
            connection_id: "c".to_string(),
            config_revision: 0,
            db_session_id: "s".to_string(),
            target: MetadataTarget::Namespace(NamespaceTarget::default()),
        };
        assert_eq!(
            metadata_cache_key(&input),
            r#"["c",0,"s",null,null,null,[],null]"#
        );
        // The batch bucket ends at the path array — no object element at all.
        assert_eq!(
            metadata_batch_bucket(&input),
            r#"["c",0,"s",null,null,null,[]]"#
        );
    }

    #[test]
    fn the_revision_stays_a_bare_number_up_to_the_frontend_safe_ceiling() {
        // Counter travels the wire as a decimal string, but identity sees the
        // decoded JS number, so the key prints bare digits — not `"7"`. The
        // ceiling case (2^53) is the largest integer the frontend can hold.
        let mut input = MetadataIdentityInput {
            connection_id: "c".to_string(),
            config_revision: 7,
            db_session_id: "s".to_string(),
            target: MetadataTarget::Namespace(NamespaceTarget::default()),
        };
        assert_eq!(
            metadata_cache_key(&input),
            r#"["c",7,"s",null,null,null,[],null]"#
        );
        input.config_revision = 9_007_199_254_740_992;
        assert_eq!(
            metadata_cache_key(&input),
            r#"["c",9007199254740992,"s",null,null,null,[],null]"#
        );
    }

    #[test]
    fn string_escaping_matches_json_stringify_byte_for_byte() {
        // Literal captured from Node's JSON.stringify on inputs carrying a
        // quote, a backslash, a newline, a C0 control character, and an empty
        // path segment: `"`, `\`, `\n` and `\u0001` must escape exactly as
        // JavaScript escapes them.
        let input = MetadataIdentityInput {
            connection_id: "i\"d\\x\ny".to_string(),
            config_revision: 0,
            db_session_id: "s\u{0001}z".to_string(),
            target: MetadataTarget::Namespace(NamespaceTarget::new(
                Some("d\"b".to_string()),
                None,
                None,
                vec!["p\"q\\r".to_string(), String::new()],
            )),
        };
        assert_eq!(
            metadata_cache_key(&input),
            r#"["i\"d\\x\ny",0,"s\u0001z","d\"b",null,null,["p\"q\\r",""],null]"#
        );
    }

    #[test]
    fn every_dimension_isolates_the_cache_key() {
        // The P4 contract, exercised dimension by dimension: mutating any
        // single coordinate of the input must move the key.
        let base = metadata_cache_key(&execution_input());
        let mutated = |mutate: fn(&mut MetadataIdentityInput)| {
            let mut variant = execution_input();
            mutate(&mut variant);
            metadata_cache_key(&variant)
        };

        // identity: which persistent config asked.
        let key = mutated(|input| input.connection_id = "conn-2".to_string());
        assert_ne!(key, base);

        // configuration: which revision it saw.
        let key = mutated(|input| input.config_revision = 8);
        assert_ne!(key, base);

        // runtime session.
        let key = mutated(|input| input.db_session_id = "sess-10".to_string());
        assert_ne!(key, base);

        // target: each namespace layer.
        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.database = Some("other".to_string());
            }
        });
        assert_ne!(key, base);

        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.catalog = Some("cat".to_string());
            }
        });
        assert_ne!(key, base);

        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.schema = Some("other".to_string());
            }
        });
        assert_ne!(key, base);

        // target: path content…
        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.path = vec!["a".to_string(), "c".to_string()];
            }
        });
        assert_ne!(key, base);

        // …and path order (["a","b"] vs ["b","a"] are different targets).
        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.path = vec!["b".to_string(), "a".to_string()];
            }
        });
        assert_ne!(key, base);

        // Layer order matters even when values are interchangeable: swapping
        // database and schema values must move the key.
        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                target.namespace.database = Some("public".to_string());
                target.namespace.schema = Some("app".to_string());
            }
        });
        assert_ne!(key, base);

        // object: kind, name, and signature each isolate.
        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                if let Some(object) = &mut target.object {
                    object.kind = "view".to_string();
                }
            }
        });
        assert_ne!(key, base);

        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                if let Some(object) = &mut target.object {
                    object.name = "events".to_string();
                }
            }
        });
        assert_ne!(key, base);

        let key = mutated(|input| {
            if let MetadataTarget::Execution(target) = &mut input.target {
                if let Some(object) = &mut target.object {
                    object.signature = None;
                }
            }
        });
        assert_ne!(key, base);
    }
}
