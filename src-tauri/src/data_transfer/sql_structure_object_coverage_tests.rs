//! Branch coverage that `sql_structure_object_tests` does not reach.
//!
//! Every test here isolates one `build_database_structure_plan` branch that the
//! first module leaves untouched: the DDL tie-breaker inside the index/FK sort,
//! the foreign-key half of the naming gate, the `depends_on` name space when
//! both tables are renamed, the table-scoped *and* case-sensitive namespace
//! combination, and a selection that contains no enabled table.
//!
//! The fakes below inherit every `SyncTargetAdapter` default except the single
//! flag under test, so the assertions still reflect the shipped driver-api
//! defaults rather than fake-owned output.

use std::collections::HashMap;

use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, TableSchema};

use crate::db::Value;
use crate::transfer::adapter::SyncTargetAdapter;
use crate::transfer::ir::{IRDefault, IRType};

use super::model::DdlPreviewKind;
use super::sql_structure::build_database_structure_plan;
use super::sql_structure_fixtures::*;

/// Target whose *foreign-key* constraint names are local to their table while
/// index names stay schema-scoped (the PostgreSQL combination).
struct TableScopedFkTarget;

impl SyncTargetAdapter for TableScopedFkTarget {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        Target::default_target().ir_type_to_native(ir_type)
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        Target::default_target().format_default(default)
    }
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String {
        Target::default_target().format_literal(value, ir_type)
    }
    fn foreign_key_names_are_table_scoped(&self) -> bool {
        true
    }
}

/// Target that is both table-scoped and case-preserving.
struct TableScopedCaseSensitiveTarget;

impl SyncTargetAdapter for TableScopedCaseSensitiveTarget {
    fn ir_type_to_native(&self, ir_type: &IRType) -> String {
        Target::default_target().ir_type_to_native(ir_type)
    }
    fn format_default(&self, default: &IRDefault) -> Option<String> {
        Target::default_target().format_default(default)
    }
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String {
        Target::default_target().format_literal(value, ir_type)
    }
    fn index_names_are_table_scoped(&self) -> bool {
        true
    }
    fn object_names_are_case_sensitive(&self) -> bool {
        true
    }
}

fn fk(
    name: &str,
    columns: &[&str],
    referenced_table: &str,
    referenced_columns: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: columns.iter().map(|column| (*column).into()).collect(),
        referenced_table: referenced_table.into(),
        referenced_columns: referenced_columns
            .iter()
            .map(|column| (*column).into())
            .collect(),
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    }
}

/// Extract the quoted object name that `CREATE [UNIQUE]INDEX` / `ADD CONSTRAINT`
/// emitted, so order assertions read as names instead of full statements.
fn emitted_name(ddl: &str, prefix: &str) -> String {
    let rest = ddl
        .split_once(prefix)
        .unwrap_or_else(|| panic!("`{prefix}` missing from {ddl}"))
        .1;
    let quoted = rest
        .split_once('"')
        .unwrap_or_else(|| panic!("quoted name missing from {ddl}"))
        .1;
    quoted
        .split_once('"')
        .unwrap_or_else(|| panic!("closing quote missing from {ddl}"))
        .0
        .to_string()
}

/// Declaration order is `ix_zulu`, `ix_mike`, `ix_alpha` (the unique one last),
/// so an implementation that skipped the `ddl` tie-breaker would emit that order
/// and fail the assertion below.
#[test]
fn several_indexes_on_one_table_are_sorted_by_ddl_not_declaration_order() {
    let records = with_index(
        with_index(
            with_index(
                schema(
                    "records",
                    &[
                        ("id", true),
                        ("alpha_col", false),
                        ("mike_col", false),
                        ("zulu_col", false),
                    ],
                ),
                "ix_zulu",
                &["zulu_col"],
                false,
            ),
            "ix_mike",
            &["mike_col"],
            false,
        ),
        "ix_alpha",
        &["alpha_col"],
        true,
    );
    let schemas = HashMap::from([("records".into(), records)]);
    let columns = [
        ("id", "id"),
        ("alpha_col", "alpha_col"),
        ("mike_col", "mike_col"),
        ("zulu_col", "zulu_col"),
    ];
    let mappings = vec![mapping("records", "records", &columns)];
    let inspected = vec![inspected("records", "records", &columns)];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("three indexes on one table are all representable");

    let index_names: Vec<String> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
        .map(|item| emitted_name(&item.ddl, "INDEX "))
        .collect();
    assert_eq!(
        index_names,
        vec!["ix_mike", "ix_zulu", "ix_alpha"],
        "same-table indexes are ordered by their full DDL text, not by name and \
         not by declaration order (`CREATE INDEX` sorts before `CREATE UNIQUE INDEX`)"
    );
    for item in plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
    {
        assert_eq!(item.source_table, "records");
        assert_eq!(item.target_table, "records");
        assert_eq!(item.depends_on, vec!["records"]);
    }
    assert!(
        plan.iter().any(|item| item.ddl
            == "CREATE UNIQUE INDEX \"ix_alpha\" ON \"archive\".\"records\" (\"alpha_col\")"),
        "the unique flag is part of the sorted DDL: {plan:?}"
    );
}

#[test]
fn several_foreign_keys_on_one_table_are_sorted_and_keep_their_own_dependency() {
    let mut orders = schema(
        "orders",
        &[("id", true), ("customer_id", false), ("product_id", false)],
    );
    // Declared newest-last so a declaration-order implementation would fail.
    orders
        .foreign_keys
        .push(fk("fk_zeta", &["customer_id"], "public.customers", &["id"]));
    orders
        .foreign_keys
        .push(fk("fk_alpha", &["product_id"], "public.products", &["id"]));
    let schemas = HashMap::from([
        ("orders".into(), orders),
        (
            "customers".into(),
            schema("customers", &[("id", true), ("name", false)]),
        ),
        (
            "products".into(),
            schema("products", &[("id", true), ("sku", false)]),
        ),
    ]);
    let mappings = vec![
        mapping(
            "orders",
            "orders",
            &[
                ("id", "id"),
                ("customer_id", "customer_id"),
                ("product_id", "product_id"),
            ],
        ),
        mapping("customers", "customers", &[("id", "id"), ("name", "name")]),
        mapping("products", "products", &[("id", "id"), ("sku", "sku")]),
    ];
    let inspected = vec![
        inspected(
            "orders",
            "orders",
            &[
                ("id", "id"),
                ("customer_id", "customer_id"),
                ("product_id", "product_id"),
            ],
        ),
        inspected("customers", "customers", &[("id", "id"), ("name", "name")]),
        inspected("products", "products", &[("id", "id"), ("sku", "sku")]),
    ];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("both foreign keys reference selected tables");

    let fk_items: Vec<_> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::ForeignKey)
        .collect();
    assert_eq!(fk_items.len(), 2);
    assert_eq!(
        fk_items
            .iter()
            .map(|item| emitted_name(&item.ddl, "ADD CONSTRAINT "))
            .collect::<Vec<_>>(),
        vec!["fk_alpha", "fk_zeta"],
        "same-table constraints are ordered by DDL, not by declaration"
    );
    assert_eq!(fk_items[0].depends_on, vec!["products"]);
    assert_eq!(fk_items[1].depends_on, vec!["customers"]);
    assert!(
        fk_items[0]
            .ddl
            .contains(r#"REFERENCES "archive"."products""#),
        "{}",
        fk_items[0].ddl
    );
    assert!(
        fk_items[1]
            .ddl
            .contains(r#"REFERENCES "archive"."customers""#),
        "{}",
        fk_items[1].ddl
    );
}

#[test]
fn a_renamed_child_and_parent_split_depends_on_sources_from_ddl_targets() {
    let mut orders = schema("orders", &[("id", true), ("customer_id", false)]);
    orders.foreign_keys.push(fk(
        "fk_orders_customer",
        &["customer_id"],
        "public.customers",
        &["id"],
    ));
    let schemas = HashMap::from([
        ("orders".into(), orders),
        (
            "customers".into(),
            schema("customers", &[("id", true), ("name", false)]),
        ),
    ]);
    let mappings = vec![
        mapping(
            "orders",
            "archive_orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        mapping(
            "customers",
            "archive_customers",
            &[("id", "id"), ("name", "name")],
        ),
    ];
    let inspected = vec![
        inspected(
            "orders",
            "archive_orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        inspected(
            "customers",
            "archive_customers",
            &[("id", "id"), ("name", "name")],
        ),
    ];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect("renaming both ends of a foreign key stays plannable");

    let fk_item = plan
        .iter()
        .find(|item| item.kind == DdlPreviewKind::ForeignKey)
        .expect("the foreign key is planned");
    assert_eq!(fk_item.source_table, "orders");
    assert_eq!(fk_item.target_table, "archive_orders");
    // `depends_on` names SOURCE tables on purpose: both consumers
    // (`commands/data_transfer/exec/execution.rs` and `data_transfer/sql_file.rs`)
    // resolve it against the selected *source* tables. A target name here would
    // silently disable the "parent table must also be selected" guard whenever a
    // table is renamed.
    assert_eq!(fk_item.depends_on, vec!["customers"]);
    assert!(
        fk_item
            .ddl
            .contains(r#"ALTER TABLE "archive"."archive_orders""#),
        "{}",
        fk_item.ddl
    );
    assert!(
        fk_item
            .ddl
            .contains(r#"REFERENCES "archive"."archive_customers" ("id")"#),
        "{}",
        fk_item.ddl
    );
    assert!(
        !fk_item.ddl.contains("public.customers"),
        "the source schema qualifier never leaks into DDL: {}",
        fk_item.ddl
    );
    let parent = plan
        .iter()
        .find(|item| item.kind == DdlPreviewKind::Table && item.source_table == "customers")
        .expect("the renamed parent table is created");
    assert_eq!(parent.target_table, "archive_customers");
}

/// Two unrelated children pointing at two different parents, both constraints
/// named `fk_shared`, so the FK half of the naming gate is the only variable.
fn duplicate_fk_schemas() -> HashMap<String, TableSchema> {
    let mut orders = schema("orders", &[("id", true), ("customer_id", false)]);
    orders.foreign_keys.push(fk(
        "fk_shared",
        &["customer_id"],
        "public.customers",
        &["id"],
    ));
    let mut refunds = schema("refunds", &[("id", true), ("customer_id", false)]);
    refunds.foreign_keys.push(fk(
        "fk_shared",
        &["customer_id"],
        "public.customers",
        &["id"],
    ));
    HashMap::from([
        ("orders".into(), orders),
        ("refunds".into(), refunds),
        (
            "customers".into(),
            schema("customers", &[("id", true), ("name", false)]),
        ),
    ])
}

#[test]
fn duplicate_foreign_key_names_are_accepted_when_the_target_scopes_fk_names_per_table() {
    let schemas = duplicate_fk_schemas();
    let mappings = vec![
        mapping(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        mapping(
            "refunds",
            "refunds",
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
        inspected(
            "refunds",
            "refunds",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        inspected("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];

    let plan = build_database_structure_plan(
        &Source,
        &TableScopedFkTarget,
        &job(mappings),
        &inspected,
        &schemas,
    )
    .expect("per-table constraint names may repeat across tables");

    let fk_tables: Vec<&str> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::ForeignKey)
        .map(|item| item.target_table.as_str())
        .collect();
    assert_eq!(fk_tables.len(), 2);
    assert!(fk_tables.contains(&"orders"), "{fk_tables:?}");
    assert!(fk_tables.contains(&"refunds"), "{fk_tables:?}");
    for item in plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::ForeignKey)
    {
        assert_eq!(emitted_name(&item.ddl, "ADD CONSTRAINT "), "fk_shared");
        assert_eq!(item.depends_on, vec!["customers"]);
    }
}

#[test]
fn duplicate_foreign_key_names_are_rejected_when_the_target_scopes_names_per_schema() {
    let schemas = duplicate_fk_schemas();
    let mappings = vec![
        mapping(
            "orders",
            "orders",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        mapping(
            "refunds",
            "refunds",
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
        inspected(
            "refunds",
            "refunds",
            &[("id", "id"), ("customer_id", "customer_id")],
        ),
        inspected("customers", "customers", &[("id", "id"), ("name", "name")]),
    ];

    let error =
        build_database_structure_plan(&Source, &Target, &job(mappings), &inspected, &schemas)
            .expect_err("schema-scoped constraint names must not be emitted twice");

    let message = error.to_string();
    assert!(
        message.contains("target foreign key name 'fk_shared' collides between"),
        "{message}"
    );
    assert!(
        message.contains("rename one source object before transfer"),
        "{message}"
    );
}

/// Both children keep identity renames; only the *target table* spelling differs
/// in case, which is the only signal a table-scoped namespace has.
fn case_only_target_tables() -> HashMap<String, TableSchema> {
    HashMap::from([
        (
            "upper".into(),
            with_index(
                schema("upper", &[("id", true), ("ref", false)]),
                "ix_shared",
                &["ref"],
                false,
            ),
        ),
        (
            "lower".into(),
            with_index(
                schema("lower", &[("id", true), ("ref", false)]),
                "ix_shared",
                &["ref"],
                false,
            ),
        ),
    ])
}

fn case_only_selection() -> (
    Vec<super::model::TableMapping>,
    Vec<super::model::TableInspectResult>,
) {
    let columns = [("id", "id"), ("ref", "ref")];
    (
        vec![
            mapping("upper", "Ledger", &columns),
            mapping("lower", "ledger", &columns),
        ],
        vec![
            inspected("upper", "Ledger", &columns),
            inspected("lower", "ledger", &columns),
        ],
    )
}

#[test]
fn a_table_scoped_case_sensitive_target_namespaces_indexes_by_the_exact_table_name() {
    let schemas = case_only_target_tables();
    let (mappings, inspected) = case_only_selection();

    let plan = build_database_structure_plan(
        &Source,
        &TableScopedCaseSensitiveTarget,
        &job(mappings),
        &inspected,
        &schemas,
    )
    .expect("`Ledger` and `ledger` are different tables on a case-preserving target");

    let index_tables: Vec<&str> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::Index)
        .map(|item| item.target_table.as_str())
        .collect();
    assert_eq!(index_tables.len(), 2, "{index_tables:?}");
    assert!(index_tables.contains(&"Ledger"), "{index_tables:?}");
    assert!(index_tables.contains(&"ledger"), "{index_tables:?}");
}

#[test]
fn a_table_scoped_case_folding_target_collides_on_the_folded_table_namespace() {
    let schemas = case_only_target_tables();
    let (mappings, inspected) = case_only_selection();

    let error = build_database_structure_plan(
        &Source,
        &TableScopedIndexTarget,
        &job(mappings),
        &inspected,
        &schemas,
    )
    .expect_err("case-folding folds the table namespace, so both indexes share one namespace");

    let message = error.to_string();
    assert!(
        message.contains("target index name 'ix_shared' collides between"),
        "{message}"
    );
    assert!(message.contains("upper.Ledger"), "{message}");
    assert!(message.contains("lower.ledger"), "{message}");
}

#[test]
fn a_selection_without_any_enabled_table_plans_nothing_instead_of_failing() {
    let records = with_index(
        schema("records", &[("id", true), ("label", false)]),
        "ix_records_label",
        &["label"],
        false,
    );
    let schemas = HashMap::from([("records".into(), records)]);
    let mut disabled = inspected("records", "records", &[("id", "id"), ("label", "label")]);
    disabled.enabled = false;
    let mappings = vec![mapping(
        "records",
        "records",
        &[("id", "id"), ("label", "label")],
    )];

    let plan =
        build_database_structure_plan(&Source, &Target, &job(mappings), &[disabled], &schemas)
            .expect("an empty selection is a valid, empty plan");

    assert!(plan.is_empty(), "{plan:?}");
}
