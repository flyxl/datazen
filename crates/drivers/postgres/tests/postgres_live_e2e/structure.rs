use super::Fixture;
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, StatementRisk, StructureChangeMode, StructureChangePlan,
    StructureChangeRequest, StructureColumnDraft, StructureIndexDraft,
};
use datazen_driver_postgres::PostgresDriver;
use serde_json::json;

fn column(value: serde_json::Value) -> StructureColumnDraft {
    serde_json::from_value(value).expect("test structure draft")
}

fn index(value: serde_json::Value) -> StructureIndexDraft {
    serde_json::from_value(value).expect("test structure index draft")
}

async fn execute_plan(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    plan: &StructureChangePlan,
    commit: bool,
) {
    let tx = driver
        .begin_transaction(handle)
        .await
        .expect("plan transaction");
    for statement in &plan.statements {
        driver
            .execute(handle, &statement.sql)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "reviewed structure statement failed: {} ({error})",
                    statement.summary
                )
            });
    }
    if commit {
        driver.commit(tx).await.expect("commit reviewed plan");
    } else {
        driver.rollback(tx).await.expect("rollback reviewed plan");
    }
}

#[tokio::test]
async fn reviewed_structure_plan_round_trips_real_postgres_catalog_and_rolls_back() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");
    let schema = format!("dz_structure_{}", uuid::Uuid::new_v4().simple());
    let table = "records with space";
    let qualified = format!("\"{schema}\".\"{table}\"");
    sqlx::query(&format!("CREATE SCHEMA \"{schema}\""))
        .execute(&mut fixture.oracle)
        .await
        .expect("disposable structure schema");

    let caps = driver
        .structure_capabilities(&handle)
        .await
        .expect("live structure capabilities");
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("server version oracle");
    assert_eq!(caps.index_include, version >= 110_000);
    assert!(caps.create_table && caps.alter_type && caps.index_filter);
    assert!(!caps.reorder_column);
    let id = column(
        json!({"id":"key","name":"id","dataType":"integer","nullable":false,"isPrimaryKey":true}),
    );
    let value = column(
        json!({"id":"v","name":"value","dataType":"integer","nullable":true,"defaultValue":"1"}),
    );
    let label = column(
        json!({"id":"label","name":"label","dataType":"text","nullable":false,"isUnique":true}),
    );
    let dead = column(json!({"id":"dead","name":"old","dataType":"text","nullable":true}));
    let original_columns = vec![id.clone(), value, label.clone(), dead];
    let create = StructureChangeRequest {
        mode: StructureChangeMode::Create,
        schema: Some(schema.clone()),
        table: table.into(),
        original_columns: Vec::new(),
        current_columns: original_columns.clone(),
        original_indexes: Vec::new(),
        current_indexes: Vec::new(),
    };
    let create_plan = driver
        .plan_structure_changes(&handle, &create)
        .await
        .expect("live CREATE plan");
    assert_eq!(create_plan.statements.len(), 1);
    assert_eq!(create_plan.statements[0].risk, StatementRisk::Additive);
    let absent = sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NULL")
        .bind(&qualified)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("read-only planning oracle");
    assert!(absent, "planning alone may not create any object");
    execute_plan(&driver, &handle, &create_plan, true).await;

    let original_indexes = vec![
        index(
            json!({"id":"by_label","name":"old_label_index","columns":["label"],"isUnique":false}),
        ),
        index(json!({"id":"obsolete","name":"old_note_index","columns":["old"],"isUnique":false})),
    ];
    sqlx::raw_sql(&format!(
        "INSERT INTO {qualified} (id, label, old) VALUES (1, 'keep', 'removed only after apply');
         CREATE INDEX old_label_index ON {qualified} (label);
         CREATE INDEX old_note_index ON {qualified} (old)"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("seed real original rows and indexes");
    let changed = column(json!({
        "id":"v","name":"amount","dataType":"bigint","nullable":false,
        "defaultValue":"2","comment":"amount's reviewed description"
    }));
    let mut changed_label = label;
    changed_label.default_value = Some("'unused'".into());
    let new = column(json!({"id":"n","name":"added","dataType":"text","nullable":true}));
    let current_columns = vec![id, changed.clone(), changed_label, new];
    let revised_index = json!({
        "id":"by_label","name":"new_label_index","columns":["label"],"isUnique":true,
        "indexType":"btree","includeColumns":["amount"],"filter":"amount > 0"
    });
    let current_indexes = vec![
        index(revised_index.clone()),
        index(
            json!({"id":"amount","name":"new_amount_index","columns":["amount"],"isUnique":false}),
        ),
    ];

    let mut alter = StructureChangeRequest {
        mode: StructureChangeMode::Alter,
        schema: Some(schema.clone()),
        table: table.into(),
        original_columns: original_columns.clone(),
        current_columns,
        original_indexes,
        current_indexes,
    };
    let plan = driver
        .plan_structure_changes(&handle, &alter)
        .await
        .expect("live ALTER plan");
    assert!(plan
        .statements
        .iter()
        .any(|s| s.risk == StatementRisk::Destructive));
    assert!(plan
        .statements
        .iter()
        .any(|s| s.risk == StatementRisk::Rewrite));

    let verify_before = async |conn: &mut sqlx::PgConnection| {
        let rows = sqlx::query_as::<_, (i32, i32, String, String)>(&format!(
            "SELECT id,value,label,old FROM {qualified} ORDER BY id"
        ))
        .fetch_all(conn)
        .await
        .expect("pre-apply catalog and row oracle");
        assert_eq!(
            rows,
            [(1, 1, "keep".into(), "removed only after apply".into())]
        );
    };
    verify_before(&mut fixture.oracle).await;
    execute_plan(&driver, &handle, &plan, false).await;
    verify_before(&mut fixture.oracle).await;
    execute_plan(&driver, &handle, &plan, true).await;
    let rows = sqlx::query_as::<_, (i32, i64, String, Option<String>)>(&format!(
        "SELECT id,amount,label,added FROM {qualified} ORDER BY id"
    ))
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("independent changed rows and data types");
    assert_eq!(rows, [(1, 1, "keep".into(), None)]);
    let (ty, notnull, default, comment) = sqlx::query_as::<_, (String, bool, String, String)>(
        r#"SELECT format_type(a.atttypid,a.atttypmod), a.attnotnull,
                  pg_get_expr(d.adbin,d.adrelid), col_description(a.attrelid,a.attnum)
           FROM pg_attribute a JOIN pg_attrdef d ON d.adnum = a.attnum
           WHERE a.attrelid = to_regclass($1) AND d.adrelid = a.attrelid AND a.attname = 'amount'"#,
    )
    .bind(&qualified)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("independent PG column catalog");
    assert_eq!(
        (ty.as_str(), notnull, comment.as_str()),
        ("bigint", true, "amount's reviewed description")
    );
    assert!(
        default == "2" || default == "2::bigint" || default == "'2'::bigint",
        "actual default {default}"
    );
    let definitions: Vec<(String, String)> = sqlx::query_as(
        "SELECT indexname::text,indexdef FROM pg_indexes WHERE schemaname = $1 AND tablename = $2 ORDER BY indexname"
    )
    .bind(&schema)
    .bind(table)
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("actual PostgreSQL index definitions");
    assert!(definitions
        .iter()
        .any(|(name, def)| name == "new_label_index"
            && def.contains("UNIQUE INDEX")
            && def.contains("INCLUDE (amount)")
            && def.contains("WHERE (amount > 0)")));
    assert!(definitions
        .iter()
        .any(|(name, _)| name == "new_amount_index"));
    assert!(!definitions.iter().any(|(name, _)| name.starts_with("old_")));

    // Unsupported reorder and unsupported index method are rejected before a
    // plan reaches the server. Verify that the live table remains unchanged.
    alter.current_columns.swap(0, 1);
    assert!(driver
        .plan_structure_changes(&handle, &alter)
        .await
        .is_err());
    alter.current_columns.swap(0, 1);
    alter.current_indexes[0].index_type = "nonexistent".into();
    assert!(driver
        .plan_structure_changes(&handle, &alter)
        .await
        .is_err());
    sqlx::query(&format!("DROP SCHEMA \"{schema}\" CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove disposable structure schema");
    driver.disconnect(handle).await.expect("driver cleanup");
}
