//! Port of
//! `packages/frontend/src/windows/schema-diff/schemaDiffObjectDiscovery.ts`
//! (T351): the schema-diff object-discovery run — ensure both endpoints,
//! list each side's relations, load the unified object catalog, restore the
//! previous selection, and answer the merged table picks — plus the
//! diagnostic vocabulary every discovery failure is reported with.
//!
//! The React file is three exports:
//!
//! - `discoveryFailure(stage, error, phase?)` folds any thrown value into
//!   the banner key `schemaObjectDiscovery.<stage>[:<phase>]:<category>`.
//!   Only fixed stages and explicitly allowed categories enter the
//!   discovery banner: a registered `ApiError` code, the failure kind of
//!   `executeTargetCommand`'s controlled execution receipt
//!   (`<kind>; effect: <effect>`), one of the four builtin JS error class
//!   names, or `unknown`.
//! - `SchemaObjectDiscoveryError` — the Error subclass the run throws so
//!   `SchemaDiffWindow` can show `error.message` (the human text) and
//!   `error.diagnostic` (this key) separately.
//! - `discoverSchemaDiffObjects(endpoints, objects)` — the orchestration.
//!   `null` (not an error) means "quietly stop": `ensureConnected` already
//!   reported its own failure, or a changed endpoint invalidated the
//!   catalog mid-run (do not publish stale table picks).
//!
//! ## What maps where
//!
//! - `DiscoveryStage` → [`DiscoveryStageVm`], wire spellings verbatim. The
//!   two `ensure-*` stages are produced by `useSchemaDiffEndpoints` (T384),
//!   the caller-to-be; the three listing/catalog stages are wrapped here.
//! - `TargetCommandPhase` (`lib/session/targetCommandPhase.ts`, unported)
//!   → [`TargetCommandPhaseVm`], the full six-boundary union. The session
//!   module's `ReadPhase` carries only the two the execution projection
//!   emits; this local union is the seam until that file ports.
//! - `ApiError` + `API_ERROR_CODES` (`@datazen/backend-client`) → the
//!   [`API_ERROR_CODES`] const and [`DiscoveryErrorVm::Api`]. The runtime
//!   guard `API_ERROR_CODES.includes(error.code)` stays a runtime string
//!   membership check, so an out-of-set code still falls through to the
//!   plain-Error branch exactly like React (where `ApiError`'s class name
//!   is none of the four builtins → `unknown`).
//! - The receipt regex is hand-rolled ([`receipt_category`]):
//!   `split_once("; effect: ")` plus closed-set membership is the anchored
//!   regex — neither set's literals contain the separator, and a message
//!   with two separators fails the tail membership exactly as it fails `$`.
//! - Host/IPC seams: `endpoints.ensureConnected`, `databaseCommands.listTables`
//!   (with the `onPhase` hook the run's `sourcePhase`/`targetPhase` locals
//!   close over), `objects.load`, `objects.selected*Objects` and
//!   `objects.restoreSelection` become the [`SchemaDiffDiscoveryHost`]
//!   trait the owning window state implements (the wizard track); the
//!   endpoint fields the run reads travel as the
//!   [`SchemaDiffDiscoveryEndpoints`] call-time snapshot.
//! - The run's return value is `mergeSchemaDiffTablePicks(...)` from
//!   `schemaDiffTableNames.ts` (T374) — ported 1:1 as the sibling
//!   [`crate::views::schema_diff::schema_diff_table_names`] seam module
//!   (the deploy panel's T379-seam precedent).
//! - `SchemaDiffObjectCatalog` / `SchemaDiffObjectIdentity` reuse T363's
//!   [`SchemaDiffObjectIdentityVm`] — the same identity the picker and the
//!   saved setups key on.
//!
//! ## Keys
//!
//! The React file has no `data-testid` values. The contract is the
//! diagnostic string itself: `schemaObjectDiscovery.<stage>:<phase>:<category>`
//! with the whole `<phase>:` slot omitted when the phase is absent. Stage,
//! phase and category spell verbatim (`ensure-source`, `submission-token`,
//! `sqlError`, `SessionLost`, …).
//!
//! Known deltas (documented, not silent):
//!
//! - React's two `listTables` calls race inside `Promise.all`; the port
//!   lists source then target, so when both fail the source failure wins
//!   deterministically instead of whichever rejection landed first.
//! - The selection snapshot is read before the catalog load: React's
//!   closure still sees the pre-load memo after `load` resets the keys.

// The production consumer is the schema-diff wizard track (the window
// state that implements the host trait); until then the inline tests are
// the only callers, so the dead-code lint is silenced here rather than in
// a crate-wide allow.
#![allow(dead_code)]

use super::schema_diff_object_identity::SchemaDiffObjectIdentityVm;
use super::schema_diff_table_names::{
    merge_schema_diff_table_picks, TableInfoVm, SchemaDiffTablePickVm,
};

/// React `TargetCommandPhase` (`lib/session/targetCommandPhase.ts`): the
/// fixed boundaries a target command reports, in call order. Callbacks
/// never receive request or error content — only these markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetCommandPhaseVm {
    Profiles,
    SubmissionToken,
    TargetAdmission,
    ExecutionView,
    ArtifactRead,
    Dispose,
}

impl TargetCommandPhaseVm {
    /// The wire spelling (the diagnostic's middle slot).
    pub(crate) fn wire_str(self) -> &'static str {
        match self {
            Self::Profiles => "profiles",
            Self::SubmissionToken => "submission-token",
            Self::TargetAdmission => "target-admission",
            Self::ExecutionView => "execution-view",
            Self::ArtifactRead => "artifact-read",
            Self::Dispose => "dispose",
        }
    }
}

/// React `DiscoveryStage` (`schemaDiffObjectDiscovery.ts:8`): the fixed
/// stages of a discovery run. `EnsureSource`/`EnsureTarget` are reported by
/// the endpoints hook (T384); the rest are wrapped by the run itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryStageVm {
    EnsureSource,
    EnsureTarget,
    ListSourceTables,
    ListTargetTables,
    ObjectCatalog,
}

impl DiscoveryStageVm {
    /// The wire spelling (the diagnostic's first slot).
    pub(crate) fn wire_str(self) -> &'static str {
        match self {
            Self::EnsureSource => "ensure-source",
            Self::EnsureTarget => "ensure-target",
            Self::ListSourceTables => "list-source-tables",
            Self::ListTargetTables => "list-target-tables",
            Self::ObjectCatalog => "object-catalog",
        }
    }
}

/// React `API_ERROR_CODES` (`backend-client/src/errors.ts:61`): the §13
/// code table, in document order. Declared as a runtime array because the
/// categorizer validates a string code against it (`includes`).
pub(crate) const API_ERROR_CODES: [&str; 31] = [
    "InvalidArgument",
    "TargetRequired",
    "TargetConflict",
    "TargetUnsupported",
    "SessionNotFound",
    "SessionLost",
    "RuntimeEpochMismatch",
    "ContextConflict",
    "PermissionDenied",
    "ResourceBusy",
    "QueueFull",
    "TransactionResolutionRequired",
    "CapabilityUnsupported",
    "UnsupportedPlan",
    "EndpointOverlap",
    "SessionQuotaExceeded",
    "IdempotencyExpired",
    "RollbackFailed",
    "CleanupFailed",
    "OutcomeUnknown",
    "PlanStale",
    "SourceChanged",
    "TargetConflictRows",
    "IdempotencyConflict",
    "Unauthenticated",
    "NotFound",
    "PayloadTooLarge",
    "QuotaExceeded",
    "RateLimited",
    "ServiceUnavailable",
    "ConfigRevisionMismatch",
];

/// The receipt kinds `executeTargetCommand` can throw
/// (`targetCommand.ts:59`, `${view.errorCode}; effect: ${view.effectOutcome}`).
const RECEIPT_KINDS: [&str; 7] = [
    "sqlError",
    "protocolError",
    "cancelled",
    "timeout",
    "resourceLost",
    "pipelineAborted",
    "hostRejected",
];

/// The receipt effects (the second half of the controlled string).
const RECEIPT_EFFECTS: [&str; 5] = [
    "notStarted", "completed", "rolledBack", "partiallyApplied", "unknown",
];

/// The JS builtin error class names allowed into the banner verbatim.
const PLAIN_ERROR_NAMES: [&str; 4] = ["Error", "TypeError", "RangeError", "TimeoutError"];

/// The `error` argument of React `discoveryFailure` — the JS error taxonomy
/// the categorizer walks, as the host seam delivers it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DiscoveryErrorVm {
    /// `error instanceof ApiError` — the §13 code plus the message.
    Api { code: String, message: String },
    /// Any other `instanceof Error` — the JS class name plus the message.
    Plain { name: String, message: String },
    /// A thrown non-Error value; `display` is React's `String(error)`.
    Opaque { display: String },
}

impl DiscoveryErrorVm {
    /// React's category rule: the registered ApiError code, else the
    /// receipt kind from the message, else the builtin class name, else
    /// `unknown`.
    fn category(&self) -> String {
        match self {
            // `error instanceof ApiError && API_ERROR_CODES.includes(error.code)`.
            Self::Api { code, .. } if API_ERROR_CODES.contains(&code.as_str()) => code.clone(),
            // ApiError extends Error, so an out-of-set code walks the plain
            // branch with the class name 'ApiError' — receipt first, then
            // (never one of the four) `unknown`.
            Self::Api { message, .. } => plain_category("ApiError", message),
            Self::Plain { name, message } => plain_category(name, message),
            Self::Opaque { .. } => "unknown".to_owned(),
        }
    }

    /// React `error instanceof Error ? error.message : String(error)` —
    /// the `SchemaObjectDiscoveryError` message.
    fn display_message(&self) -> String {
        match self {
            Self::Api { message, .. } | Self::Plain { message, .. } => message.clone(),
            Self::Opaque { display } => display.clone(),
        }
    }
}

/// The plain-Error branch: the receipt kind from the message, else the
/// class name when it is one of the four builtins, else `unknown`.
fn plain_category(name: &str, message: &str) -> String {
    if let Some(kind) = receipt_category(message) {
        return kind.to_owned();
    }
    if PLAIN_ERROR_NAMES.contains(&name) {
        name.to_owned()
    } else {
        "unknown".to_owned()
    }
}

/// The receipt regex, hand-rolled: the message must be exactly
/// `<kind>; effect: <effect>` with both halves in their closed sets. Only
/// the kind enters the category — the effect half is validation, so a
/// half-written receipt falls through to the class-name rule.
fn receipt_category(message: &str) -> Option<&'static str> {
    let (kind, effect) = message.split_once("; effect: ")?;
    if !RECEIPT_EFFECTS.contains(&effect) {
        return None;
    }
    RECEIPT_KINDS.iter().copied().find(|candidate| *candidate == kind)
}

/// React `discoveryFailure` (`schemaDiffObjectDiscovery.ts:11`): the
/// discovery banner key. Only fixed stages and explicitly allowed
/// categories enter the string; the `:<phase>` slot is omitted entirely
/// (no trailing colon) when the phase is absent.
pub(crate) fn discovery_failure(
    stage: DiscoveryStageVm,
    error: &DiscoveryErrorVm,
    phase: Option<TargetCommandPhaseVm>,
) -> String {
    let mut diagnostic = format!("schemaObjectDiscovery.{}:", stage.wire_str());
    if let Some(phase) = phase {
        diagnostic.push_str(phase.wire_str());
        diagnostic.push(':');
    }
    diagnostic.push_str(&error.category());
    diagnostic
}

/// React `SchemaObjectDiscoveryError` (`schemaDiffObjectDiscovery.ts:22`):
/// the message the window shows plus the diagnostic it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SchemaObjectDiscoveryError {
    /// `error instanceof Error ? error.message : String(error)`.
    pub(crate) message: String,
    /// `discoveryFailure(stage, error, phase)`.
    pub(crate) diagnostic: String,
}

impl SchemaObjectDiscoveryError {
    pub(crate) fn new(
        stage: DiscoveryStageVm,
        error: &DiscoveryErrorVm,
        phase: Option<TargetCommandPhaseVm>,
    ) -> Self {
        Self {
            message: error.display_message(),
            diagnostic: discovery_failure(stage, error, phase),
        }
    }
}

// ---- the discovery run ------------------------------------------------------

/// The side of a schema diff (`'source' | 'target'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoverySide {
    Source,
    Target,
}

/// The endpoint fields `discoverSchemaDiffObjects` reads off
/// `UseSchemaDiffEndpointsReturn` at call time. Schemas keep the React
/// spelling: `''` is "no schema" and the run folds it with `||` (JS
/// truthiness — only the empty string folds, whitespace does not).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaDiffDiscoveryEndpoints {
    /// `endpoints.sourceId` / `targetId` — the connection ids (the
    /// `connectionId` IPC arguments).
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    pub(crate) source_schema: String,
    pub(crate) target_schema: String,
}

/// React `SchemaDiffObjectCatalog` (`useSchemaDiffUnifiedObjects.ts:8`) —
/// the unified-object half of a discovery run (the T420 seam's shape).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaDiffObjectCatalogVm {
    pub(crate) source_objects: Vec<SchemaDiffObjectIdentityVm>,
    pub(crate) target_objects: Vec<SchemaDiffObjectIdentityVm>,
}

/// The host wiring for one discovery run — the three objects React's
/// `discoverSchemaDiffObjects(endpoints, objects)` closes over, reduced to
/// the calls it makes. The owning window state implements it (the
/// schema-diff wizard track); this port's tests implement it with a script.
pub(crate) trait SchemaDiffDiscoveryHost {
    /// `endpoints.ensureConnected(side)` — the db session id, or `None`
    /// when the hook already reported the failure itself.
    fn ensure_connected(&mut self, side: DiscoverySide) -> Option<String>;
    /// `databaseCommands.listTables(dbSessionId, database, schema ?? null,
    /// connectionId, onPhase)`. The phase sink is React's `onPhase` hook
    /// (the run's `sourcePhase`/`targetPhase` locals); it must observe the
    /// last boundary reached, which is the phase the wrapped error carries.
    fn list_tables(
        &mut self,
        db_session_id: &str,
        database: &str,
        schema: Option<&str>,
        connection_id: &str,
        on_phase: &mut dyn FnMut(TargetCommandPhaseVm),
    ) -> Result<Vec<TableInfoVm>, DiscoveryErrorVm>;
    /// `objects.selectedSourceObjects` / `selectedTargetObjects` — the
    /// selection memos the run closes over. Read before `load_catalog`:
    /// React's closure still sees the pre-load memo after `load` resets
    /// the selected keys.
    fn selected_source_objects(&self) -> Vec<SchemaDiffObjectIdentityVm>;
    fn selected_target_objects(&self) -> Vec<SchemaDiffObjectIdentityVm>;
    /// `objects.load(source, target, sourceDatabase, targetDatabase,
    /// sourceSchema, targetSchema, sourceId, targetId)` — `Ok(None)` is the
    /// invalidated request (a changed endpoint); the schemas arrive raw
    /// (the hook's `listObjects` does its own `|| null` fold).
    fn load_catalog(
        &mut self,
        source_session: &str,
        target_session: &str,
        endpoints: &SchemaDiffDiscoveryEndpoints,
    ) -> Result<Option<SchemaDiffObjectCatalogVm>, DiscoveryErrorVm>;
    /// `objects.restoreSelection(catalog, desiredSource, desiredTarget)`.
    fn restore_selection(
        &mut self,
        catalog: &SchemaDiffObjectCatalogVm,
        desired_source: &[SchemaDiffObjectIdentityVm],
        desired_target: &[SchemaDiffObjectIdentityVm],
    );
}

/// React's `schema || null` / `schema || undefined` fold: only the empty
/// string is falsy — a whitespace schema is a real value.
fn non_empty(schema: &str) -> Option<&str> {
    if schema.is_empty() {
        None
    } else {
        Some(schema)
    }
}

/// React `discoverSchemaDiffObjects` (`schemaDiffObjectDiscovery.ts:30`).
///
/// `Ok(None)` is React's `null` — the quiet stop, not a failure. `Err` is
/// the `SchemaObjectDiscoveryError` the window catches to show the message
/// and report the diagnostic.
pub(crate) fn discover_schema_diff_objects<H: SchemaDiffDiscoveryHost>(
    host: &mut H,
    endpoints: &SchemaDiffDiscoveryEndpoints,
) -> Result<Option<Vec<SchemaDiffTablePickVm>>, SchemaObjectDiscoveryError> {
    let source = match host.ensure_connected(DiscoverySide::Source) {
        Some(session) => session,
        None => return Ok(None),
    };
    let target = match host.ensure_connected(DiscoverySide::Target) {
        Some(session) => session,
        None => return Ok(None),
    };
    // The selection memos React's closure captured — read before `load`
    // resets them.
    let desired_source = host.selected_source_objects();
    let desired_target = host.selected_target_objects();
    let mut source_phase = TargetCommandPhaseVm::Profiles;
    let source_rows = host
        .list_tables(
            &source,
            &endpoints.source_database,
            non_empty(&endpoints.source_schema),
            &endpoints.source_id,
            &mut |phase| source_phase = phase,
        )
        .map_err(|error| {
            SchemaObjectDiscoveryError::new(
                DiscoveryStageVm::ListSourceTables,
                &error,
                Some(source_phase),
            )
        })?;
    let mut target_phase = TargetCommandPhaseVm::Profiles;
    let target_rows = host
        .list_tables(
            &target,
            &endpoints.target_database,
            non_empty(&endpoints.target_schema),
            &endpoints.target_id,
            &mut |phase| target_phase = phase,
        )
        .map_err(|error| {
            SchemaObjectDiscoveryError::new(
                DiscoveryStageVm::ListTargetTables,
                &error,
                Some(target_phase),
            )
        })?;
    let catalog = host
        .load_catalog(&source, &target, endpoints)
        .map_err(|error| {
            SchemaObjectDiscoveryError::new(DiscoveryStageVm::ObjectCatalog, &error, None)
        })?;
    // A changed endpoint invalidates this catalog; do not publish stale
    // table picks.
    let catalog = match catalog {
        Some(catalog) => catalog,
        None => return Ok(None),
    };
    host.restore_selection(&catalog, &desired_source, &desired_target);
    // React folds the schemas with `|| undefined` for the merge; the merge
    // itself treats `''` as no filter (the same truthiness), so the raw
    // strings are that fold's behavior.
    Ok(Some(merge_schema_diff_table_picks(
        &source_rows,
        &target_rows,
        &endpoints.source_schema,
        &endpoints.target_schema,
    )))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use super::super::schema_diff_object_identity::SchemaDiffObjectKindVm;
    use super::super::schema_diff_table_names::{TablePickOriginVm, TableTypeVm};

    fn api(code: &str, message: &str) -> DiscoveryErrorVm {
        DiscoveryErrorVm::Api { code: code.to_owned(), message: message.to_owned() }
    }

    fn plain(name: &str, message: &str) -> DiscoveryErrorVm {
        DiscoveryErrorVm::Plain { name: name.to_owned(), message: message.to_owned() }
    }

    fn table(schema: Option<&str>, name: &str) -> TableInfoVm {
        TableInfoVm {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type: TableTypeVm::Table,
            row_count: None,
        }
    }

    fn relation(kind: TableTypeVm, schema: Option<&str>, name: &str) -> TableInfoVm {
        TableInfoVm { table_type: kind, ..table(schema, name) }
    }

    fn view_object(name: &str) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::View,
            schema: None,
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    fn endpoints() -> SchemaDiffDiscoveryEndpoints {
        SchemaDiffDiscoveryEndpoints {
            source_id: "conn-a".to_owned(),
            target_id: "conn-b".to_owned(),
            source_database: "sales".to_owned(),
            target_database: "sales_copy".to_owned(),
            source_schema: "public".to_owned(),
            target_schema: String::new(),
        }
    }

    fn pick(
        name: &str,
        enabled: bool,
        origin: TablePickOriginVm,
        source_name: Option<&str>,
        target_name: Option<&str>,
    ) -> SchemaDiffTablePickVm {
        SchemaDiffTablePickVm {
            name: name.to_owned(),
            enabled,
            origin,
            source_name: source_name.map(str::to_owned),
            target_name: target_name.map(str::to_owned),
        }
    }

    /// The scripted host: every seam answers from a script and records the
    /// calls it received. `load_catalog` clears the live selection the way
    /// the React hook resets the selected keys, so the port's read-before-
    /// load ordering stays observable.
    struct ScriptedHost {
        ensure_source: Option<String>,
        ensure_target: Option<String>,
        source_phases: Vec<TargetCommandPhaseVm>,
        source_listing: Result<Vec<TableInfoVm>, DiscoveryErrorVm>,
        target_phases: Vec<TargetCommandPhaseVm>,
        target_listing: Result<Vec<TableInfoVm>, DiscoveryErrorVm>,
        catalog: Result<Option<SchemaDiffObjectCatalogVm>, DiscoveryErrorVm>,
        selection: Vec<SchemaDiffObjectIdentityVm>,
        calls: Vec<&'static str>,
        listings: Vec<(String, String, Option<String>, String)>,
        loaded_sessions: Option<(String, String)>,
        restored: Option<(Vec<SchemaDiffObjectIdentityVm>, Vec<SchemaDiffObjectIdentityVm>)>,
    }

    impl ScriptedHost {
        fn scripted() -> Self {
            Self {
                ensure_source: Some("source-session".to_owned()),
                ensure_target: Some("target-session".to_owned()),
                source_phases: Vec::new(),
                source_listing: Ok(Vec::new()),
                target_phases: Vec::new(),
                target_listing: Ok(Vec::new()),
                catalog: Ok(Some(SchemaDiffObjectCatalogVm::default())),
                selection: Vec::new(),
                calls: Vec::new(),
                listings: Vec::new(),
                loaded_sessions: None,
                restored: None,
            }
        }
    }

    impl SchemaDiffDiscoveryHost for ScriptedHost {
        fn ensure_connected(&mut self, side: DiscoverySide) -> Option<String> {
            self.calls
                .push(match side { DiscoverySide::Source => "ensure-source", _ => "ensure-target" });
            match side {
                DiscoverySide::Source => self.ensure_source.clone(),
                DiscoverySide::Target => self.ensure_target.clone(),
            }
        }

        fn list_tables(
            &mut self,
            db_session_id: &str,
            database: &str,
            schema: Option<&str>,
            connection_id: &str,
            on_phase: &mut dyn FnMut(TargetCommandPhaseVm),
        ) -> Result<Vec<TableInfoVm>, DiscoveryErrorVm> {
            self.calls.push("list-tables");
            self.listings.push((
                db_session_id.to_owned(),
                database.to_owned(),
                schema.map(str::to_owned),
                connection_id.to_owned(),
            ));
            let (phases, outcome) = if db_session_id == "source-session" {
                (&self.source_phases, &self.source_listing)
            } else {
                (&self.target_phases, &self.target_listing)
            };
            for phase in phases {
                on_phase(*phase);
            }
            outcome.clone()
        }

        fn selected_source_objects(&self) -> Vec<SchemaDiffObjectIdentityVm> {
            self.selection.clone()
        }

        fn selected_target_objects(&self) -> Vec<SchemaDiffObjectIdentityVm> {
            Vec::new()
        }

        fn load_catalog(
            &mut self,
            source_session: &str,
            target_session: &str,
            _endpoints: &SchemaDiffDiscoveryEndpoints,
        ) -> Result<Option<SchemaDiffObjectCatalogVm>, DiscoveryErrorVm> {
            self.calls.push("load");
            self.loaded_sessions = Some((source_session.to_owned(), target_session.to_owned()));
            let outcome = self.catalog.clone();
            if let Ok(Some(_)) = &outcome {
                self.selection.clear();
            }
            outcome
        }

        fn restore_selection(
            &mut self,
            _catalog: &SchemaDiffObjectCatalogVm,
            desired_source: &[SchemaDiffObjectIdentityVm],
            desired_target: &[SchemaDiffObjectIdentityVm],
        ) {
            self.calls.push("restore");
            self.restored = Some((desired_source.to_vec(), desired_target.to_vec()));
        }
    }

    // ---- discoveryFailure ---------------------------------------------------

    /// A registered ApiError code is the category, verbatim; the phase slot
    /// splices between stage and category and is omitted without a phase.
    #[test]
    fn registered_api_code_is_the_category() {
        assert_eq!(
            discovery_failure(DiscoveryStageVm::ListSourceTables, &api("SessionLost", "gone"), None),
            "schemaObjectDiscovery.list-source-tables:SessionLost"
        );
        assert_eq!(
            discovery_failure(
                DiscoveryStageVm::ListTargetTables,
                &api("ResourceBusy", "busy"),
                Some(TargetCommandPhaseVm::TargetAdmission),
            ),
            "schemaObjectDiscovery.list-target-tables:target-admission:ResourceBusy"
        );
    }

    /// The receipt kind from `executeTargetCommand`'s controlled string
    /// beats the error class name, for every registered kind.
    #[test]
    fn receipt_kind_beats_the_error_name() {
        for kind in RECEIPT_KINDS {
            let error = plain("Error", &format!("{kind}; effect: rolledBack"));
            assert_eq!(
                discovery_failure(DiscoveryStageVm::ObjectCatalog, &error, None),
                format!("schemaObjectDiscovery.object-catalog:{kind}")
            );
        }
    }

    /// The receipt only counts when both halves are registered and the
    /// message is nothing else — otherwise the class-name rule applies.
    #[test]
    fn receipt_requires_registered_halves_and_full_match() {
        for message in [
            "transientError; effect: rolledBack",
            "sqlError; effect: applied",
            "failed: sqlError; effect: rolledBack",
            "sqlError; effect: rolledBack; effect: unknown",
            "sqlError; effect: rolledBack\n",
            "",
        ] {
            assert_eq!(
                discovery_failure(DiscoveryStageVm::ObjectCatalog, &plain("Error", message), None),
                "schemaObjectDiscovery.object-catalog:Error",
                "message {message:?} must not parse as a receipt"
            );
        }
    }

    /// The four builtin error names pass through; other names and thrown
    /// non-Error values fold to `unknown`.
    #[test]
    fn builtin_names_pass_and_everything_else_is_unknown() {
        for name in PLAIN_ERROR_NAMES {
            assert_eq!(
                discovery_failure(DiscoveryStageVm::EnsureSource, &plain(name, "boom"), None),
                format!("schemaObjectDiscovery.ensure-source:{name}")
            );
        }
        assert_eq!(
            discovery_failure(DiscoveryStageVm::EnsureTarget, &plain("SyntaxError", "boom"), None),
            "schemaObjectDiscovery.ensure-target:unknown"
        );
        assert_eq!(
            discovery_failure(
                DiscoveryStageVm::EnsureTarget,
                &DiscoveryErrorVm::Opaque { display: "weird".to_owned() },
                None,
            ),
            "schemaObjectDiscovery.ensure-target:unknown"
        );
    }

    /// An ApiError with an unregistered code walks the plain-Error branch
    /// (receipt first, `ApiError` never one of the four builtins → unknown)
    /// — the runtime `includes` guard, kept.
    #[test]
    fn out_of_set_api_code_walks_the_plain_branch() {
        assert_eq!(
            discovery_failure(DiscoveryStageVm::ObjectCatalog, &api("MadeUpCode", "boom"), None),
            "schemaObjectDiscovery.object-catalog:unknown"
        );
        assert_eq!(
            discovery_failure(
                DiscoveryStageVm::ObjectCatalog,
                &api("MadeUpCode", "timeout; effect: unknown"),
                None,
            ),
            "schemaObjectDiscovery.object-catalog:timeout"
        );
    }

    /// The five stages and six phases wire-spell verbatim (the diagnostic
    /// string's slots).
    #[test]
    fn stages_and_phases_wire_spell_verbatim() {
        let stages = [
            (DiscoveryStageVm::EnsureSource, "ensure-source"),
            (DiscoveryStageVm::EnsureTarget, "ensure-target"),
            (DiscoveryStageVm::ListSourceTables, "list-source-tables"),
            (DiscoveryStageVm::ListTargetTables, "list-target-tables"),
            (DiscoveryStageVm::ObjectCatalog, "object-catalog"),
        ];
        let phases = [
            (TargetCommandPhaseVm::Profiles, "profiles"),
            (TargetCommandPhaseVm::SubmissionToken, "submission-token"),
            (TargetCommandPhaseVm::TargetAdmission, "target-admission"),
            (TargetCommandPhaseVm::ExecutionView, "execution-view"),
            (TargetCommandPhaseVm::ArtifactRead, "artifact-read"),
            (TargetCommandPhaseVm::Dispose, "dispose"),
        ];
        for (stage, wire) in stages {
            assert_eq!(stage.wire_str(), wire);
        }
        for (phase, wire) in phases {
            assert_eq!(phase.wire_str(), wire);
        }
    }

    /// The §13 code table is the React array, verbatim and in order.
    #[test]
    fn the_api_code_table_is_the_react_table() {
        assert_eq!(
            API_ERROR_CODES,
            [
                "InvalidArgument", "TargetRequired", "TargetConflict", "TargetUnsupported",
                "SessionNotFound", "SessionLost", "RuntimeEpochMismatch", "ContextConflict",
                "PermissionDenied", "ResourceBusy", "QueueFull", "TransactionResolutionRequired",
                "CapabilityUnsupported", "UnsupportedPlan", "EndpointOverlap",
                "SessionQuotaExceeded", "IdempotencyExpired", "RollbackFailed", "CleanupFailed",
                "OutcomeUnknown", "PlanStale", "SourceChanged", "TargetConflictRows",
                "IdempotencyConflict", "Unauthenticated", "NotFound", "PayloadTooLarge",
                "QuotaExceeded", "RateLimited", "ServiceUnavailable", "ConfigRevisionMismatch",
            ]
        );
    }

    /// The thrown error carries the human message (or `String(error)`) and
    /// the diagnostic side by side.
    #[test]
    fn discovery_error_carries_message_and_diagnostic() {
        let error = SchemaObjectDiscoveryError::new(
            DiscoveryStageVm::ListSourceTables,
            &plain("Error", "sqlError; effect: rolledBack"),
            Some(TargetCommandPhaseVm::ExecutionView),
        );
        assert_eq!(error.message, "sqlError; effect: rolledBack");
        assert_eq!(
            error.diagnostic,
            "schemaObjectDiscovery.list-source-tables:execution-view:sqlError"
        );
        let opaque = SchemaObjectDiscoveryError::new(
            DiscoveryStageVm::ObjectCatalog,
            &DiscoveryErrorVm::Opaque { display: "[object Object]".to_owned() },
            None,
        );
        assert_eq!(opaque.message, "[object Object]");
        assert_eq!(opaque.diagnostic, "schemaObjectDiscovery.object-catalog:unknown");
    }

    // ---- the run -------------------------------------------------------------

    /// Either `ensureConnected` failure is a quiet stop — no listing, no
    /// catalog load, nothing published.
    #[test]
    fn ensure_failure_stops_quietly() {
        let mut host = ScriptedHost::scripted();
        host.ensure_source = None;
        assert_eq!(discover_schema_diff_objects(&mut host, &endpoints()).unwrap(), None);
        assert_eq!(host.calls, vec!["ensure-source"]);

        let mut host = ScriptedHost::scripted();
        host.ensure_target = None;
        assert_eq!(discover_schema_diff_objects(&mut host, &endpoints()).unwrap(), None);
        assert_eq!(host.calls, vec!["ensure-source", "ensure-target"]);
    }

    /// A listing failure throws the wrapped error: the stage names the side,
    /// the phase is the last boundary the hook observed — and 'profiles'
    /// (the captured local's initial value) when none was observed.
    #[test]
    fn listing_failure_reports_stage_and_last_phase() {
        let mut host = ScriptedHost::scripted();
        host.source_phases = vec![
            TargetCommandPhaseVm::Profiles,
            TargetCommandPhaseVm::SubmissionToken,
            TargetCommandPhaseVm::TargetAdmission,
        ];
        host.source_listing = Err(plain("Error", "protocolError; effect: partiallyApplied"));
        let error = discover_schema_diff_objects(&mut host, &endpoints()).unwrap_err();
        assert_eq!(error.message, "protocolError; effect: partiallyApplied");
        assert_eq!(
            error.diagnostic,
            "schemaObjectDiscovery.list-source-tables:target-admission:protocolError"
        );
        assert_eq!(host.calls, vec!["ensure-source", "ensure-target", "list-tables"]);

        let mut host = ScriptedHost::scripted();
        host.target_phases = vec![TargetCommandPhaseVm::ArtifactRead];
        host.target_listing = Err(api("NotFound", "nope"));
        let error = discover_schema_diff_objects(&mut host, &endpoints()).unwrap_err();
        assert_eq!(
            error.diagnostic,
            "schemaObjectDiscovery.list-target-tables:artifact-read:NotFound"
        );

        // Fails before the first boundary: the initial 'profiles'.
        let mut host = ScriptedHost::scripted();
        host.source_listing = Err(api("SessionLost", "gone"));
        let error = discover_schema_diff_objects(&mut host, &endpoints()).unwrap_err();
        assert_eq!(
            error.diagnostic,
            "schemaObjectDiscovery.list-source-tables:profiles:SessionLost"
        );
    }

    /// The catalog failure wraps with the object-catalog stage and never a
    /// phase (React passes none).
    #[test]
    fn catalog_failure_reports_object_catalog_without_phase() {
        let mut host = ScriptedHost::scripted();
        host.catalog = Err(api("ServiceUnavailable", "down"));
        let error = discover_schema_diff_objects(&mut host, &endpoints()).unwrap_err();
        assert_eq!(
            error.diagnostic,
            "schemaObjectDiscovery.object-catalog:ServiceUnavailable"
        );
        assert_eq!(
            host.calls,
            vec!["ensure-source", "ensure-target", "list-tables", "list-tables", "load"]
        );
    }

    /// An invalidated catalog (a changed endpoint) is the quiet stop: no
    /// restore, no stale picks.
    #[test]
    fn invalidated_catalog_stops_before_restore() {
        let mut host = ScriptedHost::scripted();
        host.catalog = Ok(None);
        assert_eq!(discover_schema_diff_objects(&mut host, &endpoints()).unwrap(), None);
        assert!(!host.calls.contains(&"restore"));
    }

    /// The happy path: both sides listed with their exact IPC arguments
    /// (the `|| null` schema fold included), the catalog loaded, the
    /// pre-load selection restored, and the merged picks published.
    #[test]
    fn happy_path_lists_loads_restores_and_merges() {
        let mut host = ScriptedHost::scripted();
        host.source_listing = Ok(vec![
            table(Some("public"), "orders"),
            table(Some("public"), "users"),
            // Empty-name sentinel and a view: never presented as tables.
            relation(TableTypeVm::Table, Some("public"), ""),
            relation(TableTypeVm::View, Some("public"), "orders_view"),
        ]);
        host.target_listing =
            Ok(vec![table(None, "orders"), table(Some("reports"), "report_lines")]);
        host.selection = vec![view_object("old_selection")];
        let picks = discover_schema_diff_objects(&mut host, &endpoints())
            .unwrap()
            .unwrap();
        assert_eq!(
            host.calls,
            vec![
                "ensure-source", "ensure-target", "list-tables", "list-tables", "load", "restore"
            ]
        );
        // Source listing: session, database, schema ('public'), connection.
        assert_eq!(
            host.listings[0],
            ("source-session".into(), "sales".into(), Some("public".into()), "conn-a".into())
        );
        // Target listing: the empty schema folds to None.
        assert_eq!(
            host.listings[1],
            ("target-session".into(), "sales_copy".into(), None, "conn-b".into())
        );
        // The catalog load got both ensured sessions.
        assert_eq!(
            host.loaded_sessions,
            Some(("source-session".into(), "target-session".into()))
        );
        // The restore saw the pre-load selection, not the emptied one.
        assert_eq!(host.restored, Some((vec![view_object("old_selection")], Vec::new())));
        assert_eq!(
            picks,
            vec![
                pick("public.orders", true, TablePickOriginVm::Both, Some("public.orders"), Some("orders")),
                pick("public.users", true, TablePickOriginVm::SourceOnly, Some("public.users"), None),
                pick("reports.report_lines", false, TablePickOriginVm::TargetOnly, None, Some("reports.report_lines")),
            ]
        );
    }
}
