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

#[test]
fn sqlserver_renders_every_catalog_index_type_without_manual_translation() {
    // The four `index_type` values `parse_indexes` can produce, each with the
    // `is_unique` flag the catalog would report for it. Whatever the parser
    // emits must already be renderable, so no caller has to pre-translate.
    let adapter = SqlServerSyncAdapter;
    let schema = table_schema_with_indexes(&[
        ("IX_a", "customer_id", false, "NONCLUSTERED"),
        ("IX_b", "slot", false, "CLUSTERED"),
        ("UQ_c", "email", false, "UNIQUE_CONSTRAINT:NONCLUSTERED"),
        ("UQ_d", "code", false, "UNIQUE_CONSTRAINT:CLUSTERED"),
    ]);
    let rendered = adapter
        .table_objects_to_ir(&schema)
        .indexes
        .iter()
        .map(|index| {
            let ddl = adapter
                .render_index_ddl("[dbo].[orders]", index)
                .expect("every catalog index type must render");
            (
                index.name.clone(),
                index.index_type.clone(),
                ddl.expect("a secondary index renders a CREATE INDEX statement"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            (
                "IX_a".to_string(),
                String::new(),
                "CREATE INDEX [IX_a] ON [dbo].[orders] ([customer_id])".to_string()
            ),
            (
                "IX_b".to_string(),
                String::new(),
                "CREATE INDEX [IX_b] ON [dbo].[orders] ([slot])".to_string()
            ),
            (
                "UQ_c".to_string(),
                String::new(),
                // `is_unique` is the only source of UNIQUE; the catalog prefix
                // must not add a second one.
                "CREATE UNIQUE INDEX [UQ_c] ON [dbo].[orders] ([email])".to_string()
            ),
            (
                "UQ_d".to_string(),
                String::new(),
                "CREATE UNIQUE INDEX [UQ_d] ON [dbo].[orders] ([code])".to_string()
            ),
        ]
    );
    for (_, _, ddl) in &rendered {
        assert_eq!(
            ddl.matches("UNIQUE").count(),
            usize::from(ddl.contains("UNIQUE")),
            "expected at most one UNIQUE keyword in {ddl}"
        );
    }
}

#[test]
fn sqlserver_untranslatable_index_type_still_fails_closed_in_the_renderer() {
    // The mapping translates SQL Server's own vocabulary; it must not act as a
    // blanket reset that would let an index kind the target cannot express
    // through silently.
    let adapter = SqlServerSyncAdapter;
    let schema = table_schema_with_indexes(&[("IX_orders_hash", "customer_id", false, "HASH")]);
    let objects = adapter.table_objects_to_ir(&schema);
    assert_eq!(objects.indexes[0].index_type, "HASH");
    let error = adapter
        .render_index_ddl("[dbo].[orders]", &objects.indexes[0])
        .expect_err("an untranslated index type must be rejected");
    assert!(
        error.contains("cannot represent index type 'HASH'"),
        "unexpected rejection: {error}"
    );
}

#[test]
fn sqlserver_precheck_escapes_catalog_schema_and_table_names() {
    let adapter = SqlServerSyncAdapter;
    // A catalog name with a bracket must be double-bracketed, and quote
    // characters in schema/table must be doubled so the precheck cannot be
    // closed early by the identifier it is inspecting.
    let query = adapter
        .unsupported_transfer_structure_query("db]", Some("sales data"), "people's")
        .expect("SQL Server source preflight query");
    assert!(
        query.contains("FROM [db]]].sys.computed_columns"),
        "{query}"
    );
    assert!(query.contains("s.name = N'sales data'"), "{query}");
    assert!(query.contains("t.name = N'people''s'"), "{query}");

    // An explicit non-default schema is used verbatim; only an absent schema
    // falls back to dbo.
    let explicit = adapter
        .unsupported_transfer_structure_query("sales", Some("archive"), "orders")
        .expect("SQL Server source preflight query");
    assert!(
        explicit.contains("FROM [sales].sys.computed_columns"),
        "{explicit}"
    );
    assert!(explicit.contains("s.name = N'archive'"), "{explicit}");

    // Blank catalog and absent schema: no leading dot, dbo default, and the
    // three retained branches stay joined by exactly two UNION ALL.
    for (database, schema) in [("", None), ("   ", Some("dbo"))] {
        let query = adapter
            .unsupported_transfer_structure_query(database, schema, "orders")
            .expect("SQL Server source preflight query");
        assert!(query.contains("FROM sys.computed_columns"), "{query}");
        assert!(!query.contains("FROM ."), "{query}");
        assert!(query.contains("s.name = N'dbo'"), "{query}");
        assert!(!query.contains("s.name = N''"), "{query}");
        assert_eq!(query.matches("UNION ALL").count(), 2, "{query}");
        // The removed branches must not come back through another catalog view.
        assert!(!query.contains("sys.indexes"), "{query}");
        assert!(!query.contains("sys.foreign_keys"), "{query}");
    }
}

#[test]
fn sqlserver_removed_foreign_key_branch_keeps_foreign_keys_fail_closed() {
    // Dropping the foreign-key precheck row must not make an inexpressible
    // referential action renderable: the shared emitter is the last gate.
    let adapter = SqlServerSyncAdapter;
    let mut schema = table_schema_with_indexes(&[]);
    schema.foreign_keys = vec![ForeignKeyInfo {
        name: "FK_orders_customers".into(),
        columns: vec!["customer_id".into()],
        referenced_table: "[dbo].[customers]".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        // `normalize_action` turns SQL Server's `SET_DEFAULT` into this.
        on_delete: "SET DEFAULT".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    }];
    let objects = adapter.table_objects_to_ir(&schema);
    assert_eq!(objects.foreign_keys.len(), 1);
    assert_eq!(objects.foreign_keys[0].on_delete, "SET DEFAULT");
    let error = adapter
        .render_foreign_key_ddl(
            "[dbo].[orders]",
            &objects.foreign_keys[0],
            "[dbo].[customers]",
        )
        .expect_err("SET DEFAULT must stay unrenderable");
    assert!(
        error.contains("cannot represent foreign-key action 'SET DEFAULT'"),
        "unexpected rejection: {error}"
    );
}
