use std::collections::{BTreeMap, BTreeSet};

use crate::SqlNamespace;

use super::types::NamespaceMergeKind;

pub fn path_key(segments: &[String]) -> String {
    segments.join("/")
}

fn branch_entries(node: &SqlNamespace) -> BTreeMap<String, SqlNamespace> {
    match node {
        SqlNamespace::Branch(entries) => entries.clone(),
        SqlNamespace::Leaf => BTreeMap::new(),
    }
}

pub fn collect_table_leaf_names(tree: &SqlNamespace) -> BTreeSet<String> {
    fn walk(node: &SqlNamespace, names: &mut BTreeSet<String>) {
        if let SqlNamespace::Branch(entries) = node {
            for (name, child) in entries {
                if matches!(child, SqlNamespace::Leaf) {
                    names.insert(name.clone());
                } else {
                    walk(child, names);
                }
            }
        }
    }
    let mut names = BTreeSet::new();
    walk(tree, &mut names);
    names
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
            next.retain(|_, child| !matches!(child, SqlNamespace::Leaf));
        }
        for name in names {
            match kind {
                NamespaceMergeKind::Tables => {
                    next.insert(name.clone(), SqlNamespace::Leaf);
                }
                NamespaceMergeKind::Branch => {
                    let needs_branch = next
                        .get(name)
                        .is_none_or(|child| matches!(child, SqlNamespace::Leaf));
                    if needs_branch {
                        next.insert(name.clone(), SqlNamespace::Branch(BTreeMap::new()));
                    }
                }
            }
        }
        return next;
    }
    let head = &segments[0];
    let child = branch_entries(next.get(head).unwrap_or(&SqlNamespace::Leaf));
    let updated = set_at(child, &segments[1..], kind, names, replace);
    next.insert(head.clone(), SqlNamespace::Branch(updated));
    next
}

pub fn merge_namespace_path(
    tree: &SqlNamespace,
    segments: &[String],
    kind: NamespaceMergeKind,
    names: &[String],
    replace: bool,
) -> SqlNamespace {
    SqlNamespace::Branch(set_at(branch_entries(tree), segments, kind, names, replace))
}

pub fn omit_table_leaf(tree: &SqlNamespace, table_name: &str) -> SqlNamespace {
    match tree {
        SqlNamespace::Leaf => SqlNamespace::Leaf,
        SqlNamespace::Branch(entries) => {
            let mut out = BTreeMap::new();
            for (name, child) in entries {
                match child {
                    SqlNamespace::Leaf => {
                        if name != table_name {
                            out.insert(name.clone(), SqlNamespace::Leaf);
                        }
                    }
                    branch => {
                        out.insert(name.clone(), omit_table_leaf(branch, table_name));
                    }
                }
            }
            SqlNamespace::Branch(out)
        }
    }
}
