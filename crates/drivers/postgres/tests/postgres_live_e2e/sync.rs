use super::Fixture;
use datazen_driver_api::{
    inventory, DatabaseDriver, IRDefault, IRType, SyncAdapterFactory, SyncKeyValue, Value,
};
use datazen_driver_postgres::PostgresDriver;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

#[tokio::test]
async fn sync_adapters_round_trip_live_scalar_catalog_values_and_refuse_lossy_structure() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");
    let factory = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|factory| factory.db_types.contains(&"postgresql"))
        .expect("linked adapter");
    let adapter = (factory.create)();
    let source = adapter.source;
    let target = adapter.target;
    let schema = format!("dz_sync_{}", uuid::Uuid::new_v4().simple());
    let source_table = format!("{schema}.sample");
    let target_table = format!("{schema}.converted");
    sqlx::raw_sql(&format!(r#"
        CREATE SCHEMA {schema};
        CREATE TABLE {source_table} (
            k varchar(16) PRIMARY KEY, big bigint, short smallint, active boolean DEFAULT true,
            precise numeric(40,7), r real, d double precision, tag char(3),
            payload bytea, details jsonb, response text DEFAULT 'it''s good',
            event_date date, raw_time time, zone_time time with time zone,
            ts timestamp DEFAULT now(), tz timestamptz, uid uuid, mask bit(7),
            computed integer DEFAULT length('marker'::text)
        );
        INSERT INTO {source_table}(k,big,short,precise,r,d,tag,payload,details,response,event_date,raw_time,ts,tz,uid,computed)
        VALUES
           ('雪',-9007199254740992,12,-1234567890123456789012345.0010000,1.25,-2.125,'a',decode('00ff27','hex'),'{{"a":"雪"}}','',DATE '2024-02-29',TIME '12:34:56',TIMESTAMP '2024-02-29 01:02:03',TIMESTAMPTZ '2024-02-29 02:03:04+00','e137a508-6655-46a1-8d96-f9b96063ab2a',1),
           ('z',42,20,0,3.25,4.125,'b',NULL,NULL,NULL,NULL,NULL,TIMESTAMP '2024-03-01 01:02:03',NULL,NULL,2);
        CREATE TYPE {schema}.choice AS ENUM('one', 'two');
        CREATE SEQUENCE {schema}.free_seq INCREMENT 2;
        CREATE TABLE {schema}.unportable (
            id integer GENERATED ALWAYS AS IDENTITY, source_amount integer,
            generated integer GENERATED ALWAYS AS (source_amount + 1) STORED,
            arr integer[], custom {schema}.choice, sequence_default integer DEFAULT nextval('{schema}.free_seq')
        );
        CREATE INDEX part ON {schema}.unportable (source_amount DESC) WHERE source_amount >= 0;
    "#)).execute(&mut fixture.oracle).await.expect("independent fixture data and schema");

    let fields = driver
        .get_table_schema(&handle, "sample", database, Some(&schema))
        .await
        .expect("live source schema");
    let metadata = driver
        .query(
            &handle,
            &source
                .full_column_types_query(&source_table)
                .expect("full type catalog"),
        )
        .await
        .expect("full physical types");
    let full_types: HashMap<_, _> = metadata
        .rows
        .into_iter()
        .map(|row| match row.as_slice() {
            [Some(Value::String(name)), Some(Value::String(ty))] => (name.clone(), ty.clone()),
            _ => panic!("unexpected format_type metadata"),
        })
        .collect();
    let unsupported = |name| {
        source
            .unsupported_transfer_structure_query(database, Some(&schema), name)
            .expect("structure preflight")
    };
    assert!(driver
        .query(&handle, &unsupported("sample"))
        .await
        .expect("portable source scope")
        .rows
        .is_empty());
    let unsupported_fields = driver
        .get_table_schema(&handle, "unportable", database, Some(&schema))
        .await
        .expect("live unportable schema");
    let blockers = driver
        .query(&handle, &unsupported("unportable"))
        .await
        .expect("preflight blockers");
    let blockers: Vec<_> = blockers
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(Some(Value::String(reason))) => Some(reason.as_str()),
            _ => None,
        })
        .collect();
    for expected in [
        "identity column id",
        "generated column generated",
        "index part (expression",
        "index part (sort",
        "sequence on column sequence_default",
    ] {
        assert!(
            blockers.iter().any(|r| r.contains(expected)),
            "expected live guard {expected} in {blockers:?}"
        );
    }
    let actual_partial = sqlx::query_scalar::<_, bool>(
        "SELECT i.indpred IS NOT NULL FROM pg_index i JOIN pg_class t ON t.oid=i.indexrelid JOIN pg_namespace n ON n.oid=t.relnamespace WHERE n.nspname=$1 AND t.relname='part'"
    ).bind(&schema).fetch_one(&mut fixture.oracle).await.expect("independent partial index oracle");
    assert!(actual_partial);
    let get_unportable = |name| {
        unsupported_fields
            .columns
            .iter()
            .find(|column| column.name == name)
            .expect("actual catalog column")
    };
    assert!(source
        .validate_transfer_source_column(get_unportable("arr"))
        .is_err());
    let custom = get_unportable("custom");
    let native_only = source.column_to_ir(custom, Some(&format!("{schema}.choice")));
    assert!(source.transfer_source_type_is_native_only(custom, &native_only));
    assert!(source
        .column_to_ir(get_unportable("sequence_default"), Some("integer"))
        .default_expr
        .is_none());

    let key = fields
        .columns
        .iter()
        .find(|column| column.name == "k")
        .expect("actual primary key");
    assert!(source.transfer_source_requires_collation_preservation(key));
    let key_full = full_types.get("k").expect("full varchar type");
    let mut length_aware_key = key.clone();
    length_aware_key.data_type.clone_from(key_full);
    assert_eq!(
        source.transfer_source_text_limit_bytes(&length_aware_key),
        Some(64)
    );
    let key_contract = source
        .sync_key_contract(key)
        .expect("live key ordering contract");
    let order = source.sync_key_order_expression("k", &key_contract);
    let source_rows = driver.query(&handle,&format!(
        "SELECT k,big,short,active,precise,r,d,tag,payload,details,response,event_date,raw_time,ts,tz,uid,computed FROM {source_table} ORDER BY {order}"
    )).await.expect("real key-ordered source row stream");
    assert_eq!(source_rows.rows.len(), 2);
    let keys: Vec<_> = source_rows
        .rows
        .iter()
        .map(|row| {
            source
                .normalize_sync_key(&row[0], &key_contract)
                .expect("typed key")
        })
        .collect();
    assert_eq!(
        keys,
        [
            SyncKeyValue::Text(vec![b'z']),
            SyncKeyValue::Text("雪".as_bytes().to_vec())
        ]
    );

    let ir = source.table_to_ir(&fields, Some(&full_types));
    let column = |name| {
        ir.columns
            .iter()
            .find(|column| column.name == name)
            .expect("real IR column")
    };
    assert_eq!(
        column("precise").ir_type,
        IRType::Decimal {
            precision: 40,
            scale: 7
        }
    );
    assert_eq!(column("tag").ir_type, IRType::Char { length: 3 });
    assert_eq!(column("event_date").ir_type, IRType::Date);
    assert_eq!(
        column("zone_time").ir_type,
        IRType::Time {
            with_timezone: true
        }
    );
    assert_eq!(column("mask").ir_type, IRType::Bit { length: 7 });
    assert_eq!(column("ts").default_expr, Some(IRDefault::CurrentTimestamp));
    assert!(matches!(
        column("computed").default_expr,
        Some(IRDefault::RawExpression(_))
    ));
    let fields = ir
        .columns
        .iter()
        .map(|column| {
            let name = &column.name;
            let native = target.ir_type_to_native(&column.ir_type);
            let nullable = if column.nullable { "" } else { " NOT NULL" };
            let default = column
                .default_expr
                .as_ref()
                .and_then(|def| target.format_default(def))
                .map(|s| format!(" DEFAULT {s}"))
                .unwrap_or_default();
            format!("{name} {native}{nullable}{default}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    assert!(
        driver
            .get_table_schema(&handle, "converted", database, Some(&schema))
            .await
            .is_err(),
        "refusal did not create or modify a target"
    );
    driver
        .execute(&handle, &format!("CREATE TABLE {target_table} ({fields})"))
        .await
        .expect("actual rendered target columns");
    let column_list = source_rows
        .columns
        .iter()
        .map(|col| col.name.as_str())
        .collect::<Vec<_>>();
    let tx = driver
        .begin_transaction(&handle)
        .await
        .expect("mapped copy transaction");
    for row in source_rows.rows {
        let literals = column_list
            .iter()
            .zip(row)
            .map(|(name, value)| {
                let value = if name == &"tz" {
                    value.map(|wire| match wire {
                        Value::String(s) => Value::Timestamp(s),
                        other => other,
                    })
                } else {
                    value
                };
                target.format_literal(&value, &column(name).ir_type)
            })
            .collect::<Vec<_>>();
        let sql = format!(
            "INSERT INTO {target_table} ({}) VALUES ({})",
            column_list.join(","),
            literals.join(",")
        );
        assert_eq!(
            driver
                .execute(&handle, &sql)
                .await
                .expect("rendered copy written to PG"),
            1
        );
    }
    driver.commit(tx).await.expect("commit mapped copy");
    let read_rows = async |oracle: &mut sqlx::PgConnection, table: &str| -> Vec<Json> {
        sqlx::query_scalar(&format!(
            "SELECT to_jsonb(t) FROM {table} t ORDER BY k COLLATE \"C\""
        ))
        .fetch_all(oracle)
        .await
        .expect("independent mapped rows oracle")
    };
    assert_eq!(
        read_rows(&mut fixture.oracle, &target_table).await,
        read_rows(&mut fixture.oracle, &source_table).await
    );
    let actual_types: HashSet<String> = sqlx::query_scalar(
        "SELECT format_type(atttypid,atttypmod) FROM pg_attribute WHERE attrelid = to_regclass($1) AND attnum > 0"
    ).bind(&target_table).fetch_all(&mut fixture.oracle).await.expect("actual target type schema").into_iter().collect();
    for ty in [
        "numeric(40,7)",
        "character(3)",
        "character varying(16)",
        "bit(7)",
        "time with time zone",
        "timestamp with time zone",
        "bytea",
    ] {
        assert!(actual_types.contains(ty), "target type missing {ty}");
    }
    assert!(target.auto_increment_keyword().is_some());
    assert!(
        target.supports_explicit_identity_values()
            && target.foreign_key_names_are_table_scoped()
            && target.object_names_are_case_sensitive()
    );
    driver.disconnect(handle).await.expect("driver cleanup");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove sync fixtures");
}

#[tokio::test]
async fn sync_preserves_live_postgres_network_and_interval_text_at_target_width() {
    let mut fixture = Fixture::open().await;
    let schema = format!("sync_physical_{}", uuid::Uuid::new_v4().simple());
    let physical = format!("{schema}.physical");
    sqlx::raw_sql(&format!(
        r#"
        CREATE SCHEMA {schema};
        CREATE TABLE {physical} (
            id integer PRIMARY KEY, ip inet, subnet cidr, hardware macaddr, wide_hardware macaddr8,
            elapsed interval, any_precision numeric, short_flag bit varying(3), total money,
            object_id oid, document xml, labels integer[]
        );
        INSERT INTO {physical} (id,ip,subnet,hardware,wide_hardware,elapsed,any_precision)
        VALUES (7,'1234:5678:9abc:def0:2345:6789:abcd:ef01/127',
                '1234:5678:9abc:def0:2345:6789:abcd:ef00/127',
                '08:00:2B:01:02:03','08:00:2B:FF:FE:01:02:03',
                '-01:02:03.456789',12345678901234567890.01);
    "#
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("real PostgreSQL native storage");
    let factory = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|factory| factory.db_types.contains(&"postgresql"))
        .expect("linked PostgreSQL sync adapter");
    let adapter = (factory.create)();
    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained transfer session");
    driver
        .execute(&handle, &format!("SET search_path TO {schema}, public"))
        .await
        .expect("explicit source search path");
    let metadata = driver
        .query(
            &handle,
            &adapter
                .source
                .full_column_types_query("physical")
                .expect("catalog SQL"),
        )
        .await
        .expect("resolve real unqualified source");
    let full_types: HashMap<_, _> = metadata
        .rows
        .into_iter()
        .map(|row| match row.as_slice() {
            [Some(Value::String(name)), Some(Value::String(ty))] => (name.clone(), ty.clone()),
            _ => panic!("unknown catalog row"),
        })
        .collect();
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("disposable PostgreSQL database");
    let schema_result = driver
        .get_table_schema(&handle, "physical", database, Some(&schema))
        .await
        .expect("native source catalog");
    let ir = adapter
        .source
        .table_to_ir(&schema_result, Some(&full_types));
    let ty = |name| {
        &ir.columns
            .iter()
            .find(|col| col.name == name)
            .expect("native source column")
            .ir_type
    };
    assert_eq!(ty("ip"), &IRType::Varchar { length: Some(45) });
    assert_eq!(ty("subnet"), &IRType::Varchar { length: Some(43) });
    assert_eq!(ty("hardware"), &IRType::Varchar { length: Some(17) });
    assert_eq!(ty("elapsed"), &IRType::Varchar { length: Some(255) });
    assert_eq!(
        ty("any_precision"),
        &IRType::Decimal {
            precision: 0,
            scale: 0
        }
    );
    assert_eq!(ty("short_flag"), &IRType::Blob);
    assert_eq!(
        ty("total"),
        &IRType::Decimal {
            precision: 19,
            scale: 2
        }
    );
    assert_eq!(ty("object_id"), &IRType::Int32);
    assert_eq!(ty("document"), &IRType::Text);
    assert_eq!(ty("labels"), &IRType::Json);
    let labels = schema_result
        .columns
        .iter()
        .find(|col| col.name == "labels")
        .expect("actual array column");
    assert!(
        adapter
            .source
            .validate_transfer_source_column(labels)
            .is_err(),
        "array transfer fails closed rather than inserting a null"
    );
    let key = schema_result
        .columns
        .iter()
        .find(|col| col.name == "id")
        .expect("actual numeric key");
    let key_contract = adapter
        .source
        .sync_key_contract(key)
        .expect("numeric key contract");
    let order = adapter
        .source
        .sync_key_order_expression("id", &key_contract);
    let names = [
        "ip",
        "subnet",
        "hardware",
        "wide_hardware",
        "elapsed",
        "any_precision",
    ];
    let projected = names.join(", ");
    let observed = driver
        .query(
            &handle,
            &format!("SELECT {projected} FROM physical ORDER BY {order}"),
        )
        .await
        .expect("query actual binary source values");
    assert_eq!(observed.rows.len(), 1);
    let columns = names
        .iter()
        .map(|name| format!("{name} {}", adapter.target.ir_type_to_native(ty(name))))
        .collect::<Vec<_>>()
        .join(", ");
    driver
        .execute(&handle, &format!("CREATE TABLE mapped ({columns})"))
        .await
        .expect("apply mapped PostgreSQL DDL");
    let values = names
        .iter()
        .zip(&observed.rows[0])
        .map(|(name, value)| adapter.target.format_literal(value, ty(name)))
        .collect::<Vec<_>>()
        .join(", ");
    driver
        .execute(&handle, &format!("INSERT INTO mapped VALUES ({values})"))
        .await
        .expect("live source values fit their mapped destination types");
    let source = sqlx::query_as::<_, (String, String, String, String, String, String)>(&format!(
        "SELECT ip::text,subnet::text,hardware::text,wide_hardware::text,elapsed::text,any_precision::text FROM {physical}"
    )).fetch_one(&mut fixture.oracle).await.expect("independent PostgreSQL source text");
    let target = sqlx::query_as::<_, (String, String, String, String, String, String)>(&format!(
        "SELECT ip,subnet,hardware,wide_hardware,elapsed,any_precision::text FROM {schema}.mapped"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("independent PostgreSQL mapped result");
    assert_eq!(
        source, target,
        "no truncation, lost sign, or altered precision"
    );
    driver
        .disconnect(handle)
        .await
        .expect("close transfer session");
}
