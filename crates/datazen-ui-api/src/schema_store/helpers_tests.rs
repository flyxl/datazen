use std::collections::{BTreeMap, BTreeSet};

use crate::SqlNamespace;

use super::*;

fn item(name: &str, schema: Option<&str>) -> SchemaTableInfo {
    SchemaTableInfo {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
fn surface_names_the_six_helpers() {
    assert_eq!(SCHEMA_STORE_HELPERS_SURFACE.len(), 6);
    assert_eq!(
        SCHEMA_STORE_HELPERS_SURFACE,
        [
            "computeIsMultiDatabase",
            "resolvePreferredDatabase",
            "resolveVisibleDatabases",
            "parsePathHierarchyDatabaseEntry",
            "knownTableNames",
            "LoadForConnectionOptions",
        ]
    );
    assert_eq!(crate::SCHEMA_STORE_HELPERS_SURFACE.len(), 6);
}

#[test]
fn multi_database_requires_the_capability_and_two_visible_databases() {
    assert!(!compute_is_multi_database(false, 9));
    assert!(!compute_is_multi_database(true, 0));
    assert!(!compute_is_multi_database(true, 1));
    assert!(compute_is_multi_database(true, 2));
    assert!(compute_is_multi_database(true, 100));
}

#[test]
fn preferred_database_skips_system_names_but_honors_a_configured_one() {
    let databases = vec![
        "information_schema".to_owned(),
        "mysql".to_owned(),
        "analytics".to_owned(),
        "shop".to_owned(),
    ];
    assert_eq!(resolve_preferred_database(&databases, None).as_deref(), Some("analytics"));
    assert_eq!(resolve_preferred_database(&databases, Some("shop")).as_deref(), Some("shop"));
    assert_eq!(resolve_preferred_database(&databases, Some("nope")).as_deref(), Some("analytics"));
    assert_eq!(resolve_preferred_database(&[], None), None);
    assert_eq!(resolve_preferred_database(&[], Some("x")), None);
    let only_system = vec!["postgres".to_owned(), "template0".to_owned()];
    assert_eq!(resolve_preferred_database(&only_system, None).as_deref(), Some("postgres"));
}

#[test]
fn visible_databases_trims_the_configured_name_before_locking() {
    let all = vec!["alpha".to_owned(), "beta".to_owned()];
    let locked = resolve_visible_databases(&all, Some("  beta  "));
    assert_eq!(locked.databases, vec!["beta".to_owned()]);
    assert_eq!(locked.preferred.as_deref(), Some("beta"));
    assert!(locked.locked_to_configured);

    let all_whitespace = resolve_visible_databases(&all, Some("   "));
    assert_eq!(all_whitespace.databases, all);
    assert_eq!(all_whitespace.preferred.as_deref(), Some("alpha"));
    assert!(!all_whitespace.locked_to_configured);
}

#[test]
fn path_entries_strip_the_backend_suffix_and_keep_the_id() {
    assert_eq!(
        parse_path_hierarchy_database_entry("c1:sales (postgres)"),
        PathDatabaseEntry { id: "c1".to_owned(), name: "sales".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("c1:sales"),
        PathDatabaseEntry { id: "c1".to_owned(), name: "sales".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("sales"),
        PathDatabaseEntry { id: "sales".to_owned(), name: "sales".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("c1:a (b) (c)"),
        PathDatabaseEntry { id: "c1".to_owned(), name: "a (b)".to_owned() }
    );
    assert_eq!(
        parse_path_hierarchy_database_entry("c1:"),
        PathDatabaseEntry { id: "c1".to_owned(), name: String::new() }
    );
}

#[test]
fn known_table_names_takes_last_path_segment_and_skips_pseudo_schemas() {
    let tree = merge_namespace_path(
        &SqlNamespace::Leaf,
        &[],
        NamespaceMergeKind::Tables,
        &["tree_table".to_owned()],
        false,
    );
    let path_items = BTreeMap::from([(
        "p".to_owned(),
        vec![
            item("catalog/ignored", Some("CATALOG")),
            item("schema/ignored", Some("SCHEMA")),
            item("bucket//nested/target", None),
        ],
    )]);
    let names = known_table_names(
        &tree,
        &[item("real", Some("public"))],
        &[item("a_view", None)],
        &path_items,
    );
    assert_eq!(
        names,
        BTreeSet::from([
            "tree_table".to_owned(),
            "real".to_owned(),
            "a_view".to_owned(),
            "target".to_owned(),
        ])
    );
    assert_eq!(known_table_names(&SqlNamespace::Leaf, &[], &[], &BTreeMap::new()), BTreeSet::new());
}

#[test]
fn load_options_default_to_focusing_the_session() {
    let options = LoadForConnectionOptions::default();
    assert!(!options.skip_load_tables);
    assert_eq!(options.preferred_database, None);
    assert_eq!(options.database_type, None);
    assert!(options.activates(), "React defaults activate to true (activate !== false)");
    assert!(!LoadForConnectionOptions { activate: false, ..Default::default() }.activates());
}

#[test]
fn crate_root_helpers_are_reexported() {
    let _ = crate::compute_is_multi_database;
    let _ = crate::resolve_preferred_database;
    let _ = crate::resolve_visible_databases;
    let _ = crate::parse_path_hierarchy_database_entry;
    let _ = crate::known_table_names;
    let _ = crate::is_system_database_name;
    let _ = crate::collect_table_leaf_names;
    assert_eq!(crate::SCHEMA_STORE_HELPERS_SURFACE.len(), 6);
    let _: crate::LoadForConnectionOptions = Default::default();
    let _: crate::VisibleDatabases = Default::default();
    let _: crate::PathDatabaseEntry = PathDatabaseEntry { id: String::new(), name: String::new() };
}
