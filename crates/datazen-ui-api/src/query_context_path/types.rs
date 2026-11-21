use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamespaceNode {
    pub children: BTreeMap<String, NamespaceNode>,
    pub leaf: bool,
}

impl NamespaceNode {
    pub fn from_paths(paths: &[Vec<String>]) -> Self {
        let mut root = Self::default();
        for path in paths {
            let mut node = &mut root;
            for (index, segment) in path.iter().enumerate() {
                node = node.children.entry(segment.clone()).or_default();
                if index + 1 == path.len() {
                    node.leaf = true;
                }
            }
        }
        root
    }

    pub fn at(&self, segments: &[String]) -> Option<&Self> {
        let mut node = self;
        for segment in segments {
            node = node.children.get(segment)?;
        }
        Some(node)
    }
}

pub fn is_leaf(node: &NamespaceNode) -> bool {
    node.leaf
}

pub fn namespace_has_child(tree: &NamespaceNode, segments: &[String]) -> bool {
    if segments.is_empty() {
        return true;
    }
    tree.at(segments).is_some()
}

pub fn namespace_branch_child_names(tree: &NamespaceNode, segments: &[String]) -> Vec<String> {
    let Some(node) = tree.at(segments) else {
        return Vec::new();
    };
    if node.leaf {
        return Vec::new();
    }
    node.children
        .iter()
        .filter(|(_, child)| !child.leaf)
        .map(|(name, _)| name.clone())
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathHierarchySelectorSegment {
    Label { name: String },
    Select { level_index: usize, options: Vec<String>, value: String },
}

pub const PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS: [PathHierarchySelectorSegment; 2] = [
    PathHierarchySelectorSegment::Select { level_index: 0, options: Vec::new(), value: String::new() },
    PathHierarchySelectorSegment::Select { level_index: 1, options: Vec::new(), value: String::new() },
];

pub struct QueryContextResolveOpts<'a> {
    pub databases: &'a [String],
    pub namespace_roots: &'a [String],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|segment| (*segment).to_owned()).collect()
    }

    #[test]
    fn a_built_tree_knows_its_leaf_and_branch_children() {
        let tree = NamespaceNode::from_paths(&[
            path(&["hive", "snap", "orders"]),
            path(&["hive", "ads", "events"]),
            path(&["other"]),
        ]);
        assert_eq!(namespace_branch_child_names(&tree, &[]), vec!["hive"]);
        assert_eq!(namespace_branch_child_names(&tree, &path(&["hive"])), vec!["ads", "snap"]);
        assert_eq!(namespace_branch_child_names(&tree, &path(&["hive", "snap"])), Vec::<String>::new());
        assert!(is_leaf(tree.at(&path(&["other"])).unwrap_or(&tree)));
    }

    #[test]
    fn has_child_is_true_for_the_root_and_known_paths_only() {
        let tree = NamespaceNode::from_paths(&[path(&["hive", "snap", "orders"])]);
        assert!(namespace_has_child(&tree, &[]));
        assert!(namespace_has_child(&tree, &path(&["hive", "snap"])));
        assert!(!namespace_has_child(&tree, &path(&["hive", "missing"])));
    }

    #[test]
    fn the_placeholder_has_two_empty_selects() {
        assert_eq!(
            PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS[0],
            PathHierarchySelectorSegment::Select { level_index: 0, options: Vec::new(), value: String::new() }
        );
        assert_eq!(
            PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS[1],
            PathHierarchySelectorSegment::Select { level_index: 1, options: Vec::new(), value: String::new() }
        );
    }
}
