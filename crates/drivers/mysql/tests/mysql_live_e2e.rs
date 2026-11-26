//! Independent end-to-end oracles use the real MySQL wire connection, never mocks.
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use datazen_driver_mysql::MysqlDriver;
use serde_json::json;
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlSslMode},
    Connection, MySqlConnection,
};

struct Fixture {
    cfg: ConnectionConfig,
    oracle: MySqlConnection,
    database: String,
}

impl Fixture {
    async fn open() -> Self {
        let require = |name: &str| {
            std::env::var(name).unwrap_or_else(|_| panic!("missing fixture variable {name}"))
        };
        let host = require("TEST_MYSQL_HOST");
        assert_eq!(
            host, "127.0.0.1",
            "fixture must not address an external server"
        );
        let port: u16 = require("TEST_MYSQL_PORT").parse().expect("port");
        assert_ne!(port, 0);
        let database = require("TEST_MYSQL_DATABASE");
        assert!(database.starts_with("dz_fixture_mysql_"));
        let user = require("TEST_MYSQL_USER");
        let pass = require("TEST_MYSQL_PASSWORD");
        let marker = require("DATAZEN_MYSQL_E2E_MARKER");
        let mut oracle = MySqlConnection::connect_with(
            &MySqlConnectOptions::new()
                .host(&host)
                .port(port)
                .database(&database)
                .username(&user)
                .password(&pass)
                .ssl_mode(MySqlSslMode::Disabled),
        )
        .await
        .expect("connect to real disposable MySQL");
        let measured: (String, String) =
            sqlx::query_as("SELECT @@version_comment, marker FROM rune_e2e_fixture")
                .fetch_one(&mut oracle)
                .await
                .expect("actual MySQL fixture marker");
        assert!(measured.0.contains("MySQL") && !measured.0.contains("MariaDB"));
        assert_eq!(measured.1, marker);
        let cfg = serde_json::from_value(json!({
            "id": uuid::Uuid::new_v4().to_string(), "name":"Disposable MySQL E2E", "databaseType":"mysql",
            "host":host,"port":port,"username":user,"password":pass,"database":database,
            "sslMode":"disable","connectionTimeout":5,"maxPoolSize":4,
        })).expect("fixture profile");
        Self {
            cfg,
            oracle,
            database,
        }
    }

    async fn connect(&self) -> (MysqlDriver, datazen_driver_api::ConnectionHandle) {
        let d = MysqlDriver::new(false);
        let handle = d.connect(&self.cfg).await.expect("open MySQL driver");
        (d, handle)
    }
}

#[path = "mysql_live_e2e/admin.rs"]
mod admin;
#[path = "mysql_live_e2e/structure.rs"]
mod structure;
#[path = "mysql_live_e2e/sync.rs"]
mod sync;
#[path = "mysql_live_e2e/values.rs"]
mod values;

#[tokio::test]
async fn mysql_dependency_catalog_status_matches_real_server_visibility() {
    use datazen_driver_api::schema_dependencies::{
        mysql_table_dependencies_sql, MYSQL_DEPENDENCY_GRANTS_SQL,
    };
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_dep_status_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table} (id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let raw = driver
        .query(
            &handle,
            &mysql_table_dependencies_sql(&table, Some(&f.database)),
        )
        .await
        .expect("native dependency catalog SQL");
    let grants = driver
        .query(&handle, MYSQL_DEPENDENCY_GRANTS_SQL)
        .await
        .expect("native SHOW GRANTS");
    let statuses: Vec<_> = grants
        .rows
        .iter()
        .map(|row| match row.first().and_then(Option::as_ref) {
            Some(datazen_driver_api::Value::String(_)) => "text",
            Some(datazen_driver_api::Value::Bytes(_)) => "bytes",
            _ => "other",
        })
        .collect();
    assert!(
        driver
            .has_complete_foreign_key_catalog_visibility(&handle)
            .await
            .expect("real permission proof"),
        "{statuses:?}"
    );
    let contract = driver
        .execute_command(
            &handle,
            "get_object_dependencies",
            json!({"kind":"table","name":table,"schema":f.database}),
        )
        .await
        .expect("real dependency API");
    assert_eq!(
        contract.data["complete"], true,
        "native status {:?}, grant wire types {statuses:?}",
        raw.rows
    );
    assert_eq!(contract.data["dependencies"], json!([]));
    driver.disconnect(handle).await.unwrap();
}
#[path = "mysql_live_e2e/catalog.rs"]
mod catalog;
#[path = "mysql_live_e2e/connection.rs"]
mod connection;
#[path = "mysql_live_e2e/execution.rs"]
mod execution;
#[path = "mysql_live_e2e/grid.rs"]
mod grid;
#[path = "mysql_live_e2e/migration_objects.rs"]
mod migration_objects;
#[path = "mysql_live_e2e/observation.rs"]
mod observation;
#[path = "mysql_live_e2e/query_modes.rs"]
mod query_modes;
#[path = "mysql_live_e2e/resource.rs"]
mod resource;
#[path = "mysql_live_e2e/scope.rs"]
mod scope;
