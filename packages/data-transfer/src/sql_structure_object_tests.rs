//! Host-side coverage for the secondary-index and foreign-key stages of the
//! SQL-file structure plan.
//!
//! These tests drive the plan builder with fake adapters so the shipped
//! `SyncTargetAdapter` defaults decide the outcome: index/FK naming scope, case
//! folding and the conservative index renderer.

use std::collections::HashMap;

use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo, TableSchema};

use super::model::DdlPreviewKind;
use super::sql_structure::build_database_structure_plan;
use super::sql_structure_fixtures::*;

/// A table with one non-primary index on `label`.
fn records_with_label_index(index_name: &str) -> TableSchema {
    with_index(
        schema("records", &[("id", true), ("label", false)]),
        index_name,
        &["label"],
        false,
    )
}

#[test]
fn secondary_indexes_are_emitted_after_every_table_and_depend_on_their_source_table() {
    let orders = with_index(
        schema("orders", &[("id", true), ("customer_id", false)]),
        "ix_orders_customer",
        &["customer_id"],
        false,
    );
    let customers = with_index(
        schema("customers", &[("id", true), ("name", false)]),
        "uq_customers_name",
        &["name"],
        true,
    );
    let schemas = HashMap::from([("orders".into(), orders), ("customers".into(), customers)]);
    let mappings = vec![
        mapping(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        mapping("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];
    let inspected = vec![
        inspected(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        inspected("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("two secondary indexes must be planned");

    let kinds: Vec<DdlPreviewKind> = plan.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DdlPreviewKind::Table,
            DdlPreviewKind::Table,
            DdlPreviewKind::Index,
            DdlPreviewKind::Index,
        ]
    );

    let table_count = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Table)
        .count();
    for (position, item) in plan.iter().enumerate() {
        if item.kind == DdlPreviewKind::Index {
            assert!(
                position >= table_count,
                "index DDL must follow every CREATE TABLE: {item:?}"
            );
        }
    }

    let orders_index = plan
        .iter()
        .find(|item| item.ddl.contains("ix_orders_customer"))
        .expect("orders index must be planned");
    assert_eq!(orders_index.kind, DdlPreviewKind::Index);
    assert_eq!(orders_index.source_table, "orders");
    assert_eq!(orders_index.depends_on, vec!["orders"]);
    assert!(orders_index.ddl.contains(r#"ON "archive"."orders""#));
    assert!(orders_index.ddl.contains(r#"("customer_id")"#));

    let customers_index = plan
        .iter()
        .find(|item| item.ddl.contains("uq_customers_name"))
        .expect("customers index must be planned");
    assert!(customers_index.ddl.starts_with("CREATE UNIQUE INDEX"));
    assert_eq!(customers_index.source_table, "customers");
    assert_eq!(customers_index.depends_on, vec!["customers"]);
    assert!(customers_index.ddl.contains(r#"ON "archive"."customers""#));
}

#[test]
fn a_primary_index_produces_no_index_statement_because_create_table_owns_it() {
    let records = with_primary_index(
        records_with_label_index("ix_records_label"),
        "PK_records",
        &["id"],
    );
    let schemas = HashMap::from([("records".into(), records)]);
    let mappings = vec![mapping(
        "records",
        "records",
        &[("id", "id"), ("label", "label")],
    )];
    let inspected = vec![inspected(
        "records",
        "records",
        &[("id", "id"), ("label", "label")],
    )];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("a primary key must not block the plan");

    let index_items: Vec<_> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
        .collect();
    assert_eq!(index_items.len(), 1, "only the secondary index is emitted");
    assert!(index_items[0].ddl.contains("ix_records_label"));
    assert!(
        !plan.iter().any(|item| item.ddl.contains("PK_records")),
        "the primary key belongs to CREATE TABLE, not to a separate CREATE INDEX"
    );
    assert!(plan[0].kind == DdlPreviewKind::Table);
    assert!(plan[0].ddl.contains("PRIMARY KEY"), "{}", plan[0].ddl);
}

#[test]
fn a_foreign_key_is_emitted_after_both_tables_and_depends_on_the_referenced_table() {
    let mut orders = schema("orders", &[("id", true), ("customer_id", false)]);
    orders.foreign_keys.push(ForeignKeyInfo {
        name: "fk_orders_customer".into(),
        columns: vec!["customer_id".into()],
        referenced_table: "public.customers".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let customers = schema("customers", &[("id", true), ("name", false)]);
    let schemas = HashMap::from([("orders".into(), orders), ("customers".into(), customers)]);
    let mappings = vec![
        mapping(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        mapping("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];
    let inspected = vec![
        inspected(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        inspected("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("a resolvable foreign key must be planned");

    let kinds: Vec<DdlPreviewKind> = plan.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DdlPreviewKind::Table,
            DdlPreviewKind::Table,
            DdlPreviewKind::ForeignKey,
        ]
    );

    let last_table = plan
        .iter()
        .rposition(|item| item.kind == DdlPreviewKind::Table)
        .expect("both tables are created");
    let fk_position = plan
        .iter()
        .position(|item| item.kind == DdlPreviewKind::ForeignKey)
        .expect("foreign key is planned");
    assert!(
        fk_position > last_table,
        "the constraint follows every CREATE TABLE so it can be added with ALTER TABLE"
    );

    let fk = &plan[fk_position];
    assert_eq!(fk.source_table, "orders");
    assert_eq!(fk.target_table, "orders");
    assert_eq!(fk.depends_on, vec!["customers"]);
    assert!(
        fk.ddl.contains(r#"ALTER TABLE "archive"."orders""#),
        "{}",
        fk.ddl
    );
    assert!(
        fk.ddl
            .contains(r#"REFERENCES "archive"."customers" ("id")"#),
        "{}",
        fk.ddl
    );
}

#[test]
fn a_foreign_key_pointing_outside_the_selection_is_rejected_instead_of_dropped() {
    let mut orders = schema("orders", &[("id", true), ("customer_id", false)]);
    orders.foreign_keys.push(ForeignKeyInfo {
        name: "fk_orders_customer".into(),
        columns: vec!["customer_id".into()],
        referenced_table: "public.customers".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let schemas = HashMap::from([("orders".into(), orders)]);
    let mappings = vec![mapping(
        "orders",
        "orders",
        &[("id", "id"), ("customer_id", "customer_id")],
    )];
    let inspected = vec![inspected(
        "orders",
        "orders",
        &[("id", "id"), ("customer_id", "customer_id")],
    )];

    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("a foreign key to an unmigrated table must fail the whole plan");

    let message = error.to_string();
    assert!(
        message.contains("unselected or out-of-scope table"),
        "{message}"
    );
    assert!(message.contains("public.customers"), "{message}");
}

#[test]
fn duplicate_index_names_are_rejected_when_the_target_scopes_names_per_schema() {
    let schemas = HashMap::from([
        (
            "orders".into(),
            with_index(
                schema("orders", &[("id", true), ("ref", false)]),
                "idx_shared",
                &["ref"],
                false,
            ),
        ),
        (
            "customers".into(),
            with_index(
                schema("customers", &[("id", true), ("code", false)]),
                "idx_shared",
                &["code"],
                false,
            ),
        ),
    ]);
    let mappings = vec![
        mapping("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        mapping("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];
    let inspected = vec![
        inspected("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        inspected("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];

    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("schema-scoped index names must not be emitted twice");

    let message = error.to_string();
    assert!(message.contains("collides between"), "{message}");
    assert!(
        message.contains("rename one source object before transfer"),
        "{message}"
    );
    assert!(message.contains("idx_shared"), "{message}");
    assert!(
        message.contains("orders") && message.contains("customers"),
        "{message}"
    );
}

#[test]
fn duplicate_index_names_are_accepted_when_the_target_scopes_index_names_per_table() {
    let schemas = HashMap::from([
        (
            "orders".into(),
            with_index(
                schema("orders", &[("id", true), ("ref", false)]),
                "idx_shared",
                &["ref"],
                false,
            ),
        ),
        (
            "customers".into(),
            with_index(
                schema("customers", &[("id", true), ("code", false)]),
                "idx_shared",
                &["code"],
                false,
            ),
        ),
    ]);
    let mappings = vec![
        mapping("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        mapping("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];
    let inspected = vec![
        inspected("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        inspected("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];

    let plan = build_database_structure_plan(
        &Source,
        &TableScopedIndexTarget,
        &job(mappings),
        &inspected,
        &schemas,
    )
    .expect("table-scoped index names may repeat across tables");

    let index_items: Vec<_> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
        .collect();
    assert_eq!(index_items.len(), 2);
    // Secondary objects are emitted in (target table, DDL) order, so the plan
    // is deterministic even when both indexes share a name.
    let customers_index = plan
        .iter()
        .find(|item| item.kind == DdlPreviewKind::Index && item.ddl.contains(r#""customers""#))
        .expect("customers index is planned");
    let orders_index = plan
        .iter()
        .find(|item| item.kind == DdlPreviewKind::Index && item.ddl.contains(r#""orders""#))
        .expect("orders index is planned");
    assert!(customers_index.ddl.contains(r#"ON "archive"."customers""#));
    assert!(orders_index.ddl.contains(r#"ON "archive"."orders""#));
    assert_eq!(customers_index.depends_on, vec!["customers"]);
    assert_eq!(orders_index.depends_on, vec!["orders"]);
    assert!(!customers_index.ddl.contains(r#""orders""#));
}

#[test]
fn index_names_that_differ_only_by_case_collide_on_a_case_insensitive_target() {
    let schemas = HashMap::from([
        (
            "orders".into(),
            with_index(
                schema("orders", &[("id", true), ("ref", false)]),
                "Idx_Shared",
                &["ref"],
                false,
            ),
        ),
        (
            "customers".into(),
            with_index(
                schema("customers", &[("id", true), ("code", false)]),
                "idx_shared",
                &["code"],
                false,
            ),
        ),
    ]);
    let mappings = vec![
        mapping("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        mapping("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];
    let inspected = vec![
        inspected("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        inspected("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];

    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("a case-folding target such as SQL Server cannot carry both spellings");

    let message = error.to_string();
    assert!(message.contains("collides between"), "{message}");
    assert!(
        message.contains("rename one source object before transfer"),
        "{message}"
    );
}

#[test]
fn index_names_that_differ_only_by_case_coexist_on_a_case_sensitive_target() {
    let schemas = HashMap::from([
        (
            "orders".into(),
            with_index(
                schema("orders", &[("id", true), ("ref", false)]),
                "Idx_Shared",
                &["ref"],
                false,
            ),
        ),
        (
            "customers".into(),
            with_index(
                schema("customers", &[("id", true), ("code", false)]),
                "idx_shared",
                &["code"],
                false,
            ),
        ),
    ]);
    let mappings = vec![
        mapping("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        mapping("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];
    let inspected = vec![
        inspected("orders", "orders", &[("id", "id"), ("ref", "ref")]),
        inspected("customers", "customers", &[("id", "id"), ("code", "code")]),
    ];

    let plan = build_database_structure_plan(
        &Source,
        &CaseSensitiveTarget,
        &job(mappings),
        &inspected,
        &schemas,
    )
    .expect("a case-preserving target keeps both spellings distinct");

    let index_ddls: Vec<&str> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
        .map(|item| item.ddl.as_str())
        .collect();
    assert_eq!(index_ddls.len(), 2);
    assert!(index_ddls.iter().any(|ddl| ddl.contains(r#""Idx_Shared""#)));
    assert!(index_ddls.iter().any(|ddl| ddl.contains(r#""idx_shared""#)));
}

#[test]
fn index_ddl_uses_the_renamed_target_column() {
    let orders = with_index(
        schema("orders", &[("id", true), ("user_id", false)]),
        "ix_orders_user",
        &["user_id"],
        false,
    );
    let schemas = HashMap::from([("orders".into(), orders)]);
    let mappings = vec![mapping(
        "orders",
        "buyers",
        &[("id", "id"), ("user_id", "buyer_id")],
    )];
    let inspected = vec![inspected(
        "orders",
        "buyers",
        &[("id", "id"), ("user_id", "buyer_id")],
    )];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("a renamed column still yields an index");

    let index_item = plan
        .iter()
        .find(|item| item.kind == DdlPreviewKind::Index)
        .expect("secondary index is planned");
    assert!(
        index_item.ddl.contains(r#"("buyer_id")"#),
        "{}",
        index_item.ddl
    );
    assert!(
        !index_item.ddl.contains("user_id"),
        "the source name must never leak into target DDL: {}",
        index_item.ddl
    );
    assert!(plan[0].ddl.contains(r#""buyer_id""#), "{}", plan[0].ddl);
}

#[test]
fn an_index_on_a_skipped_column_is_rejected_with_a_clear_message() {
    let orders = with_index(
        schema("orders", &[("id", true), ("user_id", false)]),
        "ix_orders_user",
        &["user_id"],
        false,
    );
    let schemas = HashMap::from([("orders".into(), orders)]);
    let mappings = vec![mapping_with_skipped_column(
        "orders",
        "orders",
        &[("id", "id"), ("user_id", "user_id")],
        "user_id",
    )];
    let inspected = vec![inspected(
        "orders",
        "orders",
        &[("id", "id"), ("user_id", "user_id")],
    )];

    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("an index on an excluded column cannot be preserved");

    let message = error.to_string();
    assert!(
        message.contains("source constraint column 'user_id' is skipped"),
        "{message}"
    );
}

#[test]
fn a_prefix_length_index_is_rejected_instead_of_emitting_wrong_ddl() {
    let mut records = schema("records", &[("id", true), ("name", false)]);
    records.indexes.push(IndexInfo {
        name: "ix_records_name_prefix".into(),
        columns: vec!["name(10)".into()],
        is_unique: false,
        is_primary: false,
        index_type: "btree".into(),
    });
    let schemas = HashMap::from([("records".into(), records)]);
    let inspected = vec![inspected(
        "records",
        "records",
        &[("id", "id"), ("name", "name")],
    )];

    // Without a column mapping the catalog spelling reaches the target IR
    // lookup first: `name(10)` is not a created column, so the plan fails
    // closed before `render_index_ddl` is even consulted. Documented current
    // behaviour - the wording names the key column but neither the index nor
    // the "prefix length" cause.
    let error = build_database_structure_plan(
        &Source,
        &Target,
        &job(vec![mapping_without_columns("records", "records")]),
        &inspected,
        &schemas,
    )
    .expect_err("a prefix-length index must not become a full-column index");

    let message = error.to_string();
    assert!(
        message.contains("mapped target index column 'name(10)' was not created"),
        "{message}"
    );
    assert!(!message.contains("CREATE INDEX"), "{message}");
}

#[test]
fn a_prefix_length_index_is_reported_as_a_missing_mapping_when_columns_are_mapped() {
    let mut records = schema("records", &[("id", true), ("name", false)]);
    records.indexes.push(IndexInfo {
        name: "ix_records_name_prefix".into(),
        columns: vec!["name(10)".into()],
        is_unique: false,
        is_primary: false,
        index_type: "btree".into(),
    });
    let schemas = HashMap::from([("records".into(), records)]);
    let mappings = vec![mapping(
        "records",
        "records",
        &[("id", "id"), ("name", "name")],
    )];
    let inspected = vec![inspected(
        "records",
        "records",
        &[("id", "id"), ("name", "name")],
    )];

    // Documented current behaviour: the column mapping is consulted before the
    // renderer, so the user sees a mapping complaint instead of the precise
    // "prefix lengths" rejection. Still fail-closed, but the wording hides the
    // real cause.
    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("an unmapped index key must still fail the plan");

    let message = error.to_string();
    assert!(
        message.contains("source constraint column 'name(10)' is not mapped"),
        "{message}"
    );
    assert!(
        !message.contains("prefix"),
        "the message hides the prefix-index cause: {message}"
    );
}
