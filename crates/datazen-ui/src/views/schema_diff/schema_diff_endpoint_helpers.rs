//! Endpoint helpers carried for the `useSchemaDiffEndpoints.ts` port
//! (T384).
//!
//! Sources of truth — all **other segments'** ledger entries, carried
//! until they land and absorb them (the `lib_helpers` /
//! `grid_clipboard_helpers` precedent):
//!
//! * `lib/endpointIdentity.ts` (T482): [`EndpointIdentity`] +
//!   [`is_same_endpoint_identity`].
//! * `lib/migrationWindowPrefill.ts` (T563):
//!   [`MigrationWindowPrefill`], [`resolve_default_database`],
//!   [`pick_prefill_database`], [`pick_prefill_schema`] (the
//!   consume-once URL prefill picks; `readMigrationPrefillFromUrl` is
//!   the host's URL seam, `useMigrationEndpointPrefill` the mount
//!   effect the caller runs).
//! * `windows/data-sync/utils.ts`'s [`pick_default_schema`].
//! * `@datazen/backend-client`'s `API_ERROR_CODES` (§13 table):
//!   [`is_api_error_code`], behind the discovery-failure classifier.
//! * `lib/dedicatedDbSession.ts` (T467): the
//!   [`DedicatedSideSession`] shape only — the ref-counted
//!   ensure/release machinery is the host's session manager.
//!
//! Not carried: `DB_REGISTRY` (`lib/databaseTypes.ts`, T450) — the
//! per-type meta is build-time injected in React; the port takes the
//! two flags it reads as a host-supplied
//! [`EndpointDbMeta`](super::use_schema_diff_endpoints::EndpointDbMeta).
//!
//! Known deltas: none behavioural — every rule is the source's
//! verbatim.

/// `EndpointIdentity`: the three fields the same-place rule reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointIdentity {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
}

/// `isSameEndpointIdentity`: true when both sides point at the same
/// connection, database and schema. An empty database on either side
/// never matches — "the same place" has to be a named place.
pub fn is_same_endpoint_identity(source: &EndpointIdentity, target: &EndpointIdentity) -> bool {
    source.connection_id == target.connection_id
        && !source.database.is_empty()
        && !target.database.is_empty()
        && source.database == target.database
        && source.schema.trim() == target.schema.trim()
}

/// `MigrationWindowPrefill`: the URL parameters a migration window
/// consumes once (absent key = not prefilled).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationWindowPrefill {
    pub source_id: Option<String>,
    pub target_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

/// `resolveDefaultDatabase(databases, preferred, prev)`: the
/// configured database when offered, the previous when still offered,
/// else the first, else empty.
pub fn resolve_default_database(
    databases: Option<&[String]>,
    preferred: &str,
    prev: &str,
) -> String {
    let list = databases.unwrap_or(&[]);
    if list.iter().any(|d| d == preferred) {
        return preferred.to_owned();
    }
    if !prev.is_empty() && list.iter().any(|d| d == prev) {
        return prev.to_owned();
    }
    list.first().cloned().unwrap_or_default()
}

/// `pickPrefillDatabase`: consume the prefill once when the offered
/// list contains it, else the fallback.
pub fn pick_prefill_database(
    prefill: &mut MigrationWindowPrefill,
    side: crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide,
    databases: &[String],
    fallback: impl FnOnce(&str) -> String,
    prev: &str,
) -> String {
    let slot = match side {
        crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide::Source => {
            &mut prefill.source_database
        }
        crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide::Target => {
            &mut prefill.target_database
        }
    };
    if let Some(value) = slot.clone() {
        if databases.contains(&value) {
            *slot = None;
            return value;
        }
    }
    fallback(prev)
}

/// `pickPrefillSchema`: the schema face of the same consume-once rule.
pub fn pick_prefill_schema(
    prefill: &mut MigrationWindowPrefill,
    side: crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide,
    schemas: &[String],
    fallback: impl FnOnce(&str) -> String,
    prev: &str,
) -> String {
    let slot = match side {
        crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide::Source => {
            &mut prefill.source_schema
        }
        crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide::Target => {
            &mut prefill.target_schema
        }
    };
    if let Some(value) = slot.clone() {
        if schemas.contains(&value) {
            *slot = None;
            return value;
        }
    }
    fallback(prev)
}

/// `pickDefaultSchema(schemas, previous)`: the previous when still
/// offered, `public` when present, else the first, else empty.
pub fn pick_default_schema(schemas: &[String], previous: Option<&str>) -> String {
    if let Some(previous) = previous {
        if schemas.iter().any(|s| s == previous) {
            return previous.to_owned();
        }
    }
    if schemas.iter().any(|s| s == "public") {
        return "public".to_owned();
    }
    schemas.first().cloned().unwrap_or_default()
}

/// The §13 API error codes, in document order
/// (`API_ERROR_CODES`).
pub const API_ERROR_CODES: &[&str] = &[
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

/// `isApiErrorCode(value)`.
pub fn is_api_error_code(value: &str) -> bool {
    API_ERROR_CODES.contains(&value)
}

/// `DedicatedSideSession` (the shape): the dedicated sub-window
/// session a side holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedSideSession {
    pub connection_id: String,
    pub database: String,
    pub db_session_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide;

    fn identity(connection: &str, database: &str, schema: &str) -> EndpointIdentity {
        EndpointIdentity {
            connection_id: connection.to_owned(),
            database: database.to_owned(),
            schema: schema.to_owned(),
        }
    }

    #[test]
    fn the_same_place_rule_requires_a_named_place() {
        let same = identity("c1", "db", "public");
        assert!(is_same_endpoint_identity(&same, &identity("c1", "db", "public")));
        // The schema compares after trimming.
        assert!(is_same_endpoint_identity(&same, &identity("c1", "db", " public ")));
        assert!(!is_same_endpoint_identity(&same, &identity("c1", "db", "other")));
        // A different connection or database is a different place.
        assert!(!is_same_endpoint_identity(&same, &identity("c2", "db", "public")));
        assert!(!is_same_endpoint_identity(&same, &identity("c1", "other", "public")));
        // An empty database never matches.
        assert!(!is_same_endpoint_identity(&same, &identity("c1", "", "public")));
        assert!(!is_same_endpoint_identity(
            &EndpointIdentity { connection_id: "c1".into(), database: String::new(), schema: String::new() },
            &same
        ));
    }

    #[test]
    fn the_default_database_prefers_configured_then_previous_then_first() {
        let list = vec!["a".to_string(), "b".to_string()];
        assert_eq!(resolve_default_database(Some(&list), "b", "a"), "b");
        assert_eq!(resolve_default_database(Some(&list), "gone", "a"), "a");
        assert_eq!(resolve_default_database(Some(&list), "gone", "also-gone"), "a");
        assert_eq!(resolve_default_database(None, "x", "y"), "");
        assert_eq!(resolve_default_database(Some(&[]), "x", "y"), "");
    }

    #[test]
    fn the_prefill_consumes_once_when_offered() {
        let mut prefill = MigrationWindowPrefill {
            source_database: Some("b".to_owned()),
            ..Default::default()
        };
        let list = vec!["a".to_string(), "b".to_string()];
        // First pick: the prefill wins and is consumed.
        assert_eq!(
            pick_prefill_database(&mut prefill, EndpointSide::Source, &list, |_| "a".into(), ""),
            "b"
        );
        // Second pick: the fallback answers (consumed).
        assert_eq!(
            pick_prefill_database(&mut prefill, EndpointSide::Source, &list, |_| "a".into(), ""),
            "a"
        );
        // A prefill not in the list does not consume.
        let mut prefill = MigrationWindowPrefill {
            target_schema: Some("gone".to_owned()),
            ..Default::default()
        };
        let schemas = vec!["public".to_string()];
        assert_eq!(
            pick_prefill_schema(&mut prefill, EndpointSide::Target, &schemas, |prev| {
                pick_default_schema(&schemas, Some(prev))
            }, ""),
            "public"
        );
        assert_eq!(prefill.target_schema, Some("gone".to_owned()), "not consumed");
    }

    #[test]
    fn the_default_schema_prefers_previous_then_public_then_first() {
        let schemas = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_default_schema(&schemas(&["a", "public"]), Some("a")), "a");
        assert_eq!(pick_default_schema(&schemas(&["a", "public"]), Some("gone")), "public");
        assert_eq!(pick_default_schema(&schemas(&["a", "b"]), None), "a");
        assert_eq!(pick_default_schema(&schemas(&[]), None), "");
    }

    #[test]
    fn the_error_code_table_is_the_section13_list() {
        assert_eq!(API_ERROR_CODES.len(), 31);
        assert!(is_api_error_code("SessionLost"));
        assert!(is_api_error_code("ConfigRevisionMismatch"));
        assert!(!is_api_error_code("unknown"));
        assert!(!is_api_error_code(""));
    }
}
