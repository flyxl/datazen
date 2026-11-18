//! Session-scoped relation metadata cache — a faithful Rust port of React
//! `packages/frontend/src/lib/relationMetadata/metadataCache.ts` (T032).
//!
//! Pure cache/invalidation logic: no Rune tree, no `oklch`, no i18n, no IPC.
//! It carries the partition rules, debounce/dedup/error-TTL and the snapshot
//! generation counter so the SQL editor and Visual Query Builder can share one
//! cache. Identity/config/schema contracts live in sibling modules.
//!
//! # Immediate-mode adaptations (semantic, documented deliberately)
//!
//! * React is async (`Promise`, `setTimeout`, `Promise.allSettled`). Rune is
//!   immediate-mode and single-threaded, so loads are **synchronous**
//!   (`Fn -> Result<TableSchema, ()>`) and a rejected load is simply `Err`. There
//!   is no real concurrency, but inflight dedup is still enforced by request
//!   value, so "one load per key per flush" is unchanged.
//! * `setTimeout(debounceMs)` becomes a per-session **due-at timestamp**. A
//!   queued batch is drained by `tick()` once due, or immediately by
//!   `flush_now()` (React's "cancel timers then flush").
//! * `getSnapshot`'s frozen, referentially-stable object becomes an **owned
//!   clone**. Stability is an unchanged `epoch` and value equality, not pointer
//!   identity.
//! * `resolveEditorSessionConfig` was a process-global registry; the host
//!   injects a resolver closure at construction instead.
//! * `subscribeSchemaInvalidation` was another process-global; callers pass an
//!   explicit invalidation bus (the "process-wide" singleton is built by the
//!   host via [`default_metadata_cache`]).
//! * `subscribe`'s unsubscribe **function** becomes an id token plus
//!   [`MetadataCache::unsubscribe`]; callbacks are a `Vec`, `epoch` a `u64`
//!   generation counter. A callback must not mutate listeners during `notify`.
//! * React's `loadOne` rebind guard (`sessions.get(key) === session` /
//!   `inflight.get(key) === request`) is checked by key/value equality; a rebind
//!   during an await is not representable with synchronous loads.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::identity::{
    build_editor_relation_key, has_namespace, qualified_name_text, relation_base_name,
    EditorRelationKey, EditorRelationKind, QualifiedRelationId, SqlRelationSourceKind,
};
use super::invalidation::SchemaInvalidationBus;
use super::ordered_map::OrderedMap;
use super::schema::{
    schema_to_metadata, EditorMetadataContext, EditorRelationMetadata, EditorRelationRequest,
    EditorMetadataSnapshot, TableSchema,
};
use super::session_config::{
    editor_session_cache_key, resolve_editor_session_config_with, ConfigResolver, EditorSessionConfig,
};

const DEFAULT_ERROR_TTL_MS: i64 = 10_000;
const DEFAULT_DEBOUNCE_MS: i64 = 40;
const DEFAULT_DIALECT: &str = "standard";

/// Result of a host schema load. `Err` is React's rejected promise.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) type LoadResult = Result<TableSchema, ()>;

/// A queued/loading request.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRequest {
    identity: QualifiedRelationId,
    kind: EditorRelationKind,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl PendingRequest {
    fn same_request(&self, other: &Self) -> bool {
        self.identity == other.identity && self.kind == other.kind
    }
}

#[derive(Clone, Debug)]
struct ErrorMarker {
    at: i64,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct MetadataCacheDeps {
    pub load_table_schema: Option<Box<dyn Fn(&str, &str, &str, Option<&str>) -> LoadResult>>,
    pub config_resolver: Option<Box<ConfigResolver>>,
    pub invalidation_bus: Option<Rc<RefCell<SchemaInvalidationBus>>>,
    pub debounce_ms: i64,
    pub error_ttl_ms: i64,
    pub allow_qualified: bool,
    pub now: Option<Box<dyn Fn() -> i64>>,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl Default for MetadataCacheDeps {
    fn default() -> Self {
        Self {
            load_table_schema: None,
            config_resolver: None,
            invalidation_bus: None,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            error_ttl_ms: DEFAULT_ERROR_TTL_MS,
            allow_qualified: true,
            now: None,
        }
    }
}

struct SessionState {
    #[allow(dead_code)]
    config: EditorSessionConfig,
    db_session_id: String,
    dialect_id: String,
    database: String,
    schema: Option<String>,
    epoch: u64,
    relations: OrderedMap<EditorRelationKey, EditorRelationMetadata>,
    errors: OrderedMap<EditorRelationKey, ErrorMarker>,
    pending: OrderedMap<EditorRelationKey, PendingRequest>,
    inflight: OrderedMap<EditorRelationKey, PendingRequest>,
    cached_snapshot: Option<EditorMetadataSnapshot>,
    /// React's debounce `timer`, expressed as a due-at timestamp.
    flush_due_at: Option<i64>,
}

fn empty_snapshot(db_session_id: &str) -> EditorMetadataSnapshot {
    EditorMetadataSnapshot {
        db_session_id: db_session_id.to_owned(),
        database: String::new(),
        schema: None,
        epoch: 0,
        relations: BTreeMap::new(),
    }
}

fn build_snapshot(session: &SessionState) -> EditorMetadataSnapshot {
    EditorMetadataSnapshot {
        db_session_id: session.db_session_id.clone(),
        database: session.database.clone(),
        schema: session.schema.clone(),
        epoch: session.epoch,
        relations: session
            .relations
            .iter()
            .map(|(key, meta)| (key.clone(), meta.clone()))
            .collect(),
    }
}

struct CacheInner {
    sessions: BTreeMap<String, SessionState>,
    empty_snapshots: BTreeMap<String, EditorMetadataSnapshot>,
    listeners: Vec<(usize, Box<dyn Fn()>)>,
    next_listener_id: usize,
    load_table_schema: Box<dyn Fn(&str, &str, &str, Option<&str>) -> LoadResult>,
    config_resolver: Box<ConfigResolver>,
    debounce_ms: i64,
    error_ttl_ms: i64,
    allow_qualified: bool,
    now: Box<dyn Fn() -> i64>,
}

fn system_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn cancel_timer(session: &mut SessionState) {
    session.flush_due_at = None;
}

fn to_loadable_kind(request: &EditorRelationRequest) -> Option<EditorRelationKind> {
    match request.kind {
        Some(SqlRelationSourceKind::View) => Some(EditorRelationKind::View),
        Some(SqlRelationSourceKind::Cte) | Some(SqlRelationSourceKind::Subquery) => None,
        Some(SqlRelationSourceKind::Table) | None => Some(EditorRelationKind::Table),
    }
}

fn normalize_dialect(dialect_id: &str) -> String {
    if dialect_id.is_empty() {
        DEFAULT_DIALECT.to_owned()
    } else {
        dialect_id.to_owned()
    }
}

fn relation_matches_name(dialect_id: &str, identity: &QualifiedRelationId, table_name: &str) -> bool {
    qualified_name_text(identity, dialect_id) == table_name
        || relation_base_name(identity, dialect_id) == table_name
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl CacheInner {
    fn now_ms(&self) -> i64 {
        (self.now)()
    }

    fn resolve_config(&self, db_session_id: &str) -> EditorSessionConfig {
        resolve_editor_session_config_with(&*self.config_resolver, db_session_id)
    }

    fn notify(&self) {
        for (_, listener) in &self.listeners {
            listener();
        }
    }

    fn refresh_session(&mut self, key: &str) {
        let snapshot = {
            let session = match self.sessions.get_mut(key) {
                Some(session) => session,
                None => return,
            };
            session.epoch += 1;
            build_snapshot(session)
        };
        if let Some(session) = self.sessions.get_mut(key) {
            session.cached_snapshot = Some(snapshot);
        }
        self.notify();
    }

    /// `getOrCreateSession`, returning its partition key.
    fn ensure_session(&mut self, db_session_id: &str, database: Option<&str>) -> String {
        let config = self.resolve_config(db_session_id);
        let key = editor_session_cache_key(db_session_id, &config);
        // Exit transition for a config rebind: tear down other partitions.
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|(candidate, session)| {
                candidate.as_str() != key && session.db_session_id == db_session_id
            })
            .map(|(candidate, _)| candidate.clone())
            .collect();
        for stale_key in stale {
            self.sessions.remove(&stale_key);
            self.empty_snapshots.remove(&stale_key);
        }
        if !self.sessions.contains_key(&key) {
            let config = self.resolve_config(db_session_id);
            let mut session = SessionState {
                config,
                db_session_id: db_session_id.to_owned(),
                dialect_id: DEFAULT_DIALECT.to_owned(),
                // Empty until switchContext/ensureRelations pins the database.
                database: database.unwrap_or("").to_owned(),
                schema: None,
                epoch: 0,
                relations: OrderedMap::new(),
                errors: OrderedMap::new(),
                pending: OrderedMap::new(),
                inflight: OrderedMap::new(),
                cached_snapshot: None,
                flush_due_at: None,
            };
            session.cached_snapshot = Some(build_snapshot(&session));
            self.sessions.insert(key.clone(), session);
        }
        key
    }

    fn current_key(&self, db_session_id: &str) -> Option<String> {
        let config = self.resolve_config(db_session_id);
        let key = editor_session_cache_key(db_session_id, &config);
        if self.sessions.contains_key(&key) {
            Some(key)
        } else {
            None
        }
    }

    fn enqueue(&mut self, key: &str, request: &EditorRelationRequest) {
        let loadable_kind = match to_loadable_kind(request) {
            Some(kind) => kind,
            None => return,
        };
        if !self.allow_qualified && has_namespace(&request.identity) {
            return;
        }
        let now = self.now_ms();
        let error_ttl = self.error_ttl_ms;
        let (db_session_id, dialect_id) = match self.sessions.get(key) {
            Some(session) => (session.db_session_id.clone(), session.dialect_id.clone()),
            None => return,
        };
        let relation_key = build_editor_relation_key(&db_session_id, &request.identity, &dialect_id);

        let session = match self.sessions.get(key) {
            Some(session) => session,
            None => return,
        };
        if session.relations.contains_key(&relation_key)
            || session.inflight.contains_key(&relation_key)
            || session.pending.contains_key(&relation_key)
        {
            return;
        }
        if let Some(error) = session.errors.get(&relation_key) {
            if now.saturating_sub(error.at) < error_ttl {
                return;
            }
        }
        if let Some(session) = self.sessions.get_mut(key) {
            session.pending.insert(
                relation_key,
                PendingRequest {
                    identity: request.identity.clone(),
                    kind: loadable_kind,
                },
            );
        }
    }

    fn schedule_flush(&mut self, key: &str) {
        let now = self.now_ms();
        let debounce = self.debounce_ms;
        if let Some(session) = self.sessions.get_mut(key) {
            if session.flush_due_at.is_some() || session.pending.is_empty() {
                return;
            }
            session.flush_due_at = Some(now.saturating_add(debounce));
        }
    }

    fn is_current(
        &self,
        key: &str,
        db_session_id: &str,
        database: &str,
        schema_ctx: &Option<String>,
        dialect_id: &str,
        relation_key: &str,
        request: &PendingRequest,
    ) -> bool {
        let session = match self.sessions.get(key) {
            Some(session) => session,
            None => return false,
        };
        if session.db_session_id != db_session_id
            || editor_session_cache_key(db_session_id, &self.resolve_config(db_session_id)) != key
            || session.database != database
            || &session.schema != schema_ctx
            || session.dialect_id != dialect_id
        {
            return false;
        }
        session
            .inflight
            .get(&relation_key.to_owned())
            .map(|pending| pending.same_request(request))
            .unwrap_or(false)
    }

    fn load_one(
        &mut self,
        key: &str,
        relation_key: &EditorRelationKey,
        request: &PendingRequest,
        captured: &(String, String, String, Option<String>),
    ) {
        let (db_session_id, dialect_id, database, schema_ctx) = captured;
        // Database is mandatory: requeue when the tab context has not arrived.
        if database.is_empty() {
            if let Some(session) = self.sessions.get_mut(key) {
                session.pending.insert(relation_key.clone(), request.clone());
            }
            return;
        }
        let table = qualified_name_text(&request.identity, dialect_id);
        let result = (self.load_table_schema)(db_session_id, &table, database, schema_ctx.as_deref());
        let current = self.is_current(
            key,
            db_session_id,
            database,
            schema_ctx,
            dialect_id,
            relation_key,
            request,
        );
        if !current {
            return;
        }
        let now = self.now_ms();
        match result {
            Ok(schema) => {
                let metadata = schema_to_metadata(
                    relation_key.clone(),
                    &request.identity,
                    request.kind,
                    &schema,
                    now,
                );
                if let Some(session) = self.sessions.get_mut(key) {
                    session.relations.insert(relation_key.clone(), metadata);
                    session.errors.remove(relation_key);
                }
                self.refresh_session(key);
            }
            Err(()) => {
                // No epoch bump: the relation simply stays absent.
                if let Some(session) = self.sessions.get_mut(key) {
                    session.errors.insert(relation_key.clone(), ErrorMarker { at: now });
                }
            }
        }
    }

    fn flush_session(&mut self, key: &str) {
        if let Some(session) = self.sessions.get_mut(key) {
            cancel_timer(session);
        }
        let batch: Vec<(EditorRelationKey, PendingRequest)> = {
            let session = match self.sessions.get_mut(key) {
                Some(session) => session,
                None => return,
            };
            let batch = session
                .pending
                .iter()
                .map(|(relation_key, request)| (relation_key.clone(), request.clone()))
                .collect();
            session.pending.clear();
            batch
        };
        if batch.is_empty() {
            return;
        }
        for (relation_key, request) in batch {
            let already_inflight = self
                .sessions
                .get(key)
                .map(|session| session.inflight.contains_key(&relation_key))
                .unwrap_or(true);
            if already_inflight {
                continue;
            }
            let captured = {
                let session = match self.sessions.get(key) {
                    Some(session) => session,
                    None => return,
                };
                (
                    session.db_session_id.clone(),
                    session.dialect_id.clone(),
                    session.database.clone(),
                    session.schema.clone(),
                )
            };
            if let Some(session) = self.sessions.get_mut(key) {
                session.inflight.insert(relation_key.clone(), request.clone());
            }
            self.load_one(key, &relation_key, &request, &captured);
            // React's `.finally`: drop the inflight marker if this request is
            // still the one associated with the key.
            let still_current = self
                .sessions
                .get(key)
                .and_then(|session| session.inflight.get(&relation_key))
                .map(|pending| pending.same_request(&request))
                .unwrap_or(false);
            if still_current {
                if let Some(session) = self.sessions.get_mut(key) {
                    session.inflight.remove(&relation_key);
                }
            }
        }
    }

    fn reset_context(&mut self, key: &str, ctx: &EditorMetadataContext) {
        if let Some(session) = self.sessions.get_mut(key) {
            cancel_timer(session);
            session.database = ctx.database.clone();
            session.schema = ctx.schema.clone();
            session.dialect_id = normalize_dialect(&ctx.dialect_id);
            session.relations.clear();
            session.errors.clear();
            session.pending.clear();
            session.inflight.clear();
        }
        self.refresh_session(key);
    }

    fn ensure_relations_target(
        &mut self,
        db_session_id: &str,
        requests: &[EditorRelationRequest],
        ctx: &EditorMetadataContext,
    ) {
        let key = self.ensure_session(db_session_id, Some(&ctx.database));
        let dialect = normalize_dialect(&ctx.dialect_id);
        let (epoch, relation_count, inflight_count, database, schema, old_dialect) = {
            let session = match self.sessions.get(&key) {
                Some(session) => session,
                None => return,
            };
            (
                session.epoch,
                session.relations.len(),
                session.inflight.len(),
                session.database.clone(),
                session.schema.clone(),
                session.dialect_id.clone(),
            )
        };
        if database != ctx.database || schema != ctx.schema || old_dialect != dialect {
            if epoch == 0 && relation_count == 0 && inflight_count == 0 {
                if let Some(session) = self.sessions.get_mut(&key) {
                    session.database = ctx.database.clone();
                    session.schema = ctx.schema.clone();
                    session.dialect_id = dialect;
                    session.cached_snapshot = Some(build_snapshot(session));
                }
            } else {
                self.reset_context(&key, ctx);
            }
        }
        for request in requests {
            self.enqueue(&key, request);
        }
        self.schedule_flush(&key);
    }

    fn get_snapshot(&mut self, db_session_id: &str) -> EditorMetadataSnapshot {
        let config = self.resolve_config(db_session_id);
        let key = editor_session_cache_key(db_session_id, &config);
        if let Some(session) = self.sessions.get(&key) {
            return session
                .cached_snapshot
                .clone()
                .unwrap_or_else(|| build_snapshot(session));
        }
        if let Some(empty) = self.empty_snapshots.get(&key) {
            return empty.clone();
        }
        let empty = empty_snapshot(db_session_id);
        self.empty_snapshots.insert(key, empty.clone());
        empty
    }

    fn relation_matches_name_target(&mut self, db_session_id: &str, table_name: Option<&str>) {
        let keys: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.db_session_id == db_session_id)
            .map(|(key, _)| key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        let table = match table_name {
            Some(table) => table,
            None => {
                // Whole-session invalidation reaches every configuration.
                for key in &keys {
                    if let Some(session) = self.sessions.get_mut(key) {
                        cancel_timer(session);
                    }
                    self.sessions.remove(key);
                    self.empty_snapshots.remove(key);
                }
                self.notify();
                return;
            }
        };
        for key in &keys {
            let dialect_id = match self.sessions.get(key) {
                Some(session) => session.dialect_id.clone(),
                None => continue,
            };
            let stale_relations: Vec<EditorRelationKey> = {
                let session = match self.sessions.get(key) {
                    Some(session) => session,
                    None => continue,
                };
                session
                    .relations
                    .iter()
                    .filter(|(_, meta)| relation_matches_name(&dialect_id, &meta.identity, table))
                    .map(|(relation_key, _)| relation_key.clone())
                    .collect()
            };
            for relation_key in stale_relations {
                if let Some(session) = self.sessions.get_mut(key) {
                    session.relations.remove(&relation_key);
                    session.errors.remove(&relation_key);
                }
            }
            let stale_requests: Vec<EditorRelationKey> = {
                let session = match self.sessions.get(key) {
                    Some(session) => session,
                    None => continue,
                };
                session
                    .pending
                    .iter()
                    .chain(session.inflight.iter())
                    .filter(|(_, request)| {
                        relation_matches_name(&dialect_id, &request.identity, table)
                    })
                    .map(|(relation_key, _)| relation_key.clone())
                    .collect()
            };
            for relation_key in stale_requests {
                if let Some(session) = self.sessions.get_mut(key) {
                    session.pending.remove(&relation_key);
                    session.inflight.remove(&relation_key);
                    session.errors.remove(&relation_key);
                }
            }
            // Refresh even when only an in-flight relation was invalidated.
            self.refresh_session(key);
        }
    }
}

/// Clonable handle to the cache (React's single object, shared with the
/// invalidation listener without a static global).
#[derive(Clone)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct MetadataCache {
    inner: Rc<RefCell<CacheInner>>,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl MetadataCache {
    /// Build a cache with injected dependencies (React `createMetadataCache`).
    pub fn new(deps: MetadataCacheDeps) -> Self {
        let MetadataCacheDeps {
            load_table_schema,
            config_resolver,
            invalidation_bus,
            debounce_ms,
            error_ttl_ms,
            allow_qualified,
            now,
        } = deps;
        let inner = Rc::new(RefCell::new(CacheInner {
            sessions: BTreeMap::new(),
            empty_snapshots: BTreeMap::new(),
            listeners: Vec::new(),
            next_listener_id: 0,
            load_table_schema: load_table_schema.unwrap_or_else(|| Box::new(|_, _, _, _| Err(()))),
            config_resolver: config_resolver.unwrap_or_else(|| Box::new(|_| None)),
            debounce_ms,
            error_ttl_ms,
            allow_qualified,
            now: now.unwrap_or_else(|| Box::new(system_now_ms)),
        }));
        if let Some(bus) = invalidation_bus {
            let handle = Rc::clone(&inner);
            bus.borrow_mut().subscribe(Box::new(move |db_session_id, table_name| {
                handle
                    .borrow_mut()
                    .relation_matches_name_target(db_session_id, table_name);
            }));
        }
        Self { inner }
    }

    /// Queue relation requests (debounced, deduplicated).
    pub fn ensure_relations(
        &self,
        db_session_id: &str,
        requests: &[EditorRelationRequest],
        ctx: &EditorMetadataContext,
    ) {
        self.inner
            .borrow_mut()
            .ensure_relations_target(db_session_id, requests, ctx);
    }

    /// Force-flush every session's queued requests immediately.
    pub fn flush_now(&self) {
        let keys: Vec<String> = self.inner.borrow().sessions.keys().cloned().collect();
        for key in keys {
            self.inner.borrow_mut().flush_session(&key);
        }
    }

    /// Drain batches whose debounce deadline has passed (the `setTimeout` tick).
    pub fn tick(&self) {
        let now = self.inner.borrow().now_ms();
        let due: Vec<String> = self
            .inner
            .borrow()
            .sessions
            .iter()
            .filter(|(_, session)| {
                session
                    .flush_due_at
                    .map(|deadline| deadline <= now)
                    .unwrap_or(false)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in due {
            self.inner.borrow_mut().flush_session(&key);
        }
    }

    /// A cloned snapshot; stable in value/epoch until the session epoch bumps.
    pub fn get_snapshot(&self, db_session_id: &str) -> EditorMetadataSnapshot {
        self.inner.borrow_mut().get_snapshot(db_session_id)
    }

    pub fn get_relation(&self, db_session_id: &str, key: &str) -> Option<EditorRelationMetadata> {
        let inner = self.inner.borrow();
        let current = inner.current_key(db_session_id)?;
        inner
            .sessions
            .get(&current)
            .and_then(|session| session.relations.get(&key.to_owned()).cloned())
    }

    /// Register a snapshot-change callback; returns a token for `unsubscribe`.
    pub fn subscribe(&self, listener: Box<dyn Fn()>) -> usize {
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_listener_id;
        inner.next_listener_id += 1;
        inner.listeners.push((id, listener));
        id
    }

    pub fn unsubscribe(&self, id: usize) {
        self.inner
            .borrow_mut()
            .listeners
            .retain(|(current, _)| *current != id);
    }

    pub fn invalidate_relation(
        &self,
        db_session_id: &str,
        identity: &QualifiedRelationId,
        dialect_id: Option<&str>,
    ) {
        let mut inner = self.inner.borrow_mut();
        let key = match inner.current_key(db_session_id) {
            Some(key) => key,
            None => return,
        };
        let dialect = dialect_id.map(normalize_dialect).unwrap_or_else(|| {
            inner
                .sessions
                .get(&key)
                .map(|session| session.dialect_id.clone())
                .unwrap_or_else(|| DEFAULT_DIALECT.to_owned())
        });
        let relation_key = build_editor_relation_key(db_session_id, identity, &dialect);
        if let Some(session) = inner.sessions.get_mut(&key) {
            session.relations.remove(&relation_key);
            session.errors.remove(&relation_key);
            session.pending.remove(&relation_key);
            session.inflight.remove(&relation_key);
        }
        inner.refresh_session(&key);
    }

    pub fn invalidate_session(&self, db_session_id: &str) {
        let mut inner = self.inner.borrow_mut();
        let keys: Vec<String> = inner
            .sessions
            .iter()
            .filter(|(_, session)| session.db_session_id == db_session_id)
            .map(|(key, _)| key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        for key in &keys {
            inner.sessions.remove(key);
            inner.empty_snapshots.remove(key);
        }
        inner.notify();
    }

    pub fn invalidate_all(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.sessions.clear();
        inner.empty_snapshots.clear();
        inner.notify();
    }

    pub fn switch_context(&self, db_session_id: &str, ctx: &EditorMetadataContext) {
        let mut inner = self.inner.borrow_mut();
        let key = inner.ensure_session(db_session_id, Some(&ctx.database));
        inner.reset_context(&key, ctx);
    }
}

/// The "process-wide" default cache, wired to a schema invalidation bus. The
/// host owns the bus/cache handles instead of a process-global static.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn default_metadata_cache(bus: &Rc<RefCell<SchemaInvalidationBus>>) -> MetadataCache {
    MetadataCache::new(MetadataCacheDeps {
        invalidation_bus: Some(Rc::clone(bus)),
        ..Default::default()
    })
}
