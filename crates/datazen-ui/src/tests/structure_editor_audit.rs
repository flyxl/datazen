//! T036 independent audit: extra coverage-driven edge cases for the three
//! ported `lib/structureEditor` `.ts` helpers, on top of the coder's
//! `structure_editor_helpers.rs`. Pure value logic — no Rune tree or state.

use crate::structure_editor::{
    build_structure_change_request::{
        build_structure_change_request, BuildStructureChangeRequestParams,
    },
    can_open_structure_editor::can_open_structure_editor,
    control_hints::{cap_enabled, control_disabled_key},
    ConnectionView, DatabaseTypeMeta, StructureCapabilities, StructureCapabilityFlag,
    StructureChangeMode, StructureColumnDraft, StructureEditorUiConfig, StructureIndexDraft,
};

fn column(name: &str) -> StructureColumnDraft {
    StructureColumnDraft {
        id: format!("col-{name}"),
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
        is_unique: false,
    }
}

fn index(name: &str) -> StructureIndexDraft {
    StructureIndexDraft {
        id: format!("idx-{name}"),
        name: name.into(),
        columns: vec!["email".into()],
        is_unique: false,
        is_primary: false,
        index_type: None,
        include_columns: None,
        filter: None,
        comment: None,
    }
}

fn params(mode: StructureChangeMode) -> BuildStructureChangeRequestParams {
    BuildStructureChangeRequestParams {
        mode,
        table: "users".into(),
        schema: None,
        original_columns: Vec::new(),
        current_columns: Vec::new(),
        original_indexes: Vec::new(),
        current_indexes: Vec::new(),
    }
}

fn sql_meta() -> DatabaseTypeMeta {
    DatabaseTypeMeta {
        is_key_value: false,
        connection_view: ConnectionView::Sql,
        structure_editor: None,
        supports_sql: true,
    }
}

const ALL_FLAGS: [StructureCapabilityFlag; 17] = [
    StructureCapabilityFlag::CreateTable,
    StructureCapabilityFlag::AddColumn,
    StructureCapabilityFlag::DropColumn,
    StructureCapabilityFlag::RenameColumn,
    StructureCapabilityFlag::AlterType,
    StructureCapabilityFlag::AlterNullability,
    StructureCapabilityFlag::AlterDefault,
    StructureCapabilityFlag::AlterPrimaryKey,
    StructureCapabilityFlag::ReorderColumn,
    StructureCapabilityFlag::Comment,
    StructureCapabilityFlag::CreateIndex,
    StructureCapabilityFlag::DropIndex,
    StructureCapabilityFlag::RebuildIndex,
    StructureCapabilityFlag::IndexType,
    StructureCapabilityFlag::IndexInclude,
    StructureCapabilityFlag::IndexFilter,
    StructureCapabilityFlag::IndexComment,
];

// --- buildStructureChangeRequest edge cases -------------------------------

#[test]
fn schema_empty_string_survives_the_nullish_coalesce() {
    // `schema ?? null` only collapses undefined/null; "" stays "".
    let mut p = params(StructureChangeMode::Create);
    p.schema = Some(String::new());
    assert_eq!(build_structure_change_request(p).schema.as_deref(), Some(""));

    let p = params(StructureChangeMode::Create);
    assert_eq!(build_structure_change_request(p).schema, None);
}

#[test]
fn schema_is_passed_through_untrimmed_while_the_table_is_trimmed() {
    let mut p = params(StructureChangeMode::Alter);
    p.table = "  users \t".into();
    p.schema = Some("  public  ".into());
    let request = build_structure_change_request(p);

    assert_eq!(request.table, "users");
    assert_eq!(request.schema.as_deref(), Some("  public  "));
}

#[test]
fn original_snapshots_keep_blank_names_in_alter_mode() {
    // TS copies `params.original*` verbatim; only `current*` is filtered.
    let mut p = params(StructureChangeMode::Alter);
    p.original_columns = vec![column(""), column("  ")];
    p.original_indexes = vec![index("   ")];
    let request = build_structure_change_request(p);

    assert_eq!(request.original_columns, vec![column(""), column("  ")]);
    assert_eq!(request.original_indexes, vec![index("   ")]);
}

#[test]
fn whitespace_name_variants_are_dropped_but_a_one_char_name_survives() {
    let mut p = params(StructureChangeMode::Create);
    p.current_columns = vec![
        column(""),
        column(" "),
        column("\t"),
        column("\n\r "),
        column("\u{00a0}"),
        column("x"),
    ];
    let request = build_structure_change_request(p);

    assert_eq!(request.current_columns.len(), 1);
    assert_eq!(request.current_columns[0].name, "x");
}

// --- canOpenStructureEditor edge cases ------------------------------------

#[test]
fn disabled_structure_editor_is_rejected_even_with_sql_support() {
    let meta = DatabaseTypeMeta {
        structure_editor: Some(StructureEditorUiConfig {
            enabled: Some(false),
        }),
        ..sql_meta()
    };
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn document_view_alone_rejects_and_keyvalue_view_alone_does_not() {
    let document = DatabaseTypeMeta {
        connection_view: ConnectionView::Document,
        ..sql_meta()
    };
    assert!(!can_open_structure_editor(Some(&document)));

    let keyvalue = DatabaseTypeMeta {
        connection_view: ConnectionView::Keyvalue,
        ..sql_meta()
    };
    assert!(can_open_structure_editor(Some(&keyvalue)));
}

#[test]
fn enabled_true_without_sql_support_is_still_rejected() {
    let meta = DatabaseTypeMeta {
        structure_editor: Some(StructureEditorUiConfig {
            enabled: Some(true),
        }),
        supports_sql: false,
        ..sql_meta()
    };
    assert!(!can_open_structure_editor(Some(&meta)));
}

// --- controlHints aggregate guards ----------------------------------------

#[test]
fn every_flag_maps_to_a_distinct_boolean_field() {
    // With all 17 flags true, every control must read true; this only passes
    // if each flag points at its own field and none is wired to a non-flag.
    let mut caps = StructureCapabilities::default();
    for flag in ALL_FLAGS {
        match flag {
            StructureCapabilityFlag::CreateTable => caps.create_table = true,
            StructureCapabilityFlag::AddColumn => caps.add_column = true,
            StructureCapabilityFlag::DropColumn => caps.drop_column = true,
            StructureCapabilityFlag::RenameColumn => caps.rename_column = true,
            StructureCapabilityFlag::AlterType => caps.alter_type = true,
            StructureCapabilityFlag::AlterNullability => caps.alter_nullability = true,
            StructureCapabilityFlag::AlterDefault => caps.alter_default = true,
            StructureCapabilityFlag::AlterPrimaryKey => caps.alter_primary_key = true,
            StructureCapabilityFlag::ReorderColumn => caps.reorder_column = true,
            StructureCapabilityFlag::Comment => caps.comment = true,
            StructureCapabilityFlag::CreateIndex => caps.create_index = true,
            StructureCapabilityFlag::DropIndex => caps.drop_index = true,
            StructureCapabilityFlag::RebuildIndex => caps.rebuild_index = true,
            StructureCapabilityFlag::IndexType => caps.index_type = true,
            StructureCapabilityFlag::IndexInclude => caps.index_include = true,
            StructureCapabilityFlag::IndexFilter => caps.index_filter = true,
            StructureCapabilityFlag::IndexComment => caps.index_comment = true,
        }
    }
    for flag in ALL_FLAGS {
        assert!(cap_enabled(Some(&caps), flag), "flag: {flag:?}");
        assert_eq!(control_disabled_key(flag), "structEditor.capDisabled");
    }
}

#[test]
fn non_boolean_fields_never_enable_a_control() {
    let mut caps = StructureCapabilities::default();
    caps.dialect_id = "postgres".into();
    caps.index_methods = vec!["btree".into()];
    caps.alter_strategy = crate::structure_editor::AlterStrategy::Direct;
    for flag in ALL_FLAGS {
        assert!(!cap_enabled(Some(&caps), flag), "flag: {flag:?}");
    }
}
