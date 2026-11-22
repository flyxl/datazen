//! React `windows/connection/navigator/useNavigatorDbState.ts` — the
//! navigator's per-database caches and the refresh ladder over them (T580).
//!
//! The React hook holds five pieces of state (`dbTablesMap`, `dbErrors`,
//! `dbObjectsMap`, `loadingDbs`, `openDbs`) and the operations that read and
//! write them; every store or IPC touch is a callback it either receives or
//! reaches for. The port is the same shape as a state machine: the caches
//! live here, each operation is a pure function from a snapshot to the
//! [`DbEffect`]s the host must run, and the host reports results back through
//! the `apply_*` methods. Schema-store writes (`setLoadedTables`,
//! `setCurrentDatabase`, `recordSessionError`) are effects too — they are
//! writes the host owns, sequenced exactly where React sequences them.
//!
//! The contracts worth naming, because they were bugs:
//!
//! * **The click owns the pointer.** `toggle_db` pins `setCurrentDatabase`
//!   synchronously first, so the selected database moves even on cache hits
//!   and in-flight fetches — previously it only moved when `listTables`
//!   completed inside `setLoadedTables`, which every early return skipped.
//! * **Background loads stay neutral.** `reload_db_tables` publishes cached
//!   tables with `pin_current_database: false` — the pointer belongs to the
//!   user's last click, not to whichever fan-out finishes last. The refresh
//!   ladder (`refresh_database`, `refresh_connection`) passes `activate:
//!   false` for the same reason: refreshing a background connection must not
//!   steal the session the user is working in.
//! * **A failed read is not "no tables".** Errors land in `db_errors` next
//!   to the (empty) list they left, and the tree paints them as error rows.
//!
//! Split per the shard's split mark: the `useExpandedDbCacheRefresh` watch
//! (a schema-tree file, formal task T604) is [`expanded_cache_refresh`],
//! invoked from [`NavigatorDbState::cache_refresh_pass`].

pub(crate) mod expanded_cache_refresh;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::views::navigator::utils::should_use_multi_database_tree;
use crate::views::navigator::build_flat_rows::inputs::{
    ActiveEntrySnapshot, ConnectionSnapshot, DatabaseObject, DriverMeta, SchemaStateSnapshot,
    TableInfo,
};
use expanded_cache_refresh::cache_refresh_pass;

/// React `LoadForConnectionOptions`, the read set the navigator passes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LoadForConnectionOpts {
    pub preferred_database: Option<String>,
    pub skip_load_tables: bool,
    pub database_type: String,
    /// False for every background refresh: never steal the active session.
    pub activate: bool,
}

/// Every side effect the db state machine can ask the host for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DbEffect {
    /// `databaseCommands.listTables(dbSessionId, dbName, null, owner)`.
    ListTables {
        db_session_id: String,
        db_name: String,
        /// The connection whose session this is (`owner?.connectionId`),
        /// resolved from the live active-connection map by the host.
        owner_connection_id: Option<String>,
    },
    /// `connectionCommands.getOpenDatabases(dbSessionId)`.
    GetOpenDatabases { db_session_id: String },
    /// `databaseCommands.getDatabaseObjects(dbSessionId, catId, dbName)`.
    GetDatabaseObjects {
        db_session_id: String,
        cat_id: String,
        db_name: String,
    },
    /// `loadForConnection(dbSessionId, opts)` — the schema-store loader.
    LoadForConnection { db_session_id: String, opts: LoadForConnectionOpts },
    /// `ensureNamespacePath(segments, dbSessionId)` — path-hierarchy drivers.
    EnsureNamespacePath { segments: Vec<String>, db_session_id: String },
    /// `useSchemaStore.setLoadedTables(dbName, tables, dbSessionId, opts)`.
    SetLoadedTables {
        db_name: String,
        tables: Vec<TableInfo>,
        db_session_id: String,
        /// `pinCurrentDatabase`: true when the user clicked (activate,
        /// toggle cache hit), false for background cache fills.
        pin_current_database: bool,
    },
    /// `useSchemaStore.setCurrentDatabase(dbName, dbSessionId)` — the
    /// pointer move. Validates known databases (namespace leaves pass fetch
    /// PATHs; the store drops those).
    SetCurrentDatabase { db_name: String, db_session_id: String },
    /// `useSchemaStore.recordSessionError(dbSessionId, message)`.
    RecordSessionError { db_session_id: String, message: String },
}

/// The snapshot the refresh ladder reads.
pub(crate) struct DbStateInputs<'a> {
    pub active_connections: &'a HashMap<String, ActiveEntrySnapshot>,
    pub connections: &'a [ConnectionSnapshot],
    pub expanded_dbs: &'a HashSet<String>,
    pub expanded_cats: &'a HashSet<String>,
    pub schemas: &'a HashMap<String, SchemaStateSnapshot>,
    /// Caller-supplied registry (`DB_REGISTRY`), the builder's contract.
    pub registry: &'a HashMap<String, DriverMeta>,
}

/// React `databaseNameForCategory` — the database a category key belongs
/// to, matched by prefix against the known databases *and* the expanded db
/// keys of the connection. Longest name wins, so `db` does not swallow
/// `db_extra`.
pub(crate) fn database_name_for_category(
    connection_id: &str,
    cat_key: &str,
    expanded_dbs: &HashSet<String>,
    known_databases: &[String],
) -> Option<String> {
    let prefix = format!("{connection_id}::");
    let mut database_names: Vec<String> = known_databases.to_vec();
    for db_key in expanded_dbs {
        if let Some(name) = db_key.strip_prefix(&prefix) {
            database_names.push(name.to_owned());
        }
    }
    database_names
        .into_iter()
        .filter(|database| cat_key.starts_with(&format!("{prefix}{database}::")))
        .max_by_key(|database| database.len())
}

/// The db caches and the operations over them.
#[derive(Default)]
pub(crate) struct NavigatorDbState {
    /// The expanded-cache-refresh watch's `prevFpRef` (T604's file; the
    /// watch runs through [`Self::cache_refresh_pass`]).
    fingerprints: HashMap<String, String>,
    db_tables_map: BTreeMap<String, Vec<TableInfo>>,
    /// Per-`${dbSessionId}::${dbName}` load failures. Keyed like
    /// `db_tables_map` because a failure is only meaningful next to the
    /// (empty) list it left behind.
    db_errors: BTreeMap<String, String>,
    db_objects_map: BTreeMap<String, Vec<DatabaseObject>>,
    loading_dbs: HashSet<String>,
    /// Databases the backend currently holds an open resource for, per
    /// session. A driver with no per-database resource reports none, and
    /// the tree then shows no marker at all rather than claiming everything
    /// is closed.
    open_dbs: HashMap<String, HashSet<String>>,
}

impl NavigatorDbState {
    // -----------------------------------------------------------------
    // cache reads (the row builder's inputs)
    // -----------------------------------------------------------------

    pub(crate) fn db_tables_map(&self) -> &BTreeMap<String, Vec<TableInfo>> {
        &self.db_tables_map
    }

    pub(crate) fn db_errors(&self) -> &BTreeMap<String, String> {
        &self.db_errors
    }

    pub(crate) fn db_objects_map(&self) -> &BTreeMap<String, Vec<DatabaseObject>> {
        &self.db_objects_map
    }

    pub(crate) fn loading_dbs(&self) -> &HashSet<String> {
        &self.loading_dbs
    }

    pub(crate) fn open_dbs(&self) -> &HashMap<String, HashSet<String>> {
        &self.open_dbs
    }

    pub(crate) fn tables_of(&self, db_session_id: &str, db_name: &str) -> Option<&Vec<TableInfo>> {
        self.db_tables_map.get(&table_key(db_session_id, db_name))
    }

    /// The raw setter the table menu's refresh-after-mutation uses
    /// (React exposes `setDbTablesMap`).
    pub(crate) fn set_tables(&mut self, db_session_id: &str, db_name: &str, tables: Vec<TableInfo>) {
        self.db_tables_map.insert(table_key(db_session_id, db_name), tables);
    }

    /// Drop one relation's row from the cached table list (the menu's
    /// `refreshAfterMutation` filter). Returns whether anything changed.
    pub(crate) fn drop_relation_row(
        &mut self,
        db_session_id: &str,
        db_name: &str,
        table_name: &str,
    ) -> bool {
        let key = table_key(db_session_id, db_name);
        match self.db_tables_map.get_mut(&key) {
            Some(rows) => {
                let before = rows.len();
                rows.retain(|item| item.name != table_name);
                before != rows.len()
            }
            None => false,
        }
    }

    // -----------------------------------------------------------------
    // open databases
    // -----------------------------------------------------------------

    /// React `refreshOpenDatabases` — reading a database is what opens its
    /// pool, so the marker is refreshed after reads, not only on demand.
    pub(crate) fn refresh_open_databases(&self, db_session_id: &str) -> Vec<DbEffect> {
        vec![DbEffect::GetOpenDatabases { db_session_id: db_session_id.to_owned() }]
    }

    /// Apply the marker read: a session gone (or not connected) drops its
    /// entry, so a stale marker cannot outlive the pool it described.
    pub(crate) fn apply_open_databases(
        &mut self,
        db_session_id: &str,
        open: Result<Vec<String>, String>,
    ) {
        match open {
            Ok(names) => {
                self.open_dbs
                    .insert(db_session_id.to_owned(), names.into_iter().collect());
            }
            Err(_) => {
                self.open_dbs.remove(db_session_id);
            }
        }
    }

    // -----------------------------------------------------------------
    // table loads
    // -----------------------------------------------------------------

    /// React `reloadDbTables` — the one table read everything funnels
    /// through. The owner lookup (which connection this session belongs to)
    /// is the host's live-map read, passed in.
    pub(crate) fn reload_db_tables(
        &self,
        db_session_id: &str,
        db_name: &str,
        owner_connection_id: Option<&str>,
    ) -> Vec<DbEffect> {
        vec![DbEffect::ListTables {
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            owner_connection_id: owner_connection_id.map(str::to_owned),
        }]
    }

    /// The read succeeded: cache the list, clear any stale error, publish
    /// into the session **without** pinning (background fills are neutral),
    /// and refresh the open-databases marker — the read is what opens the
    /// pool. Also clears a toggle's loading mark (React's `finally`).
    pub(crate) fn apply_list_tables_ok(
        &mut self,
        db_session_id: &str,
        db_name: &str,
        tables: Vec<TableInfo>,
    ) -> Vec<DbEffect> {
        let key = table_key(db_session_id, db_name);
        self.db_tables_map.insert(key.clone(), tables.clone());
        self.db_errors.remove(&key);
        self.loading_dbs.remove(&key);
        vec![
            DbEffect::SetLoadedTables {
                db_name: db_name.to_owned(),
                tables,
                db_session_id: db_session_id.to_owned(),
                pin_current_database: false,
            },
            DbEffect::GetOpenDatabases { db_session_id: db_session_id.to_owned() },
        ]
    }

    /// The read failed: a schema load that fails silently is
    /// indistinguishable from a database with no tables, so record the
    /// error where the sidebar can show it. The previous list (if any)
    /// stays — a cached list beats a failed refresh. Also clears a toggle's
    /// loading mark.
    pub(crate) fn apply_list_tables_err(
        &mut self,
        db_session_id: &str,
        db_name: &str,
        message: String,
    ) -> Vec<DbEffect> {
        let key = table_key(db_session_id, db_name);
        self.db_tables_map.entry(key.clone()).or_default();
        self.db_errors.insert(key.clone(), message.clone());
        self.loading_dbs.remove(&key);
        vec![
            DbEffect::RecordSessionError {
                db_session_id: db_session_id.to_owned(),
                message,
            },
            DbEffect::GetOpenDatabases { db_session_id: db_session_id.to_owned() },
        ]
    }

    // -----------------------------------------------------------------
    // activation
    // -----------------------------------------------------------------

    /// React `activateDatabase` — make this database the session's current
    /// one. Cache hit: publish the cached catalog (pinned). Miss: the
    /// pointer still has to move, but only for a database the session
    /// actually knows — `setCurrentDatabase` validates.
    pub(crate) fn activate_database(
        &self,
        db_session_id: &str,
        db_name: &str,
    ) -> Vec<DbEffect> {
        match self.db_tables_map.get(&table_key(db_session_id, db_name)) {
            Some(cached) => vec![DbEffect::SetLoadedTables {
                db_name: db_name.to_owned(),
                tables: cached.clone(),
                db_session_id: db_session_id.to_owned(),
                pin_current_database: true,
            }],
            None => vec![DbEffect::SetCurrentDatabase {
                db_name: db_name.to_owned(),
                db_session_id: db_session_id.to_owned(),
            }],
        }
    }

    // -----------------------------------------------------------------
    // cache clearing
    // -----------------------------------------------------------------

    /// React `clearDbLocalCache` — closing (or dropping) a database
    /// releases its pool, so it must stop being marked open immediately;
    /// the tables, the error and the object caches for that database go
    /// with it, so the next expand re-fetches.
    pub(crate) fn clear_db_local_cache(
        &mut self,
        connection_id: &str,
        db_session_id: &str,
        db_name: &str,
    ) {
        if let Some(current) = self.open_dbs.get_mut(db_session_id) {
            current.remove(db_name);
        }
        let key = table_key(db_session_id, db_name);
        self.db_tables_map.remove(&key);
        self.db_errors.remove(&key);
        let prefix = format!("{connection_id}::{db_name}::");
        self.db_objects_map.retain(|k, _| !k.starts_with(&prefix));
    }

    /// The `clearCaches` callback the expanded-cache refresh passes: drop
    /// every table entry of the session, and — when the connection id is
    /// known — every object entry of the connection (the two caches key by
    /// different owners).
    pub(crate) fn clear_session_caches(
        &mut self,
        db_session_id: &str,
        connection_id: Option<&str>,
    ) {
        let prefix = format!("{db_session_id}::");
        self.db_tables_map.retain(|k, _| !k.starts_with(&prefix));
        self.db_errors.retain(|k, _| !k.starts_with(&prefix));
        if let Some(connection_id) = connection_id {
            let prefix = format!("{connection_id}::");
            self.db_objects_map.retain(|k, _| !k.starts_with(&prefix));
        }
    }

    // -----------------------------------------------------------------
    // object categories
    // -----------------------------------------------------------------

    /// React `reloadDbObjectCategory` — tables and views live in the table
    /// cache, never here.
    pub(crate) fn reload_db_object_category(
        &self,
        db_session_id: &str,
        cat_key: &str,
        cat_id: &str,
        db_name: &str,
    ) -> Vec<DbEffect> {
        if cat_id == "tables" || cat_id == "views" {
            return Vec::new();
        }
        vec![DbEffect::GetDatabaseObjects {
            db_session_id: db_session_id.to_owned(),
            cat_id: cat_id.to_owned(),
            db_name: db_name.to_owned(),
        }]
    }

    /// Apply the object read. A failure caches an empty list (React's
    /// catch), so the category shows as empty rather than spinning forever.
    pub(crate) fn apply_database_objects(
        &mut self,
        cat_key: &str,
        objects: Result<Vec<DatabaseObject>, String>,
    ) {
        self.db_objects_map
            .insert(cat_key.to_owned(), objects.unwrap_or_default());
    }

    /// React `toggleCategoryLoad` — expanding a category loads it once.
    pub(crate) fn toggle_category_load(
        &self,
        db_session_id: &str,
        cat_key: &str,
        cat_id: &str,
        db_name: &str,
    ) -> Vec<DbEffect> {
        if cat_id == "tables" || cat_id == "views" {
            return Vec::new();
        }
        if self.db_objects_map.contains_key(cat_key) {
            return Vec::new();
        }
        self.reload_db_object_category(db_session_id, cat_key, cat_id, db_name)
    }

    // -----------------------------------------------------------------
    // the refresh ladder
    // -----------------------------------------------------------------

    /// React `reloadExpandedObjectCategories` — every expanded non-table
    /// category of the connection, its database resolved from the key.
    pub(crate) fn reload_expanded_object_categories(
        &self,
        inputs: &DbStateInputs<'_>,
        connection_id: &str,
        db_session_id: &str,
    ) -> Vec<DbEffect> {
        let prefix = format!("{connection_id}::");
        let mut effects = Vec::new();
        for cat_key in inputs.expanded_cats {
            if !cat_key.starts_with(&prefix) {
                continue;
            }
            let cat_id = cat_key.rsplit("::").next().unwrap_or_default();
            if cat_id.is_empty() || cat_id == "tables" || cat_id == "views" {
                continue;
            }
            let known_databases = inputs
                .schemas
                .get(db_session_id)
                .map(|s| s.databases.clone())
                .unwrap_or_default();
            let Some(db_name) = database_name_for_category(
                connection_id,
                cat_key,
                inputs.expanded_dbs,
                &known_databases,
            ) else {
                continue;
            };
            effects.extend(self.reload_db_object_category(db_session_id, cat_key, cat_id, &db_name));
        }
        effects
    }

    /// React `refreshConnection` — a background reload of one connection:
    /// never steal the active session (the loader runs with `activate:
    /// false`), then the open marker, then the path-hierarchy root for
    /// namespace drivers, then every expanded database of a multi-db
    /// connection, then the expanded object categories.
    pub(crate) fn refresh_connection(
        &self,
        inputs: &DbStateInputs<'_>,
        connection_id: &str,
    ) -> Vec<DbEffect> {
        let Some(entry) = inputs.active_connections.get(connection_id) else {
            return Vec::new();
        };
        if entry.status != "connected" {
            return Vec::new();
        }
        let Some(db_session_id) = entry.db_session_id.clone() else {
            return Vec::new();
        };
        let Some(conn) = inputs.connections.iter().find(|c| c.id == connection_id) else {
            return Vec::new();
        };
        let meta = inputs
            .registry
            .get(&conn.database_type)
            .cloned()
            .unwrap_or_default();
        let is_custom_tree = meta.schema_tree_mode.as_deref() == Some("custom");
        let is_path_hierarchy = meta.namespace_ensure.as_deref() == Some("path-hierarchy");
        let is_plugin_managed = is_custom_tree || is_path_hierarchy;
        let is_multi_db = should_use_multi_database_tree(
            meta.has_multi_database,
            &meta.database_field_type,
            conn.database.as_deref(),
        );

        let mut effects = vec![
            DbEffect::LoadForConnection {
                db_session_id: db_session_id.clone(),
                opts: LoadForConnectionOpts {
                    preferred_database: conn.database.clone(),
                    skip_load_tables: is_multi_db || is_plugin_managed,
                    database_type: conn.database_type.clone(),
                    activate: false,
                },
            },
            DbEffect::GetOpenDatabases { db_session_id: db_session_id.clone() },
        ];

        if is_path_hierarchy {
            effects.push(DbEffect::EnsureNamespacePath {
                segments: Vec::new(),
                db_session_id: db_session_id.clone(),
            });
        }

        if is_multi_db {
            let prefix = format!("{connection_id}::");
            // React's Set iterates insertion order; sorted is the
            // deterministic equivalent of the same fan-out.
            let mut db_names: Vec<&str> = inputs
                .expanded_dbs
                .iter()
                .filter_map(|k| k.strip_prefix(&prefix))
                .collect();
            db_names.sort_unstable();
            for db_name in db_names {
                effects.extend(
                    self.reload_db_tables(&db_session_id, db_name, Some(connection_id)),
                );
            }
        }

        effects.extend(
            self.reload_expanded_object_categories(inputs, connection_id, &db_session_id),
        );
        effects
    }

    /// React `refreshAllConnections` — the fan-out; every connected entry,
    /// each a full `refresh_connection`.
    pub(crate) fn refresh_all_connections(&self, inputs: &DbStateInputs<'_>) -> Vec<DbEffect> {
        let mut effects = Vec::new();
        for connection_id in inputs.active_connections.keys() {
            effects.extend(self.refresh_connection(inputs, connection_id));
        }
        effects
    }

    /// React `refreshDatabase` — multi-db drivers reload the database's
    /// tables directly; single-db drivers re-run the loader with the
    /// database preferred (still `activate: false` — refreshing one
    /// connection's database must not re-focus that session). Then the
    /// database's expanded object categories.
    pub(crate) fn refresh_database(
        &self,
        inputs: &DbStateInputs<'_>,
        connection_id: &str,
        db_name: &str,
    ) -> Vec<DbEffect> {
        let Some(entry) = inputs.active_connections.get(connection_id) else {
            return Vec::new();
        };
        let Some(db_session_id) = entry.db_session_id.clone() else {
            return Vec::new();
        };
        let Some(conn) = inputs.connections.iter().find(|c| c.id == connection_id) else {
            return Vec::new();
        };
        let meta = inputs
            .registry
            .get(&conn.database_type)
            .cloned()
            .unwrap_or_default();
        let is_multi_db = should_use_multi_database_tree(
            meta.has_multi_database,
            &meta.database_field_type,
            conn.database.as_deref(),
        );

        let mut effects = if is_multi_db {
            self.reload_db_tables(&db_session_id, db_name, Some(connection_id))
        } else {
            vec![
                DbEffect::LoadForConnection {
                    db_session_id: db_session_id.clone(),
                    opts: LoadForConnectionOpts {
                        preferred_database: Some(db_name.to_owned()),
                        skip_load_tables: false,
                        database_type: conn.database_type.clone(),
                        activate: false,
                    },
                },
                DbEffect::GetOpenDatabases { db_session_id: db_session_id.clone() },
            ]
        };

        let prefix = format!("{connection_id}::{db_name}::");
        for cat_key in inputs.expanded_cats {
            if !cat_key.starts_with(&prefix) {
                continue;
            }
            let cat_id = cat_key.rsplit("::").next().unwrap_or_default();
            if cat_id.is_empty() || cat_id == "tables" || cat_id == "views" {
                continue;
            }
            effects.extend(self.reload_db_object_category(&db_session_id, cat_key, cat_id, db_name));
        }
        effects
    }

    /// React `refreshSchema` — the tables of the database, then the
    /// schema's expanded object categories.
    pub(crate) fn refresh_schema(
        &self,
        inputs: &DbStateInputs<'_>,
        connection_id: &str,
        db_name: &str,
        schema_name: &str,
    ) -> Vec<DbEffect> {
        let Some(entry) = inputs.active_connections.get(connection_id) else {
            return Vec::new();
        };
        let Some(db_session_id) = entry.db_session_id.clone() else {
            return Vec::new();
        };

        let mut effects = self.reload_db_tables(&db_session_id, db_name, Some(connection_id));

        let prefix = format!("{connection_id}::{db_name}::{schema_name}::");
        for cat_key in inputs.expanded_cats {
            if !cat_key.starts_with(&prefix) {
                continue;
            }
            let cat_id = cat_key.rsplit("::").next().unwrap_or_default();
            if cat_id.is_empty() || cat_id == "tables" || cat_id == "views" {
                continue;
            }
            effects.extend(self.reload_db_object_category(&db_session_id, cat_key, cat_id, db_name));
        }
        effects
    }

    // -----------------------------------------------------------------
    // toggles
    // -----------------------------------------------------------------

    /// React `toggleDb` — expanding (or clicking) a database. The click
    /// itself owns the selection: pin synchronously so the pointer moves
    /// even on cache hits, in-flight fetches, and before the IPC resolves.
    /// A cache hit publishes the cached catalog into the session; a miss
    /// marks the load and reads.
    pub(crate) fn toggle_db(
        &mut self,
        db_session_id: &str,
        db_name: &str,
    ) -> Vec<DbEffect> {
        let key = table_key(db_session_id, db_name);
        let mut effects = vec![DbEffect::SetCurrentDatabase {
            db_name: db_name.to_owned(),
            db_session_id: db_session_id.to_owned(),
        }];
        if let Some(cached) = self.db_tables_map.get(&key).cloned() {
            effects.push(DbEffect::SetLoadedTables {
                db_name: db_name.to_owned(),
                tables: cached,
                db_session_id: db_session_id.to_owned(),
                pin_current_database: true,
            });
            return effects;
        }
        if self.loading_dbs.contains(&key) {
            // Already reading: the pin above is all this click does.
            return effects;
        }
        self.loading_dbs.insert(key);
        effects.push(DbEffect::ListTables {
            db_session_id: db_session_id.to_owned(),
            db_name: db_name.to_owned(),
            owner_connection_id: None,
        });
        effects
    }

    /// The cache-refresh watch's fingerprint swap: store `next`, return
    /// what was there (React's `prev = prevFpRef.current;
    /// prevFpRef.current = nextFp`).
    pub(crate) fn take_fingerprints(
        &mut self,
        next: HashMap<String, String>,
    ) -> HashMap<String, String> {
        std::mem::replace(&mut self.fingerprints, next)
    }

    /// The `useExpandedDbCacheRefresh` watch, mounted: one synchronous
    /// invalidate-then-schedule pass over the schema store's fingerprints
    /// (see [`expanded_cache_refresh`] for the semantics).
    pub(crate) fn cache_refresh_pass(
        &mut self,
        inputs: &DbStateInputs<'_>,
    ) -> Vec<DbEffect> {
        cache_refresh_pass(self, inputs)
    }
}

/// React's cache key: `${dbSessionId}::${dbName}`.
fn table_key(db_session_id: &str, db_name: &str) -> String {
    format!("{db_session_id}::{db_name}")
}
