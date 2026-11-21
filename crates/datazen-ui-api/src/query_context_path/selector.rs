use std::collections::BTreeMap;

use crate::query_context_path::types::{
    namespace_branch_child_names, NamespaceNode, PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS,
    PathHierarchySelectorSegment,
};

pub fn namespace_roots_from(
    namespace_tree: &NamespaceNode,
    path_aliases: &BTreeMap<String, String>,
    databases: &[String],
) -> Vec<String> {
    let mut names = databases.to_vec();
    for alias in path_aliases.keys() {
        if !names.contains(alias) {
            names.push(alias.clone());
        }
    }
    for name in namespace_branch_child_names(namespace_tree, &[]) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

pub fn paths_equal(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(left, right)| left == right)
}

pub fn auto_complete_path_hierarchy_path(
    namespace_tree: &NamespaceNode,
    path_aliases: &BTreeMap<String, String>,
    databases: &[String],
    context_path: &[String],
) -> Option<Vec<String>> {
    let roots = namespace_roots_from(namespace_tree, path_aliases, databases);
    if roots.is_empty() {
        return None;
    }
    let mut extended = context_path.to_vec();
    if extended.is_empty() && roots.len() == 1 {
        extended.push(roots[0].clone());
    }
    loop {
        let children = namespace_branch_child_names(namespace_tree, &extended);
        if children.len() != 1 {
            break;
        }
        extended.push(children[0].clone());
    }
    if paths_equal(&extended, context_path) {
        return None;
    }
    Some(extended)
}

pub fn build_path_hierarchy_selector_segments(
    namespace_tree: &NamespaceNode,
    path_aliases: &BTreeMap<String, String>,
    databases: &[String],
    context_path: &[String],
) -> Vec<PathHierarchySelectorSegment> {
    let roots = namespace_roots_from(namespace_tree, path_aliases, databases);
    if roots.is_empty() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut options = roots;
    while !options.is_empty() {
        let depth = path.len();
        if options.len() == 1 {
            let only = options[0].clone();
            segments.push(PathHierarchySelectorSegment::Label { name: only.clone() });
            path.push(only);
        } else {
            let value = context_path.get(depth).cloned().unwrap_or_default();
            segments.push(PathHierarchySelectorSegment::Select {
                level_index: depth,
                options: options.clone(),
                value: value.clone(),
            });
            if value.is_empty() {
                break;
            }
            path.push(value);
        }
        let children = namespace_branch_child_names(namespace_tree, &path);
        if children.is_empty() {
            let next_index = path.len();
            let already_has_next = segments.iter().any(|segment| {
                matches!(segment, PathHierarchySelectorSegment::Select { level_index, .. } if *level_index == next_index)
            });
            let show_loading = !already_has_next
                && !path.is_empty()
                && path.len() < 2
                && next_index <= context_path.len();
            if show_loading {
                segments.push(PathHierarchySelectorSegment::Select {
                    level_index: next_index,
                    options: Vec::new(),
                    value: context_path.get(next_index).cloned().unwrap_or_default(),
                });
            }
            break;
        }
        options = children;
    }
    segments
}

pub fn path_hierarchy_selector_segments_for_ui(
    namespace_tree: &NamespaceNode,
    path_aliases: &BTreeMap<String, String>,
    databases: &[String],
    context_path: &[String],
) -> Vec<PathHierarchySelectorSegment> {
    let segments = build_path_hierarchy_selector_segments(
        namespace_tree,
        path_aliases,
        databases,
        context_path,
    );
    if segments.is_empty() {
        PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS.to_vec()
    } else {
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|segment| (*segment).to_owned()).collect()
    }

    #[test]
    fn roots_keep_database_alias_and_tree_order() {
        let tree = NamespaceNode::from_paths(&[path(&["tree-root", "x"])]);
        let mut aliases = BTreeMap::new();
        aliases.insert("alias".to_owned(), "1".to_owned());
        aliases.insert("db".to_owned(), "2".to_owned());
        assert_eq!(
            namespace_roots_from(&tree, &aliases, &path(&["db"])),
            vec!["db", "alias", "tree-root"]
        );
    }

    #[test]
    fn paths_equal_is_exact() {
        assert!(paths_equal(&path(&["a", "b"]), &path(&["a", "b"])));
        assert!(!paths_equal(&path(&["a", "b"]), &path(&["b", "a"])));
        assert!(!paths_equal(&path(&["a"]), &path(&["a", "b"])));
    }

    #[test]
    fn auto_complete_fills_single_option_levels() {
        let tree = NamespaceNode::from_paths(&[path(&["hive", "snap", "orders"])]);
        assert_eq!(
            auto_complete_path_hierarchy_path(&tree, &BTreeMap::new(), &[], &[]),
            Some(path(&["hive", "snap"]))
        );
        assert_eq!(
            auto_complete_path_hierarchy_path(&tree, &BTreeMap::new(), &[], &path(&["hive"])),
            Some(path(&["hive", "snap"]))
        );
    }

    #[test]
    fn auto_complete_leaves_multi_option_levels_alone() {
        let tree = NamespaceNode::from_paths(&[
            path(&["hive", "snap", "a"]),
            path(&["hive", "ads", "b"]),
        ]);
        assert_eq!(
            auto_complete_path_hierarchy_path(&tree, &BTreeMap::new(), &[], &[]),
            Some(path(&["hive"]))
        );
        assert_eq!(
            auto_complete_path_hierarchy_path(&tree, &BTreeMap::new(), &[], &path(&["hive"])),
            None
        );
        assert_eq!(auto_complete_path_hierarchy_path(&NamespaceNode::default(), &BTreeMap::new(), &[], &[]), None);
    }

    #[test]
    fn selector_segments_use_labels_for_single_options() {
        let tree = NamespaceNode::from_paths(&[path(&["hive", "snap", "orders"])]);
        let segments = build_path_hierarchy_selector_segments(
            &tree, &BTreeMap::new(), &[], &path(&["hive", "snap"]),
        );
        assert_eq!(
            segments,
            vec![
                PathHierarchySelectorSegment::Label { name: "hive".to_owned() },
                PathHierarchySelectorSegment::Label { name: "snap".to_owned() },
            ]
        );
    }

    #[test]
    fn selector_segments_keep_multi_options_as_selects() {
        let tree = NamespaceNode::from_paths(&[
            path(&["hive", "snap", "a"]),
            path(&["hive", "ads", "b"]),
        ]);
        let segments = build_path_hierarchy_selector_segments(
            &tree, &BTreeMap::new(), &[], &path(&["hive", "snap"]),
        );
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], PathHierarchySelectorSegment::Label { name: "hive".to_owned() });
        match &segments[1] {
            PathHierarchySelectorSegment::Select { level_index, options, value } => {
                assert_eq!(*level_index, 1);
                assert_eq!(options, &path(&["ads", "snap"]));
                assert_eq!(value, "snap");
            }
            _ => panic!("expected a select"),
        }
    }

    #[test]
    fn ui_segments_fall_back_to_the_two_select_placeholder() {
        let segments = path_hierarchy_selector_segments_for_ui(
            &NamespaceNode::default(), &BTreeMap::new(), &[], &[],
        );
        assert_eq!(segments, PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS.to_vec());
    }
}
