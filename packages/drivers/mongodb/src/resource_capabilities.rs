//! The declared capability surface and namespace shape of the mongodb driver,
//! matching `docs/architecture/platform/driver-capability-migration.md` §2.1.
//!
//! Kept apart from the provider itself so the declarations can be read — and
//! tested — without wading through the connection plumbing. Almost every cell
//! here is a refusal, and that is the honest answer for a document store: a
//! MongoDB client is a stateless handle onto a deployment, not a session the
//! host can observe, switch, reset or cancel.

use std::collections::BTreeMap;

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation as TransactionObservationCap, TransactionSupport,
};
use datazen_driver_api::namespace::{
    CanonicalIdRules, CaseFolding, CaseRules, NamespaceLevel, NamespaceLevelKind,
    NamespacePathSegment, NamespaceShape,
};
use datazen_driver_api::DdlAtomicity;

/// `DdlAtomicitySupport::by_operation` has no canonical key vocabulary in
/// `driver-api` — the contract only says an unknown key must fail closed. The
/// keys used here are the snake_case names of
/// `datazen_driver_api::schema_migration::MigrationOperation`, the same
/// provider-local convention the sibling mysql/sqlite providers use.
///
/// MongoDB files under nothing: it has no DDL at all, so every key maps to
/// `Unknown`. Leaving the map empty would be equivalent, but an explicit
/// `Unknown` per operation documents that the question was asked rather than
/// forgotten.
const MIGRATION_OPERATIONS: [&str; 33] = [
    "create_table",
    "drop_table",
    "add_column",
    "drop_column",
    "alter_column_type",
    "set_nullable",
    "set_default",
    "set_comment",
    "set_table_options",
    "set_auto_increment",
    "add_primary_key",
    "drop_primary_key",
    "create_index",
    "drop_index",
    "add_foreign_key",
    "drop_foreign_key",
    "add_check_constraint",
    "drop_check_constraint",
    "create_view",
    "replace_view",
    "drop_view",
    "create_routine",
    "replace_routine",
    "drop_routine",
    "create_trigger",
    "replace_trigger",
    "drop_trigger",
    "create_sequence",
    "replace_sequence",
    "drop_sequence",
    "create_type",
    "replace_type",
    "drop_type",
];

/// mongodb's declared capability set. It is fixed, not parameterised: the one
/// factory in this crate backs exactly one provider id and has no `supports_*`
/// hook of its own that could widen a cell.
pub(crate) fn mongodb_capability_set() -> CapabilitySet {
    let mut capabilities = CapabilitySet::default();
    // `connect` stores a `Client` — MongoDB's own stateless connection pool —
    // and every operation names its database explicitly. §2.1 事实二 plus §6.1
    // forbid reading any of that as a stateful session, so this stays refused
    // and the descriptor reports `SessionContinuity::Leased`.
    capabilities.stateful_session = Availability::Unsupported;
    // §2.1 marks this cell 「待 P0 验证」. The code reading supports
    // `unsupported` (`resolve_database` takes the target from the call
    // argument and never mutates pool state), but the spec defers the verdict,
    // so the fail-closed reading is declared: `Unknown`, which gates the
    // feature off, while `change_context` independently answers `Unsupported`.
    capabilities.namespace_switch = NamespaceSwitch::Unknown;
    // There is no server-side session to interrogate: a MongoDB client reports
    // no current database, no autocommit flag and no search path, so nothing
    // here can be read back and nothing may be invented. `observe_session`
    // returns `SessionObservation::unobservable()`.
    capabilities.context_observation = ContextObservation::Unsupported;
    // A MongoDB deployment has no ambient transaction this driver opened.
    capabilities.transaction_observation = TransactionObservationCap::Unsupported;
    // No cursor, session or server-side handle is handed to the host, so there
    // is nothing to report — and an empty list must stay empty.
    capabilities.session_scoped_handles = SessionScopedHandleSupport::Unsupported;
    // No verified path back to an initialization baseline for a client that
    // holds no state at all; resetting one would only hide a routing change.
    // `reset_resource` therefore always answers `Discard`.
    capabilities.reset_for_reuse = ResetForReuse::Unsupported;
    // `cancel_query` has nothing to interrupt — this crate holds no cursor
    // handle — and returns `DriverError::Unsupported` rather than the `Ok(())`
    // that `driver-capability-migration.md` §7.1 calls out as the last broken
    // fail-closed default.
    capabilities.precise_cancel = PreciseCancelSupport::Unsupported;
    // No snapshot primitive: a read concern is per-operation and not exposed as
    // a resource-level snapshot, so claiming one would be an invention.
    capabilities.snapshots = SnapshotSupport::Unsupported;
    capabilities.transactions = TransactionSupport {
        // A MongoDB deployment only offers multi-document transactions inside
        // a session this driver never opens. No level can be declared, and
        // `begin_transaction` refuses with the trait's `TransactionError`
        // default rather than faking one.
        isolation_levels: Vec::new(),
        savepoints: Availability::Unsupported,
        max_open_transactions: None,
    };
    capabilities.ddl_atomicity = ddl_atomicity_support();
    capabilities
}

/// The per-operation DDL atomicity of a document store.
pub(crate) fn ddl_atomicity_support() -> DdlAtomicitySupport {
    let mut support = DdlAtomicitySupport::default();
    for operation in MIGRATION_OPERATIONS {
        support
            .by_operation
            .insert(operation.to_string(), DdlAtomicity::Unknown);
    }
    support
}

/// The operation names this provider files its DDL atomicity under.
#[cfg(test)]
pub(crate) fn migration_operation_keys() -> &'static [&'static str; 33] {
    &MIGRATION_OPERATIONS
}

/// mongodb's namespace is the database: there is no catalog above it and no
/// schema inside it. Collections are not a namespace level this provider
/// advertises, because they are addressed per statement rather than as part of
/// a resolved target.
pub(crate) fn mongodb_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, false),
            NamespaceLevel::absent(NamespaceLevelKind::Catalog),
            NamespaceLevel::absent(NamespaceLevelKind::Schema),
        ],
        path_segments: vec![NamespacePathSegment {
            position: 0,
            kind: NamespaceLevelKind::Database,
            meaning: "database".to_string(),
        }],
        // Whether a given deployment treats database names case-sensitively is a
        // server configuration question this crate never checks, so neither
        // side is claimed.
        case_rules: CaseRules {
            unquoted: CaseFolding::Unknown,
            quoted: CaseFolding::Unknown,
        },
        canonical_id_rules: CanonicalIdRules {
            case_sensitive: false,
            delimiter: ".".to_string(),
        },
        aliases: BTreeMap::new(),
    }
}
