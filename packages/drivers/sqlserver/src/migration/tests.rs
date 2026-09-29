use super::*;

fn column(name: &str, data_type: &str) -> MigrationColumn {
    MigrationColumn {
        name: name.into(),
        data_type: data_type.into(),
        nullable: false,
        default_value: None,
        comment: None,
        is_auto_increment: false,
    }
}

#[test]
fn create_table_quotes_scope_and_preserves_supported_column_features() {
    let mut id = column("id", "int");
    id.is_auto_increment = true;
    let mut label = column("label]", "nvarchar(40)");
    label.nullable = true;
    label.default_value = Some("(N'new')".into());
    label.comment = Some("label description".into());
    let statement = SqlServerMigrationRenderer
        .render(&MigrationOperation::CreateTable {
            table: "[audit].[event.log]".into(),
            columns: vec![id, label],
            primary_keys: vec!["id".into()],
        })
        .unwrap();
    assert!(statement
        .sql
        .starts_with("CREATE TABLE [audit].[event.log] ([id] INT IDENTITY(1,1) NOT NULL"));
    assert!(statement
        .sql
        .contains("[label]]] NVARCHAR(40) NULL DEFAULT (N'new')"));
    assert!(statement.sql.contains("PRIMARY KEY ([id])"));
    assert!(statement.sql.contains("sp_addextendedproperty"));
    assert_eq!(statement.risk, MigrationRisk::Additive);
}

#[test]
fn default_changes_resolve_the_catalog_constraint_name_and_verify_snapshot() {
    let operation = MigrationOperation::SetDefault {
        table: "sales.orders".into(),
        column: "status".into(),
        from: Some("('queued')".into()),
        to: Some("('ready')".into()),
    };
    let statement = SqlServerMigrationRenderer.render(&operation).unwrap();
    assert!(statement.sql.contains("@datazen_default_name"));
    assert!(statement.sql.contains("sys.default_constraints"));
    assert!(statement.sql.contains("QUOTENAME(@datazen_default_name)"));
    assert!(statement
        .sql
        .contains("SQL Server DEFAULT changed after schema comparison"));
    assert!(statement
        .sql
        .contains("ADD DEFAULT (('ready')) FOR [status]"));
    assert!(statement.rollback_sql.unwrap().contains("('queued')"));

    let unsafe_default = MigrationOperation::SetDefault {
        table: "sales.orders".into(),
        column: "status".into(),
        from: None,
        to: Some("0; DROP TABLE sales.orders".into()),
    };
    assert!(!SqlServerMigrationCapabilities.supports(&unsafe_default));
    assert!(SqlServerMigrationRenderer.render(&unsafe_default).is_err());
}

#[test]
fn drops_primary_key_by_actual_name_after_checking_ordered_columns() {
    let statement = SqlServerMigrationRenderer
        .render(&MigrationOperation::DropPrimaryKey {
            table: "sales.orders".into(),
            columns: vec!["tenant_id".into(), "order_id".into()],
        })
        .unwrap();
    assert!(statement.sql.contains("sys.key_constraints"));
    assert!(statement.sql.contains("QUOTENAME(@datazen_pk_name)"));
    assert!(statement
        .sql
        .contains("ic.key_ordinal = 1 AND COL_NAME(ic.object_id, ic.column_id) = N'tenant_id'"));
    assert!(statement
        .sql
        .contains("ic.key_ordinal = 2 AND COL_NAME(ic.object_id, ic.column_id) = N'order_id'"));
}

#[test]
fn indexes_preserve_constraint_kind_and_schema_scoped_drop_syntax() {
    let unique_constraint = IndexInfo {
        name: "UQ orders number".into(),
        columns: vec!["number".into()],
        is_unique: true,
        is_primary: false,
        index_type: "UNIQUE_CONSTRAINT:NONCLUSTERED".into(),
    };
    let create = SqlServerMigrationRenderer
        .render(&MigrationOperation::CreateIndex {
            table: "sales.orders".into(),
            index: unique_constraint.clone(),
        })
        .unwrap();
    assert!(create
        .sql
        .contains("ADD CONSTRAINT [UQ orders number] UNIQUE NONCLUSTERED ([number])"));
    let drop = SqlServerMigrationRenderer
        .render(&MigrationOperation::DropIndex {
            table: "sales.orders".into(),
            index: unique_constraint,
        })
        .unwrap();
    assert!(drop
        .sql
        .contains("ALTER TABLE [sales].[orders] DROP CONSTRAINT"));
    assert!(drop.rollback_sql.unwrap().contains("UNIQUE NONCLUSTERED"));

    let index = IndexInfo {
        name: "ix_order".into(),
        columns: vec!["number".into()],
        is_unique: false,
        is_primary: false,
        index_type: "CLUSTERED".into(),
    };
    let statement = SqlServerMigrationRenderer
        .render(&MigrationOperation::DropIndex {
            table: "sales.orders".into(),
            index,
        })
        .unwrap();
    assert!(statement
        .sql
        .contains("DROP INDEX [ix_order] ON [sales].[orders]"));
}

#[test]
fn foreign_keys_and_checks_use_exact_names_and_tsql_actions() {
    let foreign_key = ForeignKeyInfo {
        name: "FK order line".into(),
        columns: vec!["order_id".into()],
        referenced_table: "[sales].[orders]".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "CASCADE".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    };
    let statement = SqlServerMigrationRenderer
        .render(&MigrationOperation::AddForeignKey {
            table: "sales.order_lines".into(),
            foreign_key,
        })
        .unwrap();
    assert!(statement
        .sql
        .contains("REFERENCES [sales].[orders] ([id]) ON DELETE CASCADE"));
    assert!(statement
        .rollback_sql
        .unwrap()
        .contains("DROP CONSTRAINT [FK order line]"));

    let check = CheckConstraint {
        name: "CK positive".into(),
        expression: "[amount] >= 0".into(),
    };
    let statement = SqlServerMigrationRenderer
        .render(&MigrationOperation::AddCheckConstraint {
            table: "sales.order_lines".into(),
            constraint: check.clone(),
        })
        .unwrap();
    assert!(statement
        .sql
        .contains("ADD CONSTRAINT [CK positive] CHECK ([amount] >= 0)"));
    let unsafe_check = MigrationOperation::AddCheckConstraint {
        table: "sales.order_lines".into(),
        constraint: CheckConstraint {
            name: "ck".into(),
            expression: "1=1; DROP TABLE sales.orders".into(),
        },
    };
    assert!(!SqlServerMigrationCapabilities.supports(&unsafe_check));
}

#[test]
fn alterations_preserve_target_nullability_and_refuse_identity_changes() {
    let operation = MigrationOperation::AlterColumnType {
        table: "sales.orders".into(),
        column: "amount".into(),
        from: "INT".into(),
        to: "DECIMAL(12, 2)".into(),
    };
    let statement = SqlServerMigrationRenderer.render(&operation).unwrap();
    assert!(statement.sql.contains("N'DECIMAL(12,2)'"));
    assert!(statement.sql.contains("@datazen_nullability"));
    assert!(statement.sql.contains("@datazen_collation"));

    let nullable = SqlServerMigrationRenderer
        .render(&MigrationOperation::SetNullable {
            table: "sales.orders".into(),
            column: "amount".into(),
            nullable: false,
        })
        .unwrap();
    assert!(nullable.sql.contains("sys.columns"));
    assert!(nullable.sql.contains("@datazen_collation"));
    assert!(nullable.sql.contains("N' NOT NULL'"));

    let identity_change = MigrationOperation::SetAutoIncrement {
        table: "sales.orders".into(),
        column: "id".into(),
        from: false,
        to: true,
    };
    assert!(!SqlServerMigrationCapabilities.supports(&identity_change));
    assert!(SqlServerMigrationRenderer.render(&identity_change).is_err());
}

#[test]
fn views_require_exact_identity_and_single_server_batch() {
    let view = MigrationView {
        schema: Some("审计".into()),
        name: "事件".into(),
        definition: "CREATE VIEW [审计].[事件] WITH SCHEMABINDING AS SELECT id FROM [审计].[源]"
            .into(),
    };
    let create = SqlServerMigrationRenderer
        .render(&MigrationOperation::CreateView { view: view.clone() })
        .unwrap();
    assert!(create.sql.starts_with("CREATE VIEW [审计].[事件]"));
    assert!(create.sql.contains("WITH SCHEMABINDING AS"));
    let replace = SqlServerMigrationRenderer
        .render(&MigrationOperation::ReplaceView {
            current: view.clone(),
            desired: MigrationView {
                definition: view.definition.replace("SELECT id", "SELECT [id]"),
                ..view.clone()
            },
        })
        .unwrap();
    assert!(replace.sql.starts_with("ALTER VIEW"));

    let mismatch = MigrationView {
        definition: "CREATE VIEW [dbo].[wrong] AS SELECT 1".into(),
        ..view.clone()
    };
    assert!(!SqlServerMigrationCapabilities
        .supports(&MigrationOperation::CreateView { view: mismatch }));
    let batch = MigrationView {
        definition: "CREATE VIEW [审计].[事件] AS SELECT 1; DROP TABLE x".into(),
        ..view
    };
    assert!(
        !SqlServerMigrationCapabilities.supports(&MigrationOperation::CreateView { view: batch })
    );

    let unsupported_header = MigrationView {
        schema: Some("dbo".into()),
        name: "v".into(),
        definition: "CREATE OR ALTER VIEW [dbo].[v] AS SELECT 1".into(),
    };
    assert!(
        !SqlServerMigrationCapabilities.supports(&MigrationOperation::CreateView {
            view: unsupported_header
        })
    );
}

#[test]
fn unsupported_operations_and_unrepresented_table_layouts_fail_closed() {
    let caps = SqlServerMigrationCapabilities;
    assert!(!caps.supports(&MigrationOperation::CreateRoutine {
        routine: MigrationRoutine {
            kind: ObjectKind::Procedure,
            schema: Some("dbo".into()),
            name: "p".into(),
            signature: None,
            definition: "CREATE PROCEDURE [dbo].[p] AS SELECT 1".into(),
        },
    }));
    assert!(!caps.supports(&MigrationOperation::SetTableOptions {
        table: "dbo.t".into(),
        from: TableOptions::default(),
        to: TableOptions::default(),
    }));
    let create = MigrationOperation::CreateTable {
        table: "dbo.t".into(),
        columns: vec![column("id", "int")],
        primary_keys: vec!["id".into()],
    };
    let blocker = TableOptions {
        migration_blockers: vec!["nonclustered primary key".into()],
        ..TableOptions::default()
    };
    assert!(SqlServerMigrationRenderer
        .render_create_table_with_options(&create, &blocker)
        .is_err());

    let unsupported_type = MigrationOperation::AddColumn {
        table: "dbo.t".into(),
        column: column("payload", "xml"),
    };
    assert!(!caps.supports(&unsupported_type));
    assert!(SqlServerMigrationRenderer
        .render(&unsupported_type)
        .is_err());

    for invalid_type in ["CHAR(MAX)", "NCHAR(MAX)", "BINARY(MAX)"] {
        let operation = MigrationOperation::AddColumn {
            table: "dbo.t".into(),
            column: column("invalid", invalid_type),
        };
        assert!(!caps.supports(&operation), "{invalid_type}");
        assert!(SqlServerMigrationRenderer.render(&operation).is_err());
    }
}
