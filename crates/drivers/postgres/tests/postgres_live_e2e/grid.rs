use super::Fixture;
use datazen::migration::schema_diff::diff_table_schemas;
use datazen::table_browsing::executor::validate_raw_where;
use datazen_driver_api::{
    sql_builder::build_update_statement_resolved, CountStrategy, DatabaseDriver, LegacySqlDialect,
    SetColumnValue, SqlTarget, Value, WriteExpression,
};
use datazen_driver_postgres::PostgresDriver;

#[tokio::test]
async fn postgres_grid_estimates_seeks_and_applies_bound_edits_against_real_rows() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let schema = format!("dz_grid_{}", uuid::Uuid::new_v4().simple());
    let related = format!("{schema}.items");
    sqlx::raw_sql(&format!(r#"
       CREATE SCHEMA {schema};
       CREATE TABLE {related} (category integer NOT NULL, id bigint NOT NULL, label varchar(40),
           enabled boolean, short smallint, real_num real, trig double precision, initials character(2),
           updated timestamp, local_time timestamp, tm timestamptz, primary key(category,id));
       INSERT INTO {related} (category,id,label) VALUES
           (1,3,'first'),(1,2,'second'),(1,1,'third'),(2,4,'fourth'),(2,2,'fifth'),(3,0,'filtered');
       ANALYZE {related};
    "#)).execute(&mut fixture.oracle).await.expect("actual table browser data and analyzed catalog");

    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("live PostgreSQL driver");
    let target = SqlTarget::new(Some(database), Some(&schema));
    assert!(matches!(driver.count_strategy(), CountStrategy::Estimated));
    let estimated_sql = driver
        .count_estimate_sql(database, Some(&schema), "items")
        .expect("dialect estimate SQL");
    let independent: i64 =
        sqlx::query_scalar("SELECT reltuples::bigint FROM pg_class WHERE oid = to_regclass($1)")
            .bind(&related)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("independent pg_class estimate");
    assert_eq!(
        driver
            .query_at(&handle, &estimated_sql, target)
            .await
            .expect("the driver-rendered estimate runs")
            .rows,
        vec![vec![Some(Value::Integer(independent))]]
    );
    assert_eq!(independent, 6);

    let raw = "id >= 1 AND (category < 3 OR category = 3)";
    assert!(driver.supports_raw_where_filter());
    validate_raw_where(raw).expect("shared row browser accepts this read-only predicate");
    assert!(driver.supports_keyset_pagination());
    let order = [("category".into(), false), ("id".into(), true)];
    let mut row_schema = driver
        .get_table_schema(&handle, "items", database, Some(&schema))
        .await
        .expect("physical table schema");
    // The browsable column catalog exposes type families. Schema Diff's
    // precision-aware comparison also needs the physical catalog type.
    let exact: std::collections::HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT attname::text, format_type(atttypid,atttypmod) FROM pg_attribute WHERE attrelid=to_regclass($1) AND attnum>0"
    ).bind(&related).fetch_all(&mut fixture.oracle).await.expect("physical size/precision metadata").into_iter().collect();
    for column in &mut row_schema.columns {
        column
            .data_type
            .clone_from(exact.get(&column.name).expect("all real fields typed"));
    }
    let canonical = driver
        .type_normalizer()
        .expect("PostgreSQL type normalizer");
    let mut equivalent = row_schema.clone();
    for column in &mut equivalent.columns {
        column.data_type = match column.name.as_str() {
            "category" => "INT4".into(),
            "id" => "INT8".into(),
            "label" => "VARCHAR(40)".into(),
            "enabled" => "BOOL".into(),
            "short" => "INT2".into(),
            "real_num" => "FLOAT4".into(),
            "trig" => "FLOAT8".into(),
            "initials" => "CHAR(2)".into(),
            "local_time" => "TIMESTAMP".into(),
            "tm" => "TIMESTAMPTZ".into(),
            _ => column.data_type.clone(),
        };
    }
    let diff = diff_table_schemas("items", &equivalent, &row_schema, Some(canonical.as_ref()));
    assert!(
        diff.changed.is_empty(),
        "PostgreSQL catalog aliases must not plan changes: {:?}",
        diff.changed
    );
    let mut mismatching = equivalent.clone();
    mismatching
        .columns
        .iter_mut()
        .find(|c| c.name == "label")
        .expect("label")
        .data_type = "VARCHAR(41)".into();
    assert_eq!(
        diff_table_schemas("items", &mismatching, &row_schema, Some(canonical.as_ref())).changed[0]
            .name,
        "label"
    );

    let ordered: Vec<(i32, i64)> = sqlx::query_as(&format!(
        "SELECT category,id FROM {related} WHERE id >= 1 ORDER BY category ASC,id DESC"
    ))
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("independent order oracle");
    let mut found = Vec::new();
    for _ in 0..3 {
        let seek = if let Some(&(category, id)) = found.last() {
            driver
                .keyset_predicate_sql(
                    &order,
                    &[
                        ("category".into(), Value::Integer(i64::from(category))),
                        ("id".into(), Value::Integer(id)),
                    ],
                )
                .map(|sql| format!(" AND ({sql})"))
                .expect("mixed-direction keyset")
        } else {
            String::new()
        };
        let sql = format!("SELECT category,id FROM items WHERE ({raw}){seek} ORDER BY category ASC,id DESC LIMIT 2");
        let page = driver
            .query_at(&handle, &sql, target)
            .await
            .expect("live keyset page");
        found.extend(page.rows.into_iter().map(|row| match row.as_slice() {
            [Some(Value::Integer(a)), Some(Value::Integer(b))] => {
                (i32::try_from(*a).expect("int4"), *b)
            }
            _ => panic!("unexpected typed keyset row"),
        }));
    }
    assert_eq!(found, ordered);
    assert_eq!(found.len(), 5);

    let summary = driver
        .query_multi_at(
            &handle,
            r#"
       WITH t AS (SELECT id FROM items WHERE id > 0 LIMIT 5)
       SELECT id, 'LIMIT ''quoted''' AS "LIMIT payload", $tag$ LIMIT literal $tag$ AS marker
       FROM t /* LIMIT ignored /* nested comment */ */ ORDER BY id
    "#,
            Some(2),
            target,
        )
        .await
        .expect("top-level limit does not confuse embedded dialect terms");
    assert_eq!(summary.results[0].rows.len(), 2);
    assert!(summary.results[0].truncated);
    let no_truncation = driver
        .query_multi_at(
            &handle,
            "SELECT id FROM items WHERE id>0 ORDER BY id LIMIT 1",
            Some(2),
            target,
        )
        .await
        .expect("existing outer limit preserved");
    assert_eq!(no_truncation.results[0].rows.len(), 1);
    assert!(!no_truncation.results[0].truncated);

    let retained = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("table edit retains its selected scope");
    driver
        .execute(
            &retained,
            &format!("SET search_path TO {schema}, pg_catalog"),
        )
        .await
        .expect("select exact edit schema");
    assert!(driver.supports_bound_writes());
    let native = driver
        .native_write_expression_sql(WriteExpression::CurrentTimestamp)
        .expect("native write expression");
    let dialect = LegacySqlDialect::new(&driver);
    let changed = "updated ' quote · 雪";
    let update = build_update_statement_resolved(
        &dialect,
        4,
        "items",
        &[
            (
                "label",
                SetColumnValue::Literal(Some(Value::String(changed.into()))),
            ),
            ("updated", SetColumnValue::Expression(native)),
        ],
        &[
            ("category", Some(Value::Integer(1))),
            ("id", Some(Value::Integer(2))),
        ],
    )
    .expect("real table edit SQL with bound payload");
    assert!(!update.sql.contains(changed));
    let before: chrono::NaiveDateTime = sqlx::query_scalar("SELECT CURRENT_TIMESTAMP::timestamp")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("server timestamp lower bound");
    assert_eq!(
        driver
            .execute_with_params(&retained, &update.sql, &update.parameters)
            .await
            .expect("bound edit"),
        1
    );
    let changed_row: (String, chrono::NaiveDateTime) = sqlx::query_as(&format!(
        "SELECT label,updated FROM {related} WHERE category=1 AND id=2"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("independent row and native expression readback");
    let after: chrono::NaiveDateTime = sqlx::query_scalar("SELECT CURRENT_TIMESTAMP::timestamp")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("server timestamp upper bound");
    assert_eq!(changed_row.0, changed);
    assert!((before..=after).contains(&changed_row.1));
    let other: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {related} WHERE updated IS NOT NULL"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("unselected rows untouched");
    assert_eq!(other, 1);
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove table browser fixtures");
    driver
        .disconnect(retained)
        .await
        .expect("closed retained edit scope");
    driver.disconnect(handle).await.expect("clean live driver");
}

#[tokio::test]
async fn postgres_typed_keyset_boundary_agrees_with_catalog_ordering() {
    let mut fixture = Fixture::open().await;
    let table = format!("dz_typed_grid_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id int PRIMARY KEY, flag bool NOT NULL,
          bytes bytea NOT NULL, score float8 NOT NULL, shape jsonb NOT NULL, stamp timestamptz NOT NULL);
         INSERT INTO {table}(id, flag, bytes, score, shape, stamp) VALUES
           (1,false,decode('00ff','hex'),1.5,'[1,2]','2024-02-29+00'),
           (2,false,decode('00ff','hex'),1.5,'[1,2]','2024-02-29+00'),
           (3,false,decode('00ff','hex'),1.5,'[1,2]','2024-03-01+00'),
           (4,false,decode('00ff','hex'),1.5,'[1,3]','2024-02-29+00'),
           (5,false,decode('00ff','hex'),2.0,'[1,2]','2024-02-29+00'),
           (6,false,decode('abcd','hex'),1.5,'[1,2]','2024-02-29+00'),
           (7,true,decode('00ff','hex'),1.5,'[1,2]','2024-02-29+00')"
    ))
    .execute(&mut fixture.oracle).await.expect("independent typed ordering fixture");
    let driver = PostgresDriver::new();
    let handle = driver.connect(&fixture.config).await.unwrap();
    let order_names = ["flag", "bytes", "score", "shape", "stamp", "id"];
    let order = order_names
        .iter()
        .map(|name| ((*name).to_string(), false))
        .collect::<Vec<_>>();
    let ordering = order_names.join(", ");
    let entire: Vec<i32> =
        sqlx::query_scalar(&format!("SELECT id FROM {table} ORDER BY {ordering}"))
            .fetch_all(&mut fixture.oracle)
            .await
            .expect("independent native row ordering");
    assert_eq!(entire, [1, 2, 3, 4, 5, 6, 7]);
    let projection = driver
        .query(
            &handle,
            &format!("SELECT {ordering} FROM {table} ORDER BY {ordering} LIMIT 2"),
        )
        .await
        .expect("typed first page");
    let mut values: Vec<_> = projection.rows[1]
        .iter()
        .cloned()
        .map(Option::unwrap)
        .collect();
    // The explicit Timestamp cursor kind also needs a usable SQL predicate.
    if let Value::String(stamp) = &values[4] {
        values[4] = Value::Timestamp(stamp.clone());
    }
    let boundary: Vec<_> = order_names
        .iter()
        .zip(values)
        .map(|(name, value)| ((*name).to_string(), value))
        .collect();
    let seek = driver
        .keyset_predicate_sql(&order, &boundary)
        .expect("typed quoted keyset boundary");
    let later = driver
        .query(
            &handle,
            &format!("SELECT id FROM {table} WHERE {seek} ORDER BY {ordering}"),
        )
        .await
        .expect("PostgreSQL accepts binary, JSON, boolean, float and zoned time literals");
    assert_eq!(
        later.rows,
        entire[2..]
            .iter()
            .map(|id| vec![Some(Value::Integer(i64::from(*id)))])
            .collect::<Vec<_>>()
    );
    let only_true = [("flag".to_string(), Value::Bool(true))];
    let seek = driver
        .keyset_predicate_sql(&order[..1], &only_true)
        .unwrap();
    assert!(driver
        .query(&handle, &format!("SELECT id FROM {table} WHERE {seek}"))
        .await
        .unwrap()
        .rows
        .is_empty());
    driver.disconnect(handle).await.unwrap();
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}
