//! T036: real assertions for the three ported `lib/structureEditor` `.ts`
//! helpers. Every branch and edge case from the React source and its vitest
//! suites is covered here (no rendering, pure value logic).

use crate::structure_editor::{
    build_structure_change_request::{
        build_structure_change_request, BuildStructureChangeRequestParams,
    },
    can_open_structure_editor::can_open_structure_editor,
    control_hints::{
        cap_enabled, control_disabled_key, is_control_enabled, CONTROL_DISABLED_KEY,
    },
    ConnectionView, DatabaseTypeMeta, StructureCapabilities, StructureCapabilityFlag,
    StructureChangeMode, StructureColumnDraft, StructureEditorUiConfig, StructureIndexDraft,
};

// --- fixtures -------------------------------------------------------------

fn column(id: &str, name: &str) -> StructureColumnDraft {
    StructureColumnDraft {
        id: id.into(),
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

fn index(id: &str, name: &str) -> StructureIndexDraft {
    StructureIndexDraft {
        id: id.into(),
        name: name.into(),
        columns: vec!["email".into()],
        is_unique: true,
        is_primary: false,
        index_type: Some("btree".into()),
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

/// Independent wiring: set only `flag` to true. Mirrors the React field names
/// without reusing the production match, so a swapped mapping is detectable.
fn caps_with_only(flag: StructureCapabilityFlag) -> StructureCapabilities {
    let mut caps = StructureCapabilities::default();
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
    caps
}

// --- buildStructureChangeRequest -----------------------------------------

#[test]
fn build_omits_whitespace_only_column_names_but_keeps_original_text() {
    let mut p = params(StructureChangeMode::Create);
    p.current_columns = vec![column("c1", "id"), column("c2", "  "), column("c3", " \t ")];
    let request = build_structure_change_request(p);

    assert_eq!(request.current_columns.len(), 1);
    assert_eq!(request.current_columns[0].id, "c1");
    assert_eq!(request.current_columns[0].name, "id");
}

#[test]
fn build_keeps_a_draft_whose_name_has_surrounding_whitespace_untrimmed() {
    let mut p = params(StructureChangeMode::Alter);
    p.current_columns = vec![column("c1", "  id  ")];
    let request = build_structure_change_request(p);

    // `.filter((c) => c.name.trim())` tests truthiness; it does not rewrite.
    assert_eq!(request.current_columns.len(), 1);
    assert_eq!(request.current_columns[0].name, "  id  ");
}

#[test]
fn build_trims_the_table_name() {
    let mut p = params(StructureChangeMode::Create);
    p.table = "  users \t\n".into();
    let request = build_structure_change_request(p);

    assert_eq!(request.table, "users");
}

#[test]
fn build_clears_original_snapshots_in_create_mode() {
    let orig_col = column("o1", "old");
    let orig_idx = index("i1", "idx_old");
    let mut p = params(StructureChangeMode::Create);
    p.original_columns = vec![orig_col.clone()];
    p.current_columns = vec![column("c1", "id")];
    p.original_indexes = vec![orig_idx];
    p.current_indexes = vec![index("i2", "idx_new")];
    let request = build_structure_change_request(p);

    assert!(request.original_columns.is_empty());
    assert!(request.original_indexes.is_empty());
    assert_eq!(request.current_columns.len(), 1);
    assert_eq!(request.current_indexes.len(), 1);
    assert_eq!(request.current_indexes[0].name, "idx_new");
}

#[test]
fn build_preserves_original_snapshots_verbatim_in_alter_mode() {
    let orig_col = column("c1", "email");
    let orig_idx = index("i1", "idx_email");
    let mut changed = orig_col.clone();
    changed.data_type = "varchar(255)".into();

    let mut p = params(StructureChangeMode::Alter);
    p.schema = Some("public".into());
    p.original_columns = vec![orig_col.clone()];
    p.current_columns = vec![changed.clone()];
    p.original_indexes = vec![orig_idx.clone()];
    p.current_indexes = vec![orig_idx.clone()];
    let request = build_structure_change_request(p);

    assert_eq!(request.mode, StructureChangeMode::Alter);
    assert_eq!(request.original_columns, vec![orig_col]);
    assert_eq!(request.original_indexes, vec![orig_idx]);
    assert_eq!(request.current_columns, vec![changed]);
    assert_eq!(request.schema.as_deref(), Some("public"));
}

#[test]
fn build_omits_whitespace_only_index_names_in_both_modes() {
    for mode in [StructureChangeMode::Create, StructureChangeMode::Alter] {
        let mut p = params(mode);
        p.current_indexes = vec![index("i1", "idx_email"), index("i2", "   ")];
        let request = build_structure_change_request(p);

        assert_eq!(request.current_indexes.len(), 1, "mode: {mode:?}");
        assert_eq!(request.current_indexes[0].name, "idx_email");
    }
}

#[test]
fn build_passes_schema_through_or_leaves_it_null() {
    let mut p = params(StructureChangeMode::Create);
    p.schema = Some("app".into());
    assert_eq!(
        build_structure_change_request(p).schema.as_deref(),
        Some("app")
    );

    let p = params(StructureChangeMode::Create);
    assert_eq!(build_structure_change_request(p).schema, None);
}

#[test]
fn build_of_empty_inputs_yields_empty_arrays_and_trimmed_empty_table() {
    let mut p = params(StructureChangeMode::Create);
    p.table = "   ".into();
    let request = build_structure_change_request(p);

    assert_eq!(request.table, "");
    assert!(request.current_columns.is_empty());
    assert!(request.current_indexes.is_empty());
    assert!(request.original_columns.is_empty());
    assert!(request.original_indexes.is_empty());
}

// --- canOpenStructureEditor ------------------------------------------------

#[test]
fn can_open_true_for_a_sql_table_driver() {
    assert!(can_open_structure_editor(Some(&sql_meta())));
}

#[test]
fn can_open_false_for_absent_meta() {
    assert!(!can_open_structure_editor(None));
}

#[test]
fn can_open_false_for_key_value_driver() {
    let mut meta = sql_meta();
    meta.is_key_value = true;
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn can_open_false_for_document_connection_view() {
    let mut meta = sql_meta();
    meta.connection_view = ConnectionView::Document;
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn can_open_false_when_structure_editor_is_explicitly_disabled() {
    let mut meta = sql_meta();
    meta.structure_editor = Some(StructureEditorUiConfig {
        enabled: Some(false),
    });
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn can_open_true_when_structure_editor_is_absent_or_enabled_unspecified() {
    let absent = sql_meta();
    assert!(can_open_structure_editor(Some(&absent)));

    let no_enabled = DatabaseTypeMeta {
        structure_editor: Some(StructureEditorUiConfig { enabled: None }),
        ..sql_meta()
    };
    assert!(can_open_structure_editor(Some(&no_enabled)));

    let enabled = DatabaseTypeMeta {
        structure_editor: Some(StructureEditorUiConfig {
            enabled: Some(true),
        }),
        ..sql_meta()
    };
    assert!(can_open_structure_editor(Some(&enabled)));
}

#[test]
fn can_open_false_when_supports_sql_is_false() {
    let meta = DatabaseTypeMeta {
        supports_sql: false,
        ..sql_meta()
    };
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn can_open_rejects_combinations_even_when_later_checks_would_pass() {
    // key-value + document + disabled + no SQL: still false (branch order).
    let meta = DatabaseTypeMeta {
        is_key_value: true,
        connection_view: ConnectionView::Document,
        structure_editor: Some(StructureEditorUiConfig {
            enabled: Some(false),
        }),
        supports_sql: false,
    };
    assert!(!can_open_structure_editor(Some(&meta)));
}

#[test]
fn can_open_keyvalue_view_without_the_flag_is_not_rejected_by_the_view_check() {
    // The React guard tests only `connectionView === 'document'`, not
    // `'keyvalue'`; a driver using the KV view without `isKeyValue` still opens.
    let meta = DatabaseTypeMeta {
        connection_view: ConnectionView::Keyvalue,
        ..sql_meta()
    };
    assert!(can_open_structure_editor(Some(&meta)));
}

// --- controlHints / isControlEnabled --------------------------------------

#[test]
fn disabled_key_is_the_same_literal_for_every_flag() {
    for flag in ALL_FLAGS {
        assert_eq!(control_disabled_key(flag), "structEditor.capDisabled");
        assert_eq!(control_disabled_key(flag), CONTROL_DISABLED_KEY);
    }
}

#[test]
fn cap_enabled_fails_closed_for_null_capabilities() {
    for flag in ALL_FLAGS {
        assert!(!cap_enabled(None, flag), "flag: {flag:?}");
    }
}

#[test]
fn is_control_enabled_reads_false_and_true_from_the_field() {
    let caps = StructureCapabilities::default();
    assert!(!is_control_enabled(&caps, StructureCapabilityFlag::RenameColumn));

    let caps = caps_with_only(StructureCapabilityFlag::RenameColumn);
    assert!(is_control_enabled(&caps, StructureCapabilityFlag::RenameColumn));
}

#[test]
fn each_flag_reads_its_own_field_and_no_other() {
    for enabled_flag in ALL_FLAGS {
        let caps = caps_with_only(enabled_flag);
        for queried_flag in ALL_FLAGS {
            let expected = enabled_flag == queried_flag;
            assert_eq!(
                cap_enabled(Some(&caps), queried_flag),
                expected,
                "enabled {enabled_flag:?}, queried {queried_flag:?}"
            );
        }
    }
}

#[test]
fn non_boolean_capability_fields_are_not_control_flags() {
    // AlterStrategy/dialectId/indexMethods exist on the VM but cannot be
    // selected by StructureCapabilityFlag, so they can never enable a control.
    let mut caps = StructureCapabilities::default();
    caps.dialect_id = "pg".into();
    caps.index_methods = vec!["btree".into()];
    caps.alter_strategy = crate::structure_editor::AlterStrategy::SqliteRebuild;
    for flag in ALL_FLAGS {
        assert!(!cap_enabled(Some(&caps), flag));
    }
}
