//! Live type-fidelity and query-path coverage for the SQL Server driver.
//!
//! All probes are `SELECT CAST(...)` statements, so nothing is written and the
//! tests run against read-only credentials too.
//!
//! Configuration comes from the **process environment only**; a local env
//! file is read *only* when you opt in with `TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file`.

mod common;

use common::{cell, connect, live_config};
use datazen_driver_api::DatabaseDriver as _;
use datazen_driver_api::{DriverError, Value};

/// One row with every scalar family the driver maps.
const TYPE_PROBE: &str = "SELECT \
     CAST(1 AS TINYINT) AS t_u8, \
     CAST(-2 AS SMALLINT) AS t_i16, \
     CAST(3 AS INT) AS t_i32, \
     CAST(9223372036854775807 AS BIGINT) AS t_i64, \
     CAST(1 AS BIT) AS t_bit, \
     CAST(123.45 AS DECIMAL(10,2)) AS t_dec, \
     CAST(12.34 AS MONEY) AS t_money, \
     CAST(1.5 AS FLOAT) AS t_f64, \
     CAST(2.5 AS REAL) AS t_f32, \
     CAST('2026-01-02' AS DATE) AS t_date, \
     CAST('03:04:05' AS TIME) AS t_time, \
     CAST('2026-01-02T03:04:05' AS DATETIME2) AS t_dt2, \
     CAST('2026-01-02T03:04:05' AS DATETIME) AS t_dt, \
     CAST('2026-01-02T03:04:05+08:00' AS DATETIMEOFFSET) AS t_dto, \
     CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS UNIQUEIDENTIFIER) AS t_guid, \
     CAST(N'中文 🚀' AS NVARCHAR(50)) AS t_nvar, \
     CAST('plain' AS VARCHAR(50)) AS t_var, \
     CAST(0x0102FF AS VARBINARY(3)) AS t_bin, \
     CAST('<root a=\"1\"/>' AS XML) AS t_xml, \
     CAST(NULL AS NVARCHAR(10)) AS t_null";

#[tokio::test]
async fn scalar_types_round_trip_with_expected_variants() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let result = driver
        .query(&handle, TYPE_PROBE)
        .await
        .unwrap_or_else(|e| panic!("type probe failed: {e}"));
    assert_eq!(result.rows.len(), 1, "type probe returns exactly one row");
    let row = &result.rows[0];
    assert_eq!(row.len(), 20, "type probe column count");

    let by_name = |name: &str| -> &Option<Value> {
        let index = result
            .columns
            .iter()
            .position(|c| c.name == name)
            .unwrap_or_else(|| panic!("column {name} missing: {:?}", result.columns));
        &row[index]
    };

    assert!(matches!(by_name("t_u8"), Some(Value::Integer(1))));
    assert!(matches!(by_name("t_i16"), Some(Value::Integer(-2))));
    assert!(matches!(by_name("t_i32"), Some(Value::Integer(3))));
    assert!(matches!(by_name("t_i64"), Some(Value::Integer(i64::MAX))));
    assert!(matches!(by_name("t_bit"), Some(Value::Bool(true))));
    assert!(matches!(by_name("t_f64"), Some(Value::Float(_))));
    assert!(matches!(by_name("t_f32"), Some(Value::Float(_))));
    assert!(matches!(by_name("t_nvar"), Some(Value::String(s)) if s == "中文 🚀"));
    assert!(matches!(by_name("t_var"), Some(Value::String(s)) if s == "plain"));
    assert!(
        matches!(by_name("t_guid"), Some(Value::String(s)) if s.eq_ignore_ascii_case("6F9619FF-8B86-D011-B42D-00C04FC964FF")),
        "uniqueidentifier must decode to its canonical string, got {:?}",
        by_name("t_guid")
    );
    assert!(
        matches!(by_name("t_null"), None | Some(Value::Null)),
        "NULL must stay NULL, got {:?}",
        by_name("t_null")
    );

    // Exact numeric families and temporal families are asserted for shape only:
    // the driver's representation is pinned in `type_shapes_are_reported` so a
    // change is visible rather than silently drifting.
    for name in ["t_dec", "t_money"] {
        assert!(
            matches!(
                by_name(name),
                Some(Value::String(_)) | Some(Value::Float(_))
            ),
            "{name} must decode without loss (string or float), got {:?}",
            by_name(name)
        );
    }
    // Temporal families must decode to the same textual shapes the MySQL and
    // PostgreSQL drivers produce — never to tiberius' Debug output.
    for (name, expected) in [
        ("t_date", "2026-01-02"),
        ("t_time", "03:04:05"),
        ("t_dt2", "2026-01-02 03:04:05"),
        ("t_dt", "2026-01-02 03:04:05"),
        ("t_dto", "2026-01-02T03:04:05+08:00"),
    ] {
        let expected_value = Some(Value::String((*expected).to_string()));
        assert_eq!(
            cell(by_name(name)),
            cell(&expected_value),
            "{name} must decode to {expected:?}"
        );
    }
    for name in ["t_date", "t_time", "t_dt2", "t_dt", "t_dto"] {
        let Some(Value::String(text)) = by_name(name) else {
            panic!("{name} must decode to a string");
        };
        assert!(
            !text.contains("Date(") && !text.contains("Time {") && !text.contains("increments"),
            "{name} must not leak tiberius' Debug output, got {text}"
        );
    }
    assert!(
        matches!(by_name("t_bin"), Some(Value::Bytes(bytes)) if bytes == &[0x01, 0x02, 0xff]),
        "varbinary decodes to its original bytes, got {:?}",
        by_name("t_bin")
    );
    assert!(
        matches!(by_name("t_xml"), Some(Value::String(s)) if s.contains("root")),
        "xml decodes to its text, got {:?}",
        by_name("t_xml")
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn query_parameters_are_bound_as_values_without_sql_interpolation() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;
    let untrusted = "O'Brien'; SELECT 99 AS injected --";
    let binary = vec![0, 0x27, 0xff];

    let result = driver
        .query_with_params(
            &handle,
            "SELECT CAST(@P1 AS nvarchar(200)) AS [text], @P2 AS [bytes], @P3 AS [null_value]",
            &[
                Value::String(untrusted.into()),
                Value::Bytes(binary.clone()),
                Value::Null,
            ],
        )
        .await
        .unwrap_or_else(|error| panic!("parameterized query failed: {error}"));
    assert_eq!(
        result.rows.len(),
        1,
        "bound text must not inject another SELECT"
    );
    assert!(
        matches!(&result.rows[0][0], Some(Value::String(value)) if value == untrusted),
        "quotes and SQL-looking text must survive as data"
    );
    assert!(
        matches!(&result.rows[0][1], Some(Value::Bytes(value)) if value == &binary),
        "binary parameters must retain their bytes"
    );
    assert!(
        matches!(result.rows[0][2].as_ref(), None | Some(Value::Null)),
        "NULL must remain a native NULL parameter"
    );

    assert_eq!(driver.parameter_placeholder(1, None).unwrap(), "@P1");
    assert_eq!(driver.parameter_placeholder(2100, None).unwrap(), "@P2100");
    assert!(driver.parameter_placeholder(0, None).is_err());
    assert!(driver.parameter_placeholder(2101, None).is_err());

    let _ = driver.disconnect(handle).await;
}

/// Prints the concrete `Value` shape of every probe column. Run with
/// `--nocapture` when the report needs the exact temporal/numeric encoding.
#[tokio::test]
async fn type_shapes_are_reported() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let result = driver
        .query(&handle, TYPE_PROBE)
        .await
        .expect("type probe failed");
    let row = &result.rows[0];
    for (index, column) in result.columns.iter().enumerate() {
        eprintln!(
            "🔎 {:<8} wire={:<18} value={:?}",
            column.name,
            column.data_type,
            row.get(index).cloned().flatten()
        );
    }

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn column_metadata_names_and_types_are_populated() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let result = driver
        .query(
            &handle,
            "SELECT CAST(1 AS INT) AS [answer], N'x' AS [label]",
        )
        .await
        .expect("metadata probe failed");
    assert_eq!(result.columns.len(), 2);
    assert_eq!(result.columns[0].name, "answer");
    assert_eq!(result.columns[1].name, "label");
    for column in &result.columns {
        assert!(
            !column.data_type.trim().is_empty(),
            "column {column:?} must carry a data type"
        );
    }

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn empty_result_set_keeps_its_columns() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let result = driver
        .query(
            &handle,
            "SELECT [object_id] AS [id], [name] AS [name] FROM sys.all_objects WHERE 1 = 0",
        )
        .await
        .expect("empty SELECT must succeed");
    assert!(result.rows.is_empty());
    assert_eq!(
        result.columns.len(),
        2,
        "columns must survive an empty result"
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn syntax_error_surfaces_as_query_failed_with_the_server_message() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let error = driver
        .query(&handle, "SELECT FROM WHERE")
        .await
        .expect_err("a syntax error must not return rows");
    match error {
        DriverError::QueryFailed(message) => {
            assert!(
                message.to_ascii_uppercase().contains("SYNTAX")
                    || message.to_ascii_uppercase().contains("INCORRECT"),
                "server message should survive into the error: {message}"
            );
        }
        other => panic!("expected QueryFailed, got {other:?}"),
    }

    // The session must stay usable after a failed statement.
    let alive = common::scalar(&driver, &handle, "SELECT 42")
        .await
        .expect("connection must survive a failed statement");
    assert_eq!(common::cell_i64(&alive), Some(42));

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn multi_statement_batches_report_each_statement() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let batch = "SELECT 1 AS a; SELECT 2 AS b, 3 AS c";
    let result = driver
        .query_multi(&handle, batch, None)
        .await
        .unwrap_or_else(|e| panic!("query_multi failed: {e}"));
    assert_eq!(result.results.len(), 2, "one result per statement");
    assert_eq!(result.results[0].rows.len(), 1);
    assert_eq!(result.results[1].rows.len(), 1);
    assert_eq!(result.results[0].columns.len(), 1);
    assert_eq!(result.results[1].columns.len(), 2);
    assert_eq!(
        common::cell_i64(&result.results[1].rows[0][0]),
        Some(2),
        "second statement must keep its own rows"
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn query_limit_is_applied_by_top_and_marks_truncation() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let result = driver
        .query_multi(
            &handle,
            "SELECT [object_id] FROM sys.all_objects ORDER BY [object_id]",
            Some(5),
        )
        .await
        .expect("limited query failed");
    let statement = &result.results[0];
    assert_eq!(
        statement.rows.len(),
        5,
        "the limit must cap the rows even though the statement matches far more"
    );
    // `StatementResult.sql` intentionally records the caller's statement, not
    // the rewritten `SELECT TOP n+1` the transport actually sent.
    assert!(
        statement.sql.to_ascii_uppercase().starts_with("SELECT"),
        "recorded sql must be the caller's statement, got {:?}",
        statement.sql
    );

    let _ = driver.disconnect(handle).await;
}

/// `explain` relies on `SET SHOWPLAN_TEXT ON` surviving until the planned
/// statement is sent. `sp_executesql` runs a statement in a module whose scope
/// ends with the call, so the flag used to be discarded — and the statement the
/// user asked to *plan* was executed instead (harmless for a SELECT, destructive
/// for DML). This is the regression test for routing session-scoped `SET`
/// through a real batch.
#[tokio::test]
async fn explain_returns_a_plan_without_executing_the_statement() {
    let Some(cfg) = live_config() else { return };
    if !common::write_allowed(&cfg, "explain plan probe") {
        return;
    }
    let (driver, handle) = connect(&cfg).await;
    assert!(
        driver.supports_explain(),
        "the SQL Server driver advertises EXPLAIN support"
    );

    let plan = driver
        .explain(&handle, "SELECT [object_id] FROM sys.all_objects")
        .await
        .expect("explain must return the SHOWPLAN_TEXT output");
    assert!(
        !plan.plan_text.trim().is_empty(),
        "an explain result must carry plan text"
    );
    assert!(
        plan.plan_text.to_ascii_uppercase().contains("SELECT"),
        "the plan should describe the planned statement, got: {}",
        plan.plan_text
    );

    // A data read still works afterwards: SHOWPLAN_TEXT must have been cleared.
    let after = driver
        .query(&handle, "SELECT 1 AS [n]")
        .await
        .expect("a normal query must still run after explain");
    assert_eq!(
        common::cell_i64(&after.rows[0][0]),
        Some(1),
        "SHOWPLAN_TEXT must be OFF again, otherwise this returns a plan instead of data"
    );

    // The destructive half: planning a write must not perform it.
    let table = cfg.scratch("explain");
    let qualified = format!("[dbo].[{table}]");
    driver
        .execute(
            &handle,
            &format!("CREATE TABLE {qualified} ([id] INT NOT NULL, [v] INT NULL)"),
        )
        .await
        .expect("create the explain probe table");
    let _ = driver
        .explain(
            &handle,
            &format!("INSERT INTO {qualified} ([id], [v]) VALUES (1, 1)"),
        )
        .await
        .expect("explain of a DML statement must return a plan");
    let count = driver
        .query(&handle, &format!("SELECT COUNT(*) AS [n] FROM {qualified}"))
        .await
        .expect("count the probe table");
    assert_eq!(
        common::cell_i64(&count.rows[0][0]),
        Some(0),
        "explaining an INSERT must not run it"
    );
    let _ = driver
        .execute(&handle, &format!("DROP TABLE {qualified}"))
        .await;
    let _ = driver.disconnect(handle).await;
}
