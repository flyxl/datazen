use std::collections::BTreeMap;

use crate::{is_leaf, overlay_column_map, SqlNamespace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceMergeKind {
    Branch,
    Tables,
}

fn as_branch(node: Option<&SqlNamespace>) -> BTreeMap<String, SqlNamespace> {
    match node {
        Some(SqlNamespace::Branch(branch)) => branch.clone(),
        _ => BTreeMap::new(),
    }
}

fn set_at(
    node: BTreeMap<String, SqlNamespace>,
    segments: &[String],
    kind: NamespaceMergeKind,
    names: &[String],
    replace: bool,
) -> BTreeMap<String, SqlNamespace> {
    let mut next = node;
    if segments.is_empty() {
        if kind == NamespaceMergeKind::Tables && replace {
            next.retain(|_, child| !is_leaf(child));
        }
        for name in names {
            match kind {
                NamespaceMergeKind::Tables => {
                    let existing = next.get(name);
                    match existing {
                        Some(SqlNamespace::Table(_)) => (),
                        _ => {
                            next.insert(name.clone(), SqlNamespace::Table(Vec::new()));
                        }
                    }
                }
                NamespaceMergeKind::Branch => {
                    let needs_reset = match next.get(name) {
                        None => true,
                        Some(child) => is_leaf(child),
                    };
                    if needs_reset {
                        next.insert(name.clone(), SqlNamespace::Branch(BTreeMap::new()));
                    }
                }
            }
        }
        return next;
    }
    let head = &segments[0];
    let child = as_branch(next.get(head));
    let merged = set_at(child, &segments[1..], kind, names, replace);
    next.insert(head.clone(), SqlNamespace::Branch(merged));
    next
}

pub fn merge_namespace_path(
    tree: &SqlNamespace,
    segments: &[String],
    kind: NamespaceMergeKind,
    names: &[String],
    replace: bool,
) -> SqlNamespace {
    SqlNamespace::Branch(set_at(as_branch(Some(tree)), segments, kind, names, replace))
}

pub fn omit_table_leaf(tree: &SqlNamespace, table_name: &str) -> SqlNamespace {
    let SqlNamespace::Branch(branch) = tree else {
        return tree.clone();
    };
    let mut out = BTreeMap::new();
    for (key, child) in branch {
        if is_leaf(child) {
            if key != table_name {
                out.insert(key.clone(), child.clone());
            }
        } else {
            out.insert(key.clone(), omit_table_leaf(child, table_name));
        }
    }
    SqlNamespace::Branch(out)
}

fn descend<'a>(tree: &'a SqlNamespace, segments: &[String]) -> Option<&'a SqlNamespace> {
    let mut node = tree;
    for segment in segments {
        let SqlNamespace::Branch(branch) = node else {
            return None;
        };
        node = branch.get(segment)?;
    }
    Some(node)
}

pub fn namespace_has_child(tree: &SqlNamespace, segments: &[String]) -> bool {
    descend(tree, segments).is_some()
}

pub fn namespace_child_names(tree: &SqlNamespace, segments: &[String]) -> Vec<String> {
    match descend(tree, segments) {
        Some(SqlNamespace::Branch(branch)) => branch.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

pub fn namespace_branch_child_names(tree: &SqlNamespace, segments: &[String]) -> Vec<String> {
    match descend(tree, segments) {
        Some(SqlNamespace::Branch(branch)) => branch
            .iter()
            .filter(|(_, child)| !is_leaf(child))
            .map(|(name, _)| name.clone())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn collect_table_leaf_names(tree: &SqlNamespace) -> Vec<String> {
    let mut names = BTreeMap::new();
    fn walk(node: &SqlNamespace, names: &mut BTreeMap<String, ()>) {
        let SqlNamespace::Branch(branch) = node else {
            return;
        };
        for (key, child) in branch {
            if is_leaf(child) {
                names.insert(key.clone(), ());
            } else {
                walk(child, names);
            }
        }
    }
    walk(tree, &mut names);
    names.into_keys().collect()
}

pub fn overlay_column_map_owned(
    tree: &SqlNamespace,
    column_map: &BTreeMap<String, Vec<String>>,
) -> SqlNamespace {
    overlay_column_map(tree, column_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(entries: Vec<(&str, SqlNamespace)>) -> SqlNamespace {
        SqlNamespace::Branch(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    fn table(columns: &[&str]) -> SqlNamespace {
        SqlNamespace::Table(columns.iter().map(|value| (*value).to_owned()).collect())
    }

    fn segs(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn equivalence_with_existing_ports() {
        assert!(!crate::is_schema_grouping_schema(None));
        assert!(!crate::is_schema_grouping_schema(Some("CATALOG")));
        assert!(!crate::is_schema_grouping_schema(Some("SCHEMA")));
        assert!(crate::is_schema_grouping_schema(Some("public")));
        assert_eq!(crate::path_key(&["a", "b"]), "a/b");
        assert!(is_leaf(&table(&["id"])));
        assert!(!is_leaf(&branch(vec![])));
        let mut map = BTreeMap::new();
        map.insert("users".to_owned(), vec!["id".to_owned()]);
        let overlaid = overlay_column_map_owned(&branch(vec![("users", table(&[]))]), &map);
        assert_eq!(overlaid, branch(vec![("users", table(&["id"]))]));
    }

    #[test]
    fn merge_branch_creates_missing_nodes_and_resets_leaves() {
        let tree = branch(vec![("a", table(&["x"]))]);
        let merged = merge_namespace_path(&tree, &segs(&["a"]), NamespaceMergeKind::Branch, &segs(&["b"]), false);
        assert_eq!(
            merged,
            branch(vec![("a", branch(vec![("b", branch(vec![]))]))])
        );
        let merged = merge_namespace_path(&tree, &[], NamespaceMergeKind::Branch, &segs(&["a"]), false);
        assert_eq!(merged, branch(vec![("a", branch(vec![]))]), "an existing leaf is reset for branch merges");
    }

    #[test]
    fn merge_tables_preserves_leaves_and_replace_clears_them() {
        let tree = branch(vec![
            ("users", table(&["id"])),
            ("nested", branch(vec![("inner", table(&[]))])),
        ]);
        let merged = merge_namespace_path(&tree, &[], NamespaceMergeKind::Tables, &segs(&["orders"]), false);
        assert_eq!(collect_table_leaf_names(&merged), vec!["inner", "orders", "users"]);

        let replaced = merge_namespace_path(&tree, &[], NamespaceMergeKind::Tables, &segs(&["orders"]), true);
        assert_eq!(collect_table_leaf_names(&replaced), vec!["inner", "orders"]);
        assert!(namespace_has_child(&replaced, &segs(&["nested", "inner"])), "catalog branches survive replace");
    }

    #[test]
    fn nested_merge_discards_a_table_leaf_on_the_path() {
        let tree = branch(vec![("db", table(&["x"]))]);
        let merged = merge_namespace_path(&tree, &segs(&["db", "public"]), NamespaceMergeKind::Tables, &segs(&["users"]), false);
        assert_eq!(
            merged,
            branch(vec![("db", branch(vec![("public", branch(vec![("users", table(&[]))]))]))])
        );
    }

    #[test]
    fn omit_removes_table_leaves_at_any_depth() {
        let tree = branch(vec![
            ("users", table(&[])),
            ("db", branch(vec![("users", table(&[])), ("orders", table(&[]))])),
        ]);
        let omitted = omit_table_leaf(&tree, "users");
        assert_eq!(collect_table_leaf_names(&omitted), vec!["orders"]);
        assert!(namespace_has_child(&omitted, &segs(&["db"])));
    }

    #[test]
    fn child_name_variants_and_missing_paths() {
        let tree = branch(vec![
            ("users", table(&[])),
            ("public", branch(vec![("orders", table(&[]))])),
        ]);
        assert!(namespace_has_child(&tree, &[]));
        assert!(namespace_has_child(&tree, &segs(&["public"])));
        assert!(!namespace_has_child(&tree, &segs(&["missing"])));
        assert!(!namespace_has_child(&tree, &segs(&["users", "x"])), "a table leaf has no children");
        assert_eq!(namespace_child_names(&tree, &[]), vec!["public", "users"]);
        assert_eq!(namespace_branch_child_names(&tree, &[]), vec!["public"]);
        assert_eq!(namespace_child_names(&tree, &segs(&["users"])), Vec::<String>::new());
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::merge_namespace_path;
        let _ = crate::omit_table_leaf;
        let _ = crate::namespace_tree_has_child;
        let _ = crate::namespace_child_names;
        let _ = crate::namespace_branch_child_names;
        let _ = crate::collect_table_leaf_names;
        let _ = crate::overlay_column_map_owned;
        let _: Option<crate::NamespaceMergeKind> = None;
    }
}
