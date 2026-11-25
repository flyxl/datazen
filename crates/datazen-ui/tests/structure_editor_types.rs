//! T156: contract verification for React `lib/structureEditor/types.ts`.
//!
//! Pure value tests: every exported variant/field is constructed and round-
//! tripped through the Rust mirror, the 17 boolean capability keys are pinned
//! to their exact camelCase names, and the non-boolean capability fields are
//! proven absent from the `StructureCapabilityFlag` mapped type.

use datazen_ui::structure_editor::control_hints::is_control_enabled;
use datazen_ui::structure_editor::types::{
    ExtensionStructureEditorUiConfig, StructureEditorColumnTypeOption, StructureEditorFields,
};
use datazen_ui::structure_editor::{
    AlterStrategy, PlanStatement, StatementRisk, StructureCapabilities, StructureCapabilityFlag,
    StructureChangeMode, StructureChangePlan, StructureChangeRequest, StructureColumnDraft,
    StructureIndexDraft,
};

#[test]
fn alter_strategy_round_trips_all_react_string_values() {
    let expected = [
        (AlterStrategy::None, "none"),
        (AlterStrategy::Direct, "direct"),
        (AlterStrategy::SqliteRebuild, "sqliteRebuild"),
    ];
    assert_eq!(AlterStrategy::ALL.len(), 3);
    for (index, (variant, literal)) in expected.into_iter().enumerate() {
        assert_eq!(AlterStrategy::ALL[index], variant, "source order at {index}");
        assert_eq!(variant.as_str(), literal, "React literal for {variant:?}");
        assert_eq!(AlterStrategy::from_str(literal), Some(variant), "parse {literal}");
        assert_eq!(
            AlterStrategy::from_str(&literal.to_uppercase()),
            None,
            "the parse is case-sensitive like the TS union"
        );
    }
    assert_eq!(AlterStrategy::from_str("rebuild"), None);
    assert_eq!(AlterStrategy::default(), AlterStrategy::None);
}

#[test]
fn structure_change_mode_round_trips_both_react_values() {
    assert_eq!(StructureChangeMode::ALL.len(), 2);
    for (index, (variant, literal)) in [
        (StructureChangeMode::Create, "create"),
        (StructureChangeMode::Alter, "alter"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(StructureChangeMode::ALL[index], variant);
        assert_eq!(variant.as_str(), literal, "React literal for {variant:?}");
        assert_eq!(StructureChangeMode::from_str(literal), Some(variant));
    }
    assert_eq!(StructureChangeMode::from_str("CREATE"), None);
    assert_eq!(StructureChangeMode::from_str("drop"), None);
}

#[test]
fn statement_risk_round_trips_all_three_react_values() {
    for (variant, literal) in [
        (StatementRisk::Additive, "additive"),
        (StatementRisk::Destructive, "destructive"),
        (StatementRisk::Rewrite, "rewrite"),
    ] {
        assert_eq!(variant.as_str(), literal);
        assert_eq!(StatementRisk::from_str(literal), Some(variant));
        assert_eq!(StatementRisk::from_str(literal).map(|v| v.as_str()), Some(literal));
    }
    assert_eq!(StatementRisk::from_str("risky"), None);
    assert!(StatementRisk::Destructive.is_risky());
    assert!(!StatementRisk::Additive.is_risky());
}

#[test]
fn the_seventeen_capability_flags_are_exactly_the_boolean_keys() {
    let expected = [
        "createTable",
        "addColumn",
        "dropColumn",
        "renameColumn",
        "alterType",
        "alterNullability",
        "alterDefault",
        "alterPrimaryKey",
        "reorderColumn",
        "comment",
        "createIndex",
        "dropIndex",
        "rebuildIndex",
        "indexType",
        "indexInclude",
        "indexFilter",
        "indexComment",
    ];
    assert_eq!(StructureCapabilityFlag::ALL.len(), 17, "React has 17 boolean flags");
    assert_eq!(expected.len(), 17);
    for (index, (flag, key)) in StructureCapabilityFlag::ALL.into_iter().zip(expected).enumerate() {
        assert_eq!(flag.as_str(), key, "camelCase key at {index}");
        assert_eq!(StructureCapabilityFlag::from_str(key), Some(flag), "parse {key}");
    }
    // The three non-boolean StructureCapabilities keys are not flags.
    for key in ["alterStrategy", "dialectId", "indexMethods", "create_table", ""] {
        assert_eq!(StructureCapabilityFlag::from_str(key), None, "not a flag: {key}");
    }
}

#[test]
fn a_flag_reads_only_its_own_boolean_field() {
    // For every flag: exactly that one boolean field is reachable through
    // is_control_enabled; the three non-boolean fields never change the read.
    for enabled in StructureCapabilityFlag::ALL {
        let mut caps = StructureCapabilities::default();
        set_flag(&mut caps, enabled, true);
        assert!(is_control_enabled(&caps, enabled), "{:?} reads its own field", enabled);
        for other in StructureCapabilityFlag::ALL {
            let reads = is_control_enabled(&caps, other);
            assert_eq!(reads, other == enabled, "{enabled:?} must not enable {other:?}");
        }
        caps.alter_strategy = AlterStrategy::SqliteRebuild;
        caps.dialect_id = "sqlite".to_owned();
        caps.index_methods = vec!["btree".to_owned()];
        assert!(
            is_control_enabled(&caps, enabled),
            "non-boolean fields cannot turn a flag off"
        );
        set_flag(&mut caps, enabled, false);
        assert!(!is_control_enabled(&caps, enabled), "a false field reads false");
    }
}

fn set_flag(caps: &mut StructureCapabilities, flag: StructureCapabilityFlag, value: bool) {
    match flag {
        StructureCapabilityFlag::CreateTable => caps.create_table = value,
        StructureCapabilityFlag::AddColumn => caps.add_column = value,
        StructureCapabilityFlag::DropColumn => caps.drop_column = value,
        StructureCapabilityFlag::RenameColumn => caps.rename_column = value,
        StructureCapabilityFlag::AlterType => caps.alter_type = value,
        StructureCapabilityFlag::AlterNullability => caps.alter_nullability = value,
        StructureCapabilityFlag::AlterDefault => caps.alter_default = value,
        StructureCapabilityFlag::AlterPrimaryKey => caps.alter_primary_key = value,
        StructureCapabilityFlag::ReorderColumn => caps.reorder_column = value,
        StructureCapabilityFlag::Comment => caps.comment = value,
        StructureCapabilityFlag::CreateIndex => caps.create_index = value,
        StructureCapabilityFlag::DropIndex => caps.drop_index = value,
        StructureCapabilityFlag::RebuildIndex => caps.rebuild_index = value,
        StructureCapabilityFlag::IndexType => caps.index_type = value,
        StructureCapabilityFlag::IndexInclude => caps.index_include = value,
        StructureCapabilityFlag::IndexFilter => caps.index_filter = value,
        StructureCapabilityFlag::IndexComment => caps.index_comment = value,
    }
}

#[test]
fn every_contract_struct_constructs_with_all_fields_and_round_trips() {
    let column = StructureColumnDraft {
        id: "col-1".to_owned(),
        name: "email".to_owned(),
        data_type: "TEXT".to_owned(),
        nullable: false,
        default_value: Some("'n/a'".to_owned()),
        comment: Some("contact".to_owned()),
        is_primary_key: true,
        is_auto_increment: false,
        is_unique: true,
    };
    assert_eq!(column.clone(), column);
    assert_eq!(column.id, "col-1");
    assert_eq!(column.data_type, "TEXT");
    assert_eq!(column.default_value.as_deref(), Some("'n/a'"));
    assert!(column.is_primary_key && column.is_unique && !column.nullable);

    let index = StructureIndexDraft {
        id: "idx-1".to_owned(),
        name: "idx_email".to_owned(),
        columns: vec!["email".to_owned()],
        is_unique: true,
        is_primary: false,
        index_type: Some("btree".to_owned()),
        include_columns: Some(vec!["id".to_owned()]),
        filter: Some("email IS NOT NULL".to_owned()),
        comment: Some("lookup".to_owned()),
    };
    assert_eq!(index.clone(), index);
    assert_eq!(index.columns, vec!["email".to_owned()]);
    assert_eq!(index.index_type.as_deref(), Some("btree"));
    assert_eq!(index.include_columns.as_ref().map(|v| v.len()), Some(1));

    let request = StructureChangeRequest {
        mode: StructureChangeMode::Alter,
        schema: Some("public".to_owned()),
        table: "users".to_owned(),
        original_columns: vec![column.clone()],
        current_columns: vec![column.clone()],
        original_indexes: vec![index.clone()],
        current_indexes: vec![index.clone()],
    };
    assert_eq!(request.clone(), request);
    assert_eq!(request.mode, StructureChangeMode::Alter);
    assert_eq!(request.schema.as_deref(), Some("public"));
    assert_eq!(request.original_columns.len(), 1);
    assert_eq!(request.current_indexes.len(), 1);

    let statement = PlanStatement {
        sql: "ALTER TABLE users ADD COLUMN x INT".to_owned(),
        summary: "add x".to_owned(),
        risk: StatementRisk::Additive,
    };
    let plan = StructureChangePlan {
        statements: vec![statement.clone()],
        warnings: Some(vec!["verify".to_owned()]),
    };
    assert_eq!(plan.clone(), plan);
    assert_eq!(statement.risk.as_str(), "additive");
    assert_eq!(plan.statements[0].sql, statement.sql);
    assert_eq!(plan.warnings.as_ref().map(|w| w.len()), Some(1));

    let capabilities = StructureCapabilities {
        create_table: true,
        add_column: false,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: "sqlite".to_owned(),
        index_methods: vec!["btree".to_owned(), "hash".to_owned()],
        ..Default::default()
    };
    assert!(capabilities.create_table);
    assert!(is_control_enabled(&capabilities, StructureCapabilityFlag::CreateTable));
    assert!(!is_control_enabled(&capabilities, StructureCapabilityFlag::AddColumn));
    assert_eq!(capabilities.alter_strategy.as_str(), "direct");
    assert_eq!(capabilities.index_methods.len(), 2);
}

#[test]
fn the_reexported_ui_config_has_every_extension_points_field() {
    let full = ExtensionStructureEditorUiConfig {
        enabled: Some(true),
        column_types: vec![StructureEditorColumnTypeOption {
            value: "INT".to_owned(),
            label: "Integer".to_owned(),
        }],
        default_column_type: "TEXT".to_owned(),
        fields: StructureEditorFields {
            comment: Some(true),
            charset: Some(false),
            collation: None,
            unsigned: Some(true),
            length: None,
        },
        index_methods: vec!["btree".to_owned()],
    };
    assert_eq!(full.clone(), full);
    assert_eq!(full.enabled, Some(true));
    assert_eq!(full.column_types[0].value, "INT");
    assert_eq!(full.column_types[0].label, "Integer");
    assert_eq!(full.fields.comment, Some(true));
    assert_eq!(full.fields.charset, Some(false));
    assert_eq!(full.fields.unsigned, Some(true));
    assert!(full.fields.collation.is_none() && full.fields.length.is_none());
    assert_eq!(full.index_methods, vec!["btree".to_owned()]);

    let legacy = full.to_legacy();
    assert_eq!(legacy.enabled, Some(true));
    assert_eq!(legacy.default_column_type, "TEXT");
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Returning "sqlite_rebuild" from `AlterStrategy::as_str` for
//    `SqliteRebuild` made `alter_strategy_round_trips_all_react_string_values`
//    fail with: `left: "sqlite_rebuild" right: "sqliteRebuild"`.
// 2. Returning "indexCommentX" for `StructureCapabilityFlag::IndexComment`
//    made `the_seventeen_capability_flags_are_exactly_the_boolean_keys` fail
//    with: `camelCase key at 16 / left: "indexCommentX" right: "indexComment"`.
