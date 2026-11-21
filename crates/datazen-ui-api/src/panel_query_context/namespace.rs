use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SqlNamespace {
    #[default]
    Leaf,
    Branch(BTreeMap<String, SqlNamespace>),
}

impl SqlNamespace {
    pub fn branch(entries: &[(&str, SqlNamespace)]) -> Self {
        let mut map = BTreeMap::new();
        for (name, child) in entries {
            map.insert((*name).to_owned(), child.clone());
        }
        Self::Branch(map)
    }

    pub fn leaf() -> Self {
        Self::Leaf
    }

    fn children(&self) -> Option<&BTreeMap<String, SqlNamespace>> {
        match self {
            Self::Branch(map) => Some(map),
            Self::Leaf => None,
        }
    }
}

pub fn namespace_has_child(tree: &SqlNamespace, segments: &[String]) -> bool {
    if segments.is_empty() {
        return true;
    }
    let mut node = tree;
    for segment in segments {
        let children = match node.children() {
            Some(children) => children,
            None => return false,
        };
        node = match children.get(segment) {
            Some(node) => node,
            None => return false,
        };
    }
    true
}

pub fn namespace_child_names(tree: &SqlNamespace, segments: &[String]) -> Vec<String> {
    let mut node = tree;
    for segment in segments {
        let children = match node.children() {
            Some(children) => children,
            None => return Vec::new(),
        };
        node = match children.get(segment) {
            Some(node) => node,
            None => return Vec::new(),
        };
    }
    match node.children() {
        Some(children) => children.keys().cloned().collect(),
        None => Vec::new(),
    }
}

pub fn namespace_branch_child_names(tree: &SqlNamespace, segments: &[String]) -> Vec<String> {
    let mut node = tree;
    for segment in segments {
        let children = match node.children() {
            Some(children) => children,
            None => return Vec::new(),
        };
        node = match children.get(segment) {
            Some(node) => node,
            None => return Vec::new(),
        };
    }
    match node.children() {
        Some(children) => children
            .iter()
            .filter(|(_, child)| child.children().is_some())
            .map(|(name, _)| name.clone())
            .collect(),
        None => Vec::new(),
    }
}

pub fn namespace_roots_from(
    tree: &SqlNamespace,
    path_aliases: &BTreeMap<String, String>,
    databases: &[String],
) -> Vec<String> {
    let mut names = Vec::new();
    for database in databases {
        if !names.contains(database) {
            names.push(database.clone());
        }
    }
    for alias in path_aliases.keys() {
        if !names.contains(alias) {
            names.push(alias.clone());
        }
    }
    for name in namespace_branch_child_names(tree, &[]) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

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

pub fn split_path_hierarchy_database_pin(pin: &str) -> (String, Vec<String>) {
    let trimmed = pin.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new());
    }
    match trimmed.find('/') {
        None => (trimmed.to_owned(), Vec::new()),
        Some(index) => {
            let namespace_path = trimmed[index + 1..]
                .split('/')
                .filter(|segment| !segment.is_empty())
                .map(str::to_owned)
                .collect();
            (trimmed[..index].to_owned(), namespace_path)
        }
    }
}

pub fn path_hierarchy_relative_namespace_path(
    databases: &[String],
    tree: &SqlNamespace,
    namespace_path: &[String],
) -> Vec<String> {
    let first = match namespace_path.first() {
        Some(first) => first,
        None => return namespace_path.to_vec(),
    };
    if !databases.contains(first) {
        return namespace_path.to_vec();
    }
    let is_rooted_tree_path = namespace_has_child(tree, namespace_path);
    let repeats_database_as_catalog = namespace_path.get(1) == Some(first);
    if is_rooted_tree_path || repeats_database_as_catalog {
        namespace_path[1..].to_vec()
    } else {
        namespace_path.to_vec()
    }
}

pub fn path_hierarchy_connection_root(
    databases: &[String],
    panel_database: Option<&str>,
    current_database: Option<&str>,
    path_aliases: &BTreeMap<String, String>,
    namespace_roots: &[String],
) -> Option<String> {
    let known_database_for = |root: &str| -> Option<String> {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            return None;
        }
        databases
            .iter()
            .find(|database| {
                database.as_str() == trimmed || database.split(':').next() == Some(trimmed)
            })
            .cloned()
    };
    let resolve_candidate = |value: Option<&str>| -> Option<String> {
        let trimmed = value?.trim();
        if trimmed.is_empty() {
            return None;
        }
        let (root, _) = split_path_hierarchy_database_pin(trimmed);
        let alias = path_aliases.get(&root).map(|value| value.trim());
        if namespace_roots.contains(&root) || path_aliases.contains_key(&root) {
            if let Some(alias) = alias.filter(|alias| !alias.is_empty()) {
                return known_database_for(alias).or_else(|| Some(alias.to_owned()));
            }
            return known_database_for(&root);
        }
        if let Some(known) = known_database_for(&root) {
            return Some(known);
        }
        if !root.is_empty() && root.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(root);
        }
        if databases.is_empty() && root.contains(':') {
            return Some(root);
        }
        None
    };

    if let Some(root) = resolve_candidate(panel_database) {
        return Some(root);
    }
    if let Some(root) = resolve_candidate(current_database) {
        return Some(root);
    }
    for database in databases {
        if let Some(root) = resolve_candidate(Some(database)) {
            return Some(root);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SqlNamespace {
        SqlNamespace::branch(&[
            (
                "hive",
                SqlNamespace::branch(&[("snap", SqlNamespace::leaf())]),
            ),
            ("audit", SqlNamespace::leaf()),
        ])
    }

    fn aliases() -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("hive".to_owned(), "558".to_owned());
        map
    }

    #[test]
    fn namespace_tree_queries_walk_branches_and_skip_leaves() {
        assert!(namespace_has_child(&tree(), &[]));
        assert!(namespace_has_child(&tree(), &["hive".to_owned()]));
        assert!(!namespace_has_child(
            &tree(),
            &["audit".to_owned(), "x".to_owned()]
        ));
        assert_eq!(namespace_child_names(&tree(), &[]), ["audit", "hive"]);
        assert_eq!(namespace_branch_child_names(&tree(), &[]), ["hive"]);
    }

    #[test]
    fn roots_merge_databases_aliases_and_branch_names() {
        let databases = ["db1".to_owned(), "hive".to_owned()];
        assert_eq!(
            namespace_roots_from(&tree(), &aliases(), &databases),
            ["db1", "hive"]
        );
    }

    #[test]
    fn pins_build_and_split_around_the_first_slash() {
        assert_eq!(
            build_path_hierarchy_database_pin("", &["a".to_owned()]),
            "a"
        );
        assert_eq!(build_path_hierarchy_database_pin(" 558 ", &[]), "558");
        assert_eq!(
            build_path_hierarchy_database_pin("558", &["hive".to_owned(), "snap".to_owned()]),
            "558/hive/snap"
        );
        assert_eq!(
            split_path_hierarchy_database_pin(""),
            (String::new(), Vec::new())
        );
        assert_eq!(
            split_path_hierarchy_database_pin("558"),
            ("558".to_owned(), Vec::new())
        );
        assert_eq!(
            split_path_hierarchy_database_pin(" 558/hive//snap "),
            ("558".to_owned(), vec!["hive".to_owned(), "snap".to_owned()])
        );
    }

    #[test]
    fn relative_path_drops_the_database_segment_only_when_tree_backs_it() {
        let databases = ["hive".to_owned()];
        let path = ["hive".to_owned(), "snap".to_owned()];
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree(), &path),
            ["snap"]
        );
        let repeated = ["hive".to_owned(), "hive".to_owned()];
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree(), &repeated),
            ["hive"]
        );
        let unrelated = ["other".to_owned(), "x".to_owned()];
        assert_eq!(
            path_hierarchy_relative_namespace_path(&databases, &tree(), &unrelated),
            unrelated
        );
    }

    #[test]
    fn connection_root_resolves_aliases_digits_and_id_colon_name() {
        let databases = ["558:hive".to_owned()];
        let roots = namespace_roots_from(&tree(), &aliases(), &databases);
        assert_eq!(
            path_hierarchy_connection_root(&databases, Some("hive"), None, &aliases(), &roots)
                .as_deref(),
            Some("558:hive")
        );
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("123"), None, &BTreeMap::new(), &[])
                .as_deref(),
            Some("123")
        );
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("558:hive"), None, &BTreeMap::new(), &[])
                .as_deref(),
            Some("558:hive")
        );
        assert_eq!(
            path_hierarchy_connection_root(&[], Some("???"), None, &BTreeMap::new(), &[]),
            None
        );
    }
}
