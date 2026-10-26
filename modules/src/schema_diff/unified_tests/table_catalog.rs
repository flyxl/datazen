use super::*;
use crate::schema_diff::operations::MigrationOperation as HostMigrationOperation;
use crate::schema_diff::types::ColumnSnapshot;
use std::collections::{BTreeSet, HashMap};

fn dependency_snapshot(
    name: &str,
    dependencies: Option<Vec<SchemaObjectIdentity>>,
) -> SchemaObjectDependencySnapshot {
    SchemaObjectDependencySnapshot {
        identity: target_table(name),
        dependencies,
        type_dependency_usages: None,
        sequence_dependency_usages: None,
    }
}

fn inspect_source_dependencies(
    snapshot: &SchemaObjectDependencySnapshot,
    operations: &[HostMigrationOperation],
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    created: &HashMap<SchemaObjectIdentity, usize>,
    dropped: &HashMap<SchemaObjectIdentity, usize>,
) -> (Vec<(usize, usize)>, Vec<PlanRequirement>) {
    let node_keys = vec!["table:public.items".to_owned(); operations.len()];
    let mut edges = Vec::new();
    let mut requirements = Vec::new();
    crate::schema_diff::unified_validation::validate_table_catalog_dependencies(
        std::slice::from_ref(snapshot),
        operations,
        target_catalog,
        created,
        dropped,
        &node_keys,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    (edges, requirements)
}

fn column_snapshot(name: &str, data_type: &str) -> ColumnSnapshot {
    ColumnSnapshot {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

fn type_identity(schema: &str, name: &str) -> SchemaObjectIdentity {
    SchemaObjectIdentity {
        kind: ObjectKind::Type,
        schema: Some(schema.into()),
        name: name.into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

#[test]
fn test_tester_table_dependency_catalog_requires_complete_exact_target_proof() {
    let state_type = type_identity("public", "order_state");
    let snapshot = dependency_snapshot("items", Some(vec![state_type.clone()]));
    let operations = [HostMigrationOperation::CreateTable {
        table: "public.items".into(),
        columns: vec![column_snapshot("state", "public.order_state")],
        primary_keys: Vec::new(),
        table_options: Default::default(),
    }];
    let empty = BTreeSet::new();
    let no_indices = HashMap::new();

    let (_, missing) =
        inspect_source_dependencies(&snapshot, &operations, &empty, &no_indices, &no_indices);
    assert!(format!("{missing:?}").contains("absent from the complete target snapshot"));

    let incomplete = dependency_snapshot("items", None);
    let (_, incomplete_requirements) =
        inspect_source_dependencies(&incomplete, &operations, &empty, &no_indices, &no_indices);
    assert!(format!("{incomplete_requirements:?}").contains("could not prove all dependencies"));

    let ambiguous_target = BTreeSet::from([type_identity("archive", "order_state")]);
    let (_, ambiguous) = inspect_source_dependencies(
        &snapshot,
        &operations,
        &ambiguous_target,
        &no_indices,
        &no_indices,
    );
    assert!(format!("{ambiguous:?}").contains("ambiguous basename candidates"));

    let exact_target = BTreeSet::from([state_type.clone()]);
    let (edges, exact_requirements) = inspect_source_dependencies(
        &snapshot,
        &operations,
        &exact_target,
        &no_indices,
        &no_indices,
    );
    assert!(edges.is_empty());
    assert!(exact_requirements.is_empty());

    let operations_with_created_type = [
        HostMigrationOperation::CreateType {
            type_definition: datazen_driver_api::MigrationType {
                schema: Some("public".into()),
                name: "order_state".into(),
                definition: "CREATE TYPE public.order_state AS ENUM ('new')".into(),
            },
        },
        operations[0].clone(),
    ];
    let created = HashMap::from([(state_type.clone(), 0)]);
    let (created_edges, created_requirements) = inspect_source_dependencies(
        &snapshot,
        &operations_with_created_type,
        &empty,
        &created,
        &no_indices,
    );
    assert_eq!(created_edges, vec![(0, 1)]);
    assert!(created_requirements.is_empty());

    let dropped = HashMap::from([(state_type, 0)]);
    let (_, dropped_requirements) =
        inspect_source_dependencies(&snapshot, &operations, &empty, &no_indices, &dropped);
    assert!(format!("{dropped_requirements:?}").contains("also selected for removal"));
}

#[test]
fn test_tester_table_drop_catalog_orders_dependents_and_requires_selected_foreign_key_removal() {
    let child_schema = table("orders", vec![], vec![foreign_key("public.users")]);
    let target_dependencies = [("public.orders".into(), child_schema)];
    let parent_only = [HostMigrationOperation::DropTable {
        table: "public.users".into(),
    }];
    let mut edges = Vec::new();
    let mut requirements = Vec::new();
    crate::schema_diff::unified_validation::validate_table_drop_catalog(
        &parent_only,
        &target_dependencies,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert!(edges.is_empty());
    assert!(format!("{requirements:?}").contains("Unselected target table"));

    let child_and_parent = [
        HostMigrationOperation::DropTable {
            table: "public.users".into(),
        },
        HostMigrationOperation::DropTable {
            table: "public.orders".into(),
        },
    ];
    edges.clear();
    requirements.clear();
    crate::schema_diff::unified_validation::validate_table_drop_catalog(
        &child_and_parent,
        &target_dependencies,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert_eq!(edges, vec![(1, 0)]);
    assert!(requirements.is_empty());

    let foreign_key_only = [
        HostMigrationOperation::DropTable {
            table: "public.users".into(),
        },
        HostMigrationOperation::DropForeignKey {
            table: "public.orders".into(),
            foreign_key: foreign_key("public.users"),
        },
    ];
    edges.clear();
    requirements.clear();
    crate::schema_diff::unified_validation::validate_table_drop_catalog(
        &foreign_key_only,
        &target_dependencies,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert!(requirements.is_empty());
}

#[test]
fn test_tester_custom_type_drop_requires_a_complete_catalog_and_removes_users_first() {
    let drop_type = HostMigrationOperation::DropType {
        type_definition: datazen_driver_api::MigrationType {
            schema: Some("public".into()),
            name: "order_state".into(),
            definition: "CREATE TYPE public.order_state AS ENUM ('new')".into(),
        },
    };
    let mut edges = Vec::new();
    let mut requirements = Vec::new();
    crate::schema_diff::unified_validation::validate_custom_type_drops(
        std::slice::from_ref(&drop_type),
        &[],
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert!(format!("{requirements:?}").contains("complete target table catalog is required"));

    let dependent_table = [(
        "public.items".into(),
        table("items", vec![column("state", "public.order_state")], vec![]),
    )];
    edges.clear();
    requirements.clear();
    crate::schema_diff::unified_validation::validate_custom_type_drops(
        std::slice::from_ref(&drop_type),
        &dependent_table,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert!(format!("{requirements:?}").contains("Unchanged target column"));

    let remove_table_first = [
        HostMigrationOperation::DropTable {
            table: "public.items".into(),
        },
        drop_type.clone(),
    ];
    edges.clear();
    requirements.clear();
    crate::schema_diff::unified_validation::validate_custom_type_drops(
        &remove_table_first,
        &dependent_table,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert_eq!(edges, vec![(0, 1)]);
    assert!(requirements.is_empty());

    let ambiguous_types = [
        drop_type,
        HostMigrationOperation::DropType {
            type_definition: datazen_driver_api::MigrationType {
                schema: Some("archive".into()),
                name: "order_state".into(),
                definition: "CREATE TYPE archive.order_state AS ENUM ('new')".into(),
            },
        },
    ];
    let unqualified_user = [(
        "public.items".into(),
        table("items", vec![column("state", "order_state")], vec![]),
    )];
    edges.clear();
    requirements.clear();
    crate::schema_diff::unified_validation::validate_custom_type_drops(
        &ambiguous_types,
        &unqualified_user,
        &mut edges,
        &mut requirements,
        "postgresql",
        Some("public"),
        Some("test_db"),
    );
    assert!(format!("{requirements:?}").contains("multiple selected type drops"));
}
