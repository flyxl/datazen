//! Live connection / TLS / authentication coverage for the SQL Server driver.
//!
//! Configuration comes from the **process environment only**. A local env
//! file is read *only* when you opt in by naming it:
//!
//! ```text
//! TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file cargo test -p datazen-driver-sqlserver
//! ```
//!
//! Every test skips (never fails) when the instance is not configured — see
//! `tests/common/mod.rs`.

mod common;

use common::{connect, live_config, scalar, with_resume_retry, Platform};
use datazen_driver_api::DatabaseDriver as _;
use datazen_driver_api::{DriverError, SslMode, Value};

#[tokio::test]
async fn connects_and_reports_server_version() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let version = scalar(&driver, &handle, "SELECT @@VERSION")
        .await
        .expect("SELECT @@VERSION must succeed");
    let version = common::cell_string(&version).unwrap_or_default();
    assert!(
        version.contains("SQL Server") || version.contains("SQL Azure"),
        "unexpected @@VERSION payload: {version:?}"
    );
    if cfg.platform == Platform::AzureSqlDatabase {
        assert!(
            version.contains("SQL Azure"),
            "TEST_SQLSERVER_EXPECT_PLATFORM=azure-sql-db but server reports: {version:?}"
        );
    }

    let db = scalar(&driver, &handle, "SELECT DB_NAME()")
        .await
        .expect("DB_NAME() must succeed");
    assert_eq!(
        common::cell_string(&db).as_deref(),
        Some(cfg.database.as_str()),
        "connection must land on the configured database"
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn test_connection_returns_server_info_without_leaking_credentials() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();

    let config = cfg.default_config();
    let info = with_resume_retry("test_connection", || driver.test_connection(&config))
        .await
        .expect("test_connection must succeed");
    assert!(!info.server_version.trim().is_empty());
    assert!(!info.server_type.trim().is_empty());
    assert!(
        !info.server_version.contains(&cfg.password),
        "server info must never echo the password"
    );
}

#[tokio::test]
async fn get_databases_lists_the_connected_database() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let databases = driver
        .get_databases(&handle)
        .await
        .expect("get_databases must succeed");
    assert!(
        databases
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&cfg.database)),
        "expected {} in {databases:?}",
        cfg.database
    );
    if !cfg.platform.lists_all_databases() && databases.len() > 1 {
        eprintln!(
            "ℹ️  {} lists {} databases (expected current-only on this platform)",
            cfg.platform.label(),
            databases.len()
        );
    }

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn get_tables_reads_the_catalog_without_error() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    // Azure SQL Database still exposes sys.tables/sys.schemas for the current
    // database; a failure here is a real driver defect, not a platform limit.
    let tables = driver
        .get_tables(&handle, &cfg.database, Some(&cfg.schema))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "get_tables(database={}, schema={}) failed: {e}",
                cfg.database, cfg.schema
            )
        });
    for table in &tables {
        assert!(
            table.schema.is_some(),
            "SQL Server has a real schema level; TableInfo::schema must be set (got {table:?})"
        );
    }
    eprintln!(
        "ℹ️  {} table(s) visible in {}.{}",
        tables.len(),
        cfg.database,
        cfg.schema
    );

    let _ = driver.disconnect(handle).await;
}

#[tokio::test]
async fn wrong_password_is_a_login_failure() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();
    let mut config = cfg.default_config();
    config.password = Some(format!("{}definitely-wrong", cfg.password));

    let error = driver
        .connect(&config)
        .await
        .expect_err("a wrong password must not connect");
    assert!(
        matches!(
            error,
            DriverError::ConnectionFailed(_) | DriverError::AuthenticationFailed(_)
        ),
        "expected a login failure, got {error:?}"
    );
    let text = error.to_string().to_ascii_lowercase();
    assert!(
        text.contains("login failed") || text.contains("password") || text.contains("18456"),
        "the error must be about credentials, not instance state: {error}"
    );
    assert!(
        !common::is_azure_resume_error(&error),
        "a wrong password must not be mistaken for the Azure resume window: {error}"
    );
}

#[tokio::test]
async fn unresolvable_host_is_a_connect_failure() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();
    let mut config = cfg.default_config();
    config.host = Some("no-such-host.invalid".into());
    config.connection_timeout = 5;

    let error = driver
        .connect(&config)
        .await
        .expect_err("an unresolvable host must not connect");
    assert!(
        matches!(error, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {error:?}"
    );
}

#[tokio::test]
async fn closed_port_is_a_connect_failure_within_the_timeout() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();
    let mut config = cfg.default_config();
    config.port = Some(9); // discard: nothing listens
    config.connection_timeout = 5;

    let started = std::time::Instant::now();
    let error = driver
        .connect(&config)
        .await
        .expect_err("a closed port must not connect");
    assert!(
        matches!(error, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {error:?}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "connection_timeout must bound the attempt (took {:?})",
        started.elapsed()
    );
}

#[tokio::test]
async fn missing_host_and_username_are_config_errors() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();

    let mut no_host = cfg.default_config();
    no_host.host = None;
    let error = driver
        .connect(&no_host)
        .await
        .expect_err("host is required");
    assert!(
        matches!(error, DriverError::InvalidConfig(ref m) if m.contains("host")),
        "expected InvalidConfig(host), got {error:?}"
    );

    let mut no_user = cfg.default_config();
    no_user.username = None;
    let error = driver
        .connect(&no_user)
        .await
        .expect_err("username is required");
    assert!(
        matches!(error, DriverError::InvalidConfig(ref m) if m.contains("username")),
        "expected InvalidConfig(username), got {error:?}"
    );
}

/// The GUI offers only `disable|prefer|require` for SQL Server, so the
/// certificate-verifying path (`VerifyCa`/`VerifyFull`) is otherwise never
/// exercised. The host must be an FQDN for the name check to pass.
#[tokio::test]
async fn verify_full_validates_the_certificate_chain() {
    let Some(cfg) = live_config() else { return };
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();
    let config = cfg.connection_config(SslMode::VerifyFull, false);

    let handle = with_resume_retry("VerifyFull handshake", || driver.connect(&config))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "VerifyFull against {}:{} failed — certificate chain not validated \
                 (rustls native roots). Error: {e}",
                cfg.host, cfg.port
            )
        });
    let value = scalar(&driver, &handle, "SELECT 1")
        .await
        .expect("query after verified TLS handshake");
    assert_eq!(common::cell_i64(&value), Some(1));
    let _ = driver.disconnect(handle).await;
}

/// Azure SQL Database refuses unencrypted TDS; only meaningful there.
#[tokio::test]
async fn unencrypted_tds_is_rejected_by_azure() {
    let Some(cfg) = live_config() else { return };
    if cfg.platform != Platform::AzureSqlDatabase {
        eprintln!(
            "⏭  Skipping unencrypted-TDS check: platform is {}",
            cfg.platform.label()
        );
        return;
    }
    let driver = datazen_driver_sqlserver::SqlServerDriver::new();
    let config = cfg.connection_config(SslMode::Disable, false);

    match driver.connect(&config).await {
        Err(error) => {
            eprintln!("ℹ️  unencrypted TDS rejected as expected: {error}");
        }
        Ok(handle) => {
            // Login succeeded without TLS: report it, because the driver is
            // supposed to keep `disable` meaning "plaintext TDS".
            let probe = scalar(&driver, &handle, "SELECT 1").await;
            let _ = driver.disconnect(handle).await;
            assert!(
                probe.is_err(),
                "Azure SQL accepted a plaintext TDS connection (SELECT 1 = {probe:?})"
            );
        }
    }
}

#[tokio::test]
async fn unicode_survives_a_windows_collation_session() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    // The login log showed `SQL collation changed to windows-1252`, so an
    // N-prefixed literal is the only safe way to move non-ASCII text.
    let probe = scalar(&driver, &handle, "SELECT N'中文测试 🚀 Ω' AS probe")
        .await
        .expect("unicode SELECT must succeed");
    assert_eq!(
        common::cell_string(&probe).as_deref(),
        Some("中文测试 🚀 Ω")
    );

    let round_trip = scalar(&driver, &handle, "SELECT CAST(N'数据禅' AS NVARCHAR(50))")
        .await
        .expect("CAST must succeed");
    assert_eq!(common::cell_string(&round_trip).as_deref(), Some("数据禅"));

    // A plain (non-N) literal is interpreted in the session code page; assert
    // only that it does not corrupt the connection, not its exact value.
    let ascii = scalar(&driver, &handle, "SELECT 'ascii-ok'")
        .await
        .expect("ascii literal must succeed");
    assert_eq!(common::cell_string(&ascii).as_deref(), Some("ascii-ok"));
    assert!(matches!(probe, Some(Value::String(_))));

    let _ = driver.disconnect(handle).await;
}
