//! Tests for the result-grid write-back rules.
//!
//! The negative cases are table driven on purpose: every shape that must stay
//! read-only gets its own entry, and each entry asserts the **exact** reason
//! — asserting only "not writable" would let two different reasons share one
//! bug.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use crate::schema::CachedColumns;
use datazen_driver_api::{
    async_trait, BoundSqlStatement, ColumnSchema, ConnectionConfig, ConnectionHandle,
    DatabaseDriver, DatabaseType, DriverError, MultiQueryResult, QueryResult, ResultColumnOrigin,
    ResultObjectKind, ResultOrigin, ResultOriginDetail, ResultShape, ResultTableRef, ServerInfo,
    TableInfo, TableSchema, Value,
};

use super::*;

/// A relation the result reads from.
fn table() -> ResultTableRef {
    ResultTableRef {
        catalog: None,
        schema: None,
        name: "orders".to_string(),
        aliased: false,
    }
}

/// The same relation, addressed by an alias in the statement.
fn aliased_table() -> ResultTableRef {
    ResultTableRef {
        catalog: None,
        schema: None,
        name: "orders".to_string(),
        aliased: true,
    }
}

/// One result column that is a straight copy of a base column.
fn passthrough(column: &str, source: &str) -> ResultColumnOrigin {
    ResultColumnOrigin {
        column: column.to_string(),
        passthrough: true,
        table: Some(table()),
        source_column: Some(source.to_string()),
    }
}

/// The report a driver would send for `SELECT id, total FROM orders`.
fn single_table_origin() -> ResultOrigin {
    ResultOrigin::Determined(ResultOriginDetail {
        shape: ResultShape::SingleBaseTable,
        table: Some(table()),
        referenced_tables: 1,
        deduplicated: false,
        aggregated: false,
        set_operation: None,
        has_derived_source: false,
        object_kind: ResultObjectKind::BaseTable,
        columns: vec![passthrough("id", "id"), passthrough("total", "total")],
    })
}

/// The key of the source table.
fn keys() -> Vec<String> {
    vec!["id".to_string()]
}

/// The column names the executed result reports.
fn result_columns() -> Vec<String> {
    vec!["id".to_string(), "total".to_string()]
}

/// The catalog answer for the source table.
fn cached_columns() -> CachedColumns {
    CachedColumns {
        columns: vec![
            ColumnSchema {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "total".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: keys(),
        table_name: "orders".to_string(),
        cached_at: Instant::now(),
    }
}

/// Mutate the report of a single-table result.
fn with(
    mut detail: ResultOriginDetail,
    change: impl FnOnce(&mut ResultOriginDetail),
) -> ResultOrigin {
    change(&mut detail);
    ResultOrigin::Determined(detail)
}

/// The detail of the single-table report, for mutation.
fn single_table_detail() -> ResultOriginDetail {
    match single_table_origin() {
        ResultOrigin::Determined(detail) => detail,
        ResultOrigin::Undeterminable(_) => unreachable!("the fixture is determined"),
    }
}

#[test]
fn evaluate_single_base_table_with_pk_is_writable() {
    let verdict =
        evaluate_result_editability(&single_table_origin(), &keys(), &result_columns(), true);

    assert!(verdict.writable);
    assert_eq!(verdict.reason, None);
    assert_eq!(
        verdict.table.as_ref().map(|t| t.name.as_str()),
        Some("orders")
    );
    assert_eq!(
        verdict.identity_columns,
        vec![IdentityColumn {
            column: "id".to_string(),
            source_column: "id".to_string(),
        }]
    );
    assert_eq!(
        verdict.editable_columns.len(),
        2,
        "both columns are straight copies of base columns"
    );
}

/// Every negative shape, with the reason it must produce.
#[test]
fn every_unwritable_shape_has_its_own_reason() {
    let join = with(single_table_detail(), |detail| {
        detail.shape = ResultShape::MultiTable;
        detail.table = None;
        detail.referenced_tables = 2;
        // A result that reads from several relations attributes no column to
        // one of them.
        for column in &mut detail.columns {
            column.table = None;
        }
    });
    let aggregate = with(single_table_detail(), |detail| detail.aggregated = true);
    let distinct = with(single_table_detail(), |detail| detail.deduplicated = true);
    let union = with(single_table_detail(), |detail| {
        detail.set_operation = Some(datazen_driver_api::SetOperation::Union);
    });
    let derived = with(single_table_detail(), |detail| {
        detail.has_derived_source = true
    });
    let view = with(single_table_detail(), |detail| {
        detail.object_kind = ResultObjectKind::View
    });
    let materialized = with(single_table_detail(), |detail| {
        detail.object_kind = ResultObjectKind::MaterializedView;
    });
    let unknown_kind = with(single_table_detail(), |detail| {
        detail.object_kind = ResultObjectKind::Unknown;
    });
    let wrong_column_count = with(single_table_detail(), |detail| {
        detail.columns.pop();
    });
    let duplicate_column = with(single_table_detail(), |detail| {
        detail.columns[1].column = "id".to_string();
    });
    let missing_source = with(single_table_detail(), |detail| {
        detail.columns[0].source_column = None;
    });
    let foreign_table = with(single_table_detail(), |detail| {
        detail.columns[0].table = Some(ResultTableRef {
            catalog: None,
            schema: None,
            name: "other".to_string(),
            aliased: false,
        });
    });
    let contradictory_count = with(single_table_detail(), |detail| detail.referenced_tables = 2);
    let contradictory_table = with(single_table_detail(), |detail| detail.table = None);
    let unknown_shape = with(single_table_detail(), |detail| {
        detail.shape = ResultShape::Unknown
    });
    let key_not_passthrough = with(single_table_detail(), |detail| {
        detail.columns[0].passthrough = false;
        detail.columns[0].source_column = None;
    });
    let key_absent = with(single_table_detail(), |detail| {
        // The result has two columns, neither of which is the key.
        detail.columns[0].column = "other".to_string();
        detail.columns[0].source_column = Some("other".to_string());
    });

    let cases: Vec<(&str, ResultOrigin, Vec<String>, bool, GridReadOnlyReason)> = vec![
        ("a join", join, keys(), true, GridReadOnlyReason::MultiTable),
        (
            "an aggregate",
            aggregate,
            keys(),
            true,
            GridReadOnlyReason::Aggregate,
        ),
        (
            "a distinct",
            distinct,
            keys(),
            true,
            GridReadOnlyReason::Distinct,
        ),
        (
            "a union",
            union,
            keys(),
            true,
            GridReadOnlyReason::SetOperation,
        ),
        (
            "a derived table",
            derived,
            keys(),
            true,
            GridReadOnlyReason::DerivedSource,
        ),
        ("a view", view, keys(), true, GridReadOnlyReason::View),
        (
            "a materialized view",
            materialized,
            keys(),
            true,
            GridReadOnlyReason::View,
        ),
        (
            "an unconfirmed relation",
            unknown_kind,
            keys(),
            true,
            GridReadOnlyReason::UnknownOrigin,
        ),
        (
            "a column count that does not match the result",
            wrong_column_count,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "a duplicated column name",
            duplicate_column,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "a straight copy without a source column",
            missing_source,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "a straight copy from another relation",
            foreign_table,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "a shape that contradicts the relation count",
            contradictory_count,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "a shape that contradicts the relation",
            contradictory_table,
            keys(),
            true,
            GridReadOnlyReason::AmbiguousOrigin,
        ),
        (
            "an unknown shape",
            unknown_shape,
            keys(),
            true,
            GridReadOnlyReason::UnknownOrigin,
        ),
        (
            "a key column that is not a straight copy",
            key_not_passthrough,
            keys(),
            true,
            GridReadOnlyReason::KeyNotSelected,
        ),
        (
            "a key column that is not in the result",
            key_absent,
            keys(),
            true,
            GridReadOnlyReason::KeyNotSelected,
        ),
        (
            "a table without a primary key",
            single_table_origin(),
            Vec::new(),
            true,
            GridReadOnlyReason::NoPrimaryKey,
        ),
        (
            "a read-only session",
            single_table_origin(),
            keys(),
            false,
            GridReadOnlyReason::SessionReadOnly,
        ),
        (
            "a driver that cannot tell",
            ResultOrigin::Undeterminable(datazen_driver_api::UndeterminableReason::NotImplemented),
            keys(),
            true,
            GridReadOnlyReason::UnknownOrigin,
        ),
        (
            "a driver that cannot handle this dialect",
            ResultOrigin::Undeterminable(
                datazen_driver_api::UndeterminableReason::UnsupportedDialect,
            ),
            keys(),
            true,
            GridReadOnlyReason::UnknownOrigin,
        ),
    ];

    for (name, origin, primary_keys, session_writable, expected) in cases {
        let verdict = evaluate_result_editability(
            &origin,
            &primary_keys,
            &result_columns(),
            session_writable,
        );
        assert!(!verdict.writable, "{name} must not be writable");
        assert_eq!(
            verdict.reason,
            Some(expected),
            "{name} has the wrong reason"
        );
        assert!(
            verdict.editable_columns.is_empty(),
            "{name} must not report editable columns"
        );
    }
}

/// The locating key columns survive a refusal.
///
/// They are a fact about the table, not about the verdict: the front end needs
/// them to decide whether a *row* can be addressed (the key value must not be
/// NULL, and no other row may share the identity), and those facts do not
/// depend on the result being writable.
#[test]
fn a_refused_verdict_still_reports_the_locating_key_columns() {
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(datazen_driver_api::UndeterminableReason::NotImplemented),
        &keys(),
        &result_columns(),
        true,
    );

    assert!(!verdict.writable);
    assert_eq!(
        verdict.identity_columns,
        vec![IdentityColumn {
            column: "id".to_string(),
            source_column: "id".to_string(),
        }],
        "the catalog's key list locates the row even when the driver cannot describe the result"
    );
    assert!(verdict.editable_columns.is_empty());
}

/// A key that is not among the result's columns locates nothing.
#[test]
fn a_key_that_is_not_in_the_result_reports_no_locating_column() {
    let mut columns = result_columns();
    columns[0] = "other".to_string();
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(datazen_driver_api::UndeterminableReason::NotImplemented),
        &keys(),
        &columns,
        true,
    );

    assert!(
        verdict.identity_columns.is_empty(),
        "a key the result does not show cannot locate a row"
    );
}

#[test]
fn an_aliased_key_column_is_still_writable() {
    let origin = ResultOrigin::Determined(ResultOriginDetail {
        shape: ResultShape::SingleBaseTable,
        table: Some(aliased_table()),
        referenced_tables: 1,
        deduplicated: false,
        aggregated: false,
        set_operation: None,
        has_derived_source: false,
        object_kind: ResultObjectKind::BaseTable,
        columns: vec![
            ResultColumnOrigin {
                column: "order_id".to_string(),
                passthrough: true,
                table: Some(aliased_table()),
                source_column: Some("id".to_string()),
            },
            ResultColumnOrigin {
                column: "total".to_string(),
                passthrough: true,
                table: Some(aliased_table()),
                source_column: Some("total".to_string()),
            },
        ],
    });

    let verdict = evaluate_result_editability(&origin, &keys(), &result_columns(), true);

    assert!(verdict.writable);
    assert_eq!(
        verdict.identity_columns,
        vec![IdentityColumn {
            column: "order_id".to_string(),
            source_column: "id".to_string(),
        }],
        "the write-back uses the real column name, not the alias"
    );
}

#[test]
fn a_read_only_session_wins_over_every_other_fact() {
    let verdict =
        evaluate_result_editability(&single_table_origin(), &keys(), &result_columns(), false);
    assert_eq!(verdict.reason, Some(GridReadOnlyReason::SessionReadOnly));
}

// ── The write path ─────────────────────────────────────────────────

/// A driver that counts the statements it was asked to render.
///
/// Every other method panics: the write-back rules must refuse a bad request
/// **before** any SQL exists, so a test that reaches the database has already
/// failed for the wrong reason.
struct CountingDriver {
    rendered: AtomicU32,
}

impl CountingDriver {
    fn new() -> Self {
        Self {
            rendered: AtomicU32::new(0),
        }
    }

    fn rendered(&self) -> u32 {
        self.rendered.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl DatabaseDriver for CountingDriver {
    fn driver_type(&self) -> DatabaseType {
        "postgres".to_string()
    }

    fn build_update_statement(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        self.rendered.fetch_add(1, Ordering::Relaxed);
        let mut parameters = Vec::new();
        let mut sets = Vec::new();
        for (column, value) in set_columns {
            sets.push(format!("{column} = ?{}", parameters.len() + 1));
            parameters.push(value.clone().unwrap_or(Value::Null));
        }
        let mut wheres = Vec::new();
        for (column, value) in pk_columns {
            wheres.push(format!("{column} = ?{}", parameters.len() + 1));
            parameters.push(value.clone().unwrap_or(Value::Null));
        }
        Ok(BoundSqlStatement {
            sql: format!(
                "UPDATE {table} SET {} WHERE {}",
                sets.join(", "),
                wheres.join(" AND ")
            ),
            parameters,
        })
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        unreachable!("the write-back rules never open a connection")
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        unreachable!("the write-back rules never open a connection")
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        unreachable!("the write-back rules never open a connection")
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        unreachable!("the write-back rules never read a catalog")
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        unreachable!("the write-back rules never read a catalog")
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        unreachable!("the write-back rules never read a catalog")
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        unreachable!("the write-back rules never run a query")
    }

    async fn query_multi(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        unreachable!("the write-back rules never run a query")
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        unreachable!("the write-back rules never run a query")
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        unreachable!("the write-back rules never execute a statement")
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        unreachable!("the write-back rules never cancel a query")
    }
}

/// A well-formed request for one cell of the result.
fn request() -> ResultCellChangeRequest {
    let mut row_identity = BTreeMap::new();
    row_identity.insert("id".to_string(), Some(Value::Integer(7)));
    ResultCellChangeRequest {
        connection_id: "connection-1".to_string(),
        db_session_id: "session-1".to_string(),
        driver_type: "postgres".to_string(),
        database: Some("app".to_string()),
        schema: Some("public".to_string()),
        table: "orders".to_string(),
        sql: "SELECT id, total FROM orders".to_string(),
        result_columns: vec!["id".to_string(), "total".to_string()],
        identity_columns: vec![IdentityColumn {
            column: "id".to_string(),
            source_column: "id".to_string(),
        }],
        row_identity,
        column: "total".to_string(),
        source_column: "total".to_string(),
        original_value: Some(Value::Integer(10)),
        value: Some(Value::Integer(11)),
        row_count: 1,
    }
}

/// The verdict a writable result produces.
fn writable() -> ResultEditability {
    evaluate_result_editability(&single_table_origin(), &keys(), &result_columns(), true)
}

#[test]
fn preview_rejects_empty_row_identity() {
    let driver = CountingDriver::new();
    let mut request = request();
    request.row_identity.clear();

    let error = build_result_cell_change_plan(&driver, &request, &writable(), &cached_columns())
        .expect_err("an empty identity cannot address a row");

    assert!(
        error
            .to_string()
            .starts_with("grid.result.rowNotAddressable: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_rejects_null_identity_value() {
    let driver = CountingDriver::new();
    let mut request = request();
    request
        .row_identity
        .insert("id".to_string(), Some(Value::Null));

    let error = build_result_cell_change_plan(&driver, &request, &writable(), &cached_columns())
        .expect_err("a NULL identity cannot address a row");

    assert!(
        error
            .to_string()
            .starts_with("grid.result.rowNotAddressable: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_rejects_multi_row() {
    let driver = CountingDriver::new();
    let mut request = request();
    request.row_count = 2;

    let error = build_result_cell_change_plan(&driver, &request, &writable(), &cached_columns())
        .expect_err("writing many rows at once is not supported");

    assert!(
        error
            .to_string()
            .starts_with("grid.result.multiRowNotAllowed: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_rejects_identity_key_mismatch() {
    let driver = CountingDriver::new();
    let mut request = request();
    request
        .row_identity
        .insert("other".to_string(), Some(Value::Integer(1)));

    let error = build_result_cell_change_plan(&driver, &request, &writable(), &cached_columns())
        .expect_err("an identity that is not the key cannot address a row");

    assert!(
        error
            .to_string()
            .starts_with("grid.result.rowNotAddressable: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_rejects_a_column_that_is_not_a_straight_copy() {
    let driver = CountingDriver::new();
    let mut request = request();
    request.column = "doubled".to_string();
    request.source_column = "doubled".to_string();

    let error = build_result_cell_change_plan(&driver, &request, &writable(), &cached_columns())
        .expect_err("a computed column is not writable");

    assert!(
        error.to_string().starts_with("grid.result.writeDenied: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_refuses_a_read_only_result() {
    let driver = CountingDriver::new();
    let verdict = evaluate_result_editability(
        &ResultOrigin::Undeterminable(datazen_driver_api::UndeterminableReason::NotImplemented),
        &keys(),
        &result_columns(),
        true,
    );

    let error = build_result_cell_change_plan(&driver, &request(), &verdict, &cached_columns())
        .expect_err("a read-only result cannot be written");

    assert!(
        error
            .to_string()
            .starts_with("grid.result.originUnavailable: "),
        "unexpected error: {error}"
    );
    assert_eq!(driver.rendered(), 0, "no statement may be rendered");
}

#[test]
fn preview_builds_the_plan_for_one_cell() {
    let driver = CountingDriver::new();

    let plan = build_result_cell_change_plan(&driver, &request(), &writable(), &cached_columns())
        .expect("a well-formed request produces a plan");

    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.deletes.len(), 0);
    assert_eq!(plan.updates[0].changed_columns, vec!["total".to_string()]);
    assert!(plan.updates[0].sql_template.contains("WHERE id"));
    assert_eq!(driver.rendered(), 1);
}

#[test]
fn every_read_only_reason_maps_to_a_contract_code() {
    let reasons = [
        GridReadOnlyReason::UnknownOrigin,
        GridReadOnlyReason::AmbiguousOrigin,
        GridReadOnlyReason::MultiTable,
        GridReadOnlyReason::Aggregate,
        GridReadOnlyReason::Distinct,
        GridReadOnlyReason::SetOperation,
        GridReadOnlyReason::DerivedSource,
        GridReadOnlyReason::View,
        GridReadOnlyReason::NoPrimaryKey,
        GridReadOnlyReason::KeyNotSelected,
        GridReadOnlyReason::SessionReadOnly,
    ];
    for reason in reasons {
        let code = read_only_error_code(reason);
        assert!(
            code.as_str().starts_with("grid.result."),
            "{reason:?} maps to {code}"
        );
    }
    assert_eq!(
        read_only_error_code(GridReadOnlyReason::UnknownOrigin).as_str(),
        "grid.result.originUnavailable"
    );
    assert_eq!(
        read_only_error_code(GridReadOnlyReason::AmbiguousOrigin).as_str(),
        "grid.result.originAmbiguous"
    );
}

/// A session context as one projection spells it.
fn write_context() -> crate::execution::SessionContext {
    crate::execution::SessionContext {
        namespace: crate::connection::domain::NamespaceTarget::new(
            Some("app".into()),
            None,
            Some("public".into()),
            Vec::new(),
        ),
        search_path: Some(vec!["public".into()]),
        effective_identity: Some("app_user".into()),
        transaction_state: crate::execution::TransactionState::None,
        autocommit: Some(true),
        confidence: crate::execution::ContextConfidence::Confirmed,
    }
}

/// The gate compares **targets**, so the session mode must not take part.
///
/// The two sides are captured at different moments (before/after a statement)
/// and produced by different projections; a statement that opens a transaction
/// or changes autocommit must not make a verified result unwritable.
#[test]
fn write_target_ignores_the_session_mode() {
    let before = write_context();
    let after = crate::execution::SessionContext {
        transaction_state: crate::execution::TransactionState::Active,
        autocommit: Some(false),
        search_path: Some(vec!["public".into(), "other".into()]),
        confidence: crate::execution::ContextConfidence::Partial,
        ..write_context()
    };
    assert_eq!(WriteTarget::of(&before), WriteTarget::of(&after));
}

/// "No identity was reported" has one meaning under two spellings.
#[test]
fn write_target_treats_an_empty_identity_as_absence() {
    let absent = crate::execution::SessionContext {
        effective_identity: None,
        ..write_context()
    };
    let empty = crate::execution::SessionContext {
        effective_identity: Some(String::new()),
        ..write_context()
    };
    assert_eq!(WriteTarget::of(&absent), WriteTarget::of(&empty));

    let other = crate::execution::SessionContext {
        effective_identity: Some("other_user".into()),
        ..write_context()
    };
    assert_ne!(
        WriteTarget::of(&absent),
        WriteTarget::of(&other),
        "a different identity is a different target"
    );
}

/// Where the write goes: every namespace level, and the path segments as they
/// were reported.
#[test]
fn write_target_names_the_namespace() {
    let base = write_context();
    for changed in [
        crate::execution::SessionContext {
            namespace: crate::connection::domain::NamespaceTarget::new(
                Some("other_app".into()),
                None,
                Some("public".into()),
                Vec::new(),
            ),
            ..write_context()
        },
        crate::execution::SessionContext {
            namespace: crate::connection::domain::NamespaceTarget::new(
                Some("app".into()),
                None,
                Some("other_schema".into()),
                Vec::new(),
            ),
            ..write_context()
        },
        crate::execution::SessionContext {
            namespace: crate::connection::domain::NamespaceTarget::new(
                None,
                Some("app_catalog".into()),
                Some("public".into()),
                Vec::new(),
            ),
            ..write_context()
        },
        crate::execution::SessionContext {
            namespace: crate::connection::domain::NamespaceTarget::new(
                Some("app".into()),
                None,
                Some("public".into()),
                vec!["tenant_a".into()],
            ),
            ..write_context()
        },
    ] {
        assert_ne!(
            WriteTarget::of(&base),
            WriteTarget::of(&changed),
            "a namespace change is a target change"
        );
    }
}

/// An absent level and a level projected to `""` are the same namespace.
#[test]
fn write_target_treats_empty_levels_as_absent() {
    let absent = crate::execution::SessionContext {
        namespace: crate::connection::domain::NamespaceTarget::new(None, None, None, Vec::new()),
        ..write_context()
    };
    let empty = crate::execution::SessionContext {
        namespace: crate::connection::domain::NamespaceTarget::new(
            Some(String::new()),
            Some(String::new()),
            Some(String::new()),
            Vec::new(),
        ),
        ..write_context()
    };
    assert_eq!(WriteTarget::of(&absent), WriteTarget::of(&empty));
}

// ------------------------------------------------ host-boundary 残余收口

/// `confirm_unchanged_read_sql` 的装配面：占位符按序编号、标识符引用交给
/// 方言、多键谓词 AND 拼接、None 身份值落 NULL 参数。断言与桌面原内联
/// 实现的输出逐字节一致（host `result_write.rs` 迁移前行为；MockDriver
/// 默认 `"` 引用 + `?N` 占位符）。
#[test]
fn confirm_unchanged_read_sql_assembles_quoted_predicates_in_order() {
    let driver = datazen_driver_api::mock_driver::MockDriver::new(
        "mysql",
        datazen_driver_api::mock_driver::MockDriverOptions {
            parameterized_writes: true,
            ..Default::default()
        },
    );
    let identity = [("id", Some(Value::Integer(7))), ("tenant", None)];
    let (sql, parameters) =
        confirm_unchanged_read_sql(driver.as_ref(), "orders", "status", &identity).unwrap();
    assert_eq!(
        sql,
        "SELECT \"status\" FROM \"orders\" WHERE \"id\" = ?1 AND \"tenant\" = ?2"
    );
    assert_eq!(parameters.len(), 2);
    assert!(matches!(&parameters[0], Value::Integer(n) if *n == 7));
    assert!(matches!(&parameters[1], Value::Null));
}
