use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError, SslMode, Value};
use datazen_driver_postgres::PostgresDriver;

#[tokio::test]
async fn verified_tls_authentication_and_connection_failures_are_real_postgres_outcomes() {
    let mut fixture = Fixture::open().await;
    assert!(
        std::env::var_os("PGSSLROOTCERT").is_some(),
        "live gate TLS CA required"
    );
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent PostgreSQL version");
    let driver = PostgresDriver::new();
    for (mode, tls) in [
        (SslMode::Disable, false),
        (SslMode::Prefer, true),
        (SslMode::Require, true),
        (SslMode::VerifyCa, true),
        (SslMode::VerifyFull, true),
    ] {
        let mut config = fixture.config.clone();
        config.ssl_mode = mode;
        let info = driver
            .test_connection(&config)
            .await
            .expect("actual server probe");
        assert_eq!(info.server_version, version);
        assert_eq!(info.server_type, "PostgreSQL");
        let handle = driver
            .connect(&config)
            .await
            .expect("actual driver session");
        let remote = driver.query(&handle,
            "SELECT ssl, current_database()::text FROM pg_stat_ssl WHERE pid = pg_backend_pid()")
            .await.expect("server-side transport observation");
        assert_eq!(
            remote.rows,
            vec![vec![
                Some(Value::Bool(tls)),
                config.database.clone().map(Value::String)
            ]]
        );
        driver
            .disconnect(handle)
            .await
            .expect("disconnect real session");
    }

    // The same TCP endpoint can open an encrypted Require session but not
    // by DNS name with VerifyFull: the local certificate has only an IP SAN.
    let mut comparator = fixture.config.clone();
    comparator.host = Some("localhost".into());
    comparator.ssl_mode = SslMode::Require;
    driver
        .test_connection(&comparator)
        .await
        .expect("the alternate hostname reaches the same TLS server");
    comparator.ssl_mode = SslMode::VerifyFull;
    assert!(
        matches!(
            driver.test_connection(&comparator).await,
            Err(DriverError::ConnectionFailed(_))
        ),
        "host mismatch must fail closed"
    );

    let mut wrong_password = fixture.config.clone();
    wrong_password.password = Some(uuid::Uuid::new_v4().to_string());
    wrong_password.ssl_mode = SslMode::VerifyFull;
    assert!(
        matches!(
            driver.connect(&wrong_password).await,
            Err(DriverError::ConnectionFailed(_))
        ),
        "real SCRAM rejects incorrect credentials"
    );
    let mut absent_database = fixture.config.clone();
    absent_database.database = Some(format!("absent_{}", uuid::Uuid::new_v4().simple()));
    assert!(
        matches!(
            driver.test_connection(&absent_database).await,
            Err(DriverError::ConnectionFailed(_))
        ),
        "the real server has no selected database"
    );
    // A failed probe must not strand or change the independently connected server.
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 7")
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        7
    );
}
