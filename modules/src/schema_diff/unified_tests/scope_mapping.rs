use super::*;
use crate::schema_diff::unified_scope::MySqlViewScopeContext;
use datazen_driver_api::{
    DatabaseObject, MySqlViewMetadata, SchemaObjectScopeDependency, SchemaObjectScopeMapping,
};

struct MappingRenderer;

impl MigrationRenderer for MappingRenderer {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String> {
        TestRenderer {
            omit_rollback: false,
        }
        .render(operation)
    }

    fn map_schema_object_scope(
        &self,
        kind: ObjectKind,
        _source_scope: &str,
        _target_scope: &str,
        definition: &str,
        dependencies: &[SchemaObjectScopeDependency],
    ) -> Result<Option<SchemaObjectScopeMapping>, String> {
        if kind != ObjectKind::View {
            return Ok(None);
        }
        Ok(Some(SchemaObjectScopeMapping {
            definition: definition.to_owned(),
            dependencies: dependencies
                .iter()
                .map(|dependency| dependency.target.clone())
                .collect(),
        }))
    }
}

fn db_object(kind: ObjectKind, schema: &str, name: &str) -> DatabaseObject {
    DatabaseObject {
        kind: kind.as_str().into(),
        schema: Some(schema.into()),
        name: name.into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

fn plan_view(
    source: SchemaObjectSnapshot,
    source_scope: Option<&str>,
    target_catalog: &[SchemaObjectIdentity],
) -> SchemaDiffPlan {
    plan_view_with_target_objects(source, source_scope, "target_db", &[], target_catalog)
}

fn plan_view_in_scope(
    source: SchemaObjectSnapshot,
    source_scope: Option<&str>,
    target_scope: &str,
    target_catalog: &[SchemaObjectIdentity],
) -> SchemaDiffPlan {
    plan_view_with_target_objects(source, source_scope, target_scope, &[], target_catalog)
}

fn plan_view_with_target_objects(
    source: SchemaObjectSnapshot,
    source_scope: Option<&str>,
    target_scope: &str,
    target_objects: &[SchemaObjectSnapshot],
    target_catalog: &[SchemaObjectIdentity],
) -> SchemaDiffPlan {
    build_unified_schema_diff_plan_with_source_scope(
        &[],
        &[],
        &[source],
        target_objects,
        &[],
        &[],
        target_catalog,
        true,
        &[],
        "mysql",
        "mysql",
        None,
        Some(target_scope),
        false,
        true,
        &[],
        &MappingRenderer,
        &TestCapabilities,
        source_scope,
        Some(&MySqlViewScopeContext {
            current_user: "migrator@localhost".into(),
            character_set_client: "utf8mb4".into(),
            collation_connection: "utf8mb4_0900_ai_ci".into(),
        }),
    )
}

fn scoped_view(schema: &str, name: &str, definition: &str) -> SchemaObjectSnapshot {
    SchemaObjectSnapshot::view(Some(schema), name, definition).with_mysql_view_metadata(
        MySqlViewMetadata {
            algorithm: "UNDEFINED".into(),
            definer: "migrator@localhost".into(),
            security_type: "DEFINER".into(),
            check_option: "NONE".into(),
            character_set_client: "utf8mb4".into(),
            collation_connection: "utf8mb4_0900_ai_ci".into(),
            has_explicit_column_list: false,
        },
    )
}

#[test]
fn mysql_view_scope_mapping_enters_the_reviewed_dependency_plan() {
    let source_table = SchemaObjectIdentity::table(Some("source_db"), "items");
    let target_table = SchemaObjectIdentity::table(Some("target_db"), "items");
    let view = scoped_view("source_db", "item_view", "SELECT id FROM items")
        .with_dependencies(vec![source_table]);

    let plan = plan_view(view, Some("source_db"), &[target_table]);

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 1);
    assert!(plan.statements[0].sql.contains("view+item_view"));
}

#[test]
fn mapped_missing_dependency_reaches_the_exact_dependency_blocker() {
    let view =
        scoped_view("source_db", "item_view", "SELECT id FROM items").with_dependencies(vec![
            SchemaObjectIdentity::table(Some("source_db"), "missing_items"),
        ]);

    let plan = plan_view(view.clone(), Some("source_db"), &[]);

    assert!(plan.statements.is_empty());
    let requirement_text = format!("{:?}", plan.requirements);
    assert!(requirement_text.contains("target_db.missing_items"));
    assert!(requirement_text.contains("not present in the target snapshot"));
    assert!(!requirement_text.contains("Object DDL is not rewritten"));
}

#[test]
fn source_identity_outside_configured_scope_is_blocked_before_mapping() {
    let view =
        scoped_view("other_db", "item_view", "SELECT id FROM items").with_dependencies(vec![]);

    let plan = plan_view(view, Some("source_db"), &[]);

    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements).contains("does not match configured source scope"));
}

#[test]
fn driver_mapping_cannot_be_used_for_other_cross_scope_object_kinds() {
    let routine = SchemaObjectSnapshot::routine(
        ObjectKind::Function,
        Some("source_db"),
        "calculate",
        None,
        "CREATE FUNCTION calculate() RETURNS INT RETURN 1",
    )
    .with_dependencies(vec![]);

    let plan = plan_view(routine, Some("source_db"), &[]);

    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements).contains("only proven for MySQL views"));
}

#[test]
fn schema_scope_mapper_pairs_preserve_external_dependency_identities() {
    let local = db_object(ObjectKind::Table, "source_db", "items");
    let local_target = db_object(ObjectKind::Table, "target_db", "items");
    let external = db_object(ObjectKind::View, "archive_db", "archived_items");
    let dependencies = vec![
        SchemaObjectScopeDependency {
            source: local,
            target: local_target,
        },
        SchemaObjectScopeDependency {
            source: external.clone(),
            target: external,
        },
    ];
    let mapped = MappingRenderer
        .map_schema_object_scope(
            ObjectKind::View,
            "source_db",
            "target_db",
            "SELECT id FROM items",
            &dependencies,
        )
        .unwrap()
        .unwrap();

    assert_eq!(mapped.dependencies[0].schema.as_deref(), Some("target_db"));
    assert_eq!(mapped.dependencies[1].schema.as_deref(), Some("archive_db"));
}

#[test]
fn cross_scope_view_requires_preserved_default_creation_semantics() {
    let view = SchemaObjectSnapshot::view(Some("source_db"), "item_view", "SELECT 1")
        .with_dependencies(vec![]);
    let plan = plan_view(view.clone(), Some("source_db"), &[]);
    assert!(plan.statements.is_empty());
    assert!(
        format!("{:?}", plan.requirements).contains("did not provide MySQL view creation metadata")
    );
    let same_scope = plan_view_in_scope(view.clone(), Some("source_db"), "source_db", &[]);
    assert!(same_scope.statements.is_empty());
    assert!(format!("{:?}", same_scope.requirements)
        .contains("did not provide MySQL view creation metadata"));

    let altered = scoped_view("source_db", "item_view", "SELECT 1")
        .with_dependencies(vec![])
        .with_mysql_view_metadata(MySqlViewMetadata {
            algorithm: "UNDEFINED".into(),
            check_option: "CASCADED".into(),
            has_explicit_column_list: false,
            ..MySqlViewMetadata {
                algorithm: "UNDEFINED".into(),
                definer: "migrator@localhost".into(),
                security_type: "DEFINER".into(),
                check_option: "NONE".into(),
                character_set_client: "utf8mb4".into(),
                collation_connection: "utf8mb4_0900_ai_ci".into(),
                has_explicit_column_list: false,
            }
        });
    let plan = plan_view(altered, Some("source_db"), &[]);
    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements).contains("non-default creation semantics"));
}

#[test]
fn view_algorithm_column_list_and_target_context_must_match_renderer_defaults() {
    let source = scoped_view("source_db", "item_view", "SELECT 1").with_dependencies(vec![]);
    for metadata in [
        MySqlViewMetadata {
            algorithm: "MERGE".into(),
            ..source.mysql_view_metadata.clone().unwrap()
        },
        MySqlViewMetadata {
            has_explicit_column_list: true,
            ..source.mysql_view_metadata.clone().unwrap()
        },
    ] {
        let plan = plan_view(
            source.clone().with_mysql_view_metadata(metadata),
            Some("source_db"),
            &[],
        );
        assert!(plan.statements.is_empty());
        assert!(format!("{:?}", plan.requirements).contains("non-default creation semantics"));
    }

    let definer_mismatch = source.clone().with_mysql_view_metadata(MySqlViewMetadata {
        definer: "different@localhost".into(),
        ..source.mysql_view_metadata.clone().unwrap()
    });
    let plan = plan_view(
        definer_mismatch,
        Some("source_db"),
        &[SchemaObjectIdentity::table(Some("target_db"), "items")],
    );
    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements)
        .contains("MySQL view definer differs from the target connection user"));
}

#[test]
fn target_view_with_unpreserved_metadata_blocks_create_and_rollback_rendering() {
    let source = scoped_view("source_db", "item_view", "SELECT 1").with_dependencies(vec![]);
    let target = SchemaObjectSnapshot::view(Some("target_db"), "item_view", "SELECT 0")
        .with_dependencies(vec![])
        .with_mysql_view_metadata(MySqlViewMetadata {
            check_option: "CASCADED".into(),
            ..source.mysql_view_metadata.clone().unwrap()
        });

    let plan =
        plan_view_with_target_objects(source, Some("source_db"), "target_db", &[target], &[]);

    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements).contains("target_db.item_view"));
    assert!(format!("{:?}", plan.requirements).contains("non-default creation semantics"));
}

#[test]
fn sqlserver_object_scope_uses_schema_and_rejects_cross_schema_mapping() {
    let view = SchemaObjectSnapshot::view(
        Some("dbo"),
        "item_view",
        "CREATE VIEW [dbo].[item_view] AS SELECT 1 AS value",
    )
    .with_dependencies(vec![]);
    let plan = build_unified_schema_diff_plan_with_source_scope(
        &[],
        &[],
        &[view.clone()],
        &[],
        &[],
        &[],
        &[],
        true,
        &[],
        "sqlserver",
        "sqlserver",
        Some("dbo"),
        Some("target_database"),
        false,
        true,
        &[],
        &MappingRenderer,
        &TestCapabilities,
        Some("dbo"),
        None,
    );
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 1);

    let cross_schema = build_unified_schema_diff_plan_with_source_scope(
        &[],
        &[],
        &[view],
        &[],
        &[],
        &[],
        &[],
        true,
        &[],
        "sqlserver",
        "sqlserver",
        Some("sales"),
        Some("target_database"),
        false,
        true,
        &[],
        &MappingRenderer,
        &TestCapabilities,
        Some("dbo"),
        None,
    );
    assert!(cross_schema.statements.is_empty());
    assert!(format!("{:?}", cross_schema.requirements).contains("Cross-scope migration"));
}
