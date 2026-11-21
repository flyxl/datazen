mod plans;
mod types;

use crate::sql_path_prefix::{ResolveEnsureOptions, resolve_ensure_segments};
pub use plans::{plan_default_sql, plan_path_hierarchy, plan_postgresql};
pub use types::{
    CatalogResult, EnsureDeps, EnsurePlan, EnsureStrategy, MergeCall, MergeKind,
    build_slash_fetch_path, inflight_key, is_path_nav, is_schema_grouping_schema,
    namespace_has_child, path_key, path_nav_segment, resolve_ensure_strategy, resolve_path_root,
    table_names, uses_current_database_root,
};

pub fn resolve_for_ensure(segments: &[&str], deps: &EnsureDeps) -> Vec<String> {
    let current = deps.current_database.as_deref().filter(|value| !value.trim().is_empty());
    let mut roots: Vec<&str> = deps.databases.iter().map(String::as_str).collect();
    roots.extend(deps.path_aliases.keys().map(String::as_str));
    let options = ResolveEnsureOptions {
        current_database: current,
        known_roots: &roots,
        use_current_database_root: uses_current_database_root(deps),
    };
    resolve_ensure_segments(segments, &options)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub fn namespace_ensure_pending(segments: &[&str], deps: &EnsureDeps) -> bool {
    let resolved = resolve_for_ensure(segments, deps);
    let refs: Vec<&str> = resolved.iter().map(String::as_str).collect();
    !deps.loaded_paths.contains(&path_key(&refs))
}

pub fn namespace_path_loaded(deps: &EnsureDeps, segments: &[&str]) -> bool {
    deps.loaded_paths.contains(&path_key(segments)) || namespace_has_child(&deps.namespace_paths, segments)
}

pub fn run_ensure_plan(segments: &[&str], deps: &EnsureDeps) -> Option<EnsurePlan> {
    if deps.loaded_paths.contains(&path_key(segments)) {
        return None;
    }
    Some(match resolve_ensure_strategy(deps.database_type.as_deref(), deps.declared_strategy) {
        EnsureStrategy::PathHierarchy => plan_path_hierarchy(segments, deps),
        EnsureStrategy::Postgresql => plan_postgresql(segments, deps),
        EnsureStrategy::DefaultSql => plan_default_sql(segments, deps),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uses_the_current_database_for_unqualified_paths() {
        let mut deps = EnsureDeps::default();
        deps.current_database = Some("app".to_owned());
        deps.databases = vec!["app".to_owned()];
        assert_eq!(resolve_for_ensure(&[], &deps), vec!["app"]);
        assert_eq!(resolve_for_ensure(&["app", "users"], &deps), vec!["app", "users"]);
        assert_eq!(resolve_for_ensure(&["other"], &deps), vec!["app", "other"]);
    }

    #[test]
    fn path_aliases_join_the_known_root_set() {
        let mut deps = EnsureDeps::default();
        deps.path_aliases.insert("display".to_owned(), "42".to_owned());
        deps.declared_strategy = Some(EnsureStrategy::PathHierarchy);
        assert_eq!(resolve_for_ensure(&["display", "a"], &deps), vec!["display", "a"]);
    }

    #[test]
    fn pending_uses_the_resolved_path_not_the_raw_one() {
        let mut deps = EnsureDeps::default();
        deps.current_database = Some("app".to_owned());
        deps.databases = vec!["app".to_owned()];
        deps.loaded_paths.insert("app".to_owned());
        assert!(!namespace_ensure_pending(&[], &deps));
        assert!(namespace_ensure_pending(&["users"], &deps));
    }

    #[test]
    fn loaded_checks_either_the_loaded_set_or_the_namespace_tree() {
        let mut deps = EnsureDeps::default();
        assert!(namespace_path_loaded(&deps, &[]));
        deps.loaded_paths.insert("a/b".to_owned());
        assert!(namespace_path_loaded(&deps, &["a", "b"]));
        deps.namespace_paths.insert("c/d".to_owned());
        assert!(namespace_path_loaded(&deps, &["c", "d"]));
        assert!(!namespace_path_loaded(&deps, &["x"]));
    }

    #[test]
    fn a_loaded_path_produces_no_plan() {
        let mut deps = EnsureDeps::default();
        deps.loaded_paths.insert("db".to_owned());
        assert!(run_ensure_plan(&["db"], &deps).is_none());
    }

    #[test]
    fn strategy_selection_drives_the_plan() {
        let mut deps = EnsureDeps::default();
        deps.databases_result = vec!["db".to_owned()];
        let default_plan = run_ensure_plan(&[], &deps);
        assert_eq!(default_plan.map(|plan| plan.merges.len()), Some(1));
        deps.database_type = Some("sqlite".to_owned());
        deps.declared_strategy = Some(EnsureStrategy::Postgresql);
        deps.is_multi_database = true;
        let pg_plan = run_ensure_plan(&[], &deps);
        assert_eq!(pg_plan.map(|plan| plan.merges.len()), Some(1));
        deps.declared_strategy = Some(EnsureStrategy::PathHierarchy);
        let path_plan = run_ensure_plan(&[], &deps);
        assert_eq!(path_plan.map(|plan| plan.merges.len()), Some(0));
    }

    #[test]
    fn the_request_key_matches_session_and_resolved_segments() {
        assert_eq!(inflight_key("s1", &["a".to_owned()]), "s1|a");
    }
}
