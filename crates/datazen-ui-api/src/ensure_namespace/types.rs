use std::collections::{BTreeMap, BTreeSet};

use crate::connection_types::{TableInfo, TableType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnsureStrategy {
    PathHierarchy,
    Postgresql,
    DefaultSql,
}

impl EnsureStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PathHierarchy => "path-hierarchy",
            Self::Postgresql => "postgresql",
            Self::DefaultSql => "default-sql",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeKind {
    Branch,
    Tables,
}

impl MergeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Tables => "tables",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeCall {
    pub segments: Vec<String>,
    pub kind: MergeKind,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnsurePlan {
    pub merges: Vec<MergeCall>,
    pub cached_path_items: Vec<(String, Vec<TableInfo>)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogResult {
    pub tables: Vec<TableInfo>,
    pub schemas: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnsureDeps {
    pub database_type: Option<String>,
    pub declared_strategy: Option<EnsureStrategy>,
    pub is_multi_database: bool,
    pub loaded_paths: BTreeSet<String>,
    pub path_items: BTreeMap<String, Vec<TableInfo>>,
    pub path_aliases: BTreeMap<String, String>,
    pub tables: Vec<TableInfo>,
    pub databases: Vec<String>,
    pub current_database: Option<String>,
    pub namespace_paths: BTreeSet<String>,
    pub databases_result: Vec<String>,
    pub list_tables_result: BTreeMap<String, Vec<TableInfo>>,
    pub list_catalog_result: BTreeMap<String, CatalogResult>,
}

pub fn resolve_ensure_strategy(
    database_type: Option<&str>,
    declared: Option<EnsureStrategy>,
) -> EnsureStrategy {
    match database_type {
        None => EnsureStrategy::DefaultSql,
        Some(_) => declared.unwrap_or_else(|| {
            if database_type == Some("postgresql") {
                EnsureStrategy::Postgresql
            } else {
                EnsureStrategy::DefaultSql
            }
        }),
    }
}

pub fn is_schema_grouping_schema(schema: Option<&str>) -> bool {
    match schema {
        None => false,
        Some("CATALOG") | Some("SCHEMA") => false,
        Some(_) => true,
    }
}

pub fn path_key(segments: &[&str]) -> String {
    segments.join("/")
}

pub fn namespace_has_child(paths: &BTreeSet<String>, segments: &[&str]) -> bool {
    if segments.is_empty() {
        return true;
    }
    paths.contains(&path_key(segments))
}

pub fn is_path_nav(item: &TableInfo) -> bool {
    matches!(item.schema.as_deref(), Some("CATALOG") | Some("SCHEMA"))
}

pub fn path_nav_segment(item: &TableInfo, root_id: &str) -> String {
    let prefix = format!("{root_id}/");
    let path = item.name.strip_prefix(&prefix).unwrap_or(&item.name);
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    segments.last().copied().unwrap_or(&item.name).to_owned()
}

pub fn table_names(items: &[TableInfo]) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.table_type != TableType::View)
        .map(|item| item.name.clone())
        .collect()
}

pub fn resolve_path_root(name: &str, aliases: &BTreeMap<String, String>) -> String {
    aliases.get(name).cloned().unwrap_or_else(|| name.to_owned())
}

pub fn build_slash_fetch_path(segments: &[&str], root_id: &str) -> Option<String> {
    match segments.len() {
        0 => None,
        1 => Some(root_id.to_owned()),
        _ => {
            let mut parts = vec![root_id.to_owned()];
            parts.extend(segments[1..].iter().map(|segment| (*segment).to_owned()));
            Some(parts.join("/"))
        }
    }
}

pub fn uses_current_database_root(deps: &EnsureDeps) -> bool {
    let Some(current) = deps.current_database.as_deref() else {
        return false;
    };
    if current.trim().is_empty() {
        return false;
    }
    match resolve_ensure_strategy(deps.database_type.as_deref(), deps.declared_strategy) {
        EnsureStrategy::PathHierarchy | EnsureStrategy::DefaultSql => true,
        EnsureStrategy::Postgresql => deps.is_multi_database,
    }
}

pub fn inflight_key(db_session_id: &str, resolved: &[String]) -> String {
    format!("{}|{}", db_session_id, resolved.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type,
            row_count: None,
        }
    }

    #[test]
    fn strategy_literals_and_driver_fallbacks() {
        assert_eq!(EnsureStrategy::PathHierarchy.as_str(), "path-hierarchy");
        assert_eq!(EnsureStrategy::Postgresql.as_str(), "postgresql");
        assert_eq!(EnsureStrategy::DefaultSql.as_str(), "default-sql");
        assert_eq!(resolve_ensure_strategy(None, None), EnsureStrategy::DefaultSql);
        assert_eq!(resolve_ensure_strategy(Some("sqlite"), None), EnsureStrategy::DefaultSql);
        assert_eq!(resolve_ensure_strategy(Some("postgresql"), None), EnsureStrategy::Postgresql);
        assert_eq!(
            resolve_ensure_strategy(Some("postgresql"), Some(EnsureStrategy::PathHierarchy)),
            EnsureStrategy::PathHierarchy
        );
        assert_eq!(
            resolve_ensure_strategy(Some("mysql"), Some(EnsureStrategy::Postgresql)),
            EnsureStrategy::Postgresql
        );
    }

    #[test]
    fn grouping_schemas_exclude_the_two_navigation_sentinels() {
        assert!(!is_schema_grouping_schema(None));
        assert!(!is_schema_grouping_schema(Some("CATALOG")));
        assert!(!is_schema_grouping_schema(Some("SCHEMA")));
        assert!(is_schema_grouping_schema(Some("public")));
        assert!(is_schema_grouping_schema(Some("")));
    }

    #[test]
    fn path_keys_and_child_lookups_are_slash_based() {
        assert_eq!(path_key(&[]), "");
        assert_eq!(path_key(&["a", "b"]), "a/b");
        let mut paths = BTreeSet::new();
        paths.insert("a/b".to_owned());
        assert!(namespace_has_child(&paths, &[]));
        assert!(namespace_has_child(&paths, &["a", "b"]));
        assert!(!namespace_has_child(&paths, &["a"]));
        assert!(!namespace_has_child(&paths, &["b"]));
    }

    #[test]
    fn nav_segment_strips_the_root_prefix_and_takes_the_last_part() {
        let item = info("42/alpha/beta", Some("CATALOG"), TableType::Table);
        assert_eq!(path_nav_segment(&item, "42"), "beta");
        assert_eq!(path_nav_segment(&item, "other"), "beta");
        let plain = info("users", Some("SCHEMA"), TableType::Table);
        assert_eq!(path_nav_segment(&plain, "42"), "users");
        let nested = info("42//x//y/", Some("CATALOG"), TableType::Table);
        assert_eq!(path_nav_segment(&nested, "42"), "y");
    }

    #[test]
    fn views_are_dropped_from_table_names() {
        let items = vec![
            info("users", None, TableType::Table),
            info("v_users", None, TableType::View),
            info("orders", None, TableType::MaterializedView),
        ];
        assert_eq!(table_names(&items), vec!["users", "orders"]);
    }

    #[test]
    fn path_roots_and_fetch_paths_follow_the_slash_rules() {
        let mut aliases = BTreeMap::new();
        aliases.insert("display".to_owned(), "42".to_owned());
        assert_eq!(resolve_path_root("display", &aliases), "42");
        assert_eq!(resolve_path_root("other", &aliases), "other");
        assert_eq!(build_slash_fetch_path(&[], "42"), None);
        assert_eq!(build_slash_fetch_path(&["display"], "42").as_deref(), Some("42"));
        assert_eq!(
            build_slash_fetch_path(&["display", "a", "b"], "42").as_deref(),
            Some("42/a/b")
        );
    }

    #[test]
    fn current_database_root_depends_on_the_strategy() {
        let mut deps = EnsureDeps::default();
        deps.current_database = Some("app".to_owned());
        assert!(uses_current_database_root(&deps));
        deps.database_type = Some("postgresql".to_owned());
        deps.declared_strategy = Some(EnsureStrategy::Postgresql);
        assert!(!uses_current_database_root(&deps));
        deps.is_multi_database = true;
        assert!(uses_current_database_root(&deps));
        deps.declared_strategy = Some(EnsureStrategy::PathHierarchy);
        assert!(uses_current_database_root(&deps));
        deps.declared_strategy = None;
        assert!(uses_current_database_root(&deps));
        deps.current_database = None;
        assert!(!uses_current_database_root(&deps));
    }

    #[test]
    fn the_inflight_key_uses_session_and_dotted_segments() {
        assert_eq!(
            inflight_key("s1", &["a".to_owned(), "b".to_owned()]),
            "s1|a.b"
        );
        assert_eq!(inflight_key("s1", &[]), "s1|");
    }
}
