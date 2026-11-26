use super::Fixture;
use datazen_driver_api::{
    result_origin::{ResultOrigin, ResultShape},
    sql_builder::SetColumnValue,
    DatabaseDriver, SqlTarget, Value, WriteExpression,
};
use serde_json::json;

#[tokio::test]
async fn mysql_result_grid_binds_native_defaults_and_pages_exact_order_from_real_catalog() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_grid_{}", uuid::Uuid::new_v4().simple());
    let target = std::env::var("TEST_MYSQL_DATABASE_B").unwrap();
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY AUTO_INCREMENT, score INT NOT NULL DEFAULT 2, note VARCHAR(60) DEFAULT 'original', stamp DATETIME NULL, body VARBINARY(20));
         CREATE TABLE {target}.{table} LIKE {table};
         INSERT INTO {target}.{table}(id,score,note) VALUES(1,99,'decoy');"
    )).execute(&mut f.oracle).await.expect("actual A/B fixtures for edit ownership");
    let default = driver
        .build_insert_statement(&table, &[])
        .expect("native all-default insert renderer");
    assert_eq!(
        driver
            .execute_with_params(&handle, &default.sql, &default.parameters)
            .await
            .unwrap(),
        1
    );
    let expr = driver
        .native_write_expression_sql(WriteExpression::CurrentTimestamp)
        .expect("MySQL native current timestamp");
    let update = driver
        .build_update_statement_with_expressions(
            &table,
            &[
                (
                    "note",
                    SetColumnValue::Literal(Some(Value::String("s'now".into()))),
                ),
                (
                    "body",
                    SetColumnValue::Literal(Some(Value::Bytes(vec![0, 255, 7]))),
                ),
                ("stamp", SetColumnValue::Expression(expr)),
            ],
            &[("id", Some(Value::Integer(1)))],
        )
        .expect("prepare typed UI edit");
    assert_eq!(
        driver
            .execute_with_params(&handle, &update.sql, &update.parameters)
            .await
            .unwrap(),
        1
    );
    let real: (String, Vec<u8>, i32) = sqlx::query_as(&format!(
        "SELECT note,body,CAST(stamp IS NOT NULL AS SIGNED) FROM {table} WHERE id=1"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("authoritative MySQL values");
    assert_eq!(real, ("s'now".into(), vec![0, 255, 7], 1));
    let decoy: String = sqlx::query_scalar(&format!("SELECT note FROM {target}.{table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(decoy, "decoy");
    for (score, note) in [
        (2, "same"),
        (3, "later"),
        (1, "early"),
        (3, "tie"),
        (2, "another"),
    ] {
        let inserted = driver
            .build_insert_statement(
                &table,
                &[
                    ("score", Some(Value::Integer(score))),
                    ("note", Some(Value::String(note.into()))),
                ],
            )
            .expect("native bound multi-column insert");
        driver
            .execute_with_params(&handle, &inserted.sql, &inserted.parameters)
            .await
            .expect("server applies typed real edits");
    }
    let read = format!("SELECT id,note FROM {table} WHERE id=1");
    let result = driver.query(&handle, &read).await.unwrap();
    let columns: Vec<_> = result.columns.iter().map(|c| c.name.clone()).collect();
    let origin = driver
        .describe_result_origin(
            &handle,
            SqlTarget::new(Some(&f.database), None),
            &columns,
            &read,
        )
        .await
        .expect("real catalog-backed data grid writeability");
    let ResultOrigin::Determined(origin) = origin else {
        panic!("actual one-table catalog origin")
    };
    assert_eq!(origin.shape, ResultShape::SingleBaseTable);
    let all = driver
        .get_all_columns(&handle, &f.database, None)
        .await
        .expect("real bulk autocomplete catalog");
    assert!(all
        .get(&table)
        .is_some_and(|(fields, pks)| fields.iter().any(|c| c.name == "stamp") && pks == &["id"]));

    let oracle: Vec<(i32, i32)> = sqlx::query_as(&format!(
        "SELECT score,id FROM {table} ORDER BY score ASC,id DESC"
    ))
    .fetch_all(&mut f.oracle)
    .await
    .expect("authoritative same sort including repeated keys");
    let b = oracle[1];
    let predicate = driver
        .keyset_predicate_sql(
            &[("score".into(), false), ("id".into(), true)],
            &[
                ("score".into(), Value::Integer(i64::from(b.0))),
                ("id".into(), Value::Integer(i64::from(b.1))),
            ],
        )
        .expect("MySQL mixed-sort keyset query fragment");
    let page = driver
        .query(
            &handle,
            &format!("SELECT score,id FROM {table} WHERE {predicate} ORDER BY score ASC,id DESC"),
        )
        .await
        .expect("real keyset replays the same sequence as the native oracle");
    let expected: Vec<_> = oracle[2..]
        .iter()
        .map(|(s, i)| {
            vec![
                Some(Value::Integer(i64::from(*s))),
                Some(Value::Integer(i64::from(*i))),
            ]
        })
        .collect();
    assert_eq!(page.rows, expected);
    driver
        .disconnect(handle)
        .await
        .expect("close native grid session");
}

#[tokio::test]
async fn mysql_sql_literal_roundtrip_preserves_typed_legacy_artifact_values() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_literal_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id INT,flag TINYINT,measure DOUBLE,note VARCHAR(100),blob_value VARBINARY(100),stamp DATETIME,doc JSON,absent VARCHAR(100))"))
        .execute(&mut f.oracle).await.unwrap();
    let fields = [
        Some(Value::Integer(7)),
        Some(Value::Bool(true)),
        Some(Value::Float(1.125)),
        Some(Value::String(r"literal's \ bus".into())),
        Some(Value::Bytes(vec![0, 255, 39])),
        Some(Value::Timestamp("2025-04-17 11:53:05".into())),
        Some(Value::Json(json!({"literal":"春"}))),
        None,
    ];
    let generated = fields
        .iter()
        .map(|v| driver.format_sql_literal(v))
        .collect::<Vec<_>>()
        .join(", ");
    driver
        .execute(
            &handle,
            &format!("INSERT INTO {table} VALUES ({generated})"),
        )
        .await
        .expect("native MySQL interprets the generated portable artifact");
    let actual:(i32,i32,f64,String,Vec<u8>,String,String,Option<String>)=sqlx::query_as(&format!(
        "SELECT id,CAST(flag AS SIGNED),measure,note,blob_value,CAST(stamp AS CHAR),CAST(JSON_UNQUOTE(JSON_EXTRACT(doc, '$.literal')) AS CHAR),absent FROM {table}"
    )).fetch_one(&mut f.oracle).await.unwrap();
    assert_eq!(
        actual,
        (
            7,
            1,
            1.125,
            r"literal's \ bus".into(),
            vec![0, 255, 39],
            "2025-04-17 11:53:05".into(),
            "春".into(),
            None
        )
    );
    driver
        .disconnect(handle)
        .await
        .expect("release local fixture handle");
}
