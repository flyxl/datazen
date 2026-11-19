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

    /// Supplementary fixture: an execution target with a caller-chosen
    /// connection id and all three namespace layers settable. The React
    /// fixtures never vary the target's own connectionId nor the catalog
    /// layer, so the pins below build their moves through this instead of
    /// `target_at` (which fixes connection `c1` and catalog `None`).
    fn target_with(
        connection: &str,
        database: Option<&str>,
        schema: Option<&str>,
        catalog: Option<&str>,
    ) -> ExecutionTarget {
        ExecutionTarget {
            connection_id: connection.to_string(),
            namespace: NamespaceTarget::new(
                database.map(|layer| layer.to_string()),
                catalog.map(|layer| layer.to_string()),
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

    #[test]
    fn reports_each_reason_under_its_own_label_in_rule_order() {
        // Supplementary (no React case pins it): the React suite only reads
        // `valid` / the null read, never the reasons list. Each single-rule
        // move must fire exactly its own literal — and only it — and an
        // everything-moved entry must report all four in the order the
        // rules run: handle → connection → config revision → target.
        let entry = entry();
        let base = check_temporary_result(&entry, &current());
        assert!(base.valid);
        assert!(base.reasons.is_empty());

        let mut epoch = current();
        epoch.handle = handle_at("s1", "e2");
        let decision = check_temporary_result(&entry, &epoch);
        assert!(!decision.valid);
        assert_eq!(
            decision.reasons,
            vec![InvalidationReason::RuntimeEpochChanged]
        );

        let mut connection = current();
        connection.connection_id = "c2".to_string();
        assert_eq!(
            check_temporary_result(&entry, &connection).reasons,
            vec![InvalidationReason::ConnectionChanged]
        );

        let mut revision = current();
        revision.config_revision = 4;
        assert_eq!(
            check_temporary_result(&entry, &revision).reasons,
            vec![InvalidationReason::ConfigRevisionChanged]
        );

        let mut target = current();
        target.target = target_at(Some("db2"), Some("public"));
        assert_eq!(
            check_temporary_result(&entry, &target).reasons,
            vec![InvalidationReason::TargetChanged]
        );

        let mut everything = current();
        everything.handle = handle_at("s2", "e2");
        everything.connection_id = "c2".to_string();
        everything.config_revision = 4;
        everything.target = target_at(Some("db2"), Some("public"));
        assert_eq!(
            check_temporary_result(&entry, &everything).reasons,
            vec![
                InvalidationReason::RuntimeEpochChanged,
                InvalidationReason::ConnectionChanged,
                InvalidationReason::ConfigRevisionChanged,
                InvalidationReason::TargetChanged,
            ]
        );
    }

    #[test]
    fn the_target_rule_is_four_way_on_the_targets_own_connection_and_namespace() {
        // Supplementary (no React case pins it): the React fixture only
        // moves the namespace database layer, so the schema / catalog legs
        // and the target's own connectionId leg of the four-way comparison
        // are unpinned. Each alone must drop the entry.
        let entry = entry();

        let mut schema = current();
        schema.target = target_at(Some("db"), Some("other"));
        assert_eq!(read_temporary_result(Some(&entry), &schema), None);

        let mut catalog = current();
        catalog.target = target_with("c1", Some("db"), Some("public"), Some("cat"));
        assert_eq!(read_temporary_result(Some(&entry), &catalog), None);

        // The target's own connectionId is a dimension of the *target* rule:
        // moving it alone (caller-level connectionId untouched) fires
        // TargetChanged, not ConnectionChanged.
        let mut target_connection = current();
        target_connection.target = target_with("c9", Some("db"), Some("public"), None);
        let decision = check_temporary_result(&entry, &target_connection);
        assert!(!decision.valid);
        assert_eq!(decision.reasons, vec![InvalidationReason::TargetChanged]);
    }

    #[test]
    fn a_missing_entry_reads_back_none() {
        // Supplementary (no React case pins it): React's `if (!entry) return
        // null;` — an absent entry reads back null regardless of how the
        // current context moved (the read must not inspect or demand the
        // entry at all).
        assert_eq!(read_temporary_result::<i32>(None, &current()), None);
        let mut moved = current();
        moved.handle = handle_at("s2", "e2");
        moved.connection_id = "c2".to_string();
        assert_eq!(read_temporary_result::<i32>(None, &moved), None);
    }

    #[test]
    fn is_provenance_current_compares_all_three_namespace_layers() {
        // Supplementary (no React case pins it): the React case only moves
        // the database layer, so the schema and catalog legs of the
        // namespace comparison are unpinned. Each alone flips the verdict
        // stale.
        let mut schema = provenance_context();
        schema.target = target_at(Some("db"), Some("other"));
        assert!(!is_provenance_current(&provenance(), &schema));

        let mut catalog = provenance_context();
        catalog.target = target_with("c1", Some("db"), Some("public"), Some("cat"));
        assert!(!is_provenance_current(&provenance(), &catalog));
    }

    #[test]
    fn is_provenance_current_ignores_the_targets_own_connection_id() {
        // Supplementary (no React case pins it): unlike the target rule in
        // `check_temporary_result`, `isProvenanceCurrent` deliberately does
        // not compare the target's own connectionId — the caller-level
        // connectionId comparison already covers that dimension (see the
        // fn docs). Moving the target's connectionId alone, caller-level
        // connection untouched, must leave the provenance classified
        // current.
        let mut target_connection = provenance_context();
        target_connection.target = target_with("c9", Some("db"), Some("public"), None);
        assert!(is_provenance_current(&provenance(), &target_connection));
    }
}
