//! Temporary-result invalidation rules, ported from React
//! `packages/frontend/src/lib/metadata/temporaryResults.ts` (tasklist T118).
//!
//! Temporary objects (redacted preview buffers, ad-hoc batch snapshots,
//! per-execution temp tables surfaced to the UI) are always bound to the
//! *original runtime handle* — the [`SessionHandle`] (`dbSessionId` +
//! `runtimeEpoch`) of the session they came from. They are never rebindable:
//! once the runtime epoch changes, the entry is invalid.
//!
//! Old *result* provenance ([`ResultProvenance`]: connectionId,
//! configRevision, requested target, capability snapshot) is retained for
//! audit but is marked stale once any of its identity/config/target
//! dimensions no longer matches the caller's current context. A stale
//! provenance entry may be displayed ("result from ...") but must not be
//! treated as writable or current, and any temporary result derived from it
//! is dropped.
//!
//! ## Type mirrors: where `SessionHandle` / `ResultProvenance` live
//!
//! React imports both from `@datazen/backend-client` (`types/session.ts`).
//! No Rust mirror existed anywhere the wasm-safe part of the graph can reach
//! (the kernel's equivalents sit behind the native-only `datazen` dependency
//! — see the same trade `identity.rs` (T080) and `connection_views` (T029)
//! record), so both land **here**: `temporaryResults.ts` is their first Rust
//! consumer, and `identity.rs`'s charter stays the cache-key types. Both are
//! re-exported through `metadata/mod.rs` and the crate root like every other
//! metadata type.
//!
//! Scope of the [`ResultProvenance`] mirror: the invalidation rules compare
//! exactly the connection / config-revision / requested-target dimensions,
//! so the mirror keeps those three fields and deliberately omits the
//! audit-only ones (`organizationId`, `principalId`, `contextBefore`,
//! `contextAfter`, `capabilitySnapshot`, `executedAt`) that no rule reads —
//! a task that surfaces provenance for display should extend this mirror
//! rather than fork it. `Id` mirrors as `String` and `Counter` as `u64`,
//! the same spellings `identity.rs` pinned for the metadata family.
//!
//! ## Naming and generic decisions
//!
//! React's anonymous inline parameter object
//! `{ handle, connectionId, configRevision, target }` needs a Rust name and
//! becomes [`CurrentCallContext`] (the file's own phrase: "the *current*
//! runtime handle and current call context"); `isProvenanceCurrent`'s
//! handle-free slice becomes [`CurrentProvenanceContext`].
//!
//! [`TemporaryResult<T>`] mirrors the TS interface with an unconstrained `T`
//! — the invalidation rules never look at the payload. `readTemporaryResult`
//! returns `Option<&T>` off `Option<&TemporaryResult<T>>`: JavaScript hands
//! back the same object reference, so the port borrows instead of cloning
//! (no `T: Clone` bound to impose on hosts; clone at the call site when
//! ownership is wanted).
//!
//! serde: nothing in `temporaryResults.ts` is serialized, so these stay plain
//! structs — `identity.rs` and `batch.rs` made the same call for the
//! metadata family, while `ai_sessions` (T040) is the crate's camelCase serde
//! precedent for the DTOs that *do* cross a storage/IPC boundary. Field
//! names are snake_case throughout; a wire task derives serde with
//! `rename_all = "camelCase"` (and would first need serde on `identity.rs`'s
//! target mirrors).

use super::identity::ExecutionTarget;

/// Mirror of backend-client `SessionHandle` (`types/session.ts`).
///
/// A session handle pairs the session with the runtime epoch it was bound to.
/// After an epoch change the old handle is dead — that is why it is a pair
/// and not a bare id. `dbSessionId` is the runtime, in-memory session id
/// (never persisted); `runtimeEpoch` is the epoch it was bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionHandle {
    pub db_session_id: String,
    pub runtime_epoch: String,
}

/// Mirror of backend-client `ResultProvenance` (`types/session.ts`):
/// "result provenance for the whole call, before/after, not per statement."
///
/// Kept to the dimensions the invalidation rules read — connection,
/// config revision, requested target — see the module docs for why the
/// audit-only fields are not mirrored yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultProvenance {
    pub connection_id: String,
    pub config_revision: u64,
    pub requested_target: ExecutionTarget,
}

/// A temporary object bound to the runtime handle (and, when known, the call
/// provenance) that produced it. React's `TemporaryResult<T>`, field for
/// field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporaryResult<T> {
    /// The runtime handle this temporary object is bound to (origin).
    pub original_handle: SessionHandle,
    /// Provenance of the call that produced the value, when known.
    pub provenance: Option<ResultProvenance>,
    pub value: T,
}

/// React's `InvalidationReason` string-literal union: which rule fired.
///
/// The variant names map onto the literals one-to-one
/// (`RuntimeEpochChanged` ↔ `'runtimeEpochChanged'`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidationReason {
    RuntimeEpochChanged,
    ConnectionChanged,
    ConfigRevisionChanged,
    TargetChanged,
}

/// React's `InvalidationDecision`: `valid` is true only when `reasons` is
/// empty — the list of violated rules travels with the verdict so a host can
/// report *why* a temporary result was dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidationDecision {
    pub valid: bool,
    pub reasons: Vec<InvalidationReason>,
}

/// The caller's current context: the live runtime handle plus the
/// identity/config/target coordinates the provenance rules compare against.
///
/// Named after the source's own phrase ("current runtime handle and current
/// call context"); React spells this as an anonymous inline parameter object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentCallContext {
    pub handle: SessionHandle,
    pub connection_id: String,
    pub config_revision: u64,
    pub target: ExecutionTarget,
}

/// The handle-free slice of [`CurrentCallContext`] that
/// [`is_provenance_current`] judges provenance against — React's anonymous
/// `{ connectionId, configRevision, target }` parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentProvenanceContext {
    pub connection_id: String,
    pub config_revision: u64,
    pub target: ExecutionTarget,
}

/// Decide whether a temporary result is still valid for the *current* runtime
/// handle and current call context. Returns the list of violated rules;
/// `valid` is true only when nothing changed.
///
/// The handle rule compares both fields of [`SessionHandle`] (db session and
/// epoch — either moving kills the entry). When provenance is on record, its
/// connection / config-revision / requested-target dimensions are compared
/// too: the target dimension is four-way (the target's own connectionId plus
/// the database / schema / catalog layers of its namespace).
pub fn check_temporary_result<T>(
    entry: &TemporaryResult<T>,
    current: &CurrentCallContext,
) -> InvalidationDecision {
    let mut reasons = Vec::new();
    if entry.original_handle.runtime_epoch != current.handle.runtime_epoch
        || entry.original_handle.db_session_id != current.handle.db_session_id
    {
        reasons.push(InvalidationReason::RuntimeEpochChanged);
    }
    if let Some(provenance) = &entry.provenance {
        if provenance.connection_id != current.connection_id {
            reasons.push(InvalidationReason::ConnectionChanged);
        }
        if provenance.config_revision != current.config_revision {
            reasons.push(InvalidationReason::ConfigRevisionChanged);
        }
        let requested = &provenance.requested_target;
        let live = &current.target;
        if requested.connection_id != live.connection_id
            || requested.namespace.database != live.namespace.database
            || requested.namespace.schema != live.namespace.schema
            || requested.namespace.catalog != live.namespace.catalog
        {
            reasons.push(InvalidationReason::TargetChanged);
        }
    }
    InvalidationDecision {
        valid: reasons.is_empty(),
        reasons,
    }
}

/// Read a temporary result, or null when any invalidation rule fires —
/// React's `readTemporaryResult`, with `None` standing in for the `null`
/// (a missing entry *and* an invalid entry both read back `None`).
///
/// Returns a borrow of the payload rather than a clone: JavaScript hands
/// back the same object reference, so no `T: Clone` bound is imposed (see
/// the module docs). The borrow is tied to the entry reference alone — the
/// context is only read while deciding, and never held onto.
pub fn read_temporary_result<'a, T>(
    entry: Option<&'a TemporaryResult<T>>,
    current: &CurrentCallContext,
) -> Option<&'a T> {
    match entry {
        Some(entry) if check_temporary_result(entry, current).valid => Some(&entry.value),
        _ => None,
    }
}

/// Classify a provenance record as current or stale against the live context.
///
/// The target comparison is the namespace's three resolution layers
/// (database / schema / catalog); unlike the target rule above it does not
/// compare the target's own connectionId — the caller-level `connectionId`
/// comparison already covers that dimension.
pub fn is_provenance_current(
    provenance: &ResultProvenance,
    current: &CurrentProvenanceContext,
) -> bool {
    if provenance.connection_id != current.connection_id {
        return false;
    }
    if provenance.config_revision != current.config_revision {
        return false;
    }
    let requested = &provenance.requested_target.namespace;
    let live = &current.target.namespace;
    requested.database == live.database
        && requested.schema == live.schema
        && requested.catalog == live.catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::NamespaceTarget;

    /// React fixture `handle(dbSession = 's1', epoch = 'e1')`, default arity.
    fn handle() -> SessionHandle {
        handle_at("s1", "e1")
    }

    /// React fixture `handle(dbSession, epoch)`.
    fn handle_at(db_session: &str, epoch: &str) -> SessionHandle {
        SessionHandle {
            db_session_id: db_session.to_string(),
            runtime_epoch: epoch.to_string(),
        }
    }

    /// React fixture `target(database = 'db', schema = 'public')`, default
    /// arity: connection `c1`, namespace `db`/`public`, no object.
    fn target() -> ExecutionTarget {
        target_at(Some("db"), Some("public"))
    }

    /// React fixture `target(database, schema)`: connection `c1`, the given
    /// (nullable) database/schema layers, null catalog, empty path, no
    /// object.
    fn target_at(database: Option<&str>, schema: Option<&str>) -> ExecutionTarget {
        ExecutionTarget {
            connection_id: "c1".to_string(),
            namespace: NamespaceTarget::new(
                database.map(|layer| layer.to_string()),
                None,
                schema.map(|layer| layer.to_string()),
                Vec::new(),
            ),
            object: None,
        }
    }

    /// React fixture `provenance(overrides = {})`: the defaults only — the
    /// invalidation describe block never passes overrides, and the defaults
    /// are exactly the three fields the lean mirror keeps (`connectionId:
    /// 'c1'`, `configRevision: 3`, `requestedTarget: target()`); React also
    /// fills the audit-only fields with cast placeholders this mirror omits.
    fn provenance() -> ResultProvenance {
        ResultProvenance {
            connection_id: "c1".to_string(),
            config_revision: 3,
            requested_target: target(),
        }
    }

    /// React's bottom-of-file `verifiedProvenance()` helper: it just forwards
    /// to `handle()`.
    fn verified_provenance() -> SessionHandle {
        handle()
    }

    /// The describe block's shared `current` fixture: default handle,
    /// connection `c1`, revision 3, default target.
    fn current() -> CurrentCallContext {
        CurrentCallContext {
            handle: handle(),
            connection_id: "c1".to_string(),
            config_revision: 3,
            target: target(),
        }
    }

    /// The describe block's `{ originalHandle: handle(), provenance:
    /// provenance(), value: 42 }` entry.
    fn entry() -> TemporaryResult<i32> {
        TemporaryResult {
            original_handle: handle(),
            provenance: Some(provenance()),
            value: 42,
        }
    }

    /// React's `{ connectionId, configRevision, target }` slice, at the
    /// fixture defaults.
    fn provenance_context() -> CurrentProvenanceContext {
        CurrentProvenanceContext {
            connection_id: "c1".to_string(),
            config_revision: 3,
            target: target(),
        }
    }

    #[test]
    fn drops_temp_results_when_the_runtime_epoch_changes() {
        // Port of 'drops temp results when the runtime epoch changes': the
        // check is invalid and the read is null once the epoch moves off
        // e1 — same db session s1.
        let entry = entry();
        let mut current = current();
        current.handle = handle_at("s1", "e2");
        assert!(!check_temporary_result(&entry, &current).valid);
        assert_eq!(read_temporary_result(Some(&entry), &current), None);
    }

    #[test]
    fn drops_temp_results_when_the_db_session_changes() {
        // Port of 'drops temp results when the db session changes': same
        // epoch e1, db session moved off s1 — the handle rule fires on
        // either of its two fields.
        let entry = entry();
        let mut current = current();
        current.handle = handle_at("s2", "e1");
        assert_eq!(read_temporary_result(Some(&entry), &current), None);
    }

    #[test]
    fn drops_temp_results_when_connection_or_config_revision_changes() {
        // Port of 'drops temp results when connection or config revision
        // changes': two independent provenance mismatches, each alone drops
        // the entry — connection c1→c2, then revision 3→4.
        let entry = entry();
        let mut other_connection = current();
        other_connection.connection_id = "c2".to_string();
        assert_eq!(read_temporary_result(Some(&entry), &other_connection), None);
        let mut other_revision = current();
        other_revision.config_revision = 4;
        assert_eq!(read_temporary_result(Some(&entry), &other_revision), None);
    }

    #[test]
    fn drops_temp_results_when_the_target_namespace_changes() {
        // Port of 'drops temp results when the target namespace changes':
        // React's `verifiedProvenance()` helper rides in as the original
        // handle; the namespace database db→db2 is what drops the entry.
        let entry = TemporaryResult {
            original_handle: verified_provenance(),
            provenance: Some(provenance()),
            value: 42,
        };
        let mut current = current();
        current.target = target_at(Some("db2"), Some("public"));
        assert_eq!(read_temporary_result(Some(&entry), &current), None);
    }

    #[test]
    fn accepts_entries_that_match_the_current_handle_config_and_target() {
        // Port of 'accepts entries that match the current handle + config +
        // target': readTemporaryResult(entry, current) === 42.
        let entry = entry();
        assert_eq!(read_temporary_result(Some(&entry), &current()), Some(&42));
    }

    #[test]
    fn is_provenance_current_classifies_old_provenance_as_stale() {
        // Port of 'isProvenanceCurrent classifies old provenance as stale':
        // the matching context reads true; revision 4, connection c2 and
        // target db2 each flip it stale (each from a fresh matching base).
        assert!(is_provenance_current(&provenance(), &provenance_context()));

        let mut revision = provenance_context();
        revision.config_revision = 4;
        assert!(!is_provenance_current(&provenance(), &revision));

        let mut connection = provenance_context();
        connection.connection_id = "c2".to_string();
        assert!(!is_provenance_current(&provenance(), &connection));

        let mut target = provenance_context();
        target.target = target_at(Some("db2"), Some("public"));
        assert!(!is_provenance_current(&provenance(), &target));
    }

    #[test]
    fn a_null_provenance_skips_the_provenance_rules() {
        // Supplementary (no React case pins it): `provenance:
        // ResultProvenance | null` maps to `Option::None`, and the provenance
        // rules only run when provenance is on record — with no provenance,
        // connection/revision/target moves do not drop the entry, but a
        // handle move still does.
        let entry = TemporaryResult::<i32> {
            original_handle: handle(),
            provenance: None,
            value: 42,
        };
        let mut moved = current();
        moved.connection_id = "c2".to_string();
        moved.config_revision = 4;
        moved.target = target_at(Some("db2"), Some("public"));
        assert_eq!(read_temporary_result(Some(&entry), &moved), Some(&42));
        let mut new_epoch = current();
        new_epoch.handle = handle_at("s1", "e2");
        assert_eq!(read_temporary_result(Some(&entry), &new_epoch), None);
    }
}
