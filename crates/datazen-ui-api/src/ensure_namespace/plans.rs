use std::collections::BTreeMap;

use crate::connection_types::{TableInfo, TableType};
use crate::ensure_namespace::types::{
    build_slash_fetch_path, is_path_nav, is_schema_grouping_schema, path_nav_segment,
    resolve_path_root, table_names, EnsureDeps, EnsurePlan, MergeCall, MergeKind,
};

fn merge(segments: &[&str], kind: MergeKind, names: Vec<String>) -> MergeCall {
    MergeCall {
        segments: segments.iter().map(|segment| (*segment).to_owned()).collect(),
        kind,
        names,
    }
}

fn preferred_database(deps: &EnsureDeps) -> Option<&str> {
    match deps.current_database.as_deref() {
        Some(current) if deps.databases.iter().any(|database| database == current) => Some(current),
        _ => deps.databases.first().map(String::as_str),
    }
}

fn schema_groups(items: &[TableInfo], schemas: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for schema in schemas {
        if is_schema_grouping_schema(Some(schema)) {
            groups.entry(schema.clone()).or_default();
        }
    }
    for item in items {
        let Some(schema) = item.schema.as_deref() else {
            continue;
        };
        if !is_schema_grouping_schema(Some(schema)) {
            continue;
        }
        groups.entry(schema.to_owned()).or_default().push(item.name.clone());
    }
    groups
}

pub fn plan_path_hierarchy(segments: &[&str], deps: &EnsureDeps) -> EnsurePlan {
    let mut plan = EnsurePlan::default();
    if segments.is_empty() {
        let names: Vec<String> = deps.path_aliases.keys().cloned().collect();
        if !names.is_empty() {
            plan.merges.push(merge(&[], MergeKind::Branch, names));
        }
        return plan;
    }
    let root_id = resolve_path_root(segments[0], &deps.path_aliases);
    let Some(fetch_path) = build_slash_fetch_path(segments, &root_id) else {
        return plan;
    };
    let items = match deps.path_items.get(&fetch_path) {
        Some(items) => items.clone(),
        None => {
            let fetched = deps.list_tables_result.get(&fetch_path).cloned().unwrap_or_default();
            plan.cached_path_items.push((fetch_path.clone(), fetched.clone()));
            fetched
        }
    };
    if segments.len() == 1 {
        let children: Vec<String> = items
            .iter()
            .filter(|item| is_path_nav(item))
            .map(|item| path_nav_segment(item, &root_id))
            .collect();
        plan.merges.push(merge(segments, MergeKind::Branch, children));
        return plan;
    }
    if items.iter().any(is_path_nav) && segments.len() == 2 {
        let children: Vec<String> = items
            .iter()
            .filter(|item| is_path_nav(item))
            .map(|item| path_nav_segment(item, &root_id))
            .collect();
        plan.merges.push(merge(segments, MergeKind::Branch, children));
        return plan;
    }
    let names = table_names(&items.iter().filter(|item| !is_path_nav(item)).cloned().collect::<Vec<_>>());
    plan.merges.push(merge(segments, MergeKind::Tables, names));
    plan
}

pub fn plan_postgresql(segments: &[&str], deps: &EnsureDeps) -> EnsurePlan {
    let mut plan = EnsurePlan::default();
    if segments.is_empty() {
        if deps.is_multi_database {
            plan.merges.push(merge(&[], MergeKind::Branch, deps.databases_result.clone()));
            return plan;
        }
        let Some(preferred) = preferred_database(deps) else {
            return plan;
        };
        let catalog = deps.list_catalog_result.get(preferred);
        let all = match catalog {
            Some(catalog) => catalog.tables.clone(),
            None => deps.list_tables_result.get(preferred).cloned().unwrap_or_default(),
        };
        let schemas = catalog.map(|catalog| catalog.schemas.as_slice()).unwrap_or(&[]);
        let groups = schema_groups(&all, schemas);
        plan.merges.push(merge(&[], MergeKind::Branch, groups.keys().cloned().collect()));
        for (schema, names) in groups {
            plan.merges.push(merge(&[schema.as_str()], MergeKind::Tables, names));
        }
        return plan;
    }
    if deps.is_multi_database && segments.len() == 1 {
        let database = segments[0];
        let catalog = deps.list_catalog_result.get(database);
        let all: Vec<TableInfo> = match catalog {
            Some(catalog) => catalog.tables.clone(),
            None => deps.list_tables_result.get(database).cloned().unwrap_or_default(),
        };
        let all: Vec<TableInfo> = all.into_iter().filter(|item| item.table_type != TableType::View).collect();
        let schemas = catalog.map(|catalog| catalog.schemas.as_slice()).unwrap_or(&[]);
        let groups = schema_groups(&all, schemas);
        plan.merges.push(merge(&[database], MergeKind::Branch, groups.keys().cloned().collect()));
        for (schema, names) in groups {
            plan.merges.push(merge(&[database, schema.as_str()], MergeKind::Tables, names));
        }
        return plan;
    }
    if !deps.is_multi_database && segments.len() == 1 {
        let schema = segments[0];
        let from_memory: Vec<TableInfo> = deps
            .tables
            .iter()
            .filter(|item| item.table_type != TableType::View && item.schema.as_deref() == Some(schema))
            .cloned()
            .collect();
        if !from_memory.is_empty() {
            plan.merges.push(merge(&[schema], MergeKind::Tables, table_names(&from_memory)));
            return plan;
        }
        let Some(preferred) = preferred_database(deps) else {
            return plan;
        };
        let all = deps.list_tables_result.get(preferred).cloned().unwrap_or_default();
        let names = table_names(
            &all.into_iter()
                .filter(|item| item.table_type != TableType::View && item.schema.as_deref() == Some(schema))
                .collect::<Vec<_>>(),
        );
        plan.merges.push(merge(&[schema], MergeKind::Tables, names));
    }
    plan
}

pub fn plan_default_sql(segments: &[&str], deps: &EnsureDeps) -> EnsurePlan {
    let mut plan = EnsurePlan::default();
    if segments.is_empty() {
        plan.merges.push(merge(&[], MergeKind::Branch, deps.databases_result.clone()));
        return plan;
    }
    if segments.len() == 1 {
        let database = segments[0];
        let all = deps.list_tables_result.get(database).cloned().unwrap_or_default();
        plan.merges.push(merge(&[database], MergeKind::Tables, table_names(&all)));
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ensure_namespace::types::{is_path_nav, path_key, EnsureStrategy};

    fn info(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type,
            row_count: None,
        }
    }

    #[test]
    fn path_hierarchy_with_no_segments_merges_alias_names() {
        let mut deps = EnsureDeps::default();
        deps.path_aliases.insert("a".to_owned(), "1".to_owned());
        deps.path_aliases.insert("b".to_owned(), "2".to_owned());
        let plan = plan_path_hierarchy(&[], &deps);
        assert_eq!(plan.merges.len(), 1);
        assert_eq!(plan.merges[0].segments, Vec::<String>::new());
        assert_eq!(plan.merges[0].kind, MergeKind::Branch);
        assert_eq!(plan.merges[0].names, vec!["a", "b"]);
    }

    #[test]
    fn path_hierarchy_uses_aliases_and_caches_fetched_items() {
        let mut deps = EnsureDeps::default();
        deps.path_aliases.insert("display".to_owned(), "42".to_owned());
        deps.list_tables_result.insert(
            "42/app".to_owned(),
            vec![
                info("42/t1", None, TableType::Table),
                info("42/v1", None, TableType::View),
            ],
        );
        let plan = plan_path_hierarchy(&["display", "app"], &deps);
        assert_eq!(plan.merges.len(), 1);
        assert_eq!(plan.merges[0].segments, vec!["display", "app"]);
        assert_eq!(plan.merges[0].kind, MergeKind::Tables);
        assert_eq!(plan.merges[0].names, vec!["42/t1"]);
        assert_eq!(plan.cached_path_items.len(), 1);
        assert_eq!(plan.cached_path_items[0].0, "42/app");
    }

    #[test]
    fn cached_path_items_are_not_fetched_again() {
        let mut deps = EnsureDeps::default();
        deps.path_items.insert(
            "42/app".to_owned(),
            vec![info("42/t1", None, TableType::Table)],
        );
        let plan = plan_path_hierarchy(&["42", "app"], &deps);
        assert!(plan.cached_path_items.is_empty());
        assert_eq!(plan.merges[0].names, vec!["42/t1"]);
    }

    #[test]
    fn path_nav_items_become_branch_children_at_depth_one_and_two() {
        let mut deps = EnsureDeps::default();
        deps.path_items.insert(
            "42".to_owned(),
            vec![
                info("42/alpha", Some("CATALOG"), TableType::Table),
                info("42/beta", Some("SCHEMA"), TableType::Table),
            ],
        );
        let one = plan_path_hierarchy(&["42"], &deps);
        assert_eq!(one.merges[0].kind, MergeKind::Branch);
        assert_eq!(one.merges[0].names, vec!["alpha", "beta"]);
        deps.path_items.insert(
            "42/alpha".to_owned(),
            vec![info("42/t1", Some("CATALOG"), TableType::Table)],
        );
        let two = plan_path_hierarchy(&["42", "alpha"], &deps);
        assert_eq!(two.merges[0].kind, MergeKind::Branch);
        assert_eq!(two.merges[0].names, vec!["t1"]);
    }

    #[test]
    fn path_nav_items_are_removed_from_deeper_table_lists() {
        let mut deps = EnsureDeps::default();
        deps.path_items.insert(
            "42/a/b".to_owned(),
            vec![
                info("42/nav", Some("CATALOG"), TableType::Table),
                info("42/t1", None, TableType::Table),
            ],
        );
        let plan = plan_path_hierarchy(&["42", "a", "b"], &deps);
        assert_eq!(plan.merges[0].kind, MergeKind::Tables);
        assert_eq!(plan.merges[0].names, vec!["42/t1"]);
    }

    #[test]
    fn postgresql_multi_database_lists_databases_at_the_root() {
        let mut deps = EnsureDeps::default();
        deps.is_multi_database = true;
        deps.databases_result = vec!["shop".to_owned(), "analytics".to_owned()];
        let plan = plan_postgresql(&[], &deps);
        assert_eq!(plan.merges.len(), 1);
        assert_eq!(plan.merges[0].kind, MergeKind::Branch);
        assert_eq!(plan.merges[0].names, vec!["shop", "analytics"]);
    }

    #[test]
    fn postgresql_single_database_groups_schemas_from_the_catalog() {
        let mut deps = EnsureDeps::default();
        deps.databases = vec!["shop".to_owned()];
        deps.list_catalog_result.insert(
            "shop".to_owned(),
            crate::ensure_namespace::types::CatalogResult {
                schemas: vec!["public".to_owned(), "CATALOG".to_owned(), "private".to_owned()],
                tables: vec![
                    info("users", Some("public"), TableType::Table),
                    info("v", Some("public"), TableType::View),
                    info("secret", Some("private"), TableType::Table),
                    info("nav", Some("SCHEMA"), TableType::Table),
                ],
            },
        );
        let plan = plan_postgresql(&[], &deps);
        assert_eq!(plan.merges.len(), 3);
        assert_eq!(plan.merges[0].names, vec!["private", "public"]);
        assert_eq!(plan.merges[1].segments, vec!["private"]);
        assert_eq!(plan.merges[1].names, vec!["secret"]);
        assert_eq!(plan.merges[2].segments, vec!["public"]);
        assert_eq!(plan.merges[2].names, vec!["users", "v"]);
    }

    #[test]
    fn postgresql_single_schema_prefers_memory_tables_then_lists_tables() {
        let mut deps = EnsureDeps::default();
        deps.databases = vec!["shop".to_owned()];
        deps.tables = vec![info("users", Some("public"), TableType::Table)];
        let plan = plan_postgresql(&["public"], &deps);
        assert_eq!(plan.merges.len(), 1);
        assert_eq!(plan.merges[0].names, vec!["users"]);
        deps.tables.clear();
        deps.list_tables_result.insert(
            "shop".to_owned(),
            vec![info("orders", Some("public"), TableType::Table)],
        );
        let plan = plan_postgresql(&["public"], &deps);
        assert_eq!(plan.merges[0].names, vec!["orders"]);
    }

    #[test]
    fn postgresql_multi_database_groups_under_the_database() {
        let mut deps = EnsureDeps::default();
        deps.is_multi_database = true;
        deps.list_tables_result.insert(
            "shop".to_owned(),
            vec![
                info("users", Some("public"), TableType::Table),
                info("archived", Some("archive"), TableType::Table),
                info("v", Some("public"), TableType::View),
            ],
        );
        let plan = plan_postgresql(&["shop"], &deps);
        assert_eq!(plan.merges.len(), 3);
        assert_eq!(plan.merges[0].segments, vec!["shop"]);
        assert_eq!(plan.merges[0].names, vec!["archive", "public"]);
        assert_eq!(plan.merges[2].segments, vec!["shop", "public"]);
        assert_eq!(plan.merges[2].names, vec!["users"]);
    }

    #[test]
    fn a_missing_preferred_database_produces_no_merges() {
        let mut deps = EnsureDeps::default();
        deps.declared_strategy = Some(EnsureStrategy::Postgresql);
        assert!(plan_postgresql(&[], &deps).merges.is_empty());
        assert!(plan_postgresql(&["public"], &deps).merges.is_empty());
    }

    #[test]
    fn current_database_wins_when_it_is_known() {
        let mut deps = EnsureDeps::default();
        deps.databases = vec!["shop".to_owned(), "analytics".to_owned()];
        deps.current_database = Some("analytics".to_owned());
        deps.list_tables_result.insert(
            "analytics".to_owned(),
            vec![info("events", Some("public"), TableType::Table)],
        );
        let plan = plan_postgresql(&["public"], &deps);
        assert_eq!(plan.merges[0].names, vec!["events"]);
    }

    #[test]
    fn default_sql_lists_databases_then_tables() {
        let mut deps = EnsureDeps::default();
        deps.databases_result = vec!["db1".to_owned()];
        let root = plan_default_sql(&[], &deps);
        assert_eq!(root.merges[0].kind, MergeKind::Branch);
        assert_eq!(root.merges[0].names, vec!["db1"]);
        deps.list_tables_result.insert(
            "db1".to_owned(),
            vec![info("t", None, TableType::Table), info("v", None, TableType::View)],
        );
        let tables = plan_default_sql(&["db1"], &deps);
        assert_eq!(tables.merges[0].kind, MergeKind::Tables);
        assert_eq!(tables.merges[0].names, vec!["t"]);
    }

    #[test]
    fn a_path_nav_predicate_matches_only_the_two_sentinels() {
        assert!(is_path_nav(&info("a", Some("CATALOG"), TableType::Table)));
        assert!(is_path_nav(&info("a", Some("SCHEMA"), TableType::Table)));
        assert!(!is_path_nav(&info("a", Some("public"), TableType::Table)));
        assert_eq!(path_key(&["a"]), "a");
    }
}
