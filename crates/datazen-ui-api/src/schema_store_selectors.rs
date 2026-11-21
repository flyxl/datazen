use serde_json::Value;

use crate::SchemaStoreSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRelationSourceKind {
    Table,
    View,
    Cte,
    Subquery,
}

impl SqlRelationSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Cte => "cte",
            Self::Subquery => "subquery",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "view" => Self::View,
            "cte" => Self::Cte,
            "subquery" => Self::Subquery,
            _ => Self::Table,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlIdentifierSegment {
    pub name: String,
    pub quoted: bool,
}

impl SqlIdentifierSegment {
    pub fn bare(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            quoted: false,
        }
    }

    pub fn quoted(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            quoted: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedRelationId {
    pub namespace_path: Vec<SqlIdentifierSegment>,
    pub name: SqlIdentifierSegment,
}

impl QualifiedRelationId {
    pub fn bare(name: impl Into<String>) -> Self {
        Self {
            namespace_path: Vec::new(),
            name: SqlIdentifierSegment::bare(name),
        }
    }
}

pub type EditorRelationKey = String;

#[derive(Clone, Debug, PartialEq)]
pub struct EditorRelationRequest {
    pub identity: QualifiedRelationId,
    pub kind: Option<SqlRelationSourceKind>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationBindingLike {
    pub relation: QualifiedRelationId,
    pub source_kind: Option<SqlRelationSourceKind>,
}

pub trait EditorMetadataCacheLike {
    fn ensure_relations(
        &mut self,
        db_session_id: &str,
        requests: &[EditorRelationRequest],
        ctx: &Value,
    );

    fn flush_now(&mut self);

    fn get_snapshot(&self, db_session_id: &str) -> Value;

    fn get_relation(&self, db_session_id: &str, key: &str) -> Option<Value>;

    fn subscribe(&self, listener: &dyn Fn());

    fn invalidate_relation(&mut self, db_session_id: &str, key: &str);

    fn invalidate_session(&mut self, db_session_id: &str);

    fn invalidate_all(&mut self);

    fn switch_context(&mut self, ctx: &Value);
}

pub trait IdentifierFolder {
    fn fold_segment(&self, name: &str, quoted: bool, dialect_id: &str) -> String;
}

pub trait DialectRegistryLookup {
    fn sql_dialect(&self, database_type: &str) -> Option<String>;
}

pub fn read_metadata_snapshot(db_session_id: &str, cache: &dyn EditorMetadataCacheLike) -> Value {
    cache.get_snapshot(db_session_id)
}

pub fn get_metadata_relation(
    db_session_id: &str,
    key: &str,
    cache: &dyn EditorMetadataCacheLike,
) -> Option<Value> {
    cache.get_relation(db_session_id, key)
}

pub fn ensure_metadata_relations(
    db_session_id: &str,
    requests: &[EditorRelationRequest],
    ctx: &Value,
    cache: &mut dyn EditorMetadataCacheLike,
) {
    cache.ensure_relations(db_session_id, requests, ctx);
}

pub fn binding_to_relation_requests(
    bindings: &[RelationBindingLike],
) -> Vec<EditorRelationRequest> {
    bindings
        .iter()
        .filter(|binding| {
            !matches!(
                binding.source_kind,
                Some(SqlRelationSourceKind::Cte) | Some(SqlRelationSourceKind::Subquery)
            )
        })
        .map(|binding| EditorRelationRequest {
            identity: binding.relation.clone(),
            kind: binding.source_kind,
        })
        .collect()
}

pub fn editor_relation_key(
    db_session_id: &str,
    identity: &QualifiedRelationId,
    dialect_id: &str,
    folder: &dyn IdentifierFolder,
) -> EditorRelationKey {
    let mut segments = identity
        .namespace_path
        .iter()
        .map(|segment| folder.fold_segment(&segment.name, segment.quoted, dialect_id))
        .collect::<Vec<_>>();
    segments.push(folder.fold_segment(&identity.name.name, identity.name.quoted, dialect_id));
    format!("{db_session_id}::{}", segments.join("."))
}

pub fn relation_cache_key(
    db_session_id: &str,
    identity: &QualifiedRelationId,
    dialect_id: &str,
    folder: &dyn IdentifierFolder,
) -> EditorRelationKey {
    editor_relation_key(db_session_id, identity, dialect_id, folder)
}

pub fn relation_lookup_key(
    db_session_id: &str,
    identity: &QualifiedRelationId,
    dialect_id: &str,
    folder: &dyn IdentifierFolder,
) -> EditorRelationKey {
    editor_relation_key(db_session_id, identity, dialect_id, folder)
}

pub fn resolve_editor_dialect_id(
    database_type: Option<&str>,
    registry: &dyn DialectRegistryLookup,
) -> String {
    match database_type {
        None => "standard".to_owned(),
        Some(value) if value.is_empty() => "standard".to_owned(),
        Some(value) => registry
            .sql_dialect(value)
            .unwrap_or_else(|| value.to_owned()),
    }
}

pub fn snapshot_key(db_session_id: Option<&str>) -> &str {
    db_session_id.unwrap_or("__inactive__")
}

pub fn active_session_metadata_snapshot(
    store: &SchemaStoreSnapshot,
    cache: &dyn EditorMetadataCacheLike,
) -> Value {
    read_metadata_snapshot(snapshot_key(store.active_db_session_id.as_deref()), cache)
}

pub const SCHEMA_STORE_SELECTORS_SURFACE: [&str; 10] = [
    "readMetadataSnapshot",
    "getMetadataRelation",
    "ensureMetadataRelations",
    "bindingToRelationRequests",
    "relationCacheKey",
    "resolveEditorDialectId",
    "relationLookupKey",
    "EditorMetadataCacheLike",
    "IdentifierFolder",
    "SCHEMA_STORE_SELECTORS_GAP",
];

pub const SCHEMA_STORE_SELECTORS_GAP: [&str; 5] = [
    "the process-wide metadataCache default is injected as EditorMetadataCacheLike",
    "useSyncExternalStore and the two React hooks stay DOM/React side",
    "DB_REGISTRY lookup is injected as DialectRegistryLookup",
    "foldIdentifierForDialect is injected as IdentifierFolder",
    "EditorMetadataSnapshot / EditorMetadataContext remain opaque JSON values",
];

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct RecordingCache {
        snapshots: BTreeMap<String, Value>,
        relations: BTreeMap<(String, String), Value>,
        ensures: RefCell<Vec<(String, Vec<EditorRelationRequest>, Value)>>,
        flushes: RefCell<usize>,
        subscribers: RefCell<usize>,
        invalidated_relations: RefCell<Vec<String>>,
        invalidated_sessions: RefCell<Vec<String>>,
        invalidated_all: RefCell<usize>,
        switches: RefCell<Vec<Value>>,
    }

    impl EditorMetadataCacheLike for RecordingCache {
        fn ensure_relations(
            &mut self,
            db_session_id: &str,
            requests: &[EditorRelationRequest],
            ctx: &Value,
        ) {
            self.ensures.borrow_mut().push((
                db_session_id.to_owned(),
                requests.to_vec(),
                ctx.clone(),
            ));
        }

        fn flush_now(&mut self) {
            *self.flushes.borrow_mut() += 1;
        }

        fn get_snapshot(&self, db_session_id: &str) -> Value {
            self.snapshots
                .get(db_session_id)
                .cloned()
                .unwrap_or(Value::Null)
        }

        fn get_relation(&self, db_session_id: &str, key: &str) -> Option<Value> {
            self.relations
                .get(&(db_session_id.to_owned(), key.to_owned()))
                .cloned()
        }

        fn subscribe(&self, _listener: &dyn Fn()) {
            *self.subscribers.borrow_mut() += 1;
        }

        fn invalidate_relation(&mut self, _db_session_id: &str, key: &str) {
            self.invalidated_relations.borrow_mut().push(key.to_owned());
        }

        fn invalidate_session(&mut self, db_session_id: &str) {
            self.invalidated_sessions
                .borrow_mut()
                .push(db_session_id.to_owned());
        }

        fn invalidate_all(&mut self) {
            *self.invalidated_all.borrow_mut() += 1;
        }

        fn switch_context(&mut self, ctx: &Value) {
            self.switches.borrow_mut().push(ctx.clone());
        }
    }

    struct LowerFolder;

    impl IdentifierFolder for LowerFolder {
        fn fold_segment(&self, name: &str, quoted: bool, _dialect_id: &str) -> String {
            if quoted {
                name.to_owned()
            } else {
                name.to_lowercase()
            }
        }
    }

    struct Registry;

    impl DialectRegistryLookup for Registry {
        fn sql_dialect(&self, database_type: &str) -> Option<String> {
            match database_type {
                "postgres" => Some("postgresql".to_owned()),
                _ => None,
            }
        }
    }

    fn binding(name: &str, kind: Option<SqlRelationSourceKind>) -> RelationBindingLike {
        RelationBindingLike {
            relation: QualifiedRelationId::bare(name),
            source_kind: kind,
        }
    }

    #[test]
    fn relation_kind_literals_round_trip() {
        for (kind, literal) in [
            (SqlRelationSourceKind::Table, "table"),
            (SqlRelationSourceKind::View, "view"),
            (SqlRelationSourceKind::Cte, "cte"),
            (SqlRelationSourceKind::Subquery, "subquery"),
        ] {
            assert_eq!(kind.as_str(), literal);
            assert_eq!(SqlRelationSourceKind::from_str(literal), kind);
        }
        assert_eq!(
            SqlRelationSourceKind::from_str("unknown"),
            SqlRelationSourceKind::Table
        );
    }

    #[test]
    fn bindings_drop_query_local_sources_only() {
        let requests = binding_to_relation_requests(&[
            binding("t", Some(SqlRelationSourceKind::Table)),
            binding("v", Some(SqlRelationSourceKind::View)),
            binding("cte", Some(SqlRelationSourceKind::Cte)),
            binding("sub", Some(SqlRelationSourceKind::Subquery)),
            binding("plain", None),
        ]);
        assert_eq!(
            requests
                .iter()
                .map(|request| request.identity.name.name.as_str())
                .collect::<Vec<_>>(),
            ["t", "v", "plain"]
        );
        assert_eq!(requests[0].kind, Some(SqlRelationSourceKind::Table));
        assert_eq!(requests[2].kind, None);
    }

    #[test]
    fn relation_keys_are_session_scoped_and_folded() {
        let identity = QualifiedRelationId {
            namespace_path: vec![SqlIdentifierSegment::bare("PUBLIC")],
            name: SqlIdentifierSegment::quoted("Users"),
        };
        let key = relation_cache_key("s1", &identity, "postgresql", &LowerFolder);
        assert_eq!(key, "s1::public.Users");
        assert_eq!(
            relation_lookup_key("s1", &identity, "postgresql", &LowerFolder),
            key
        );
        assert_eq!(
            editor_relation_key("s2", &identity, "postgresql", &LowerFolder),
            "s2::public.Users"
        );
    }

    #[test]
    fn dialect_ids_fall_back_from_registry_to_the_database_type() {
        let registry = Registry;
        assert_eq!(resolve_editor_dialect_id(None, &registry), "standard");
        assert_eq!(resolve_editor_dialect_id(Some(""), &registry), "standard");
        assert_eq!(
            resolve_editor_dialect_id(Some("postgres"), &registry),
            "postgresql"
        );
        assert_eq!(
            resolve_editor_dialect_id(Some("sqlite"), &registry),
            "sqlite"
        );
    }

    #[test]
    fn cache_reads_and_queues_are_observable() {
        let mut cache = RecordingCache::default();
        cache.snapshots.insert("s1".to_owned(), json!({"v": 1}));
        cache
            .relations
            .insert(("s1".to_owned(), "k".to_owned()), json!("meta"));
        assert_eq!(read_metadata_snapshot("s1", &cache), json!({"v": 1}));
        assert_eq!(read_metadata_snapshot("missing", &cache), Value::Null);
        assert_eq!(
            get_metadata_relation("s1", "k", &cache),
            Some(json!("meta"))
        );
        assert_eq!(get_metadata_relation("s1", "absent", &cache), None);

        let ctx = json!({"scope": "x"});
        let requests = vec![EditorRelationRequest {
            identity: QualifiedRelationId::bare("t"),
            kind: Some(SqlRelationSourceKind::Table),
        }];
        ensure_metadata_relations("s1", &requests, &ctx, &mut cache);
        assert_eq!(cache.ensures.borrow().len(), 1);
        assert_eq!(cache.ensures.borrow()[0].0, "s1");
        assert_eq!(cache.ensures.borrow()[0].2, ctx);
    }

    #[test]
    fn subscription_and_invalidation_seams_are_callable() {
        let mut cache = RecordingCache::default();
        cache.subscribe(&|| {});
        assert_eq!(*cache.subscribers.borrow(), 1);
        cache.flush_now();
        cache.invalidate_relation("s", "k");
        cache.invalidate_session("s");
        cache.invalidate_all();
        cache.switch_context(&json!("ctx"));
        assert_eq!(*cache.flushes.borrow(), 1);
        assert_eq!(
            cache.invalidated_relations.borrow().as_slice(),
            &["k".to_owned()]
        );
        assert_eq!(
            cache.invalidated_sessions.borrow().as_slice(),
            &["s".to_owned()]
        );
        assert_eq!(*cache.invalidated_all.borrow(), 1);
        assert_eq!(cache.switches.borrow().as_slice(), &[json!("ctx")]);
    }

    #[test]
    fn hooks_use_the_inactive_key_and_the_stores_active_session() {
        assert_eq!(snapshot_key(None), "__inactive__");
        assert_eq!(snapshot_key(Some("s1")), "s1");
        let mut cache = RecordingCache::default();
        cache
            .snapshots
            .insert("__inactive__".to_owned(), json!("empty"));
        let mut store = SchemaStoreSnapshot::default();
        assert_eq!(
            active_session_metadata_snapshot(&store, &cache),
            json!("empty")
        );
        store.active_db_session_id = Some("s1".to_owned());
        assert_eq!(
            active_session_metadata_snapshot(&store, &cache),
            Value::Null
        );
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(SCHEMA_STORE_SELECTORS_SURFACE.len(), 10);
        assert!(SCHEMA_STORE_SELECTORS_SURFACE.contains(&"bindingToRelationRequests"));
        assert_eq!(SCHEMA_STORE_SELECTORS_GAP.len(), 5);
        assert_eq!(crate::SCHEMA_STORE_SELECTORS_SURFACE.len(), 10);
        assert_eq!(crate::SCHEMA_STORE_SELECTORS_GAP.len(), 5);
        let _: Option<crate::EditorRelationRequest> = Some(EditorRelationRequest {
            identity: QualifiedRelationId::bare("t"),
            kind: None,
        });
    }
}
