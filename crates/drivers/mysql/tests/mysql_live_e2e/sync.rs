use super::Fixture;
use datazen_driver_api::{inventory, DatabaseDriver, IRDefault, IRType, SyncAdapterFactory, Value};

#[tokio::test]
async fn mysql_transfer_validates_real_catalog_width_and_replays_exact_values() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let from = format!("dz_source_{id}");
    let to = format!("dz_target_{id}");
    sqlx::raw_sql(&format!(r#"
        CREATE TABLE {from}(id BIGINT PRIMARY KEY, keycode VARCHAR(12), signed_small SMALLINT,
            signed_tiny TINYINT, medium MEDIUMINT, normal INT DEFAULT 3, yes_no TINYINT(1),
            amount DECIMAL(50,12), flag BIT(3), label CHAR(7),
            bytes VARBINARY(50), fixed_bytes BINARY(2), payload JSON, day DATE,
            clock TIME, capture DATETIME DEFAULT CURRENT_TIMESTAMP, y YEAR, f FLOAT, d DOUBLE,
            token ENUM('a','b'), tokens SET('red','green'), long_text TEXT, long_bytes BLOB,
            expression_default INT DEFAULT (length('literal')), location POINT) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
        CREATE TABLE {to} (id BIGINT, keycode VARCHAR(12) CHARACTER SET utf8mb4,
            too_short VARCHAR(4) CHARACTER SET utf8mb4, ascii_code VARCHAR(12) CHARACTER SET ascii,
            no_signed BIGINT UNSIGNED, small INT, tight DECIMAL(20,4), rough DOUBLE,
            amount DECIMAL(50,12), bytes VARBINARY(50), payload JSON) ENGINE=InnoDB;
        INSERT INTO {from}(id,keycode,amount,bytes,payload) VALUES
           (1,'雪',1234567890123456789012345.000000000015, X'FF0080', JSON_OBJECT('key','雪')),
           (2,'abc',-0.000000000010, X'', JSON_OBJECT('key',NULL));"#))
        .execute(&mut f.oracle).await.expect("seed independently inspected MySQL source and destination");
    let adapter = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|f| f.db_types.contains(&"mysql"))
        .expect("registered MySQL adapter");
    let adapters = (adapter.create)();
    let source = &adapters.source;
    let target = &adapters.target;
    let native = driver
        .get_table_schema(&handle, &from, &f.database, None)
        .await
        .expect("actual source catalog");
    let target_schema = driver
        .get_table_schema(&handle, &to, &f.database, None)
        .await
        .expect("actual target catalog");
    let ir = source.table_to_ir(&native, None);
    let ircol = |name: &str| ir.columns.iter().find(|c| c.name == name).unwrap();
    let srccol = |name: &str| native.columns.iter().find(|c| c.name == name).unwrap();
    let dstcol = |name: &str| {
        target_schema
            .columns
            .iter()
            .find(|c| c.name == name)
            .unwrap()
    };
    assert_eq!(ircol("id").ir_type, IRType::Int64);
    assert_eq!(ircol("signed_small").ir_type, IRType::Int16);
    assert_eq!(ircol("signed_tiny").ir_type, IRType::Int8);
    assert_eq!(ircol("medium").ir_type, IRType::Int32);
    assert_eq!(
        ircol("normal").default_expr,
        Some(IRDefault::Literal("3".into()))
    );
    assert_eq!(
        ircol("amount").ir_type,
        IRType::Decimal {
            precision: 50,
            scale: 12
        }
    );
    assert_eq!(
        ircol("keycode").ir_type,
        IRType::Varchar { length: Some(12) }
    );
    assert_eq!(ircol("label").ir_type, IRType::Char { length: 7 });
    assert_eq!(ircol("flag").ir_type, IRType::Bit { length: 3 });
    assert_eq!(ircol("payload").ir_type, IRType::Json);
    assert_eq!(ircol("long_text").ir_type, IRType::Text);
    assert_eq!(ircol("long_bytes").ir_type, IRType::Blob);
    assert_eq!(
        ircol("capture").default_expr,
        Some(IRDefault::CurrentTimestamp)
    );
    assert!(matches!(ircol("location").ir_type, IRType::Other(_)));
    let validate = |field: &str, dst: Option<&str>, creating: bool| {
        let physical = srccol(field);
        target.validate_transfer_column_type(
            physical,
            ircol(field),
            source.transfer_source_text_limit_bytes(physical),
            source.transfer_source_requires_collation_preservation(physical),
            source.transfer_source_type_is_native_only(physical, ircol(field)),
            dst.map(|d| dstcol(d).data_type.as_str()),
            creating,
        )
    };
    for field in ["id", "keycode", "amount", "bytes", "payload"] {
        validate(field, Some(field), false).expect("actual compatible destination");
    }
    for (field, dst) in [
        ("id", "no_signed"),
        ("id", "small"),
        ("amount", "tight"),
        ("amount", "rough"),
        ("keycode", "too_short"),
    ] {
        assert!(
            validate(field, Some(dst), false).is_err(),
            "real narrowed destination is rejected before inserts"
        );
    }
    assert!(
        validate("keycode", None, true).is_err(),
        "unproven collation must not be guessed"
    );
    assert!(
        validate("id", None, false).is_err(),
        "missing catalog metadata is not permission to narrow"
    );
    validate("id", None, true).expect("native transferred integer target type");
    validate("amount", None, true).expect("native exact decimal target type");

    let field_metadata = target
        .transfer_target_character_metadata_query(&f.database, None, &to)
        .expect("real catalog query");
    let fields = driver
        .query(&handle, &field_metadata)
        .await
        .expect("native destination character metadata");
    let as_text = |cell: &Option<Value>| match cell {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Bytes(b)) => String::from_utf8(b.clone()).ok(),
        _ => None,
    };
    for (name, wanted) in [("keycode", true), ("ascii_code", false)] {
        let row = fields
            .rows
            .iter()
            .find(|row| as_text(&row[0]).as_deref() == Some(name))
            .expect("exact metadata field");
        assert_eq!(
            target
                .validate_transfer_target_character_metadata(
                    ircol("keycode"),
                    &dstcol(name).data_type,
                    as_text(&row[1]).as_deref(),
                    as_text(&row[2]).as_deref(),
                    false
                )
                .is_ok(),
            wanted
        );
    }
    let ordering = source.sync_key_order_expression(
        "keycode",
        &source
            .sync_key_contract(srccol("keycode"))
            .expect("real binary sort key"),
    );
    let result = driver
        .query(
            &handle,
            &format!("SELECT id,keycode,amount,bytes,payload FROM {from} ORDER BY {ordering}"),
        )
        .await
        .expect("real source rows");
    assert_eq!(result.rows[0][0], Some(Value::Integer(2)));
    let ordered = ["id", "keycode", "amount", "bytes", "payload"];
    for row in result.rows {
        let literals = row
            .iter()
            .zip(ordered)
            .map(|(v, n)| target.format_literal(v, &ircol(n).ir_type))
            .collect::<Vec<_>>()
            .join(", ");
        driver
            .execute(
                &handle,
                &format!("INSERT INTO {to}(id,keycode,amount,bytes,payload) VALUES ({literals})"),
            )
            .await
            .expect("real independently validated target writes");
    }
    let actual = async |table: &str, oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_as::<_,(i64,String,String,Vec<u8>,String)>(&format!("SELECT id, keycode, CAST(amount AS CHAR), bytes, CAST(payload AS CHAR) FROM {table} ORDER BY id"))
          .fetch_all(oracle).await.expect("independent full row oracle")
    };
    let left = actual(&from, &mut f.oracle).await;
    let right = actual(&to, &mut f.oracle).await;
    assert_eq!(right, left);
    driver
        .disconnect(handle)
        .await
        .expect("close transfer fixture");
}

#[tokio::test]
async fn mysql_transfer_preflight_reads_native_generated_and_index_metadata_and_applies_portable_plan(
) {
    use datazen_driver_api::{IndexInfo, MigrationColumn, MigrationOperation, TableOptions};
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let unsupported = format!("dz_preflight_{id}");
    let origin = format!("dz_plan_origin_{id}");
    let copied = format!("dz_plan_copied_{id}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {unsupported}(id INT PRIMARY KEY, code VARCHAR(240), details TEXT, amount INT,
            computed INT GENERATED ALWAYS AS (amount + 1) STORED,
            INDEX idx_prefix (code(12)), INDEX idx_desc (amount DESC), INDEX idx_expr ((LOWER(code))));
         CREATE TABLE {origin}(id BIGINT PRIMARY KEY, enabled TINYINT(1) DEFAULT 1, keycode VARCHAR(12) DEFAULT 'seed',
            c CHAR(5), exact_amount DECIMAL(45,13) DEFAULT 2, short_no SMALLINT, tiny_no TINYINT,
            width_no INT, size_float FLOAT, size_double DOUBLE, marker BIT(7), raw_value VARBINARY(64),
            raw_large BLOB, text_large TEXT, json_value JSON, day DATE, clock TIME, made DATETIME DEFAULT CURRENT_TIMESTAMP)
            ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='portable copy';"
    )).execute(&mut f.oracle).await.expect("native represented and unrepresented transfer inputs");
    let af = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|f| f.db_types.contains(&"mysql"))
        .unwrap();
    let adapters = (af.create)();
    let source = &adapters.source;
    let target = &adapters.target;
    let unsupported_sql = source
        .unsupported_transfer_structure_query(&f.database, None, &unsupported)
        .expect("native structure preflight");
    let violations = driver
        .query(&handle, &unsupported_sql)
        .await
        .expect("real generated and index metadata");
    for proof in ["computed", "idx_prefix", "idx_desc", "idx_expr"] {
        assert!(
            format!("{:?}", violations.rows).contains(proof),
            "missing catalog blocker: {proof}"
        );
    }
    let schema = driver
        .get_table_schema(&handle, &origin, &f.database, None)
        .await
        .expect("native typed source metadata");
    let unsafe_schema = driver
        .get_table_schema(&handle, &unsupported, &f.database, None)
        .await
        .expect("native wide/indexed fields");
    let safe = source.table_to_ir(&schema, None);
    let unsafe_ir = source.table_to_ir(&unsafe_schema, None);
    let route = |name: &str| {
        safe.columns
            .iter()
            .find(|c| c.name == name)
            .expect("real source field")
            .clone()
    };
    let not_allowed = |name: &str| {
        unsafe_ir
            .columns
            .iter()
            .find(|c| c.name == name)
            .expect("real wide field")
            .clone()
    };
    for name in ["code", "details"] {
        assert!(
            target.validate_index_columns(&[not_allowed(name)]).is_err(),
            "must not silently widen or prefix actual column"
        );
    }
    let key = [
        "id",
        "keycode",
        "exact_amount",
        "marker",
        "raw_value",
        "short_no",
        "width_no",
        "size_float",
        "size_double",
        "day",
        "clock",
        "made",
    ];
    target
        .validate_index_columns(&key.map(route))
        .expect("bounded native transfer key shape");
    for col in &safe.columns {
        let native = target.transfer_ir_type_to_native(&col.ir_type);
        let supports = target
            .transfer_native_type_allows_column_default(&native)
            .unwrap_or(false);
        if col.default_expr.is_some() {
            assert!(supports && target.transfer_allows_column_default(&col.ir_type));
        }
    }
    assert!(
        target
            .render_source_table_options(&schema.table_options)
            .is_err(),
        "actual source collation must not be guessed"
    );
    let portable_opts = TableOptions {
        collation: None,
        ..schema.table_options.clone()
    };
    let options = target
        .render_source_table_options(&portable_opts)
        .expect("known portable native table options")
        .expect("real options are present");
    assert!(options.contains("ENGINE=InnoDB"));
    assert!(options.contains("portable copy"));
    let columns = safe
        .columns
        .iter()
        .map(|field| MigrationColumn {
            name: field.name.clone(),
            data_type: target.transfer_ir_type_to_native(&field.ir_type),
            nullable: field.nullable,
            default_value: field
                .default_expr
                .as_ref()
                .and_then(|v| target.format_default(v)),
            comment: field.comment.clone(),
            is_auto_increment: field.is_auto_increment,
        })
        .collect();
    let renderer = driver
        .migration_renderer()
        .expect("native migration serializer");
    let op = MigrationOperation::CreateTable {
        table: copied.clone(),
        columns,
        primary_keys: vec!["id".into()],
    };
    let ddl = renderer
        .render_create_table_with_options(&op, &portable_opts)
        .expect("data transfer DDL selected by source options");
    driver
        .execute(&handle, &ddl.sql)
        .await
        .expect("apply generated MySQL transfer DDL on real server");
    let idx = MigrationOperation::CreateIndex {
        table: copied.clone(),
        index: IndexInfo {
            name: "ix_verified".into(),
            columns: vec!["id".into(), "keycode".into(), "exact_amount".into()],
            is_primary: false,
            is_unique: false,
            index_type: "BTREE".into(),
        },
    };
    driver
        .execute(
            &handle,
            &renderer.render(&idx).expect("portable actual key").sql,
        )
        .await
        .expect("native multi-column index");
    sqlx::query(&format!("INSERT INTO {copied}(id) VALUES(1)"))
        .execute(&mut f.oracle)
        .await
        .expect("actual server defaults survive transfer");
    let rows: (i32, String, String) = sqlx::query_as(&format!(
        "SELECT CAST(enabled AS SIGNED),keycode,CAST(exact_amount AS CHAR) FROM {copied}"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("native expected defaults in destination");
    assert_eq!(rows, (1, "seed".into(), "2.0000000000000".into()));
    let verified = driver
        .get_table_schema(&handle, &copied, &f.database, None)
        .await
        .expect("native replay catalog");
    assert!(verified
        .indexes
        .iter()
        .any(|i| i.name == "ix_verified" && i.columns == ["id", "keycode", "exact_amount"]));
    assert_eq!(
        verified.table_options.comment.as_deref(),
        Some("portable copy")
    );
    driver
        .disconnect(handle)
        .await
        .expect("release live plan session");
}
