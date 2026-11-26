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
