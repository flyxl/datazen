use super::*;
use datazen_driver_api::ColumnSchema;

fn col(name: &str, ty: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: ty.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: cols,
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

#[test]
fn test_tester_target_only_unknown_driver_is_not_executable() {
    let src = schema(vec![]);
    let tgt = schema(vec![col("id", "integer")]);
    let plan = build_schema_diff_plan(
        &[("archive".into(), src, tgt)],
        "postgresql",
        "unknown-driver",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| {
        matches!(
            requirement,
            super::super::types::PlanRequirement::Unsupported { reason, .. }
            if reason.contains("No registered driver")
        )
    }));
}

#[test]
fn is_type_narrowing_unit_cases() {
    let snap = |ty: &str| ColumnSnapshot {
        name: "c".into(),
        data_type: ty.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    };
    assert!(is_type_narrowing(
        &snap("varchar(100)"),
        &snap("varchar(255)")
    ));
    assert!(!is_type_narrowing(
        &snap("varchar(255)"),
        &snap("varchar(100)")
    ));
    assert!(is_type_narrowing(&snap("smallint"), &snap("int")));
    assert!(!is_type_narrowing(&snap("int"), &snap("smallint")));
    assert!(!is_type_narrowing(&snap("json"), &snap("text")));
    assert!(!is_type_narrowing(
        &snap("varchar(100)"),
        &snap("varchar(255x)")
    ));
}
