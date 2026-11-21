use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::{json, Value};

use crate::TableSchema;

const VALUE_TTL_MS: i64 = 60_000;
const ERROR_TTL_MS: i64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaObjectKind {
    Table,
    View,
}

impl SchemaObjectKind {
    fn as_str(self) -> &'static str {
        match self {
            SchemaObjectKind::Table => "table",
            SchemaObjectKind::View => "view",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DdlCacheIdentity {
    pub object_kind: Option<SchemaObjectKind>,
    pub namespace_path: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DdlCacheInvalidateScope {
    pub object_kind: Option<SchemaObjectKind>,
    pub namespace_path: Vec<String>,
}

pub fn cache_key(db_session_id: &str, table_name: &str, database: &str, schema: Option<&str>) -> String {
    json!([db_session_id, database, schema, table_name]).to_string()
}

pub fn ddl_cache_key(
    db_session_id: &str,
    table_name: &str,
    database: &str,
    identity: Option<&DdlCacheIdentity>,
) -> String {
    let kind = match identity.and_then(|value| value.object_kind) {
        Some(kind) => kind.as_str(),
        None => SchemaObjectKind::Table.as_str(),
    };
    let namespace = identity.map(|value| value.namespace_path.as_slice()).unwrap_or(&[]);
    json!([db_session_id, database, kind, namespace, table_name]).to_string()
}

pub fn composite_cache_key(scope: &str, inner: &str) -> String {
    json!([scope, inner]).to_string()
}

struct ResourceCache<T> {
    values: HashMap<String, (T, i64)>,
    errors: HashMap<String, (String, i64)>,
}

impl<T: Clone> ResourceCache<T> {
    fn new() -> Self {
        Self { values: HashMap::new(), errors: HashMap::new() }
    }

    fn get<F>(&mut self, key: &str, now: i64, load: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        if let Some((value, at)) = self.values.get(key) {
            if now - *at < VALUE_TTL_MS {
                return Ok(value.clone());
            }
        }
        if let Some((error, at)) = self.errors.get(key) {
            if now - *at < ERROR_TTL_MS {
                return Err(error.clone());
            }
        }
        match load() {
            Ok(value) => {
                self.values.insert(key.to_owned(), (value.clone(), now));
                self.errors.remove(key);
                Ok(value)
            }
            Err(error) => {
                self.errors.insert(key.to_owned(), (error.clone(), now));
                Err(error)
            }
        }
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.values.keys().cloned().collect();
        for key in self.errors.keys() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    }

    fn invalidate<F>(&mut self, matches: F)
    where
        F: Fn(&str) -> bool,
    {
        let doomed: Vec<String> = self.keys().into_iter().filter(|key| matches(key)).collect();
        for key in doomed {
            self.values.remove(&key);
            self.errors.remove(&key);
        }
    }
}

type InvalidationListener = Rc<dyn Fn(&str, Option<&str>)>;

fn parse_inner(key: &str) -> Option<Vec<Value>> {
    let outer: Value = serde_json::from_str(key).ok()?;
    let candidate = match &outer {
        Value::Array(items) if items.len() == 2 => match &items[1] {
            Value::String(text) => match serde_json::from_str::<Value>(text) {
                Ok(value) => value,
                Err(_) => Value::Null,
            },
            other => other.clone(),
        },
        other => other.clone(),
    };
    match candidate {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

fn matches_invalidation(
    key: &str,
    db_session_id: Option<&str>,
    table_name: Option<&str>,
    identity: Option<&DdlCacheInvalidateScope>,
    database: Option<&str>,
    ddl: bool,
) -> Option<String> {
    let parts = parse_inner(key)?;
    let session = match parts.first() {
        Some(Value::String(value)) => value.clone(),
        _ => return None,
    };
    if db_session_id.is_some_and(|value| value != session) {
        return None;
    }
    if let Some(database) = database {
        if parts.get(1).and_then(Value::as_str) != Some(database) {
            return None;
        }
    }
    let name = if ddl { parts.get(4) } else { parts.get(3) };
    if let Some(table_name) = table_name {
        if name.and_then(Value::as_str) != Some(table_name) {
            return None;
        }
    }
    if let Some(identity) = identity {
        if ddl {
            if let Some(kind) = identity.object_kind {
                if parts.get(2).and_then(Value::as_str) != Some(kind.as_str()) {
                    return None;
                }
            }
            let expected = json!(&identity.namespace_path);
            if parts.get(3) != Some(&expected) {
                return None;
            }
        }
    }
    Some(session)
}

pub struct SchemaCacheFacade {
    schema: ResourceCache<TableSchema>,
    ddl: ResourceCache<String>,
    listeners: Vec<InvalidationListener>,
    clock: Rc<dyn Fn() -> i64>,
}

impl SchemaCacheFacade {
    pub fn new(clock: Rc<dyn Fn() -> i64>) -> Self {
        Self {
            schema: ResourceCache::new(),
            ddl: ResourceCache::new(),
            listeners: Vec::new(),
            clock,
        }
    }

    pub fn subscribe(&mut self, listener: InvalidationListener) -> Box<dyn Fn()> {
        self.listeners.push(listener.clone());
        let current = Rc::new(RefCell::new(self.listeners.clone()));
        Box::new(move || {
            let mut listeners = current.borrow_mut();
            if let Some(index) = listeners.iter().position(|item| Rc::ptr_eq(item, &listener)) {
                listeners.remove(index);
            }
        })
    }

    fn notify(&self, session: &str, table: Option<&str>) {
        let listeners = self.listeners.clone();
        for listener in listeners {
            listener(session, table);
        }
    }

    pub fn get_cached_table_schema<F>(
        &mut self,
        scope: &str,
        db_session_id: &str,
        table_name: &str,
        database: &str,
        schema: Option<&str>,
        load: F,
    ) -> Result<TableSchema, String>
    where
        F: FnOnce() -> Result<TableSchema, String>,
    {
        let key = composite_cache_key(scope, &cache_key(db_session_id, table_name, database, schema));
        let now = (self.clock)();
        self.schema.get(&key, now, load)
    }

    pub fn get_cached_ddl<F, E>(
        &mut self,
        scope: &str,
        db_session_id: &str,
        table_name: &str,
        database: &str,
        identity: Option<&DdlCacheIdentity>,
        load: F,
        extract: E,
    ) -> Result<String, String>
    where
        F: FnOnce() -> Result<Vec<Vec<Value>>, String>,
        E: FnOnce(&[Vec<Value>]) -> String,
    {
        let key = composite_cache_key(scope, &ddl_cache_key(db_session_id, table_name, database, identity));
        let now = (self.clock)();
        self.ddl.get(&key, now, || load().map(|rows| extract(&rows)))
    }

    pub fn invalidate_schema_cache(
        &mut self,
        db_session_id: Option<&str>,
        table_name: Option<&str>,
        identity: Option<&DdlCacheInvalidateScope>,
        database: Option<&str>,
    ) {
        let schema_matches = |key: &str| matches_invalidation(key, db_session_id, table_name, None, database, false).is_some();
        let ddl_matches = |key: &str| matches_invalidation(key, db_session_id, table_name, identity, database, true).is_some();
        let mut sessions = Vec::new();
        for key in self.schema.keys() {
            if schema_matches(&key) {
                if let Some(session) = matches_invalidation(&key, db_session_id, table_name, None, database, false) {
                    if !sessions.contains(&session) {
                        sessions.push(session);
                    }
                }
            }
        }
        self.schema.invalidate(schema_matches);
        self.ddl.invalidate(ddl_matches);
        match db_session_id {
            Some(session) => self.notify(session, table_name),
            None => {
                let sessions = sessions.clone();
                for session in sessions {
                    self.notify(&session, table_name);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use serde_json::json;

    use super::*;

    fn schema(name: &str) -> TableSchema {
        TableSchema {
            table_name: name.to_owned(),
            columns: Vec::new(),
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        }
    }

    fn facade(clock: Rc<Cell<i64>>) -> SchemaCacheFacade {
        SchemaCacheFacade::new(Rc::new(move || clock.get()))
    }

    #[test]
    fn keys_are_json_arrays_and_compose_with_scope() {
        assert_eq!(cache_key("s", "t", "db", Some("pub")), "[\"s\",\"db\",\"pub\",\"t\"]");
        assert_eq!(cache_key("s", "t", "db", None), "[\"s\",\"db\",null,\"t\"]");
        assert_eq!(
            ddl_cache_key("s", "t", "db", None),
            "[\"s\",\"db\",\"table\",[],\"t\"]"
        );
        let identity = DdlCacheIdentity {
            object_kind: Some(SchemaObjectKind::View),
            namespace_path: vec!["ns".to_owned()],
        };
        assert_eq!(
            ddl_cache_key("s", "t", "db", Some(&identity)),
            "[\"s\",\"db\",\"view\",[\"ns\"],\"t\"]"
        );
        assert_eq!(composite_cache_key("scope", "inner"), "[\"scope\",\"inner\"]");
    }

    #[test]
    fn schema_and_ddl_cache_by_scope_and_key_with_ttl() {
        let clock = Rc::new(Cell::new(0));
        let mut cache = facade(clock.clone());
        let loads = Rc::new(Cell::new(0usize));
        let counter = loads.clone();
        let load = move || {
            counter.set(counter.get() + 1);
            Ok(schema("t"))
        };
        let first = cache.get_cached_table_schema("scope1", "s", "t", "db", Some("pub"), load).expect("ok");
        assert_eq!(first.table_name, "t");
        let other = loads.clone();
        let _ = cache.get_cached_table_schema("scope1", "s", "t", "db", Some("pub"), move || {
            other.set(other.get() + 1);
            Ok(schema("other"))
        }).expect("ok");
        assert_eq!(loads.get(), 1, "fresh value is reused");
        let other = loads.clone();
        let _ = cache.get_cached_table_schema("scope2", "s", "t", "db", Some("pub"), move || {
            other.set(other.get() + 1);
            Ok(schema("t"))
        }).expect("ok");
        assert_eq!(loads.get(), 2, "a different scope is a different cell");
        clock.set(60_000);
        let other = loads.clone();
        let _ = cache.get_cached_table_schema("scope1", "s", "t", "db", Some("pub"), move || {
            other.set(other.get() + 1);
            Ok(schema("t"))
        }).expect("ok");
        assert_eq!(loads.get(), 3, "the value TTL expires at 60s");
    }

    #[test]
    fn failures_are_cached_for_ten_seconds() {
        let clock = Rc::new(Cell::new(0));
        let mut cache = facade(clock.clone());
        let loads = Rc::new(Cell::new(0usize));
        let make_failing = || {
            let counter = loads.clone();
            move || {
                counter.set(counter.get() + 1);
                Err("boom".to_owned())
            }
        };
        assert_eq!(cache.get_cached_table_schema("sc", "s", "t", "db", None, make_failing()), Err("boom".to_owned()));
        assert_eq!(cache.get_cached_table_schema("sc", "s", "t", "db", None, make_failing()), Err("boom".to_owned()));
        assert_eq!(loads.get(), 1);
        clock.set(10_000);
        let _ = cache.get_cached_table_schema("sc", "s", "t", "db", None, make_failing());
        assert_eq!(loads.get(), 2);
    }

    #[test]
    fn ddl_loader_runs_the_extractor() {
        let clock = Rc::new(Cell::new(0));
        let mut cache = facade(clock);
        let rows = || Ok(vec![vec![Value::Null, Value::String("CREATE TABLE t".into())]]);
        let extract = |rows: &[Vec<Value>]| -> String {
            match rows.first().and_then(|row| row.get(1)).and_then(Value::as_str) {
                Some(text) => text.to_owned(),
                None => String::new(),
            }
        };
        let ddl = cache.get_cached_ddl("sc", "s", "t", "db", None, rows, extract).expect("ok");
        assert_eq!(ddl, "CREATE TABLE t");
    }

    #[test]
    fn invalidation_matches_session_database_and_table() {
        let key = composite_cache_key("sc", &cache_key("s1", "users", "db0", Some("public")));
        assert!(matches_invalidation(&key, Some("s1"), Some("users"), None, Some("db0"), false).is_some());
        assert!(matches_invalidation(&key, Some("s2"), Some("users"), None, Some("db0"), false).is_none());
        assert!(matches_invalidation(&key, Some("s1"), Some("orders"), None, Some("db0"), false).is_none());
        assert!(matches_invalidation(&key, Some("s1"), Some("users"), None, Some("db1"), false).is_none());
        assert!(matches_invalidation("not json", None, None, None, None, false).is_none());
    }

    #[test]
    fn ddl_invalidation_respects_kind_and_namespace() {
        let identity = DdlCacheIdentity {
            object_kind: Some(SchemaObjectKind::View),
            namespace_path: vec!["ns".to_owned()],
        };
        let key = composite_cache_key("sc", &ddl_cache_key("s1", "v", "db0", Some(&identity)));
        let matching = DdlCacheInvalidateScope {
            object_kind: Some(SchemaObjectKind::View),
            namespace_path: vec!["ns".to_owned()],
        };
        assert!(matches_invalidation(&key, Some("s1"), Some("v"), Some(&matching), None, true).is_some());
        let wrong_kind = DdlCacheInvalidateScope {
            object_kind: Some(SchemaObjectKind::Table),
            namespace_path: vec!["ns".to_owned()],
        };
        assert!(matches_invalidation(&key, Some("s1"), Some("v"), Some(&wrong_kind), None, true).is_none());
        let wrong_ns = DdlCacheInvalidateScope {
            object_kind: Some(SchemaObjectKind::View),
            namespace_path: vec!["other".to_owned()],
        };
        assert!(matches_invalidation(&key, Some("s1"), Some("v"), Some(&wrong_ns), None, true).is_none());
    }

    #[test]
    fn invalidation_drops_entries_and_notifies() {
        let clock = Rc::new(Cell::new(0));
        let mut cache = facade(clock.clone());
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _unsubscribe = cache.subscribe(Rc::new(move |session, table| {
            sink.borrow_mut().push((session.to_owned(), table.map(str::to_owned)));
        }));
        let _ = cache.get_cached_table_schema("sc", "s1", "users", "db0", None, || Ok(schema("users"))).expect("ok");
        cache.invalidate_schema_cache(Some("s1"), Some("users"), None, None);
        assert_eq!(seen.borrow().as_slice(), &[("s1".to_owned(), Some("users".to_owned()))]);
        let loads = Rc::new(Cell::new(0usize));
        let counter = loads.clone();
        let _ = cache.get_cached_table_schema("sc", "s1", "users", "db0", None, move || {
            counter.set(counter.get() + 1);
            Ok(schema("users"))
        }).expect("ok");
        assert_eq!(loads.get(), 1, "the invalidated cell was refetched");
    }

    #[test]
    fn invalidate_without_session_notifies_each_matched_session() {
        let clock = Rc::new(Cell::new(0));
        let mut cache = facade(clock);
        let _ = cache.get_cached_table_schema("sc", "s1", "users", "db0", None, || Ok(schema("users"))).expect("ok");
        let _ = cache.get_cached_table_schema("sc", "s2", "users", "db0", None, || Ok(schema("users"))).expect("ok");
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _unsubscribe = cache.subscribe(Rc::new(move |session, _| {
            sink.borrow_mut().push(session.to_owned());
        }));
        cache.invalidate_schema_cache(None, Some("users"), None, None);
        let mut sessions = seen.borrow().clone();
        sessions.sort();
        assert_eq!(sessions, vec!["s1".to_owned(), "s2".to_owned()]);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::cache_key;
        let _ = crate::ddl_cache_key;
        let _ = crate::composite_cache_key;
        let _: Option<crate::DdlCacheIdentity> = None;
        let _: Option<crate::DdlCacheInvalidateScope> = None;
        let _: Option<crate::SchemaCacheFacade> = None;
        assert_eq!(crate::SchemaObjectKind::View.as_str(), "view");
        let _ = json!([]);
    }
}
