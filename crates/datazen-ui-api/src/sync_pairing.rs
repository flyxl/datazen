use std::collections::HashMap;

pub const SYNC_PAIRING_CACHE_DELIMITER: char = '\0';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPath {
    Direct,
    Ir,
    Unsupported,
}

impl SyncPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Ir => "ir",
            Self::Unsupported => "unsupported",
        }
    }
}

pub fn sync_path_from_str(value: &str) -> SyncPath {
    match value {
        "direct" => SyncPath::Direct,
        "ir" => SyncPath::Ir,
        _ => SyncPath::Unsupported,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncPairingResult {
    pub path: SyncPath,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncCategory {
    Sql,
    Document,
    Kv,
    Other,
}

impl SyncCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::Document => "document",
            Self::Kv => "kv",
            Self::Other => "other",
        }
    }
}

fn sync_category_from_str(value: &str) -> SyncCategory {
    match value {
        "document" => SyncCategory::Document,
        "kv" => SyncCategory::Kv,
        "other" => SyncCategory::Other,
        _ => SyncCategory::Sql,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRegistryMeta {
    pub sync_category: Option<String>,
    pub category: Option<String>,
    pub sync_family: Option<String>,
    pub sql_dialect: Option<String>,
}

pub fn normalize_driver_id(raw: &str) -> String {
    match raw.to_lowercase().as_str() {
        "postgres" => "postgresql".to_owned(),
        "mssql" => "sqlserver".to_owned(),
        "presto" => "trino".to_owned(),
        "tidb" | "oceanbase" => "mysql".to_owned(),
        other => other.to_owned(),
    }
}

fn lookup_meta<F>(db_type: &str, registry: &F) -> Option<SyncRegistryMeta>
where
    F: Fn(&str) -> Option<SyncRegistryMeta>,
{
    registry(db_type).or_else(|| registry(&normalize_driver_id(db_type)))
}

fn category_from_meta(meta: Option<&SyncRegistryMeta>, db_type: &str) -> SyncCategory {
    if let Some(meta) = meta {
        if let Some(sync_category) = meta.sync_category.as_deref() {
            return sync_category_from_str(sync_category);
        }
        if meta.category.as_deref() == Some("kv") {
            return SyncCategory::Kv;
        }
        if meta.category.as_deref() == Some("document") {
            return SyncCategory::Document;
        }
    }
    let id = normalize_driver_id(db_type);
    if id == "kiwi" || id == "superset" {
        return SyncCategory::Other;
    }
    SyncCategory::Sql
}

pub fn sync_category<F>(db_type: &str, registry: &F) -> SyncCategory
where
    F: Fn(&str) -> Option<SyncRegistryMeta>,
{
    category_from_meta(lookup_meta(db_type, registry).as_ref(), db_type)
}

pub fn normalize_sync_family<F>(db_type: &str, registry: &F) -> String
where
    F: Fn(&str) -> Option<SyncRegistryMeta>,
{
    let meta = lookup_meta(db_type, registry);
    if let Some(meta) = meta.as_ref() {
        if let Some(family) = meta.sync_family.as_deref() {
            return family.to_owned();
        }
        if let Some(dialect) = meta.sql_dialect.as_deref() {
            if dialect != "generic" {
                return dialect.to_owned();
            }
        }
    }
    normalize_driver_id(db_type)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PairingView {
    pub path: String,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}

pub fn view_to_result(view: &PairingView) -> SyncPairingResult {
    SyncPairingResult {
        path: sync_path_from_str(&view.path),
        supported: view.supported,
        family: view.family.clone(),
        reason: view.reason.clone(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncPairingCache {
    entries: HashMap<String, SyncPairingResult>,
}

impl SyncPairingCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn resolve<F>(
        &mut self,
        source_type: &str,
        target_type: &str,
        classify: F,
    ) -> SyncPairingResult
    where
        F: Fn(&str, &str) -> PairingView,
    {
        let key = format!(
            "{}{}{}",
            source_type, SYNC_PAIRING_CACHE_DELIMITER, target_type
        );
        if let Some(cached) = self.entries.get(&key) {
            return cached.clone();
        }
        let view = classify(source_type, target_type);
        let result = view_to_result(&view);
        self.entries.insert(key, result.clone());
        result
    }

    pub fn is_target_supported<F>(
        &mut self,
        source_type: &str,
        target_type: &str,
        classify: F,
    ) -> bool
    where
        F: Fn(&str, &str) -> PairingView,
    {
        self.resolve(source_type, target_type, classify).supported
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncConnection {
    pub id: String,
    pub database_type: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncPairingState {
    pub target_support: HashMap<String, bool>,
    pub active_pairing: Option<SyncPairingResult>,
}

pub fn compute_sync_pairing_state<S, P>(
    source_type: Option<&str>,
    connections: &[SyncConnection],
    target_id: &str,
    cancelled: bool,
    support: S,
    pairing: P,
) -> SyncPairingState
where
    S: Fn(&str) -> bool,
    P: Fn(&str) -> SyncPairingResult,
{
    let source_type = match source_type {
        Some(source_type) if !cancelled => source_type,
        _ => return SyncPairingState::default(),
    };
    let mut target_support = HashMap::new();
    for connection in connections {
        target_support.insert(connection.id.clone(), support(&connection.database_type));
    }

    let active_pairing = if target_id.is_empty() {
        None
    } else {
        connections
            .iter()
            .find(|connection| connection.id == target_id)
            .map(|connection| pairing(&connection.database_type))
    };

    SyncPairingState {
        target_support,
        active_pairing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_meta(_: &str) -> Option<SyncRegistryMeta> {
        None
    }

    #[test]
    fn driver_ids_normalize_to_the_taxonomy_canonical_form() {
        assert_eq!(normalize_driver_id("POSTGRES"), "postgresql");
        assert_eq!(normalize_driver_id("MSSQL"), "sqlserver");
        assert_eq!(normalize_driver_id("Presto"), "trino");
        assert_eq!(normalize_driver_id("TiDB"), "mysql");
        assert_eq!(normalize_driver_id("OceanBase"), "mysql");
        assert_eq!(normalize_driver_id("  SQLite  "), "  sqlite  ");
    }

    #[test]
    fn category_prefers_sync_category_then_category_then_special_ids() {
        let meta = SyncRegistryMeta {
            sync_category: Some("document".to_owned()),
            category: Some("kv".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            sync_category("x", &|_| Some(meta.clone())),
            SyncCategory::Document
        );
        let kv = SyncRegistryMeta {
            category: Some("kv".to_owned()),
            ..Default::default()
        };
        assert_eq!(sync_category("x", &|_| Some(kv.clone())), SyncCategory::Kv);
        assert_eq!(sync_category("kiwi", &no_meta), SyncCategory::Other);
        assert_eq!(sync_category("superset", &no_meta), SyncCategory::Other);
        assert_eq!(sync_category("postgres", &no_meta), SyncCategory::Sql);
    }

    #[test]
    fn registry_lookup_falls_back_to_the_normalized_id() {
        let called = std::cell::RefCell::new(Vec::new());
        let registry = |id: &str| {
            called.borrow_mut().push(id.to_owned());
            if id == "postgresql" {
                Some(SyncRegistryMeta {
                    sync_category: Some("kv".to_owned()),
                    ..Default::default()
                })
            } else {
                None
            }
        };
        assert_eq!(sync_category("postgres", &registry), SyncCategory::Kv);
        assert_eq!(called.borrow().as_slice(), ["postgres", "postgresql"]);
    }

    #[test]
    fn sync_family_prefers_sync_family_then_dialect_then_driver_id() {
        let meta = SyncRegistryMeta {
            sync_family: Some("family".to_owned()),
            sql_dialect: Some("postgres".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            normalize_sync_family("x", &|_| Some(meta.clone())),
            "family"
        );
        let dialect = SyncRegistryMeta {
            sql_dialect: Some("mysql".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            normalize_sync_family("x", &|_| Some(dialect.clone())),
            "mysql"
        );
        let generic = SyncRegistryMeta {
            sql_dialect: Some("generic".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            normalize_sync_family("mssql", &|_| Some(generic.clone())),
            "sqlserver"
        );
        assert_eq!(normalize_sync_family("presto", &no_meta), "trino");
    }

    #[test]
    fn pairing_view_maps_null_legs_and_unknown_paths() {
        let result = view_to_result(&PairingView {
            path: "ir".to_owned(),
            supported: true,
            family: None,
            reason: Some("why".to_owned()),
        });
        assert_eq!(result.path, SyncPath::Ir);
        assert!(result.supported);
        assert_eq!(result.family, None);
        assert_eq!(result.reason.as_deref(), Some("why"));
        assert_eq!(sync_path_from_str("mystery"), SyncPath::Unsupported);
        assert_eq!(SyncPath::Direct.as_str(), "direct");
    }

    #[test]
    fn cache_is_keyed_by_both_types_and_caches_negative_results() {
        let mut cache = SyncPairingCache::new();
        let calls = std::cell::RefCell::new(0usize);
        let classify = |source: &str, target: &str| {
            *calls.borrow_mut() += 1;
            PairingView {
                path: "direct".to_owned(),
                supported: source == "postgresql" && target == "mysql",
                family: None,
                reason: None,
            }
        };
        assert!(cache.is_target_supported("postgresql", "mysql", &classify));
        assert!(cache.is_target_supported("postgresql", "mysql", &classify));
        assert!(!cache.is_target_supported("postgresql", "sqlite", &classify));
        assert_eq!(*calls.borrow(), 2);
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn state_is_empty_without_a_source_or_when_cancelled() {
        let state = compute_sync_pairing_state(
            None,
            &[SyncConnection {
                id: "a".to_owned(),
                database_type: "mysql".to_owned(),
            }],
            "a",
            false,
            |_| true,
            |_| SyncPairingResult {
                path: SyncPath::Direct,
                supported: true,
                family: None,
                reason: None,
            },
        );
        assert!(state.target_support.is_empty());
        assert_eq!(state.active_pairing, None);

        let state = compute_sync_pairing_state(
            Some("postgresql"),
            &[SyncConnection {
                id: "a".to_owned(),
                database_type: "mysql".to_owned(),
            }],
            "a",
            true,
            |_| true,
            |_| SyncPairingResult {
                path: SyncPath::Direct,
                supported: true,
                family: None,
                reason: None,
            },
        );
        assert!(state.target_support.is_empty());
    }

    #[test]
    fn state_builds_support_for_every_connection_and_the_active_pair() {
        let connections = [
            SyncConnection {
                id: "a".to_owned(),
                database_type: "mysql".to_owned(),
            },
            SyncConnection {
                id: "b".to_owned(),
                database_type: "kiwi".to_owned(),
            },
        ];
        let state = compute_sync_pairing_state(
            Some("postgresql"),
            &connections,
            "b",
            false,
            |database_type| database_type == "mysql",
            |database_type| SyncPairingResult {
                path: if database_type == "mysql" {
                    SyncPath::Direct
                } else {
                    SyncPath::Unsupported
                },
                supported: database_type == "mysql",
                family: None,
                reason: None,
            },
        );
        assert_eq!(state.target_support["a"], true);
        assert_eq!(state.target_support["b"], false);
        let active = state.active_pairing.expect("active pair exists");
        assert_eq!(active.path, SyncPath::Unsupported);
        assert!(!active.supported);
    }

    #[test]
    fn state_has_no_active_pair_for_missing_or_empty_target() {
        let connections = [SyncConnection {
            id: "a".to_owned(),
            database_type: "mysql".to_owned(),
        }];
        let state = compute_sync_pairing_state(
            Some("postgresql"),
            &connections,
            "",
            false,
            |_| true,
            |_| SyncPairingResult {
                path: SyncPath::Direct,
                supported: true,
                family: None,
                reason: None,
            },
        );
        assert_eq!(state.active_pairing, None);
        let state = compute_sync_pairing_state(
            Some("postgresql"),
            &connections,
            "missing",
            false,
            |_| true,
            |_| SyncPairingResult {
                path: SyncPath::Direct,
                supported: true,
                family: None,
                reason: None,
            },
        );
        assert_eq!(state.active_pairing, None);
    }
}
