mod sql_namespace;

use std::collections::BTreeMap;

use crate::connection_types::TableInfo;
pub use sql_namespace::{is_leaf, is_namespace_empty, overlay_column_map, SqlNamespace};

pub struct BuildEditorSchemaInput<'a> {
    pub namespace_tree: SqlNamespace,
    pub tables: &'a [TableInfo],
    pub views: &'a [TableInfo],
    pub column_map: &'a BTreeMap<String, Vec<String>>,
    pub current_database: Option<&'a str>,
    pub hoist_path: Option<&'a [String]>,
}

pub fn flat_from_tables(tables: &[TableInfo], views: &[TableInfo]) -> SqlNamespace {
    let mut result = BTreeMap::new();
    for table in tables.iter().chain(views.iter()) {
        result.insert(table.name.clone(), SqlNamespace::Table(Vec::new()));
    }
    SqlNamespace::Branch(result)
}

pub fn hoist_namespace_child(tree: &SqlNamespace, child_key: Option<&str>) -> SqlNamespace {
    let Some(child_key) = child_key else {
        return tree.clone();
    };
    let SqlNamespace::Branch(branch) = tree else {
        return tree.clone();
    };
    let Some(child) = branch.get(child_key) else {
        return tree.clone();
    };
    let mut out = branch.clone();
    match child {
        SqlNamespace::Table(columns) => {
            for name in columns {
                if !out.contains_key(name) {
                    out.insert(name.clone(), SqlNamespace::Table(Vec::new()));
                }
            }
        }
        SqlNamespace::Branch(child_branch) => {
            for (name, value) in child_branch {
                if !out.contains_key(name) {
                    out.insert(name.clone(), value.clone());
                }
            }
        }
    }
    SqlNamespace::Branch(out)
}

pub fn hoist_namespace_path(tree: &SqlNamespace, path: Option<&[String]>) -> SqlNamespace {
    let mut out = tree.clone();
    for segment in path.unwrap_or_default() {
        out = hoist_namespace_child(&out, Some(segment.as_str()));
    }
    out
}

fn walk_table_leaves(node: &SqlNamespace, out: &mut BTreeMap<String, SqlNamespace>) {
    let SqlNamespace::Branch(branch) = node else {
        return;
    };
    for (name, child) in branch {
        match child {
            SqlNamespace::Table(_) => {
                if !out.contains_key(name) {
                    out.insert(name.clone(), child.clone());
                }
            }
            SqlNamespace::Branch(_) => walk_table_leaves(child, out),
        }
    }
}

pub fn hoist_nested_table_leaves(tree: &SqlNamespace) -> SqlNamespace {
    let SqlNamespace::Branch(branch) = tree else {
        return tree.clone();
    };
    let mut out = branch.clone();
    walk_table_leaves(tree, &mut out);
    SqlNamespace::Branch(out)
}

pub fn merge_unqualified_tables(
    tree: &SqlNamespace,
    tables: &[TableInfo],
    views: &[TableInfo],
) -> SqlNamespace {
    if tables.is_empty() && views.is_empty() {
        return tree.clone();
    }
    let mut out = match tree {
        SqlNamespace::Branch(branch) => branch.clone(),
        SqlNamespace::Table(_) => BTreeMap::new(),
    };
    for item in tables.iter().chain(views.iter()) {
        if !out.contains_key(&item.name) {
            out.insert(item.name.clone(), SqlNamespace::Table(Vec::new()));
        }
    }
    SqlNamespace::Branch(out)
}

pub fn build_editor_schema(input: &BuildEditorSchemaInput<'_>) -> SqlNamespace {
    let base = if is_namespace_empty(&input.namespace_tree) {
        flat_from_tables(input.tables, input.views)
    } else {
        input.namespace_tree.clone()
    };
    let with_db = hoist_namespace_child(&base, input.current_database);
    let with_path = hoist_namespace_path(&with_db, input.hoist_path);
    let with_nested = hoist_nested_table_leaves(&with_path);
    let with_tables = merge_unqualified_tables(&with_nested, input.tables, input.views);
    overlay_column_map(&with_tables, input.column_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_types::TableType;

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

    fn table(name: &str) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        }
    }

    fn view(name: &str) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: None,
            table_type: TableType::View,
            row_count: None,
        }
    }

    fn input<'a>(
        tree: SqlNamespace,
        tables: &'a [TableInfo],
        views: &'a [TableInfo],
        column_map: &'a BTreeMap<String, Vec<String>>,
        current_database: Option<&'a str>,
        hoist_path: Option<&'a [String]>,
    ) -> BuildEditorSchemaInput<'a> {
        BuildEditorSchemaInput {
            namespace_tree: tree,
            tables,
            views,
            column_map,
            current_database,
            hoist_path,
        }
    }

    #[test]
    fn an_empty_tree_flattens_tables_and_views() {
        let tables = [table("orders"), table("users")];
        let views = [view("active_users")];
        let column_map = BTreeMap::new();
        assert_eq!(
            build_editor_schema(&input(
                SqlNamespace::empty_branch(),
                &tables,
                &views,
                &column_map,
                None,
                None
            )),
            branch(&[
                ("active_users", SqlNamespace::table(&[])),
                ("orders", SqlNamespace::table(&[])),
                ("users", SqlNamespace::table(&[])),
            ])
        );
    }

    #[test]
    fn duplicate_table_and_view_names_collapse_to_one_key() {
        let tables = [table("users")];
        let views = [view("users")];
        let column_map = BTreeMap::new();
        assert_eq!(
            build_editor_schema(&input(
                SqlNamespace::empty_branch(),
                &tables,
                &views,
                &column_map,
                None,
                None
            )),
            branch(&[("users", SqlNamespace::table(&[]))])
        );
    }

    #[test]
    fn a_non_empty_tree_is_kept_and_tables_are_merged_at_the_root() {
        let tree = branch(&[("public", branch(&[("users", SqlNamespace::table(&["id"]))]))]);
        let tables = [table("orders")];
        let views = [];
        let column_map = BTreeMap::new();
        assert_eq!(
            build_editor_schema(&input(
                tree,
                &tables,
                &views,
                &column_map,
                None,
                None
            )),
            branch(&[
                ("orders", SqlNamespace::table(&[])),
                ("public", branch(&[("users", SqlNamespace::table(&["id"]))])),
                ("users", SqlNamespace::table(&["id"])),
            ])
        );
    }

    #[test]
    fn hoisting_the_current_database_copies_its_children_without_overwriting() {
        let tree = branch(&[
            ("users", SqlNamespace::table(&["root_col"])),
            ("main", branch(&[
                ("orders", SqlNamespace::table(&["id"])),
                ("users", SqlNamespace::table(&["db_col"])),
            ])),
        ]);
        assert_eq!(
            hoist_namespace_child(&tree, Some("main")),
            branch(&[
                ("main", branch(&[
                    ("orders", SqlNamespace::table(&["id"])),
                    ("users", SqlNamespace::table(&["db_col"])),
                ])),
                ("orders", SqlNamespace::table(&["id"])),
                ("users", SqlNamespace::table(&["root_col"])),
            ])
        );
    }

    #[test]
    fn hoisting_a_table_child_copies_its_names_as_empty_tables() {
        let tree = branch(&[
            ("db", SqlNamespace::table(&["users", "orders", "users"])),
            ("users", SqlNamespace::table(&["keep"])),
        ]);
        assert_eq!(
            hoist_namespace_child(&tree, Some("db")),
            branch(&[
                ("db", SqlNamespace::table(&["users", "orders", "users"])),
                ("orders", SqlNamespace::table(&[])),
                ("users", SqlNamespace::table(&["keep"])),
            ])
        );
    }

    #[test]
    fn hoisting_a_missing_or_absent_child_returns_the_tree() {
        let tree = branch(&[("main", SqlNamespace::table(&["users"]))]);
        assert_eq!(hoist_namespace_child(&tree, None), tree);
        assert_eq!(hoist_namespace_child(&tree, Some("missing")), tree);
        let leaf = SqlNamespace::table(&["users"]);
        assert_eq!(hoist_namespace_child(&leaf, Some("users")), leaf);
    }

    #[test]
    fn hoisting_a_path_applies_each_segment_in_order() {
        let tree = branch(&[("cat", branch(&[("sch", branch(&[("t", SqlNamespace::table(&["id"]))]))]))]);
        let path = [String::from("cat"), String::from("sch")];
        assert_eq!(
            hoist_namespace_path(&tree, Some(&path)),
            branch(&[
                ("cat", branch(&[("sch", branch(&[("t", SqlNamespace::table(&["id"]))]))])),
                ("sch", branch(&[("t", SqlNamespace::table(&["id"]))])),
                ("t", SqlNamespace::table(&["id"])),
            ])
        );
        assert_eq!(hoist_namespace_path(&tree, None), tree);
        assert_eq!(hoist_namespace_path(&tree, Some(&[])), tree);
    }

    #[test]
    fn nested_table_leaves_are_hoisted_at_any_depth_with_their_columns() {
        let tree = branch(&[
            ("cat", branch(&[
                ("sch", branch(&[("deep", SqlNamespace::table(&["a"]))])),
                ("shallow", SqlNamespace::table(&["b"])),
            ])),
            ("deep", SqlNamespace::table(&["root_wins"])),
        ]);
        assert_eq!(
            hoist_nested_table_leaves(&tree),
            branch(&[
                ("cat", branch(&[
                    ("sch", branch(&[("deep", SqlNamespace::table(&["a"]))])),
                    ("shallow", SqlNamespace::table(&["b"])),
                ])),
                ("deep", SqlNamespace::table(&["root_wins"])),
                ("shallow", SqlNamespace::table(&["b"])),
            ])
        );
        assert_eq!(
            hoist_nested_table_leaves(&SqlNamespace::table(&["id"])),
            SqlNamespace::table(&["id"])
        );
    }

    #[test]
    fn merging_tables_does_not_overwrite_existing_root_keys() {
        let tree = branch(&[("users", SqlNamespace::table(&["existing"]))]);
        let tables = [table("users"), table("orders")];
        let views = [view("v")];
        assert_eq!(
            merge_unqualified_tables(&tree, &tables, &views),
            branch(&[
                ("orders", SqlNamespace::table(&[])),
                ("users", SqlNamespace::table(&["existing"])),
                ("v", SqlNamespace::table(&[])),
            ])
        );
    }

    #[test]
    fn merging_into_a_leaf_replaces_it_with_the_table_names() {
        let leaf = SqlNamespace::table(&["a", "b"]);
        let tables = [table("users")];
        let views = [];
        assert_eq!(
            merge_unqualified_tables(&leaf, &tables, &views),
            branch(&[("users", SqlNamespace::table(&[]))])
        );
        assert_eq!(merge_unqualified_tables(&leaf, &[], &[]), leaf);
    }

    #[test]
    fn the_full_pipeline_hoists_db_and_path_then_overlays_columns() {
        let tree = branch(&[("cat", branch(&[("sch", branch(&[("nested", SqlNamespace::table(&["n"]))]))]))]);
        let tables = [table("users"), table("orders")];
        let views = [view("v")];
        let column_map = map(&[
            ("users", columns(&["id", "name"])),
            ("nested", columns(&["overridden"])),
            ("orders", Vec::new()),
        ]);
        let path = [String::from("cat"), String::from("sch")];
        assert_eq!(
            build_editor_schema(&input(
                tree,
                &tables,
                &views,
                &column_map,
                Some("cat"),
                Some(&path)
            )),
            branch(&[
                ("cat", branch(&[("sch", branch(&[("nested", SqlNamespace::table(&["overridden"]))]))])),
                ("nested", SqlNamespace::table(&["overridden"])),
                ("orders", SqlNamespace::table(&[])),
                ("sch", branch(&[("nested", SqlNamespace::table(&["overridden"]))])),
                ("users", SqlNamespace::table(&["id", "name"])),
                ("v", SqlNamespace::table(&[])),
            ])
        );
    }

    #[test]
    fn a_leaf_namespace_tree_is_replaced_when_tables_exist() {
        let leaf = SqlNamespace::table(&["col"]);
        let tables = [table("users")];
        let views = [];
        let column_map = map(&[("users", columns(&["id"]))]);
        assert_eq!(
            build_editor_schema(&input(leaf, &tables, &views, &column_map, None, None)),
            branch(&[("users", SqlNamespace::table(&["id"]))])
        );
    }
}
