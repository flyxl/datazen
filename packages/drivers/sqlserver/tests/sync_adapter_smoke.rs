//! SQL Server sync adapter smoke (no live server).

use datazen_driver_api::{
    ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, IRType, IndexInfo, SyncSourceAdapter,
    SyncTargetAdapter, TableOptions, TableSchema,
};
use datazen_driver_sqlserver::SqlServerSyncAdapter;

fn col(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

#[test]
fn sqlserver_nvarchar_and_bit_to_ir() {
    let adapter = SqlServerSyncAdapter;
    assert_eq!(
        adapter
            .column_to_ir(&col("name", "nvarchar(100)"), None)
            .ir_type,
        IRType::Varchar { length: Some(100) }
    );
    assert_eq!(
        adapter.column_to_ir(&col("active", "bit"), None).ir_type,
        IRType::Bool
    );
}

#[test]
fn sqlserver_quote_ident_and_target_types() {
    let adapter = SqlServerSyncAdapter;
    assert_eq!(adapter.quote_ident("col]name"), "[col]]name]");
    assert_eq!(adapter.ir_type_to_native(&IRType::Int32), "INT");
    assert_eq!(adapter.auto_increment_keyword(), Some("IDENTITY(1,1)"));
}

#[test]
fn sqlserver_index_types_are_normalized_into_the_portable_ir() {
    let adapter = SqlServerSyncAdapter;
    // A source index reported by the SQL Server catalog must survive the
    // transfer structure plan instead of failing it closed.
    let schema = table_schema_with_indexes(&[
        (
            "IX_orders_customer_id",
            "customer_id",
            false,
            "NONCLUSTERED",
        ),
        ("PK_orders", "id", true, "CLUSTERED"),
        (
            "AX_orders_total",
            "total_cents",
            false,
            "UNIQUE_CONSTRAINT:NONCLUSTERED",
        ),
        ("IX_orders_slot", "slot", false, "CLUSTERED"),
    ]);
    let objects = adapter.table_objects_to_ir(&schema);
    let rendered = objects
        .indexes
        .iter()
        .map(|index| {
            // Every translated index must be renderable by the shared renderer.
            let ddl = adapter
                .render_index_ddl("[dbo].[orders]", index)
                .ok()
                .flatten();
            (index.name.as_str(), index.index_type.as_str(), ddl)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            (
                "IX_orders_customer_id",
                "",
                Some(
                    "CREATE INDEX [IX_orders_customer_id] ON [dbo].[orders] ([customer_id])".into()
                )
            ),
            // Primary keys stay out of the standalone index statements.
            ("PK_orders", "", None),
            (
                "AX_orders_total",
                "",
                Some(
                    "CREATE UNIQUE INDEX [AX_orders_total] ON [dbo].[orders] ([total_cents])"
                        .into()
                )
            ),
            (
                "IX_orders_slot",
                "",
                Some("CREATE INDEX [IX_orders_slot] ON [dbo].[orders] ([slot])".into())
            ),
        ]
    );
}

#[test]
fn sqlserver_preserves_foreign_keys_for_the_transfer_ir() {
    let adapter = SqlServerSyncAdapter;
    let mut schema = table_schema_with_indexes(&[(
        "IX_orders_customer_id",
        "customer_id",
        false,
        "NONCLUSTERED",
    )]);
    schema.foreign_keys = vec![ForeignKeyInfo {
        name: "FK_orders_customers".into(),
        columns: vec!["customer_id".into()],
        referenced_table: "[dbo].[customers]".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "CASCADE".into(),
        deferrability: ForeignKeyDeferrability::Unknown,
    }];
    let objects = adapter.table_objects_to_ir(&schema);
    assert_eq!(objects.indexes.len(), 1);
    assert_eq!(objects.foreign_keys.len(), 1);
    let foreign_key = &objects.foreign_keys[0];
    assert_eq!(foreign_key.name, "FK_orders_customers");
    assert_eq!(foreign_key.referenced_table, "[dbo].[customers]");
    assert_eq!(foreign_key.on_delete, "CASCADE");
    assert_eq!(
        adapter.render_foreign_key_ddl("[dbo].[orders]", foreign_key, "[dbo].[customers]"),
        Ok("ALTER TABLE [dbo].[orders] ADD CONSTRAINT [FK_orders_customers] FOREIGN KEY ([customer_id]) REFERENCES [dbo].[customers] ([id]) ON DELETE CASCADE".to_string())
    );
}

/// Build a table whose catalog-level indexes carry the index types that
/// `parse_indexes` really reports for SQL Server, so the transfer mapping is
/// exercised end to end. `is_unique` mirrors the catalog: only
/// `UNIQUE_CONSTRAINT:`-prefixed rows are unique indexes.
fn table_schema_with_indexes(indexes: &[(&str, &str, bool, &str)]) -> TableSchema {
    TableSchema {
        table_name: "orders".into(),
        columns: vec![
            col("id", "int"),
            col("customer_id", "int"),
            col("total_cents", "int"),
            col("slot", "int"),
        ],
        primary_keys: vec!["id".into()],
        indexes: indexes
            .iter()
            .map(|(name, column, is_primary, index_type)| IndexInfo {
                name: (*name).into(),
                columns: vec![(*column).into()],
                is_unique: index_type.starts_with("UNIQUE_CONSTRAINT"),
                is_primary: *is_primary,
                index_type: (*index_type).into(),
            })
            .collect(),
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: TableOptions::default(),
    }
}

#[test]
fn sqlserver_preflight_defaults_schema_and_catalog() {
    let adapter = SqlServerSyncAdapter;
    let query = adapter
        .unsupported_transfer_structure_query("  ", None, "orders")
        .expect("SQL Server source preflight query");
    // An empty database means no catalog prefix; the schema falls back to dbo.
    assert!(query.contains("FROM sys.computed_columns c"));
    assert!(query.contains("s.name = N'dbo'"));
    assert!(query.contains("t.name = N'orders'"));
    // Exactly the three inexpressible object kinds survive: two UNION ALL.
    assert_eq!(query.matches("UNION ALL").count(), 2);
}

#[test]
fn sqlserver_name_scoping_matches_catalog_semantics() {
    let adapter = SqlServerSyncAdapter;
    // Index names are per-table on SQL Server (`(object_id, index_id)`), while
    // foreign keys are schema-level objects in `sys.objects` and keep the
    // stricter schema-wide rule from the trait default.
    assert!(adapter.index_names_are_table_scoped());
    assert!(!adapter.foreign_key_names_are_table_scoped());
}
