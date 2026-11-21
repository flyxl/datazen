use std::collections::{BTreeMap, BTreeSet};

use crate::query_context_path::types::{namespace_has_child, NamespaceNode};

pub fn build_path_hierarchy_database_pin(root_database: &str, namespace_path: &[String]) -> String {
    let root = root_database.trim();
    if root.is_empty() {
        return namespace_path.join("/");
    }
    if namespace_path.is_empty() {
        return root.to_owned();
    }
    let mut parts = vec![root.to_owned()];
    parts.extend(namespace_path.iter().cloned());
    parts.join("/")
}

pub struct SplitDatabasePin {
    pub root: String,
    pub namespace_path: Vec<String>,
}

pub fn split_path_hierarchy_database_pin(pin: &str) -> SplitDatabasePin {
    let trimmed = pin.trim();
    if trimmed.is_empty() {
        return SplitDatabasePin { root: String::new(), namespace_path: Vec::new() };
    }
    let Some(slash) = trimmed.find('/') else {
        return SplitDatabasePin { root: trimmed.to_owned(), namespace_path: Vec::new() };
    };
    SplitDatabasePin {
        root: trimmed[..slash].to_owned(),
        namespace_path: trimmed[slash + 1..]
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

pub fn path_hierarchy_relative_namespace_path(
    databases: &[String],
    namespace_tree: &NamespaceNode,
    namespace_path: &[String],
) -> Vec<String> {
    let Some(first) = namespace_path.first() else {
        return namespace_path.to_vec();
    };
    if !databases.iter().any(|database| database == first) {
        return namespace_path.to_vec();
    }
    let rooted = namespace_has_child(namespace_tree, namespace_path);
    let repeats = namespace_path.get(1) == Some(first);
    if rooted || repeats {
        namespace_path[1..].to_vec()
    } else {
        namespace_path.to_vec()
    }
}

fn known_database_for(databases: &[String], root: &str) -> Option<String> {
    let trimmed = root.trim();
    if trimmed.is_empty() {
        return None;
    }
    databases
        .iter()
        .find(|database| {
            database.as_str() == trimmed
                || database.split(':').next().unwrap_or_default() == trimmed
        })
        .cloned()
}

pub fn path_hierarchy_connection_root(
    databases: &[String],
    panel_database: Option<&str>,
    current_database: Option<&str>,
    path_aliases: &BTreeMap<String, String>,
    namespace_roots: &[String],
) -> Option<String> {
    let namespace_root_set: BTreeSet<&str> = namespace_roots.iter().map(String::as_str).collect();
    let resolve_candidate = |value: Option<&str>| -> Option<String> {
        let trimmed = value.map(str::trim).filter(|text| !text.is_empty())?;
        let SplitDatabasePin { root, .. } = split_path_hierarchy_database_pin(trimmed);
        let alias = path_aliases.get(&root).map(|value| value.trim()).filter(|text| !text.is_empty());
        if namespace_root_set.contains(root.as_str()) || path_aliases.contains_key(&root) {
            if let Some(alias) = alias {
                return known_database_for(databases, alias).or_else(|| Some(alias.to_owned()));
            }
            return known_database_for(databases, &root);
        }
        if let Some(known) = known_database_for(databases, &root) {
            return Some(known);
        }
        if !root.is_empty() && root.bytes().all(|byte| byte.is_ascii_digit()) {
            return Some(root);
        }
        if databases.is_empty() && root.contains(':') {
            return Some(root);
        }
        None
    };

    resolve_candidate(panel_database)
        .or_else(|| resolve_candidate(current_database))
        .or_else(|| databases.iter().find_map(|database| resolve_candidate(Some(database))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn pin_building_and_splitting_round_trip() {
        let path = names(&["558", "hive", "snap"]);
        let pin = build_path_hierarchy_database_pin("shop", &path);
        assert_eq!(pin, "shop/558/hive/snap");
        let split = split_path_hierarchy_database_pin(&pin);
        assert_eq!(split.root, "shop");
        assert_eq!(split.namespace_path, path);
    }

    #[test]
    fn pin_building_handles_empty_roots_and_paths() {
        assert_eq!(build_path_hierarchy_database_pin("  ", &names(&["a", "b"])), "a/b");
        assert_eq!(build_path_hierarchy_database_pin(" shop ", &[]), "shop");
        assert_eq!(build_path_hierarchy_database_pin("  ", &[]), "");
    }

    #[test]
    fn splitting_trims_and_drops_empty_segments() {
        let split = split_path_hierarchy_database_pin("  shop//hive/  ");
        assert_eq!(split.root, "shop");
        assert_eq!(split.namespace_path, vec!["hive"]);
        let bare = split_path_hierarchy_database_pin("  shop  ");
        assert_eq!(bare.root, "shop");
        assert!(bare.namespace_path.is_empty());
        let empty = split_path_hierarchy_database_pin("  ");
        assert_eq!(empty.root, "");
        assert!(empty.namespace_path.is_empty());
    }

    #[test]
    fn relative_path_drops_only_the_database_segment_when_rooted() {
        let databases = names(&["hive"]);
        let tree = NamespaceNode::from_paths(&[names(&["hive", "snap", "orders"])]);
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree, &names(&["hive", "snap"])),
            vec!["snap"]
        );
    }

    #[test]
    fn a_repeated_database_catalog_drops_the_first_segment() {
        let databases = names(&["hive"]);
        let tree = NamespaceNode::default();
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree, &names(&["hive", "hive", "snap"])),
            vec!["hive", "snap"]
        );
    }

    #[test]
    fn an_unrooted_path_stays_untouched() {
        let databases = names(&["hive"]);
        let tree = NamespaceNode::from_paths(&[names(&["other", "snap"])]);
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree, &names(&["other", "snap"])),
            vec!["other", "snap"]
        );
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree, &[]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn connection_root_prefers_panel_then_current_then_databases() {
        let databases = names(&["shop:prod", "other"]);
        assert_eq!(
            path_hierarchy_connection_root(&databases, Some("shop"), Some("other"), &BTreeMap::new(), &[]),
            Some("shop:prod".to_owned())
        );
        assert_eq!(
            path_hierarchy_connection_root(&databases, None, Some("other"), &BTreeMap::new(), &[]),
            Some("other".to_owned())
        );
        assert_eq!(
            path_hierarchy_connection_root(&databases, None, None, &BTreeMap::new(), &[]),
            Some("shop:prod".to_owned())
        );
    }

    #[test]
    fn namespace_aliases_resolve_to_known_databases_or_the_alias() {
        let databases = names(&["558:prod"]);
        let mut aliases = BTreeMap::new();
        aliases.insert("hive".to_owned(), "558".to_owned());
        assert_eq!(
            path_hierarchy_connection_root(&databases, Some("hive/snap"), None, &aliases, &[]),
            Some("558:prod".to_owned())
        );
        let empty_aliases: BTreeMap<String, String> = BTreeMap::new();
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("hive/snap"), None, &empty_aliases, &names(&["hive"])),
            None
        );
    }

    #[test]
    fn numeric_and_id_name_roots_are_accepted_in_boundary_cases() {
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("558/hive"), None, &BTreeMap::new(), &[]),
            Some("558".to_owned())
        );
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("558:dev/hive"), None, &BTreeMap::new(), &[]),
            Some("558:dev".to_owned())
        );
        let databases = names(&["known"]);
        assert_eq!(
            path_hierarchy_connection_root(&databases, Some("999"), None, &BTreeMap::new(), &[]),
            Some("999".to_owned())
        );
        assert_eq!(
            path_hierarchy_connection_root(&databases, Some("mystery"), None, &BTreeMap::new(), &[]),
            Some("known".to_owned())
        );
    }
}
