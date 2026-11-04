use super::*;

#[test]
fn exact_view_catalog_self_dependency_fails_closed() {
    let view_identity = SchemaObjectIdentity {
        kind: ObjectKind::View,
        schema: Some("public".into()),
        name: "recursive_view".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let view = SchemaObjectSnapshot::view(
        Some("public"),
        "recursive_view",
        "SELECT id FROM source_rows",
    )
    .with_dependencies(vec![view_identity]);

    let plan = build(
        &[],
        &[],
        &[view],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );

    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements).contains("depends on itself"));
}
