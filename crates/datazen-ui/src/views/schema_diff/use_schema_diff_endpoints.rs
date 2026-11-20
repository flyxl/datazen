//! Port of `packages/frontend/src/windows/schema-diff/
//! useSchemaDiffEndpoints.ts` (T384).
//!
//! The wizard's endpoint state: connections, the two sides'
//! id/database/schema picks, the dedicated sessions, the database
//! discovery with its owner gates, the catalog schema reads, and the
//! swap/validate/ensure seams the window drives.
//!
//! ## What maps where
//!
//! - Every async client read (`get_connections`,
//!   `listDatabasesDedicated`, `ensureDedicatedSession`,
//!   `listCatalog`, `pingConnection`) is a host seam; the machine is
//!   the state + the gates, fed by
//!   [`on_connections`]/[`on_databases`]/[`on_catalog`]/[
//!   `on_session`].
//! - The **discovery owner gates** (`discoveryOwners.current.sideId
//!   === sideId && .connections === connections`) become the
//!   connection [`epoch`](Self::on_connections) the discovery token
//!   carries: an answer from an older connections generation or an
//!   older side id is dropped whole.
//! - The cross-window bus (`datazen:connections-changed` → reload,
//!   `datazen:connection-closed` → drop the matching session) is
//!   [`note_connections_changed`] / [`on_connection_closed`].
//! - The effects' side resets: an empty side id clears the side
//!   ([`select_side`]); a database/schema pick just sets the value
//!   (the session effect notices the mismatch itself —
//!   [`session_is_fresh`]).
//! - `sessionDatabaseOverride` (SQLite's `main` alias) is
//!   [`session_database_override`], verbatim.
//! - The prefill ref is the carried consume-once
//!   [`MigrationWindowPrefill`]; `useMigrationEndpointPrefill`'s
//!   one-shot connection-id application is [`apply_connection_prefill`].
//! - `DB_REGISTRY`'s `supportsTables && supportsSQL` gate is the
//!   host-supplied [`EndpointDbMeta`] (the React registry is
//!   build-time injected; the port takes the two flags it reads).
//! - `validateEndpoints`'s three refusals are
//!   [`validate`], answered with the i18n key the React branch
//!   reports.
//!
//! Known deltas: `Promise.all` over the two sessions in
//! `refreshEndpointSessions` is the host's parallel connect; the
//! machine only records the answers.

use crate::views::schema_diff::schema_diff_endpoint_helpers::{
    DedicatedSideSession, EndpointIdentity, MigrationWindowPrefill, is_api_error_code,
    is_same_endpoint_identity, pick_default_schema, pick_prefill_database, pick_prefill_schema,
    resolve_default_database,
};

/// The side an endpoint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointSide {
    Source,
    Target,
}

/// The connection face the hook reads (the `ConnectionConfig` fields
/// it inspects).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EndpointConnection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) database_type: String,
    /// The configured database (the discovery's preferred pick).
    pub(crate) database: Option<String>,
    /// The configured schema (the catalog's preferred pick).
    pub(crate) schema: Option<String>,
}

/// `DB_REGISTRY[type]`'s two flags the catalog gate reads
/// (`supportsTables` / `supportsSQL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EndpointDbMeta {
    pub(crate) supports_tables: bool,
    pub(crate) supports_sql: bool,
}

/// One side's endpoint state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EndpointSideState {
    pub(crate) id: String,
    pub(crate) database: String,
    pub(crate) schema: String,
    pub(crate) databases: Vec<String>,
    pub(crate) schemas: Vec<String>,
    pub(crate) session: Option<DedicatedSideSession>,
}

/// `validateEndpoints`'s refusals, as the i18n keys React reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointValidation {
    SelectBoth,
    CannotSameDb,
    SelectDbRequired,
}

/// A discovery answer's identity (the owner gate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiscoveryToken {
    pub(crate) side: EndpointSide,
    pub(crate) connection_id: String,
    pub(crate) epoch: u64,
}

/// `useSchemaDiffEndpoints(options)`'s state machine.
pub(crate) struct UseSchemaDiffEndpoints {
    connections: Vec<EndpointConnection>,
    /// Increments on every connections load — the discovery owner
    /// gate's `connections` identity.
    epoch: u64,
    source: EndpointSideState,
    target: EndpointSideState,
    prefill: MigrationWindowPrefill,
    /// The discovery error messages per side (reported once via the
    /// error callback; the host clears by message).
    discovery_error: [Option<String>; 2],
}

impl UseSchemaDiffEndpoints {
    /// `useSchemaDiffEndpoints()` — with the URL prefill the mount
    /// read (`readMigrationPrefillFromUrl` is the host's seam).
    pub(crate) fn new(prefill: MigrationWindowPrefill) -> Self {
        Self {
            connections: Vec::new(),
            epoch: 0,
            source: EndpointSideState::default(),
            target: EndpointSideState::default(),
            prefill,
            discovery_error: [None, None],
        }
    }

    fn side(&self, side: EndpointSide) -> &EndpointSideState {
        match side {
            EndpointSide::Source => &self.source,
            EndpointSide::Target => &self.target,
        }
    }

    fn side_mut(&mut self, side: EndpointSide) -> &mut EndpointSideState {
        match side {
            EndpointSide::Source => &mut self.source,
            EndpointSide::Target => &mut self.target,
        }
    }

    fn discovery_error_slot(&mut self, side: EndpointSide) -> &mut Option<String> {
        match side {
            EndpointSide::Source => &mut self.discovery_error[0],
            EndpointSide::Target => &mut self.discovery_error[1],
        }
    }

    /// `connections` / `sourceConn` / `targetConn`.
    pub(crate) fn connections(&self) -> &[EndpointConnection] {
        &self.connections
    }

    pub(crate) fn source_conn(&self) -> Option<&EndpointConnection> {
        self.connections.iter().find(|c| c.id == self.source.id)
    }

    pub(crate) fn target_conn(&self) -> Option<&EndpointConnection> {
        self.connections.iter().find(|c| c.id == self.target.id)
    }

    /// `sourceId`/`targetId` and the sides.
    pub(crate) fn source(&self) -> &EndpointSideState {
        &self.source
    }

    pub(crate) fn target(&self) -> &EndpointSideState {
        &self.target
    }

    /// `connOptions` (both sides share it): `{ value, label }` pairs,
    /// the label `"{name} ({databaseType})"`.
    pub(crate) fn conn_options(&self) -> Vec<(String, String)> {
        self.connections
            .iter()
            .map(|c| (c.id.clone(), format!("{} ({})", c.name, c.database_type)))
            .collect()
    }

    /// `isCrossDialect`.
    pub(crate) fn is_cross_dialect(&self) -> bool {
        match (self.source_conn(), self.target_conn()) {
            (Some(source), Some(target)) => source.database_type != target.database_type,
            _ => false,
        }
    }

    /// `get_connections` answered (also the `connections-changed`
    /// reload): install the list and bump the owner epoch. Returns
    /// the discovery tokens the host should read for the sides that
    /// have an id (an id-less side clears instead —
    /// [`select_side`] does that).
    pub(crate) fn on_connections(&mut self, connections: Vec<EndpointConnection>) -> &mut Self {
        self.connections = connections;
        self.epoch += 1;
        self
    }

    /// The mount's prefill effect: apply the connection ids once the
    /// connections are loaded (the URL ids survive when they name a
    /// known connection).
    pub(crate) fn apply_connection_prefill(&mut self) {
        let _ = &self.connections;
        if let Some(source_id) = self.prefill.source_id.clone() {
            if self.connections.iter().any(|c| c.id == source_id) {
                self.prefill.source_id = None;
                self.select_side(EndpointSide::Source, source_id);
            }
        }
        if let Some(target_id) = self.prefill.target_id.clone() {
            if self.connections.iter().any(|c| c.id == target_id) {
                self.prefill.target_id = None;
                self.select_side(EndpointSide::Target, target_id);
            }
        }
    }

    /// `datazen:connection-closed`: drop the side session whose
    /// `dbSessionId` matches (the release is the host's
    /// ref-counted seam).
    pub(crate) fn on_connection_closed(&mut self, db_session_id: &str) {
        for side in [EndpointSide::Source, EndpointSide::Target] {
            let state = self.side_mut(side);
            if state.session.as_ref().is_some_and(|s| s.db_session_id == db_session_id) {
                state.session = None;
            }
        }
    }

    /// `setSourceId`/`setTargetId`: set the id. An empty id clears
    /// the whole side (the discovery effect's first branch); a
    /// non-empty id returns the discovery token the host reads with.
    pub(crate) fn select_side(
        &mut self,
        side: EndpointSide,
        id: impl Into<String>,
    ) -> Option<DiscoveryToken> {
        let id = id.into();
        // The effect's first order: clear the side's previous
        // discovery error.
        *self.discovery_error_slot(side) = None;
        let state = self.side_mut(side);
        state.id = id;
        if state.id.is_empty() {
            state.databases.clear();
            state.schemas.clear();
            state.schema.clear();
            state.database.clear();
            state.session = None;
            return None;
        }
        Some(DiscoveryToken { side, connection_id: state.id.clone(), epoch: self.epoch })
    }

    /// A pending discovery error to clear on the next successful
    /// answer (the `clearDiscoveryError` callback pair).
    pub(crate) fn take_discovery_error(&mut self, side: EndpointSide) -> Option<String> {
        self.discovery_error_slot(side).take()
    }

    /// `listDatabasesDedicated` answered. The owner gate: the token's
    /// side id and connections epoch must still be current. `Ok`
    /// installs the databases (the prefill pick over the default
    /// resolution); `Err` empties them and records the
    /// `databaseDiscovery.{side}:{code}` message (an unknown code is
    /// `unknown`). Returns the message to report, when the answer
    /// published.
    pub(crate) fn on_databases(
        &mut self,
        token: &DiscoveryToken,
        answer: Result<Vec<String>, Option<String>>,
    ) -> Option<String> {
        let state = self.side(token.side);
        if token.connection_id != state.id || token.epoch != self.epoch {
            return None; // superseded: dropped whole
        }
        match answer {
            Ok(databases) => {
                *self.discovery_error_slot(token.side) = None;
                let preferred = self
                    .side(token.side)
                    .id
                    .clone();
                let preferred = self
                    .connections
                    .iter()
                    .find(|c| c.id == preferred)
                    .and_then(|c| c.database.clone())
                    .unwrap_or_default();
                let prev = self.side(token.side).database.clone();
                let databases_ref = &databases;
                let picked = pick_prefill_database(
                    &mut self.prefill,
                    token.side,
                    databases_ref,
                    |prev| resolve_default_database(Some(databases_ref), &preferred, prev),
                    &prev,
                );
                let _ = &self.connections;
                let state = self.side_mut(token.side);
                state.databases = databases;
                state.database = picked;
                None
            }
            Err(code) => {
                let code = code.as_deref().filter(|c| is_api_error_code(c)).unwrap_or("unknown");
                let message =
                    format!("databaseDiscovery.{}:{code}", side_str(token.side));
                *self.discovery_error_slot(token.side) = Some(message.clone());
                let state = self.side_mut(token.side);
                state.databases.clear();
                Some(message)
            }
        }
    }

    /// `setSourceDatabase`/`setTargetDatabase` (the picks the select
    /// reports; the session effect notices the change itself).
    pub(crate) fn select_database(&mut self, side: EndpointSide, database: impl Into<String>) {
        self.side_mut(side).database = database.into();
    }

    /// `setSourceSchema`/`setTargetSchema`.
    pub(crate) fn select_schema(&mut self, side: EndpointSide, schema: impl Into<String>) {
        self.side_mut(side).schema = schema.into();
    }

    /// `sessionDatabaseOverride(connection, catalog)`: SQLite's
    /// `main` is a catalog alias — passing it as a connection
    /// override would open a different file named `main`.
    pub(crate) fn session_database_override(
        connection: Option<&EndpointConnection>,
        catalog: &str,
    ) -> Option<String> {
        match connection {
            Some(connection) if connection.database_type == "sqlite" => None,
            _ => Some(catalog.to_owned()),
        }
    }

    /// Whether the side's session is fresh for its current endpoint
    /// (the session effect's mismatch test, and `ensureConnected`'s
    /// fast path gate).
    pub(crate) fn session_is_fresh(&self, side: EndpointSide) -> bool {
        let state = self.side(side);
        state.session.as_ref().is_some_and(|session| {
            session.connection_id == state.id && session.database == state.database
        })
    }

    /// The catalog-read gate: the side is table+SQL capable
    /// (`DB_REGISTRY[type]?.supportsTables === true &&
    /// supportsSQL === true` — an absent meta refuses), and the
    /// session is fresh for the endpoint.
    pub(crate) fn catalog_ready(
        &self,
        side: EndpointSide,
        meta: impl Fn(&str) -> Option<EndpointDbMeta>,
    ) -> bool {
        let state = self.side(side);
        let Some(connection) = self.connections.iter().find(|c| c.id == state.id) else {
            return false;
        };
        let Some(meta) = meta(&connection.database_type) else {
            return false;
        };
        meta.supports_tables && meta.supports_sql && self.session_is_fresh(side)
    }

    /// `listCatalog` answered: install the schemas (the prefill pick
    /// over the configured-then-default resolution).
    pub(crate) fn on_catalog(&mut self, side: EndpointSide, schemas: Vec<String>) {
        let configured = self
            .side(side)
            .id
            .clone();
        let configured = self
            .connections
            .iter()
            .find(|c| c.id == configured)
            .and_then(|c| c.schema.clone())
            .unwrap_or_default();
        let configured = configured.trim().to_owned();
        let prev = self.side(side).schema.clone();
        let schemas_ref = &schemas;
        let picked = pick_prefill_schema(
            &mut self.prefill,
            side,
            schemas_ref,
            |prev| {
                if !configured.is_empty() && schemas_ref.contains(&configured) {
                    configured.clone()
                } else {
                    pick_default_schema(schemas_ref, Some(prev))
                }
            },
            &prev,
        );
        let state = self.side_mut(side);
        state.schemas = schemas;
        state.schema = picked;
    }

    /// A catalog read failed: the schemas clear (no error message —
    /// the React `catch` only resets).
    pub(crate) fn on_catalog_error(&mut self, side: EndpointSide) {
        let state = self.side_mut(side);
        state.schemas.clear();
        state.schema.clear();
    }

    /// `handleSwap`: swap the ten endpoint fields (ids, databases,
    /// schemas, the picks, the sessions) side for side.
    pub(crate) fn swap(&mut self) {
        std::mem::swap(&mut self.source, &mut self.target);
    }

    /// `isSameEndpoint()`.
    pub(crate) fn is_same_endpoint(&self) -> bool {
        is_same_endpoint_identity(
            &EndpointIdentity {
                connection_id: self.source.id.clone(),
                database: self.source.database.clone(),
                schema: self.source.schema.clone(),
            },
            &EndpointIdentity {
                connection_id: self.target.id.clone(),
                database: self.target.database.clone(),
                schema: self.target.schema.clone(),
            },
        )
    }

    /// `validateEndpoints()`: `Ok` when both connections are picked,
    /// they are not the same place, and both databases are named.
    pub(crate) fn validate(&self) -> Result<(), EndpointValidation> {
        if self.source.id.is_empty() || self.target.id.is_empty() {
            return Err(EndpointValidation::SelectBoth);
        }
        if self.is_same_endpoint() {
            return Err(EndpointValidation::CannotSameDb);
        }
        if self.source.database.is_empty() || self.target.database.is_empty() {
            return Err(EndpointValidation::SelectDbRequired);
        }
        Ok(())
    }
}

fn side_str(side: EndpointSide) -> &'static str {
    match side {
        EndpointSide::Source => "source",
        EndpointSide::Target => "target",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(id: &str, database_type: &str, database: Option<&str>) -> EndpointConnection {
        EndpointConnection {
            id: id.to_owned(),
            name: format!("{id} name"),
            database_type: database_type.to_owned(),
            database: database.map(str::to_owned),
            schema: None,
        }
    }

    fn hook() -> UseSchemaDiffEndpoints {
        UseSchemaDiffEndpoints::new(MigrationWindowPrefill::default())
    }

    #[test]
    fn connections_install_options_and_cross_dialect() {
        let mut hook = hook();
        hook.on_connections(vec![
            conn("pg", "postgresql", Some("app")),
            conn("my", "mysql", None),
        ]);
        assert_eq!(
            hook.conn_options(),
            vec![
                ("pg".to_owned(), "pg name (postgresql)".to_owned()),
                ("my".to_owned(), "my name (mysql)".to_owned()),
            ]
        );
        // Cross-dialect only when both sides hold connections of
        // different types.
        assert!(!hook.is_cross_dialect());
        hook.select_side(EndpointSide::Source, "pg").unwrap();
        assert!(!hook.is_cross_dialect());
        hook.select_side(EndpointSide::Target, "my").unwrap();
        assert!(hook.is_cross_dialect());
    }

    #[test]
    fn an_empty_side_id_clears_the_whole_side() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", None)]);
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["app".to_owned()]));
        assert_eq!(hook.source().databases.len(), 1);
        assert!(!hook.source().database.is_empty());
        // Clearing the id empties everything.
        assert!(hook.select_side(EndpointSide::Source, "").is_none());
        assert!(hook.source().databases.is_empty());
        assert!(hook.source().database.is_empty());
        assert!(hook.source().schemas.is_empty());
        assert!(hook.source().schema.is_empty());
        assert!(hook.source().session.is_none());
    }

    #[test]
    fn discovery_answers_are_gated_by_side_id_and_connections_epoch() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", Some("app"))]);
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        // A newer connections generation supersedes the token.
        hook.on_connections(vec![conn("pg", "postgresql", Some("app"))]);
        assert!(hook.on_databases(&token, Ok(vec!["app".to_owned()])).is_none());
        assert!(hook.source().databases.is_empty(), "the stale answer published nothing");
        // A fresh token publishes.
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        assert!(hook.on_databases(&token, Ok(vec!["app".to_owned()])).is_none());
        assert_eq!(hook.source().database, "app", "the configured database wins");
        // A side-id change supersedes too.
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        let _ = hook.select_side(EndpointSide::Source, "other");
        assert!(hook.on_databases(&token, Ok(vec!["x".to_owned()])).is_none());
    }

    #[test]
    fn a_discovery_failure_reports_the_side_and_classified_code() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", None)]);
        let token = hook.select_side(EndpointSide::Target, "pg").unwrap();
        let reported = hook
            .on_databases(&token, Err(Some("SessionLost".to_owned())))
            .expect("reported");
        assert_eq!(reported, "databaseDiscovery.target:SessionLost");
        assert!(hook.target().databases.is_empty());
        // An unknown or absent code is 'unknown'.
        let token = hook.select_side(EndpointSide::Target, "pg").unwrap();
        let reported = hook.on_databases(&token, Err(None)).expect("reported");
        assert_eq!(reported, "databaseDiscovery.target:unknown");
        let token = hook.select_side(EndpointSide::Target, "pg").unwrap();
        let reported =
            hook.on_databases(&token, Err(Some("NotACode".to_owned()))).expect("reported");
        assert_eq!(reported, "databaseDiscovery.target:unknown");
        // A later success clears the recorded error.
        let token = hook.select_side(EndpointSide::Target, "pg").unwrap();
        assert!(hook.on_databases(&token, Ok(vec!["app".to_owned()])).is_none());
        assert_eq!(hook.take_discovery_error(EndpointSide::Target), None);
    }

    #[test]
    fn the_prefill_database_consumes_once_over_the_default_resolution() {
        let mut hook = UseSchemaDiffEndpoints::new(MigrationWindowPrefill {
            source_database: Some("b".to_owned()),
            ..Default::default()
        });
        hook.on_connections(vec![conn("pg", "postgresql", Some("a"))]);
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["a".to_owned(), "b".to_owned()]));
        assert_eq!(hook.source().database, "b", "the prefill wins over the configured");
        // A second discovery run answers with the default resolution
        // (the prefill was consumed).
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["a".to_owned(), "b".to_owned()]));
        assert_eq!(hook.source().database, "a", "the configured database wins");
    }

    #[test]
    fn the_catalog_installs_the_prefill_then_configured_then_default_schema() {
        let mut hook = UseSchemaDiffEndpoints::new(MigrationWindowPrefill {
            source_schema: Some("custom".to_owned()),
            ..Default::default()
        });
        let mut connection = conn("pg", "postgresql", Some("app"));
        connection.schema = Some("configured".to_owned());
        hook.on_connections(vec![connection]);
        // The catalog effect only runs for a picked endpoint with a
        // database (the React gate).
        hook.select_side(EndpointSide::Source, "pg");
        hook.select_database(EndpointSide::Source, "app");
        let schemas = vec!["public".to_owned(), "custom".to_owned(), "configured".to_owned()];
        hook.on_catalog(EndpointSide::Source, schemas.clone());
        assert_eq!(hook.source().schema, "custom", "the prefill wins");
        hook.on_catalog(EndpointSide::Source, schemas.clone());
        assert_eq!(hook.source().schema, "configured", "the configured wins next");
        hook.on_catalog(EndpointSide::Source, vec!["public".to_owned()]);
        assert_eq!(hook.source().schema, "public", "then public, then the first");
        hook.on_catalog_error(EndpointSide::Source);
        assert!(hook.source().schemas.is_empty());
        assert!(hook.source().schema.is_empty());
    }

    #[test]
    fn a_closed_connection_drops_only_the_matching_session() {
        let mut hook = hook();
        let session = DedicatedSideSession {
            connection_id: "pg".to_owned(),
            database: "app".to_owned(),
            db_session_id: "sess-1".to_owned(),
        };
        // (sessions arrive via the host's ensure seam; the state
        // setter is the same one on_session uses)
        hook.select_side(EndpointSide::Source, "pg");
        hook.source.session = Some(session);
        hook.target.session = Some(DedicatedSideSession {
            connection_id: "my".to_owned(),
            database: "db".to_owned(),
            db_session_id: "sess-2".to_owned(),
        });
        hook.on_connection_closed("sess-1");
        assert!(hook.source().session.is_none());
        assert!(hook.target().session.is_some());
    }

    #[test]
    fn the_session_freshness_follows_the_endpoint() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", None)]);
        hook.select_side(EndpointSide::Source, "pg");
        assert!(!hook.session_is_fresh(EndpointSide::Source));
        hook.source.session = Some(DedicatedSideSession {
            connection_id: "pg".to_owned(),
            database: "app".to_owned(),
            db_session_id: "s".to_owned(),
        });
        assert!(!hook.session_is_fresh(EndpointSide::Source), "the database differs");
        hook.select_database(EndpointSide::Source, "app");
        assert!(hook.session_is_fresh(EndpointSide::Source));
    }

    #[test]
    fn sqlite_never_passes_main_as_the_connection_override() {
        assert_eq!(
            UseSchemaDiffEndpoints::session_database_override(
                Some(&conn("f", "sqlite", None)),
                "main"
            ),
            None
        );
        assert_eq!(
            UseSchemaDiffEndpoints::session_database_override(
                Some(&conn("p", "postgresql", None)),
                "main"
            ),
            Some("main".to_owned())
        );
        assert_eq!(
            UseSchemaDiffEndpoints::session_database_override(None, "main"),
            Some("main".to_owned())
        );
    }

    #[test]
    fn the_catalog_gate_needs_the_meta_and_a_fresh_session() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", None), conn("kv", "redis", None)]);
        hook.select_side(EndpointSide::Source, "pg");
        hook.select_database(EndpointSide::Source, "app");
        let meta = |t: &str| match t {
            "postgresql" => Some(EndpointDbMeta { supports_tables: true, supports_sql: true }),
            "redis" => Some(EndpointDbMeta { supports_tables: false, supports_sql: false }),
            _ => None,
        };
        assert!(!hook.catalog_ready(EndpointSide::Source, meta), "no session yet");
        hook.source.session = Some(DedicatedSideSession {
            connection_id: "pg".to_owned(),
            database: "app".to_owned(),
            db_session_id: "s".to_owned(),
        });
        assert!(hook.catalog_ready(EndpointSide::Source, meta));
        // A key-value side never reads a catalog.
        hook.select_side(EndpointSide::Target, "kv");
        hook.select_database(EndpointSide::Target, "0");
        hook.target.session = Some(DedicatedSideSession {
            connection_id: "kv".to_owned(),
            database: "0".to_owned(),
            db_session_id: "s2".to_owned(),
        });
        assert!(!hook.catalog_ready(EndpointSide::Target, meta));
    }

    #[test]
    fn swap_trades_every_field() {
        let mut hook = hook();
        hook.on_connections(vec![conn("pg", "postgresql", None), conn("my", "mysql", None)]);
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["app".to_owned()]));
        let token = hook.select_side(EndpointSide::Target, "my").unwrap();
        hook.on_databases(&token, Ok(vec!["shop".to_owned(), "other".to_owned()]));
        hook.on_catalog(EndpointSide::Source, vec!["public".to_owned()]);
        hook.swap();
        assert_eq!(hook.source().id, "my");
        assert_eq!(hook.source().database, "shop");
        assert_eq!(hook.target().id, "pg");
        assert_eq!(hook.target().database, "app");
        assert_eq!(hook.target().schemas, ["public"]);
        // And the identity rule still applies after the swap.
        assert!(!hook.is_same_endpoint());
    }

    #[test]
    fn validate_refuses_in_the_react_order() {
        let mut hook = hook();
        // Nothing picked: both connections required first.
        assert_eq!(hook.validate(), Err(EndpointValidation::SelectBoth));
        hook.select_side(EndpointSide::Source, "pg").unwrap();
        assert_eq!(hook.validate(), Err(EndpointValidation::SelectBoth));
        // Both picked, same place: refused before the database check.
        hook.on_connections(vec![conn("pg", "postgresql", None)]);
        let token = hook.select_side(EndpointSide::Source, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["app".to_owned()]));
        let token = hook.select_side(EndpointSide::Target, "pg").unwrap();
        hook.on_databases(&token, Ok(vec!["app".to_owned()]));
        assert_eq!(hook.validate(), Err(EndpointValidation::CannotSameDb));
        // Different places, one database unnamed.
        let token = hook.select_side(EndpointSide::Target, "my").unwrap();
        hook.on_databases(&token, Ok(Vec::new()));
        assert_eq!(hook.validate(), Err(EndpointValidation::SelectDbRequired));
        // Complete: valid.
        let token = hook.select_side(EndpointSide::Target, "my").unwrap();
        hook.on_databases(&token, Ok(vec!["shop".to_owned()]));
        assert_eq!(hook.validate(), Ok(()));
    }

    #[test]
    fn the_connection_prefill_applies_once_connections_are_known() {
        let mut hook = UseSchemaDiffEndpoints::new(MigrationWindowPrefill {
            source_id: Some("pg".to_owned()),
            target_id: Some("ghost".to_owned()),
            ..Default::default()
        });
        hook.on_connections(vec![conn("pg", "postgresql", None)]);
        hook.apply_connection_prefill();
        assert_eq!(hook.source().id, "pg");
        assert_eq!(hook.target().id, "", "an unknown id does not prefill");
    }
}
