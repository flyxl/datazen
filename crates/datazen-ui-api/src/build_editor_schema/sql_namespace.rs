use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlNamespace {
    Table(Vec<String>),
    Branch(BTreeMap<String, SqlNamespace>),
}

impl SqlNamespace {
    pub fn empty_branch() -> Self {
        Self::Branch(BTreeMap::new())
    }

    pub fn table(columns: &[&str]) -> Self {
        Self::Table(columns.iter().map(|column| (*column).to_owned()).collect())
    }
}

pub fn is_leaf(node: &SqlNamespace) -> bool {
    matches!(node, SqlNamespace::Table(_))
}

pub fn is_namespace_empty(tree: &SqlNamespace) -> bool {
    matches!(tree, SqlNamespace::Branch(branch) if branch.is_empty())
}

pub fn overlay_column_map(
    tree: &SqlNamespace,
    column_map: &BTreeMap<String, Vec<String>>,
) -> SqlNamespace {
    let SqlNamespace::Branch(branch) = tree else {
        return tree.clone();
    };
    let mut out = BTreeMap::new();
    for (name, child) in branch {
        match child {
            SqlNamespace::Table(columns) => {
                out.insert(
                    name.clone(),
                    SqlNamespace::Table(match column_map.get(name) {
                        Some(columns) => columns.clone(),
                        None => columns.clone(),
                    }),
                );
            }
            SqlNamespace::Branch(_) => {
                out.insert(name.clone(), overlay_column_map(child, column_map));
            }
        }
    }
    SqlNamespace::Branch(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(entries: &[(&str, SqlNamespace)]) -> SqlNamespace {
        SqlNamespace::Branch(
            entries.iter().map(|(name, node)| ((*name).to_owned(), node.clone())).collect(),
        )
    }

    fn map(entries: &[(&str, Vec<String>)]) -> BTreeMap<String, Vec<String>> {
        entries.iter().map(|(name, columns)| ((*name).to_owned(), columns.clone())).collect()
    }

    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn leaf_and_empty_branch_are_distinguished() {
        assert!(is_leaf(&SqlNamespace::table(&[])));
        assert!(!is_leaf(&SqlNamespace::empty_branch()));
        assert!(is_namespace_empty(&SqlNamespace::empty_branch()));
        assert!(!is_namespace_empty(&SqlNamespace::table(&[])));
        assert!(!is_namespace_empty(&branch(&[("a", SqlNamespace::table(&[]))])));
    }

    #[test]
    fn overlay_replaces_table_leaves_and_recurses_branches() {
        let tree = branch(&[
            ("users", SqlNamespace::table(&["id", "old"])),
            ("main", branch(&[("orders", SqlNamespace::table(&["id"]))])),
        ]);
        let column_map = map(&[
            ("users", columns(&["id", "new"])),
            ("orders", Vec::new()),
            ("missing", columns(&["x"])),
        ]);
        assert_eq!(
            overlay_column_map(&tree, &column_map),
            branch(&[
                ("users", SqlNamespace::table(&["id", "new"])),
                ("main", branch(&[("orders", SqlNamespace::table(&[]))])),
            ])
        );
    }

    #[test]
    fn overlay_leaves_a_leaf_root_untouched() {
        let leaf = SqlNamespace::table(&["id"]);
        let column_map = map(&[("id", columns(&["replaced"]))]);
        assert_eq!(
            overlay_column_map(&leaf, &column_map),
            SqlNamespace::table(&["id"])
        );
    }

    #[test]
    fn overlay_preserves_the_original_tree() {
        let tree = branch(&[("users", SqlNamespace::table(&["id"]))]);
        let column_map = map(&[("users", columns(&["x"]))]);
        let _ = overlay_column_map(&tree, &column_map);
        assert_eq!(tree, branch(&[("users", SqlNamespace::table(&["id"]))]));
    }
}
