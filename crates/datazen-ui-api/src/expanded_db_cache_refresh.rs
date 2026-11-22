use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionRef {
    pub db_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaSnapshot {
    pub databases: Vec<String>,
    pub schema_epoch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshInput {
    pub active_connections: Vec<(String, ConnectionRef)>,
    pub expanded_dbs: BTreeSet<String>,
    pub expanded_cats: BTreeSet<String>,
    pub schemas: BTreeMap<String, SchemaSnapshot>,
}

pub trait RefreshDeps {
    fn clear_caches(&self, db_session_id: &str, connection_id: &str);

    fn load_tables_for_db(&self, db_session_id: &str, db_name: &str);

    fn load_objects_for_cat(&self, db_session_id: &str, cat_key: &str, cat_id: &str, db_name: &str);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefreshEffect {
    ClearCaches {
        db_session_id: String,
        connection_id: String,
    },
    LoadTables {
        db_session_id: String,
        db_name: String,
    },
    LoadObjects {
        db_session_id: String,
        cat_key: String,
        cat_id: String,
        db_name: String,
    },
}

pub fn schema_fingerprint(databases: &[String], epoch: u64) -> String {
    format!("{}|{epoch}", databases.join("\0"))
}

pub struct ExpandedDbCacheRefresh {
    prev_fingerprints: BTreeMap<String, String>,
}

impl Default for ExpandedDbCacheRefresh {
    fn default() -> Self {
        Self {
            prev_fingerprints: BTreeMap::new(),
        }
    }
}

impl ExpandedDbCacheRefresh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fingerprint_count(&self) -> usize {
        self.prev_fingerprints.len()
    }

    pub fn fingerprint_of(&self, db_session_id: &str) -> Option<&str> {
        self.prev_fingerprints
            .get(db_session_id)
            .map(String::as_str)
    }

    pub fn on_change(
        &mut self,
        input: &RefreshInput,
        deps: &dyn RefreshDeps,
    ) -> Vec<RefreshEffect> {
        let mut next = BTreeMap::new();
        for (_connection_id, connection) in &input.active_connections {
            let session = match connection.db_session_id.as_deref() {
                Some(session) if !session.is_empty() => session,
                _ => continue,
            };
            if let Some(snapshot) = input.schemas.get(session) {
                next.insert(
                    session.to_owned(),
                    schema_fingerprint(&snapshot.databases, snapshot.schema_epoch),
                );
            }
        }

        let previous = std::mem::replace(&mut self.prev_fingerprints, next.clone());
        if previous.is_empty() {
            return Vec::new();
        }

        let mut effects = Vec::new();
        for (db_session_id, fingerprint) in &next {
            if previous.get(db_session_id) == Some(fingerprint) {
                continue;
            }
            let connection_id = input
                .active_connections
                .iter()
                .find(|(_, connection)| {
                    connection.db_session_id.as_deref() == Some(db_session_id.as_str())
                })
                .map(|(id, _)| id.clone());
            let connection_id = match connection_id {
                Some(connection_id) => connection_id,
                None => continue,
            };
            deps.clear_caches(db_session_id, &connection_id);
            effects.push(RefreshEffect::ClearCaches {
                db_session_id: db_session_id.clone(),
                connection_id: connection_id.clone(),
            });

            let prefix = format!("{connection_id}::");
            for db_key in &input.expanded_dbs {
                if !db_key.starts_with(&prefix) {
                    continue;
                }
                let db_name = db_key[prefix.len()..].to_owned();
                deps.load_tables_for_db(db_session_id, &db_name);
                effects.push(RefreshEffect::LoadTables {
                    db_session_id: db_session_id.clone(),
                    db_name,
                });
            }

            for cat_key in &input.expanded_cats {
                if !cat_key.starts_with(&prefix) {
                    continue;
                }
                let cat_id = cat_key.rsplit("::").next().unwrap_or("").to_owned();
                if cat_id.is_empty() || cat_id == "tables" || cat_id == "views" {
                    continue;
                }
                let mut database_names = input
                    .schemas
                    .get(db_session_id)
                    .map(|snapshot| snapshot.databases.clone())
                    .unwrap_or_default();
                for db_key in &input.expanded_dbs {
                    if db_key.starts_with(&prefix) {
                        database_names.push(db_key[prefix.len()..].to_owned());
                    }
                }
                let mut candidates = database_names
                    .into_iter()
                    .filter(|name| cat_key.starts_with(&format!("{prefix}{name}::")))
                    .collect::<Vec<_>>();
                candidates.sort_by(|left, right| right.len().cmp(&left.len()));
                let database = match candidates.into_iter().next() {
                    Some(database) => database,
                    None => continue,
                };
                deps.load_objects_for_cat(db_session_id, cat_key, &cat_id, &database);
                effects.push(RefreshEffect::LoadObjects {
                    db_session_id: db_session_id.clone(),
                    cat_key: cat_key.clone(),
                    cat_id,
                    db_name: database,
                });
            }
        }
        effects
    }
}

pub const EXPANDED_DB_CACHE_REFRESH_SURFACE: [&str; 7] = [
    "schemaFingerprint",
    "ExpandedDbCacheRefresh",
    "onChange",
    "RefreshInput",
    "RefreshDeps",
    "RefreshEffect",
    "EXPANDED_DB_CACHE_REFRESH_GAP",
];

pub const EXPANDED_DB_CACHE_REFRESH_GAP: [&str; 5] = [
    "React useEffect/useRef and the stable callback ref",
    "the zustand schemaStore read is a plain snapshot in RefreshInput",
    "table/object loader promises and their cancellation stay host-side",
    "no useDatabase call is modeled — all seams are session-neutral",
    "explicit user refreshes may add duplicate fetches; their writes are equally fresh",
];

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct Deps {
        calls: RefCell<Vec<RefreshEffect>>,
    }

    impl RefreshDeps for Deps {
        fn clear_caches(&self, db_session_id: &str, connection_id: &str) {
            self.calls.borrow_mut().push(RefreshEffect::ClearCaches {
                db_session_id: db_session_id.to_owned(),
                connection_id: connection_id.to_owned(),
            });
        }

        fn load_tables_for_db(&self, db_session_id: &str, db_name: &str) {
            self.calls.borrow_mut().push(RefreshEffect::LoadTables {
                db_session_id: db_session_id.to_owned(),
                db_name: db_name.to_owned(),
            });
        }

        fn load_objects_for_cat(
            &self,
            db_session_id: &str,
            cat_key: &str,
            cat_id: &str,
            db_name: &str,
        ) {
            self.calls.borrow_mut().push(RefreshEffect::LoadObjects {
                db_session_id: db_session_id.to_owned(),
                cat_key: cat_key.to_owned(),
                cat_id: cat_id.to_owned(),
                db_name: db_name.to_owned(),
            });
        }
    }

    fn connection(connection_id: &str, session: Option<&str>) -> (String, ConnectionRef) {
        (
            connection_id.to_owned(),
            ConnectionRef {
                db_session_id: session.map(str::to_owned),
            },
        )
    }

    fn input(epoch: u64) -> RefreshInput {
        RefreshInput {
            active_connections: vec![connection("c1", Some("s1")), connection("c2", Some("s2"))],
            expanded_dbs: BTreeSet::from(["c1::app".to_owned(), "c2::dw".to_owned()]),
            expanded_cats: BTreeSet::from([
                "c1::app::functions".to_owned(),
                "c2::dw::tables".to_owned(),
            ]),
            schemas: BTreeMap::from([
                (
                    "s1".to_owned(),
                    SchemaSnapshot {
                        databases: vec!["app".to_owned()],
                        schema_epoch: epoch,
                    },
                ),
                (
                    "s2".to_owned(),
                    SchemaSnapshot {
                        databases: vec!["dw".to_owned()],
                        schema_epoch: epoch,
                    },
                ),
            ]),
        }
    }

    fn as_calls(effects: &[RefreshEffect]) -> Vec<RefreshEffect> {
        effects.to_vec()
    }

    #[test]
    fn the_fingerprint_joins_databases_with_nul_and_appends_the_epoch() {
        assert_eq!(
            schema_fingerprint(&["a".to_owned(), "b".to_owned()], 7),
            "a\0b|7"
        );
        assert_eq!(schema_fingerprint(&[], 0), "|0");
    }

    #[test]
    fn the_first_pass_only_records_fingerprints() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        assert!(watcher.on_change(&input(1), &deps).is_empty());
        assert_eq!(watcher.fingerprint_count(), 2);
        assert_eq!(watcher.fingerprint_of("s1"), Some("app|1"));
        assert!(deps.calls.borrow().is_empty());
    }

    #[test]
    fn an_unchanged_fingerprint_does_not_reload() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        watcher.on_change(&input(3), &deps);
        assert!(watcher.on_change(&input(3), &deps).is_empty());
        assert!(deps.calls.borrow().is_empty());
    }

    #[test]
    fn a_changed_fingerprint_clears_then_reloads_only_that_connection() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        watcher.on_change(&input(1), &deps);
        let mut changed = input(1);
        {
            let s1 = expect_snapshot(changed.schemas.get_mut("s1"));
            s1.schema_epoch = 2;
            s1.databases.push("app2".to_owned());
        }
        let effects = watcher.on_change(&changed, &deps);
        assert_eq!(
            as_calls(&effects),
            vec![
                RefreshEffect::ClearCaches {
                    db_session_id: "s1".to_owned(),
                    connection_id: "c1".to_owned(),
                },
                RefreshEffect::LoadTables {
                    db_session_id: "s1".to_owned(),
                    db_name: "app".to_owned(),
                },
                RefreshEffect::LoadObjects {
                    db_session_id: "s1".to_owned(),
                    cat_key: "c1::app::functions".to_owned(),
                    cat_id: "functions".to_owned(),
                    db_name: "app".to_owned(),
                },
            ]
        );
        assert_eq!(watcher.fingerprint_of("s1"), Some("app\0app2|2"));
    }

    fn expect_snapshot<'a>(value: Option<&'a mut SchemaSnapshot>) -> &'a mut SchemaSnapshot {
        match value {
            Some(snapshot) => snapshot,
            None => panic!("expected a schema snapshot"),
        }
    }

    #[test]
    fn table_and_view_categories_are_skipped() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        watcher.on_change(&input(1), &deps);
        let mut changed = input(1);
        expect_snapshot(changed.schemas.get_mut("s1")).schema_epoch = 2;
        changed.expanded_cats =
            BTreeSet::from(["c1::app::tables".to_owned(), "c1::app::views".to_owned()]);
        let effects = watcher.on_change(&changed, &deps);
        assert_eq!(effects.len(), 2);
        assert!(effects
            .iter()
            .all(|effect| matches!(effect, RefreshEffect::ClearCaches { .. })
                || matches!(effect, RefreshEffect::LoadTables { .. })));
    }

    #[test]
    fn the_longest_matching_database_wins_for_a_category() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        watcher.on_change(&input(1), &deps);
        let mut changed = input(2);
        changed.expanded_dbs.insert("c1::app2".to_owned());
        changed.expanded_cats = BTreeSet::from(["c1::app::types".to_owned()]);
        let effects = watcher.on_change(&changed, &deps);
        let objects = effects.iter().find_map(|effect| match effect {
            RefreshEffect::LoadObjects { db_name, .. } => Some(db_name.as_str()),
            _ => None,
        });
        assert_eq!(objects, Some("app"));
    }

    #[test]
    fn sessions_without_a_snapshot_or_connection_are_ignored() {
        let mut watcher = ExpandedDbCacheRefresh::new();
        let deps = Deps::default();
        let mut missing = input(1);
        missing.schemas.remove("s2");
        watcher.on_change(&missing, &deps);
        assert_eq!(watcher.fingerprint_count(), 1);

        let mut changed = input(2);
        changed.schemas.remove("s2");
        let effects = watcher.on_change(&changed, &deps);
        assert_eq!(effects.len(), 3);
        assert!(effects.iter().all(|effect| match effect {
            RefreshEffect::ClearCaches { db_session_id, .. }
            | RefreshEffect::LoadTables { db_session_id, .. }
            | RefreshEffect::LoadObjects { db_session_id, .. } => db_session_id == "s1",
        }));
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(EXPANDED_DB_CACHE_REFRESH_SURFACE.len(), 7);
        assert!(EXPANDED_DB_CACHE_REFRESH_SURFACE.contains(&"schemaFingerprint"));
        assert_eq!(EXPANDED_DB_CACHE_REFRESH_GAP.len(), 5);
        assert_eq!(crate::EXPANDED_DB_CACHE_REFRESH_SURFACE.len(), 7);
        assert_eq!(crate::EXPANDED_DB_CACHE_REFRESH_GAP.len(), 5);
        let _: Option<crate::SchemaSnapshot> = Some(SchemaSnapshot::default());
    }
}
