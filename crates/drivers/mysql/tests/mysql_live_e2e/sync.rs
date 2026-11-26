use super::Fixture;
use datazen_driver_api::{DatabaseDriver, IRDefault, IRType, SyncAdapterFactory, Value, inventory};

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
async fn mysql_transfer_preflight_reads_native_generated_and_index_metadata_and_applies_portable_plan()
 {
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
    assert!(
        verified
            .indexes
            .iter()
            .any(|i| i.name == "ix_verified" && i.columns == ["id", "keycode", "exact_amount"])
    );
    assert_eq!(
        verified.table_options.comment.as_deref(),
        Some("portable copy")
    );
    driver
        .disconnect(handle)
        .await
        .expect("release live plan session");
}

#[tokio::test]
async fn mysql_transfer_replays_native_text_and_json_unchanged_under_both_escape_modes() {
    let mut f = Fixture::open().await;
    let driver = datazen_driver_mysql::MysqlDriver::new(false);
    let handle = driver.connect_fixed_session(&f.cfg).await.unwrap();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let source = format!("dz_mode_source_{nonce}");
    let destination = format!("dz_mode_target_{nonce}");
    let table_layout="(id INT AUTO_INCREMENT PRIMARY KEY, note TEXT, payload JSON, ratio DOUBLE,
                       bytes VARBINARY(12), occurred DATETIME(6), empty_text VARCHAR(8), missing_value INT)";
    sqlx::raw_sql(&format!(
        "CREATE TABLE {source} {table_layout}; CREATE TABLE {destination} {table_layout};"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("real source and destination");
    let path = "C:\\new\\temp\\owner's\\雪";
    let payload = serde_json::json!({"file":path,"line":"first\nsecond","missing":null});
    sqlx::query(&format!("INSERT INTO {source} (note,payload,ratio,bytes,occurred,empty_text) VALUES (?,?,0.625,X'F0005C','2030-01-02 03:04:05.123456','')"))
       .bind(path).bind(sqlx::types::Json(payload.clone())).execute(&mut f.oracle).await.unwrap();
    let registration = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|f| f.db_types.contains(&"mysql"))
        .unwrap();
    let adapters = (registration.create)();
    let table = driver
        .get_table_schema(&handle, &source, &f.database, None)
        .await
        .unwrap();
    let types = adapters.source.table_to_ir(&table, None);
    let target = &adapters.target;
    assert!(target.supports_explicit_identity_values());
    assert!(target.index_names_are_table_scoped());
    assert_eq!(target.auto_increment_keyword(), Some("AUTO_INCREMENT"));
    let qualified = target.qualify_relation("", Some(&f.database), &source);
    let local = target.qualify_relation("", None, &destination);
    let verify = target.qualify_relation(&f.database, None, &destination);
    let ordering = adapters.source.sync_key_order_expression(
        &target.quote_ident("id"),
        &adapters
            .source
            .sync_key_contract(&table.columns[0])
            .unwrap(),
    );
    let fields = [
        "note",
        "payload",
        "ratio",
        "bytes",
        "occurred",
        "empty_text",
        "missing_value",
    ];
    let ordered = fields.join(",");
    let data = driver
        .query(
            &handle,
            &format!("SELECT {ordered} FROM {qualified} ORDER BY {ordering}"),
        )
        .await
        .unwrap();
    let row = data.rows.first().unwrap();
    assert_eq!(row[0], Some(Value::String(path.into())));
    assert_eq!(row[1], Some(Value::Json(payload.clone())));
    assert_eq!(row[2], Some(Value::Float(0.625)));
    assert_eq!(row[3], Some(Value::Bytes(vec![240, 0, 92])));
    assert!(row[6].is_none());
    let values = row
        .iter()
        .zip(fields)
        .map(|(value, name)| {
            let ir = &types
                .columns
                .iter()
                .find(|col| col.name == name)
                .unwrap()
                .ir_type;
            target.format_literal(value, ir)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let alias = target
        .transfer_target_character_metadata_query("", None, &destination)
        .unwrap();
    let native_meta = driver
        .query(&handle, &alias)
        .await
        .expect("character metadata determined by native active catalog");
    assert_eq!(native_meta.rows.len(), 8);
    for (no_escape, relation) in [(false, local), (true, verify)] {
        let mode = if no_escape {
            "NO_BACKSLASH_ESCAPES,STRICT_TRANS_TABLES"
        } else {
            "STRICT_TRANS_TABLES"
        };
        driver
            .execute(&handle, &format!("SET SESSION sql_mode='{mode}'"))
            .await
            .unwrap();
        driver
            .execute(
                &handle,
                &format!("INSERT INTO {relation}({ordered}) VALUES ({values})"),
            )
            .await
            .expect("replay native bytes without session-dependent escaping");
    }
    type Fields = (
        String,
        sqlx::types::Json<serde_json::Value>,
        f64,
        Vec<u8>,
        String,
        String,
        Option<i32>,
    );
    let copied:Vec<Fields>=sqlx::query_as(&format!("SELECT note,payload,ratio,bytes,CAST(occurred AS CHAR),empty_text,missing_value FROM {destination} ORDER BY id"))
       .fetch_all(&mut f.oracle).await.unwrap();
    let expected = (
        path.into(),
        sqlx::types::Json(payload),
        0.625,
        vec![240, 0, 92],
        "2030-01-02 03:04:05.123456".into(),
        String::new(),
        None,
    );
    assert_eq!(copied, [expected.clone(), expected]);
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_transfer_recognizes_real_text_capacity_and_integer_widths_before_catalog_writes() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let source = format!("dz_bound_source_{nonce}");
    let destination = format!("dz_bound_target_{nonce}");
    sqlx::raw_sql(&format!("CREATE TABLE {source}(tiny TINYINT,small SMALLINT,medium MEDIUMINT,exact_value DECIMAL(18,5),
       u_small SMALLINT UNSIGNED, u_large BIGINT UNSIGNED, s_large BIGINT, oneflag BIT(1),coords POINT, t1 TINYTEXT,t2 TEXT,t3 MEDIUMTEXT,t4 LONGTEXT,
       b1 TINYBLOB,b2 BLOB,b3 MEDIUMBLOB,b4 LONGBLOB) CHARACTER SET utf8mb4;
       CREATE TABLE {destination} LIKE {source};")).execute(&mut f.oracle).await.unwrap();
    let registration = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|f| f.db_types.contains(&"mysql"))
        .unwrap();
    let adapters = (registration.create)();
    let src = driver
        .get_table_schema(&handle, &source, &f.database, None)
        .await
        .unwrap();
    let dst = driver
        .get_table_schema(&handle, &destination, &f.database, None)
        .await
        .unwrap();
    let at = |name: &str| src.columns.iter().find(|c| c.name == name).unwrap();
    let dst_at = |name: &str| dst.columns.iter().find(|c| c.name == name).unwrap();
    let prove = |name: &str, destination: Option<&str>| {
        let base = at(name);
        let ir = adapters.source.column_to_ir(base, None);
        adapters.target.validate_transfer_column_type(
            base,
            &ir,
            adapters.source.transfer_source_text_limit_bytes(base),
            adapters
                .source
                .transfer_source_requires_collation_preservation(base),
            adapters
                .source
                .transfer_source_type_is_native_only(base, &ir),
            destination.map(|n| dst_at(n).data_type.as_str()),
            false,
        )
    };
    for (tiny, bytes) in [(1, 255), (2, 65_535), (3, 16_777_215), (4, 4_294_967_295)] {
        for prefix in ["t", "b"] {
            let name = format!("{prefix}{tiny}");
            assert_eq!(
                adapters.source.transfer_source_text_limit_bytes(at(&name)),
                Some(bytes)
            );
        }
        let name = format!("t{tiny}");
        prove(&name, Some(&name)).unwrap();
        if tiny > 1 {
            let smaller = format!("t{}", tiny - 1);
            assert!(
                prove(&name, Some(&smaller))
                    .unwrap_err()
                    .contains("holds at most")
            );
        }
    }
    for name in ["tiny", "small", "medium", "exact_value"] {
        prove(name, Some(name)).unwrap();
    }
    for (field, target, wanted) in [
        ("medium", Some("tiny"), "signed range"),
        ("small", Some("t1"), "integer range"),
        ("t2", Some("coords"), "string capacity"),
        ("t2", None, "string type could not be inspected"),
        ("exact_value", None, "numeric type could not be inspected"),
    ] {
        assert!(prove(field, target).unwrap_err().contains(wanted));
    }
    prove("u_small", Some("u_small")).unwrap();
    prove("u_small", Some("medium")).unwrap();
    prove("u_large", Some("u_large")).unwrap();
    assert!(
        prove("u_small", Some("small"))
            .unwrap_err()
            .contains("unsigned range")
    );
    assert!(
        prove("u_large", Some("s_large"))
            .unwrap_err()
            .contains("unsigned range")
    );
    assert!(
        prove("small", Some("u_large"))
            .unwrap_err()
            .contains("signed range")
    );

    let flag = adapters.source.column_to_ir(at("oneflag"), None);
    assert_eq!(flag.ir_type, IRType::Bool);
    let existing_char = adapters.source.column_to_ir(at("t1"), None);
    assert!(
        adapters
            .target
            .validate_transfer_target_character_metadata(
                &flag,
                &dst_at("oneflag").data_type,
                None,
                None,
                false
            )
            .is_ok()
    );
    assert!(
        adapters
            .target
            .validate_transfer_target_character_metadata(
                &existing_char,
                &dst_at("t1").data_type,
                None,
                None,
                false
            )
            .unwrap_err()
            .contains("character set was not inspected")
    );
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {destination}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(total, 0);
    sqlx::query(&format!(
        "INSERT INTO {source}(u_small,u_large) VALUES (65535,18446744073709551615)"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let actual = driver
        .query(&handle, &format!("SELECT u_small,u_large FROM {source}"))
        .await
        .unwrap();
    let values = actual.rows[0]
        .iter()
        .zip(["u_small", "u_large"])
        .map(|(value, n)| {
            let ir = adapters.source.column_to_ir(at(n), None);
            adapters.target.format_literal(value, &ir.ir_type)
        })
        .collect::<Vec<_>>()
        .join(",");
    driver
        .execute(
            &handle,
            &format!("INSERT INTO {destination}(u_small,u_large) VALUES({values})"),
        )
        .await
        .unwrap();
    let independent: (u16, u64) =
        sqlx::query_as(&format!("SELECT u_small,u_large FROM {destination}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(independent, (u16::MAX, u64::MAX));
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_transfer_explicit_native_index_mapping_applies_after_live_catalog_preflight() {
    use datazen_driver_api::{IndexInfo, MigrationOperation, TableOptions};
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let source = format!("dz_native_override_src_{tag}");
    let target = format!("dz_native_override_dst_{tag}");
    let nonportable = format!("dz_native_policy_{tag}");
    let layout = "c CHAR(5), v VARCHAR(8), b VARBINARY(5), t TINYINT, s SMALLINT, m MEDIUMINT,
        i INT, bi BIGINT, f FLOAT, d DOUBLE, deci DECIMAL(18,5), day DATE, instant DATETIME,
        yr YEAR, uid CHAR(36), geom POINT";
    sqlx::raw_sql(&format!(
        "CREATE TABLE {source}({layout}) ENGINE=InnoDB;
      CREATE TABLE {target} LIKE {source};
      CREATE TABLE {nonportable}(body TEXT DEFAULT (CONCAT('actual ', 'default'))) ENGINE=MyISAM;
      INSERT INTO {nonportable} VALUES (DEFAULT);"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let registration = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|f| f.db_types.contains(&"mysql"))
        .unwrap();
    let adapters = (registration.create)();
    let native = driver
        .get_table_schema(&handle, &source, &f.database, None)
        .await
        .unwrap();
    let fields = [
        "c", "v", "b", "t", "s", "m", "i", "bi", "f", "d", "deci", "day", "instant", "yr", "uid",
    ];
    let mapping = |name: &str| {
        let catalog = native.columns.iter().find(|col| col.name == name).unwrap();
        let mut out = adapters.source.column_to_ir(catalog, None);
        out.ir_type = if name == "uid" {
            IRType::Uuid
        } else {
            IRType::Other(catalog.data_type.clone())
        };
        out
    };
    let mapped = fields.map(mapping);
    assert!(mapped.iter().zip(fields).all(|(m, n)| m.name == n));
    let gep = mapping("geom");
    assert!(
        adapters
            .target
            .validate_index_columns(&[gep])
            .unwrap_err()
            .contains("known bounded type")
    );
    adapters
        .target
        .validate_index_columns(&mapped)
        .expect("all explicit destination key sizes proved");
    let ddl = MigrationOperation::CreateIndex {
        table: target.clone(),
        index: IndexInfo {
            name: "proof_native".into(),
            columns: fields.map(str::to_owned).to_vec(),
            is_primary: false,
            is_unique: false,
            index_type: "BTREE".into(),
        },
    };
    let sql = driver
        .migration_renderer()
        .unwrap()
        .render(&ddl)
        .unwrap()
        .sql;
    driver
        .execute(&handle, &sql)
        .await
        .expect("native server agrees with planned composite index");
    let confirmed:Vec<String>=sqlx::query_scalar("SELECT CAST(COLUMN_NAME AS CHAR) FROM information_schema.STATISTICS
       WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND INDEX_NAME='proof_native' ORDER BY SEQ_IN_INDEX")
       .bind(&target).fetch_all(&mut f.oracle).await.unwrap();
    assert_eq!(confirmed, fields);

    let rejected = driver
        .get_table_schema(&handle, &nonportable, &f.database, None)
        .await
        .unwrap();
    let options = TableOptions {
        collation: None,
        ..rejected.table_options.clone()
    };
    assert!(
        adapters
            .target
            .render_source_table_options(&options)
            .unwrap_err()
            .contains("MyISAM")
    );
    let body = adapters.source.column_to_ir(&rejected.columns[0], None);
    assert!(matches!(
        &body.default_expr,
        Some(IRDefault::RawExpression(_))
    ));
    assert!(
        !adapters
            .target
            .transfer_allows_column_default(&body.ir_type)
    );
    assert!(
        adapters
            .target
            .transfer_default_capable_type_for(&body.ir_type)
            .is_none()
    );
    assert!(
        adapters
            .target
            .default_capable_type_for(&body.ir_type)
            .is_some()
    );
    assert!(
        body.default_expr
            .as_ref()
            .and_then(|value| adapters.target.format_default(value))
            .is_none()
    );
    let saved: String = sqlx::query_scalar(&format!("SELECT body FROM {nonportable}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(saved, "actual default");
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_native_transfer_proves_portable_key_size_defaults_and_catalog_metadata() {
    use datazen_driver_api::{IndexInfo, MigrationOperation};
    let mut f = Fixture::open().await;
    let (driver, h) = f.connect().await;
    let marker = uuid::Uuid::new_v4().simple().to_string();
    let src = format!("dz_tail_source_{marker}");
    let dst = format!("dz_tail_target_{marker}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {src}(enabled BOOLEAN DEFAULT TRUE, symbol CHAR(4) CHARACTER SET utf8mb4,
       ascii_note VARCHAR(12) CHARACTER SET ascii, raw_total DEC(16,4), amount INT,
       generated_value INT GENERATED ALWAYS AS (amount+1) STORED);
       INSERT INTO {src}(symbol,ascii_note,raw_total,amount) VALUES('雨','valid',11.25,7);"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let factory = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|r| r.db_types.contains(&"mysql"))
        .unwrap();
    let adapters = (factory.create)();
    let target = &adapters.target;
    let source = driver
        .get_table_schema(&h, &src, &f.database, None)
        .await
        .unwrap();
    let fields = adapters.source.table_to_ir(&source, None);
    let field = |name: &str| {
        fields
            .columns
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .clone()
    };
    for (database, schema) in [("", Some(f.database.as_str())), ("", None)] {
        let preflight = adapters
            .source
            .unsupported_transfer_structure_query(database, schema, &src)
            .unwrap();
        let actual = driver.query(&h, &preflight).await.unwrap();
        assert_eq!(actual.rows.len(), 2);
        let blockers = format!("{:?}", actual.rows);
        assert!(blockers.contains("generated_value") && blockers.contains("ascii_note"));
    }
    let normalized = driver.type_normalizer().unwrap();
    assert_eq!(normalized.normalize_type("BOOL"), "BOOLEAN");
    let decimal = source
        .columns
        .iter()
        .find(|c| c.name == "raw_total")
        .unwrap();
    assert_eq!(
        normalized.normalize_type("DEC(16,4)"),
        normalized.normalize_type(&decimal.data_type)
    );
    let explicit_default_width = target.ir_type_to_native(&IRType::Varchar { length: None });
    let preview_text = target.ir_type_to_native(&IRType::Text);
    driver.execute(&h,&format!("CREATE TABLE {dst}(enabled BOOLEAN, symbol CHAR(4),short_note {explicit_default_width},body {preview_text}) CHARACTER SET utf8mb4"))
       .await.unwrap();
    let key = [field("enabled"), field("symbol")];
    assert_eq!(key[0].ir_type, IRType::Bool);
    assert!(target.validate_index_columns(&key).is_ok());
    assert!(
        target
            .transfer_default_capable_type_for(&key[0].ir_type)
            .is_none()
    );
    let statement = driver
        .migration_renderer()
        .unwrap()
        .render(&MigrationOperation::CreateIndex {
            table: dst.clone(),
            index: IndexInfo {
                name: "native_bool_char".into(),
                columns: vec!["enabled".into(), "symbol".into()],
                is_primary: false,
                is_unique: false,
                index_type: "BTREE".into(),
            },
        })
        .unwrap();
    driver.execute(&h, &statement.sql).await.unwrap();
    let enabled: i8 = sqlx::query_scalar(&format!("SELECT enabled FROM {src}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    let sql = format!(
        "INSERT INTO {dst}(enabled,symbol) VALUES({},'雨')",
        target.format_literal(&Some(Value::Bool(enabled != 0)), &IRType::Bool)
    );
    driver.execute(&h, &sql).await.unwrap();
    let verified: (i8, String) = sqlx::query_as(&format!("SELECT enabled,symbol FROM {dst}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(verified, (1, "雨".into()));
    let columns:Vec<(String,String)>=sqlx::query_as("SELECT CAST(COLUMN_NAME AS CHAR),CAST(COLUMN_TYPE AS CHAR) FROM information_schema.COLUMNS
     WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND COLUMN_NAME IN ('short_note','body') ORDER BY COLUMN_NAME")
      .bind(&dst).fetch_all(&mut f.oracle).await.unwrap();
    assert_eq!(
        columns,
        [
            ("body".into(), "text".into()),
            ("short_note".into(), "varchar(255)".into())
        ]
    );
    let table = driver
        .get_table_schema(&h, &dst, &f.database, None)
        .await
        .unwrap();
    assert!(table.indexes.iter().any(|index|index.name=="native_bool_char"&&index.columns==["enabled","symbol"]));

    let metadata = target
        .transfer_target_character_metadata_query("", Some(&f.database), &src)
        .unwrap();
    let rows = driver.query(&h, &metadata).await.unwrap();
    let text = |value: &Option<Value>| match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Bytes(bytes)) => String::from_utf8(bytes.clone()).ok(),
        _ => None,
    };
    let get = |name: &str| {
        rows.rows
            .iter()
            .find(|r| text(&r[0]).as_deref() == Some(name))
            .unwrap()
    };
    let symbol = get("symbol");
    let numeric = get("raw_total");
    let ascii = get("ascii_note");
    let native = source.columns.iter().find(|c| c.name == "symbol").unwrap();
    let ir = field("symbol");
    let real_charset = text(&symbol[1]).unwrap();
    let real_collation = text(&symbol[2]).unwrap();
    assert!(
        target
            .validate_transfer_target_character_metadata(
                &ir,
                &native.data_type,
                Some(&real_charset),
                Some(&real_collation),
                false
            )
            .is_ok()
    );
    assert!(
        target
            .validate_transfer_target_character_metadata(
                &ir,
                &native.data_type,
                Some(&real_charset),
                text(&numeric[2]).as_deref(),
                false
            )
            .unwrap_err()
            .contains("collation was not inspected")
    );
    assert!(
        target
            .validate_transfer_target_character_metadata(
                &ir,
                &native.data_type,
                Some(&real_charset),
                text(&ascii[2]).as_deref(),
                false
            )
            .unwrap_err()
            .contains("does not belong to character set")
    );
    driver.disconnect(h).await.unwrap();
}
