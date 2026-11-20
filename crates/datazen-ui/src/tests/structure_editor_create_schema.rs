//! T124: real assertions for the three ported `lib/structureEditor` helpers.
//!
//! Pure value logic — no runtime, no rendering. Every branch of the React
//! sources and their vitest suites is pinned, including the no-meta fallback,
//! the `namespaceEnsure` variants, `??` precedence with empty strings, PK and
//! unique membership, and primary-index filtering.

use crate::relation_metadata::{ColumnSchema, IndexInfo, TableSchema};
use crate::structure_editor::{
    is_control_enabled::is_control_enabled,
    resolve_create_table_schema::{
        resolve_create_table_schema, resolve_create_table_schema_from_meta,
        CreateTableSchemaMeta, ResolveCreateTableSchemaParams,
    },
    schema_to_draft::schema_to_draft,
    StructureCapabilities, StructureCapabilityFlag,
};

// --- isControlEnabled ------------------------------------------------------

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

/// Test-only setter so each flag can be toggled independently.
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
fn is_control_enabled_returns_the_exact_boolean_for_every_flag() {
    // React: caps[control] === true. All-off must be false; turning one flag on
    // must enable exactly that flag and no other.
    let off = StructureCapabilities::default();
    for flag in ALL_FLAGS {
        assert!(
            !is_control_enabled(&off, flag),
            "{flag:?}: an absent/false cap must fail closed"
        );
    }

    for on_flag in ALL_FLAGS {
        let mut caps = StructureCapabilities::default();
        set_flag(&mut caps, on_flag, true);
        for flag in ALL_FLAGS {
            assert_eq!(
                is_control_enabled(&caps, flag),
                flag == on_flag,
                "{flag:?} must be {} when only {on_flag:?} is on",
                flag == on_flag
            );
        }
    }
}

// --- resolveCreateTableSchema ----------------------------------------------

#[test]
fn pg_namespace_never_uses_the_logical_database_name() {
    let meta = CreateTableSchemaMeta::new(Some("postgresql"), Some("name"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&meta),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), Some("public"))
        )
        .as_deref(),
        Some("public"),
        "contextSchema wins for a PG namespace"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&meta),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), None)
        )
        .as_deref(),
        Some("public"),
        "PG without contextSchema defaults to public"
    );
    assert_ne!(
        resolve_create_table_schema_from_meta(
            Some(&meta),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), None)
        )
        .as_deref(),
        Some("mydb"),
        "the logical database name is never a PG schema"
    );
}

#[test]
fn mysql_and_default_sql_use_the_database_name_and_ignore_context_schema() {
    let mysql = CreateTableSchemaMeta::new(Some("default-sql"), Some("name"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&mysql),
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some("app"), Some("ignored"))
        )
        .as_deref(),
        Some("app"),
        "default-sql uses currentDatabase, not contextSchema"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&mysql),
            "mysql",
            &ResolveCreateTableSchemaParams::new(None, Some("ignored"))
        ),
        None,
        "default-sql without a database is unqualified"
    );
}

#[test]
fn path_databases_are_unqualified() {
    let sqlite = CreateTableSchemaMeta::new(None, Some("path"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&sqlite),
            "sqlite",
            &ResolveCreateTableSchemaParams::new(Some("/tmp/test.db"), None)
        ),
        None,
        "a file path is never a schema qualifier"
    );
    let path_with_context = CreateTableSchemaMeta::new(Some("default-sql"), Some("path"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&path_with_context),
            "duckdb",
            &ResolveCreateTableSchemaParams::new(Some("file.db"), Some("shadow"))
        ),
        None,
        "path wins even when contextSchema is present"
    );
}

#[test]
fn absent_meta_branches_on_the_dialect_id() {
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), None)
        )
        .as_deref(),
        Some("public"),
        "no meta + postgresql defaults to public"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), Some("custom"))
        )
        .as_deref(),
        Some("custom"),
        "no meta + postgresql still honors contextSchema"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some("app"), Some("ctx"))
        )
        .as_deref(),
        Some("ctx"),
        "no meta + non-PG prefers contextSchema over currentDatabase"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "mysql",
            &ResolveCreateTableSchemaParams::new(None, None)
        ),
        None,
        "no meta + no database is unqualified"
    );
}

#[test]
fn namespace_ensure_falls_back_to_the_dialect_and_other_variants_are_not_pg() {
    let no_namespace = CreateTableSchemaMeta::new(None, Some("name"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&no_namespace),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), None)
        )
        .as_deref(),
        Some("public"),
        "absent namespaceEnsure falls back to postgresql for a PG dialect"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&no_namespace),
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some("app"), None)
        )
        .as_deref(),
        Some("app"),
        "absent namespaceEnsure falls back to default-sql otherwise"
    );
    let hierarchy = CreateTableSchemaMeta::new(Some("path-hierarchy"), Some("name"));
    assert_eq!(
        resolve_create_table_schema_from_meta(
            Some(&hierarchy),
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("app"), None)
        )
        .as_deref(),
        Some("app"),
        "path-hierarchy is not the postgresql branch"
    );
}

#[test]
fn nullish_coalescing_keeps_an_empty_string_as_a_value() {
    // JS `'' ?? 'public'` is `''`, not `'public'`.
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "postgresql",
            &ResolveCreateTableSchemaParams::new(Some("mydb"), Some(""))
        )
        .as_deref(),
        Some(""),
        "an empty contextSchema must survive the PG fallback"
    );
    assert_eq!(
        resolve_create_table_schema_from_meta(
            None,
            "mysql",
            &ResolveCreateTableSchemaParams::new(Some(""), Some("ctx"))
        )
        .as_deref(),
        Some("ctx"),
        "contextSchema takes precedence even over an empty currentDatabase"
    );
}

#[test]
fn resolve_create_table_schema_uses_the_injected_meta_lookup() {
    // The injected resolver stands in for DB_REGISTRY: it is called with the
    // dialect id, so a host never hardcodes one dialect's meta here.
    let mut asked = Vec::new();
    let result = resolve_create_table_schema(
        "custom-db",
        &ResolveCreateTableSchemaParams::new(Some("app"), None),
        |dialect| {
            asked.push(dialect.to_owned());
            if dialect == "custom-db" {
                Some(CreateTableSchemaMeta::new(Some("default-sql"), Some("name")))
            } else {
                None
            }
        },
    );
    assert_eq!(asked, ["custom-db"], "the registry lookup receives the dialect");
    assert_eq!(result.as_deref(), Some("app"));

    // An unknown dialect gets the React no-meta fallback, not a hardcoded one.
    let unknown = resolve_create_table_schema(
        "not-registered",
        &ResolveCreateTableSchemaParams::new(Some("app"), None),
        |_| None,
    );
    assert_eq!(unknown.as_deref(), Some("app"));
}

// --- schemaToDraft ---------------------------------------------------------

fn column(name: &str, auto_increment: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: "integer".into(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: auto_increment,
        comment: None,
    }
}

fn index(name: &str, columns: &[&str], unique: bool, primary: bool, method: Option<&str>) -> IndexInfo {
    IndexInfo {
        name: name.into(),
        columns: columns.iter().map(|c| (*c).into()).collect(),
        is_unique: unique,
        is_primary: primary,
        index_type: method.map(str::to_owned),
    }
}

#[test]
fn schema_to_draft_maps_columns_and_primary_key_membership_by_name() {
    let mut id_col = column("id", true);
    id_col.default_value = Some("nextval".into());
    id_col.comment = Some("the key".into());
    let mut other = column("email", false);
    other.nullable = false;
    other.data_type = "text".into();
    other.is_primary_key = true; // React ignores this field; primaryKeys wins.

    let schema = TableSchema {
        columns: vec![id_col, other],
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        ..Default::default()
    };
    let draft = schema_to_draft(&schema);

    assert_eq!(draft.columns.len(), 2);
    let id = &draft.columns[0];
    assert_eq!(id.name, "id");
    assert_eq!(id.data_type, "integer");
    assert!(id.nullable);
    assert_eq!(id.default_value.as_deref(), Some("nextval"));
    assert_eq!(id.comment.as_deref(), Some("the key"));
    assert!(id.is_primary_key, "primaryKeys membership marks the column");
    assert!(id.is_auto_increment);
    assert_eq!(id.id.len(), 10, "col_ prefix + 6 base-36 characters");
    assert!(id.id.starts_with("col_"));

    let email = &draft.columns[1];
    assert!(
        !email.is_primary_key,
        "ColumnSchema.is_primary_key is not the source of truth"
    );
    assert!(!email.nullable);
    assert_eq!(email.data_type, "text");
    assert_eq!(email.default_value, None);
    assert_eq!(email.comment, None);
    assert_ne!(id.id, email.id, "draft ids are distinct");
}

#[test]
fn schema_to_draft_marks_only_single_column_nonprimary_unique_indexes() {
    let schema = TableSchema {
        columns: vec![
            column("id", false),
            column("email", false),
            column("first", false),
            column("last", false),
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![
            index("pk_id", &["id"], true, true, None),
            index("uq_email", &["email"], true, false, Some("btree")),
            index("uq_name", &["first", "last"], true, false, None),
            index("ix_first", &["first"], false, false, None),
        ],
        ..Default::default()
    };
    let draft = schema_to_draft(&schema);
    let unique: Vec<&str> = draft
        .columns
        .iter()
        .filter(|c| c.is_unique)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        unique,
        ["email"],
        "only a single-column, non-primary, unique index makes a column unique"
    );
}

#[test]
fn schema_to_draft_filters_primary_indexes_and_keeps_index_type() {
    let schema = TableSchema {
        columns: vec![column("id", false)],
        primary_keys: vec!["id".into()],
        indexes: vec![
            index("pk_id", &["id"], true, true, None),
            index("ix_btree", &["id"], false, false, Some("btree")),
            index("ix_default", &["id"], false, false, None),
        ],
        ..Default::default()
    };
    let draft = schema_to_draft(&schema);
    assert_eq!(draft.indexes.len(), 2, "primary indexes are dropped");
    assert!(!draft.indexes.iter().any(|idx| idx.name == "pk_id"));

    let btree = &draft.indexes[0];
    assert_eq!(btree.name, "ix_btree");
    assert_eq!(btree.columns, ["id"]);
    assert!(!btree.is_unique);
    assert!(!btree.is_primary);
    assert_eq!(btree.index_type.as_deref(), Some("btree"));
    assert_eq!(btree.include_columns, None);
    assert_eq!(btree.filter, None);
    assert_eq!(btree.comment, None);
    assert_eq!(btree.id.len(), 10, "idx_ prefix + 6 base-36 characters");
    assert!(btree.id.starts_with("idx_"));

    assert_eq!(
        draft.indexes[1].index_type.as_deref(),
        Some(""),
        "React's idx.indexType ?? '' is an empty string, not absent"
    );
    assert_ne!(btree.id, draft.indexes[1].id, "draft index ids are distinct");
}
