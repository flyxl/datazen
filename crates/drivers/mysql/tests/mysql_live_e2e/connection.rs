use super::Fixture;
use datazen_driver_api::{DatabaseDriver, SslMode, Value};
use datazen_driver_mysql::MysqlDriver;

fn cell_text(value: &Option<Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Bytes(b)) => String::from_utf8(b.clone()).ok(),
        _ => None,
    }
}

#[tokio::test]
async fn mysql_tls_modes_never_silently_downgrade_verified_connections() {
    let mut f = Fixture::open().await;
    let d = MysqlDriver::new(false);
    let oracle_version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&mut f.oracle)
        .await
        .expect("native server version");
    for (mode, encrypted) in [
        (SslMode::Disable, false),
        (SslMode::Prefer, true),
        (SslMode::Require, true),
    ] {
        let mut config = f.cfg.clone();
        config.ssl_mode = mode;
        let version = d
            .test_connection(&config)
            .await
            .expect("real temporary MySQL connection");
        assert_eq!(version.server_type, "MySQL");
        assert_eq!(version.server_version, oracle_version);
        let h = d
            .connect_fixed_session(&config)
            .await
            .expect("real MySQL fixed TLS session");
        let info = d
            .get_server_info(&h)
            .await
            .expect("metadata on exact physical socket");
        assert_eq!(info.server_version, oracle_version);
        let cipher = d
            .query(&h, "SHOW SESSION STATUS LIKE 'Ssl_cipher'")
            .await
            .expect("query actual handshake state");
        let actual = cell_text(&cipher.rows[0][1]).expect("native session cipher");
        assert_eq!(
            !actual.is_empty(),
            encrypted,
            "cipher comes from real server"
        );
        d.disconnect(h)
            .await
            .expect("release fixture TLS connection");
    }
    for mode in [SslMode::VerifyCa, SslMode::VerifyFull] {
        let mut config = f.cfg.clone();
        config.ssl_mode = mode;
        config.connection_timeout = 2;
        assert!(
            d.connect(&config).await.is_err(),
            "disposable MySQL self-signed CA must not be silently trusted"
        );
    }
    let mut wrong = f.cfg.clone();
    wrong.password = Some(uuid::Uuid::new_v4().to_string());
    wrong.connection_timeout = 2;
    assert!(
        d.test_connection(&wrong).await.is_err(),
        "native authentication rejects wrong credential"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn mysql_database_identity_proves_exact_requested_catalog_and_keeps_session_unchanged() {
    let mut f = Fixture::open().await;
    let (driver, h) = f.connect().await;
    let other = std::env::var("TEST_MYSQL_DATABASE_B").expect("real secondary catalog");
    let engine_uuid: String = sqlx::query_scalar("SELECT CAST(@@server_uuid AS CHAR)")
        .fetch_one(&mut f.oracle)
        .await
        .expect("real server identifier");
    let a = driver
        .physical_database_identity(&h, &f.database)
        .await
        .expect("retrieve native identity")
        .expect("node is available");
    let b = driver
        .physical_database_identity(&h, &other)
        .await
        .expect("retrieve other catalog identity")
        .expect("other actual database proves identity");
    assert_ne!(
        a, b,
        "different databases on the same server are different endpoints"
    );
    let canonical: serde_json::Value = serde_json::from_str(&b).expect("stable native identity");
    assert_eq!(canonical["id"], engine_uuid);
    assert_eq!(canonical["database"], other);
    let fallback = driver
        .physical_database_identity(&h, "")
        .await
        .expect("default catalog fallback");
    assert_eq!(fallback.as_deref(), Some(a.as_str()));
    let absent = format!("dz_no_identity_{}", uuid::Uuid::new_v4().simple());
    assert_eq!(
        driver
            .physical_database_identity(&h, &absent)
            .await
            .expect("unverified missing identity"),
        None
    );
    assert_eq!(
        driver
            .query(&h, "SELECT DATABASE()")
            .await
            .expect("identity is not a session switch")
            .rows,
        [vec![Some(Value::String(f.database.clone()))]]
    );
    assert_eq!(
        driver
            .schema_scope_identity(&h, &f.database, "")
            .await
            .expect("MySQL single namespace identity"),
        None
    );
    driver
        .disconnect(h)
        .await
        .expect("release primary identity connection");
}

#[tokio::test]
async fn mysql_configured_but_offline_group_replication_keeps_native_node_identity() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let initial = driver
        .physical_database_identity(&handle, &f.database)
        .await
        .expect("real standalone identity")
        .expect("initial server has no replication group");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&initial).unwrap()["scope"],
        "node"
    );
    sqlx::raw_sql("INSTALL PLUGIN group_replication SONAME 'group_replication.so'")
        .execute(&mut f.oracle)
        .await
        .expect("load official native MySQL plugin in disposable fixture");
    let group = uuid::Uuid::new_v4().to_string();
    sqlx::raw_sql(&format!(
        "SET GLOBAL group_replication_group_name='{group}'"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("actual native group settings");
    let members:i64=sqlx::query_scalar("SELECT COUNT(*) FROM performance_schema.replication_group_members WHERE MEMBER_ID=@@server_uuid AND MEMBER_STATE='ONLINE'")
        .fetch_one(&mut f.oracle).await.expect("independent native membership catalog");
    assert_eq!(members, 0, "configuration alone is not a real group member");
    assert_eq!(
        driver
            .physical_database_identity(&handle, &f.database)
            .await
            .unwrap(),
        Some(initial.clone())
    );
    sqlx::raw_sql("UNINSTALL PLUGIN group_replication")
        .execute(&mut f.oracle)
        .await
        .expect("restore disposable server standalone state");
    assert_eq!(
        driver
            .physical_database_identity(&handle, &f.database)
            .await
            .unwrap()
            .as_deref(),
        Some(initial.as_str())
    );
    driver
        .disconnect(handle)
        .await
        .expect("close real fixture handle");
}

#[tokio::test]
async fn mysql_connection_limit_degrades_control_without_cancelling_an_unrelated_native_query() {
    use datazen_driver_api::{DriverError, QueryExecutionId};
    use std::{sync::Arc, time::Duration};
    let mut f = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("dz_ctl_{}", &tag[..15]);
    let password = uuid::Uuid::new_v4().simple().to_string();
    sqlx::raw_sql(&format!("CREATE USER '{user}'@'127.0.0.1' IDENTIFIED BY '{password}' WITH MAX_USER_CONNECTIONS 1; GRANT SELECT ON `{}`.* TO '{user}'@'127.0.0.1'",f.database))
        .execute(&mut f.oracle).await.expect("server enforces real limited connection quota");
    let mut cfg = f.cfg.clone();
    cfg.username = Some(user.clone());
    cfg.password = Some(password);
    cfg.max_pool_size = 1;
    cfg.connection_timeout = 1;
    let d = Arc::new(MysqlDriver::new(false));
    let handle = d
        .connect(&cfg)
        .await
        .expect("data connection succeeds despite its auxiliary control pool being denied");
    let lock = format!("quota_{tag}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT GET_LOCK(?,0)")
            .bind(&lock)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        1
    );
    let execution = QueryExecutionId::new("limited-control-owned");
    d.prepare_query_execution(&handle, &execution)
        .await
        .unwrap();
    let worker = tokio::spawn({
        let (d, handle, execution) = (d.clone(), handle.clone(), execution.clone());
        let sql = format!("SELECT GET_LOCK('{lock}',8)");
        async move {
            d.query_stream_with_execution(&handle, &execution, &sql, None, Arc::new(|_| {}))
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(4),async{
        loop{
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE STATE='User lock' AND USER=?")
                .bind(&user).fetch_one(&mut f.oracle).await.unwrap();
            if count==1{break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("limited account has one real active data thread");
    assert!(
        matches!(d.cancel_query_with_execution(&handle,&execution).await,Err(DriverError::ConnectionFailed(message)) if message.contains("Control pool not found"))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT RELEASE_LOCK(?)")
            .bind(&lock)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        1
    );
    worker.await.unwrap().expect(
        "failed specific cancellation must be cleared and not masquerade as canceled result",
    );
    assert_eq!(
        d.query(&handle, "SELECT 33").await.unwrap().rows,
        [vec![Some(Value::Integer(33))]]
    );
    d.disconnect(handle)
        .await
        .expect("physical constrained session closes");
    sqlx::raw_sql(&format!("DROP USER '{user}'@'127.0.0.1'"))
        .execute(&mut f.oracle)
        .await
        .expect("only remove created fixture account");
}
