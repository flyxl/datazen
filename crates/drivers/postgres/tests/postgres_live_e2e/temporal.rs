use super::{string, Fixture};
use datazen_driver_api::{DatabaseDriver, Value};
use datazen_driver_postgres::PostgresDriver;

#[tokio::test]
async fn postgres_wire_times_keep_offset_seconds_fraction_and_end_of_day() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real PG wire connection");
    let expressions = [
        "'12:34:56.1234+05:30:15'::timetz",
        "'24:00:00-03'::timetz",
        "'00:00:00+00'::timetz",
        "'08:09:10-04:30'::timetz",
        "'24:00:00'::time",
        "'09:01:00.1001'::time",
        "NULL::timetz",
        "NULL::time",
    ];
    let sql = format!("SELECT {}", expressions.join(","));
    let text_sql = format!(
        "SELECT ARRAY[{}]",
        expressions
            .iter()
            .map(|expr| format!("({expr})::text"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let expected: Vec<Option<String>> = sqlx::query_scalar(&text_sql)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent server text representations");
    assert_eq!(
        expected,
        [
            Some("12:34:56.1234+05:30:15".into()),
            Some("24:00:00-03".into()),
            Some("00:00:00+00".into()),
            Some("08:09:10-04:30".into()),
            Some("24:00:00".into()),
            Some("09:01:00.1001".into()),
            None,
            None
        ]
    );
    let expected: Vec<Option<Value>> = expected
        .into_iter()
        .map(|item| item.and_then(string))
        .collect();
    assert_eq!(
        driver
            .query(&handle, &sql)
            .await
            .expect("actual binary query")
            .rows,
        vec![expected]
    );
    driver.disconnect(handle).await.expect("driver cleanup");
}

#[tokio::test]
async fn postgres_bound_parameters_round_trip_native_catalog_types_and_nullable_values() {
    use serde_json::json;

    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real driver bind connection");
    let relation = format!("public.dz_bound_types_{}", uuid::Uuid::new_v4().simple());
    let amount = "1234567890123456789012345.00123456";
    let text = "contains ' quote and \\ backslash · 雪";
    let cases = [
        (
            "ident",
            "uuid",
            Value::String("e137a508-6655-46a1-8d96-f9b96063ab2a".into()),
            json!("e137a508-6655-46a1-8d96-f9b96063ab2a"),
        ),
        (
            "magnitude",
            "numeric(40,8)",
            Value::String(amount.into()),
            json!(amount),
        ),
        (
            "raw_bigint",
            "bigint",
            Value::Integer(9_007_199_254_740_992),
            json!("9007199254740992"),
        ),
        ("raw_smallint", "smallint", Value::Integer(31), json!("31")),
        (
            "raw_float",
            "double precision",
            Value::Float(-2.125),
            json!("-2.125"),
        ),
        (
            "clock_absolute",
            "timestamp with time zone",
            Value::Timestamp("2024-02-29T16:34:56Z".into()),
            json!("2024-02-29 16:34:56+00"),
        ),
        (
            "clock_local",
            "timestamp without time zone",
            Value::Timestamp("2024-02-29 18:34:56".into()),
            json!("2024-02-29 18:34:56"),
        ),
        (
            "clock_offset",
            "time with time zone",
            Value::Timestamp("12:34:56.0123+05:30".into()),
            json!("12:34:56.0123+05:30"),
        ),
        (
            "clock_simple",
            "time without time zone",
            Value::Timestamp("24:00:00".into()),
            json!("24:00:00"),
        ),
        (
            "event_date",
            "date",
            Value::String("2024-02-29".into()),
            json!("2024-02-29"),
        ),
        (
            "raw_document",
            "json",
            Value::Json(json!({"test":31})),
            json!("31"),
        ),
        (
            "sort_document",
            "jsonb",
            Value::Json(json!({"test":42})),
            json!("42"),
        ),
        (
            "duration",
            "interval",
            Value::String("02:15:00".into()),
            json!("02:15:00"),
        ),
        (
            "node_address",
            "inet",
            Value::String("203.0.113.11/24".into()),
            json!("203.0.113.11/24"),
        ),
        (
            "subnet",
            "cidr",
            Value::String("2001:db8::1/128".into()),
            json!("2001:db8::1/128"),
        ),
        ("payload", "text", Value::String(text.into()), json!(text)),
        ("absent_uuid", "uuid", Value::Null, json!(null)),
        ("empty_literal", "text", Value::String("".into()), json!("")),
    ];
    let columns = cases
        .iter()
        .map(|(name, ty, _, _)| format!("{name} {ty}"))
        .collect::<Vec<_>>()
        .join(",");
    sqlx::query(&format!("CREATE TABLE {relation} ({columns})"))
        .execute(&mut fixture.oracle)
        .await
        .expect("real persisted independent target table");
    let binds = cases
        .iter()
        .enumerate()
        .map(|(n, (_, ty, _, _))| {
            driver
                .parameter_placeholder(n + 1, Some(ty))
                .expect("native bind cast")
        })
        .collect::<Vec<_>>()
        .join(",");
    let params = cases
        .iter()
        .map(|(_, _, value, _)| value.clone())
        .collect::<Vec<_>>();
    let insert = format!("INSERT INTO {relation} VALUES ({binds})");
    assert!(!insert.contains(text) && !insert.contains(amount));
    assert_eq!(
        driver
            .execute_with_params(&handle, &insert, &params)
            .await
            .expect("actual native typed bound write"),
        1
    );

    let independent_select = cases
        .iter()
        .map(|(name, _, _, _)| match *name {
            "raw_document" | "sort_document" => format!("{name}->>'test'"),
            _ => format!("{name}::text"),
        })
        .collect::<Vec<_>>()
        .join(",");
    let observed: serde_json::Value = sqlx::query_scalar(&format!(
        "SELECT to_jsonb(ARRAY[{independent_select}]) FROM {relation}"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("PostgreSQL independently casts stored physical fields");
    assert_eq!(
        observed,
        serde_json::Value::Array(
            cases
                .into_iter()
                .map(|(_, _, _, expected)| expected)
                .collect()
        )
    );
    let materialized = driver
        .query(
            &handle,
            &format!("SELECT absent_uuid,empty_literal,clock_offset,clock_simple FROM {relation}"),
        )
        .await
        .expect("driver decodes the same physical values");
    assert_eq!(
        materialized.rows,
        vec![vec![
            None,
            string(""),
            string("12:34:56.0123+05:30"),
            string("24:00:00")
        ]]
    );
    assert_eq!(materialized.columns[0].name, "absent_uuid");
    // An empty result must describe the same prepared statement's types
    // rather than letting PostgreSQL infer every otherwise untyped param
    // as text during a separate metadata query.
    let typed_parameters = [
        Value::Integer(37),
        Value::Bool(true),
        Value::Bytes(vec![0x00, 0xff]),
        Value::Float(2.5),
        Value::Json(json!({"wire":true})),
        Value::Timestamp("2024-02-29 12:34:56".into()),
        Value::Null,
    ];
    let bind_sql = "SELECT $1 AS key, $2 AS flag, $3 AS bytes, $4 AS number, $5 AS doc, $6 AS clock, $7 AS absent";
    let types = |result: &datazen_driver_api::QueryResult| {
        result
            .columns
            .iter()
            .map(|column| (column.name.clone(), column.data_type.clone()))
            .collect::<Vec<_>>()
    };
    let nonempty = driver
        .query_with_params(&handle, bind_sql, &typed_parameters)
        .await
        .expect("actual prepared row");
    let empty_sql = format!("{bind_sql} WHERE false");
    let empty = driver
        .query_with_params(&handle, &empty_sql, &typed_parameters)
        .await
        .expect("no physical prepared rows");
    assert!(empty.rows.is_empty());
    assert_eq!(
        types(&empty),
        types(&nonempty),
        "prepared metadata must use the same bound types"
    );
    let retained = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("fixed query target");
    let retained_empty = driver
        .query_with_params(&retained, &empty_sql, &typed_parameters)
        .await
        .expect("same result schema on pinned session");
    assert_eq!(types(&retained_empty), types(&nonempty));
    driver
        .disconnect(retained)
        .await
        .expect("release retained metadata session");
    sqlx::query(&format!("DROP TABLE {relation}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose physical bind fixture");
    driver
        .disconnect(handle)
        .await
        .expect("close bound connection");
}
