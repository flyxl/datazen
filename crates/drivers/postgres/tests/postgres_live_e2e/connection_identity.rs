use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError, Value};
use datazen_driver_postgres::PostgresDriver;
use serde_json::Value as Json;
use sqlx::{
    postgres::{PgConnectOptions, PgSslMode},
    Connection, PgConnection,
};

#[tokio::test]
async fn real_cluster_identity_and_foreign_database_pool_remain_bound_to_their_targets() {
    let mut fixture = Fixture::open().await;
    let database = fixture.config.database.clone().expect("fixture DB");
    let peer = std::env::var("TEST_PG_DATABASE_B").expect("second marked fixture database");
    assert!(peer.starts_with("dz_fixture_") && peer != database);
    let connect_peer = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(fixture.config.port.unwrap())
        .username(fixture.config.username.as_deref().expect("fixture owner"))
        .password(fixture.config.password.as_deref().unwrap_or_default())
        .database(&peer)
        .ssl_mode(PgSslMode::Disable);
    let mut peer_oracle = PgConnection::connect_with(&connect_peer)
        .await
        .expect("independent peer DB");
    let marker: String = sqlx::query_scalar("SELECT marker FROM public.rune_e2e_fixture")
        .fetch_one(&mut peer_oracle)
        .await
        .expect("verified disposable peer database");
    assert_eq!(marker, "datazen-rune-disposable");
    let session_facts = async |db: &mut PgConnection| -> (String, String, String) {
        sqlx::query_as(
            "SELECT system_identifier::text,current_database()::text,version() FROM pg_control_system()",
        ).fetch_one(db).await.expect("independent server identity and version")
    };
    let (system, actual_home, version) = session_facts(&mut fixture.oracle).await;
    let (peer_system, actual_peer, _) = session_facts(&mut peer_oracle).await;
    assert_eq!(system, peer_system, "fixtures share a genuine cluster");
    assert_ne!(actual_home, actual_peer);

    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real home connection");
    let inspect = async |name: &str| -> Json {
        let raw = driver
            .physical_database_identity(&handle, name)
            .await
            .expect("identity lookup")
            .expect("fixture owner can read cluster identity");
        serde_json::from_str(&raw).expect("typed physical identity JSON")
    };
    assert_eq!(
        inspect("").await,
        serde_json::json!({"driver":"postgresql","server":system,"database":actual_home})
    );
    assert_eq!(
        inspect(&peer).await,
        serde_json::json!({"driver":"postgresql","server":peer_system,"database":actual_peer})
    );
    let info = driver
        .get_server_info(&handle)
        .await
        .expect("server version from live connection");
    assert_eq!(info.server_version, version);
    assert_eq!(info.server_type, "PostgreSQL");
    let listed = driver
        .open_databases(&handle)
        .await
        .expect("open catalog handles");
    assert_eq!(
        listed,
        [peer.clone()],
        "only separately closable pools are listed"
    );
    assert!(matches!(
        driver.close_database(&handle, &database).await,
        Err(DriverError::InvalidConfig(_))
    ));
    let home = driver
        .query(&handle, "SELECT current_database()::text")
        .await
        .expect("home remained attached");
    assert_eq!(home.rows, vec![vec![Some(Value::String(database.clone()))]]);
    driver
        .close_database(&handle, &peer)
        .await
        .expect("retire only foreign pool");
    assert!(!driver
        .open_databases(&handle)
        .await
        .unwrap()
        .iter()
        .any(|db| db == &peer));
    assert_eq!(
        inspect(&peer).await["database"],
        peer,
        "foreign pool reopened without moving home"
    );

    // The absent selected database defaults to PostgreSQL's maintenance DB,
    // read only; neither tested fixture is replaced by opening this handle.
    let mut default_config = fixture.config.clone();
    default_config.id = uuid::Uuid::new_v4().to_string();
    default_config.database = None;
    let default = driver
        .connect(&default_config)
        .await
        .expect("default maintenance session");
    assert_eq!(
        driver
            .query(&default, "SELECT current_database()::text")
            .await
            .unwrap()
            .rows,
        vec![vec![Some(Value::String("postgres".into()))]]
    );
    driver
        .disconnect(default)
        .await
        .expect("close maintenance session");
    driver
        .disconnect(handle.clone())
        .await
        .expect("close home connections");
    assert!(matches!(
        driver.get_server_info(&handle).await,
        Err(DriverError::ConnectionFailed(_) | DriverError::SessionLost)
    ));
}
