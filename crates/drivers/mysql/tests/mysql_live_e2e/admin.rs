use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError};
use serde_json::json;
use sqlx::{
    Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::time::Duration;

#[tokio::test]
async fn admin_creates_grants_inspects_and_revokes_access_on_real_mysql() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let db = format!("dz_fixture_mysql_admin_{tag}");
    let account = format!("dz_role_{}", &tag[..15]);
    let password = uuid::Uuid::new_v4().simple().to_string();
    for (command, args) in [
        ("create_database", json!({"name":db,"encoding":"utf8mb4"})),
        (
            "create_user",
            json!({"username":account,"password":password}),
        ),
    ] {
        assert_eq!(
            driver
                .execute_command(&handle, command, args)
                .await
                .expect("live administrative command")
                .data["ok"],
            true
        );
    }
    let mysql_charset: String = sqlx::query_scalar(
        "SELECT DEFAULT_CHARACTER_SET_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
    )
    .bind(&db)
    .fetch_one(&mut f.oracle)
    .await
    .expect("independent catalog");
    assert_eq!(mysql_charset, "utf8mb4");
    sqlx::raw_sql(&format!("CREATE TABLE `{db}`.`sample` (id INTEGER PRIMARY KEY, note VARCHAR(100)); INSERT INTO `{db}`.`sample` VALUES (1,'雨');"))
        .execute(&mut f.oracle).await.expect("live table");
    let opts = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(f.cfg.port.expect("port"))
        .username(&account)
        .password(&password)
        .ssl_mode(MySqlSslMode::Disabled);
    let mut limited = MySqlConnection::connect_with(&opts)
        .await
        .expect("actual account login");
    let select = format!("SELECT note FROM `{db}`.`sample`");
    assert!(
        sqlx::query_scalar::<_, String>(&select)
            .fetch_one(&mut limited)
            .await
            .is_err()
    );
    driver
        .execute_command(
            &handle,
            "grant_privileges",
            json!({"username":account,"database":db,"privileges":["SELECT"],"grantOption":true}),
        )
        .await
        .expect("grant fixture table access");
    let note: String = sqlx::query_scalar(&select)
        .fetch_one(&mut limited)
        .await
        .expect("granted read");
    assert_eq!(note, "雨");
    let grants: Vec<String> = sqlx::query_scalar("SHOW GRANTS")
        .fetch_all(&mut limited)
        .await
        .expect("actual grants");
    assert!(
        grants
            .iter()
            .any(|grant| grant.contains("SELECT") && grant.contains("WITH GRANT OPTION"))
    );
    driver
        .execute_command(
            &handle,
            "revoke_privileges",
            json!({"username":account,"database":db,"privileges":["SELECT"]}),
        )
        .await
        .expect("revoke fixture table access");
    limited.close().await.expect("end earlier account check");
    let mut limited = MySqlConnection::connect_with(&opts)
        .await
        .expect("new session observes revoked grants");
    assert!(
        sqlx::query_scalar::<_, String>(&select)
            .fetch_one(&mut limited)
            .await
            .is_err()
    );
    limited.close().await.expect("ordinary close");
    let status = driver
        .execute_command(&handle, "server_status_snapshot", json!({}))
        .await
        .expect("real server status");
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&mut f.oracle)
        .await
        .expect("real version");
    assert_eq!(status.data["version"], version);
    let estimate = driver
        .execute_command(
            &handle,
            "estimate_table_rows",
            json!({"database":db,"table":"sample"}),
        )
        .await
        .expect("estimate actual table");
    assert!(estimate.data["estimatedRows"].is_number());
    for (command, args) in [
        ("drop_user", json!({"username":account})),
        ("drop_database", json!({"name":db})),
    ] {
        driver
            .execute_command(&handle, command, args)
            .await
            .expect("remove only fixture-created object");
    }
    assert!(MySqlConnection::connect_with(&opts).await.is_err());
    let remains: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
    )
    .bind(db)
    .fetch_one(&mut f.oracle)
    .await
    .expect("independent drop proof");
    assert_eq!(remains, 0);
    driver.disconnect(handle).await.expect("disconnect");
}

#[tokio::test]
async fn admin_process_list_and_cancel_affect_only_the_fixture_backend() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let pid: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut f.oracle)
        .await
        .expect("actual fixture PID");
    let listed = driver
        .execute_command(&handle, "list_processes", json!({}))
        .await
        .expect("real process list");
    assert!(
        listed.data["processes"]
            .as_array()
            .expect("processes")
            .iter()
            .any(|p| p["pid"].as_u64() == Some(pid))
    );
    let worker = tokio::spawn(async move {
        let result = sqlx::query_scalar::<_, i32>("SELECT SLEEP(20)")
            .fetch_one(&mut f.oracle)
            .await;
        (result, f.oracle)
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = driver
                .execute_command(&handle, "list_processes", json!({}))
                .await
                .expect("observed native sleep");
            if response.data["processes"]
                .as_array()
                .expect("processes")
                .iter()
                .any(|p| {
                    p["pid"].as_u64() == Some(pid)
                        && p["query"].as_str().is_some_and(|s| s.contains("SLEEP(20)"))
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fixture query started");
    driver
        .execute_command(&handle, "kill_process", json!({"pid":pid,"force":false}))
        .await
        .expect("cancel only fixture query");
    let (killed, mut oracle) = tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("native cancel resolves")
        .expect("worker");
    assert!(killed.is_err() || matches!(killed, Ok(1)));
    let after: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut oracle)
        .await
        .expect("query-only cancel keeps same connection");
    assert_eq!(after, pid);
    assert!(matches!(
        driver
            .execute_command(&handle, "kill_process", json!({"pid":0}))
            .await,
        Err(DriverError::InvalidConfig(_))
    ));
    driver
        .execute_command(&handle, "kill_process", json!({"pid":pid,"force":true}))
        .await
        .expect("terminate only owned fixture backend");
    assert!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut oracle)
            .await
            .is_err()
    );
    assert_eq!(
        driver
            .query(&handle, "SELECT 2")
            .await
            .expect("unrelated driver session survives")
            .rows
            .len(),
        1
    );
    driver.disconnect(handle).await.expect("disconnect");
}

#[tokio::test]
async fn mysql_admin_reports_exact_native_grants_and_refuses_incomplete_or_unauthorized_requests() {
    let mut f = Fixture::open().await;
    let (admin, handle) = f.connect().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("dz_acl_{}", &tag[..20]);
    let password = uuid::Uuid::new_v4().simple().to_string();
    let database = format!("dz_fixture_mysql_acl_{}", &tag[..20]);
    for (cmd, args) in [
        ("create_database", json!({"name":database})),
        ("create_user", json!({"username":user,"password":password})),
        (
            "grant_privileges",
            json!({"username":user,"database":database,"privileges":["SELECT"]}),
        ),
        (
            "grant_privileges",
            json!({"username":user,"privileges":["PROCESS"]}),
        ),
    ] {
        admin
            .execute_command(&handle, cmd, args)
            .await
            .expect("actual administrative operations in verified fixture");
    }
    sqlx::raw_sql(&format!("CREATE TABLE {database}.tracked(id INT NOT NULL); INSERT INTO {database}.tracked VALUES(8);"))
      .execute(&mut f.oracle).await.expect("actual limited catalog");
    let grants = admin
        .execute_command(&handle, "list_privileges", json!({}))
        .await
        .expect("real user privilege listing");
    assert!(grants.data["grants"].as_array().unwrap().iter().any(|g| {
        g["grantee"]
            .as_str()
            .is_some_and(|name| name.contains(&user))
            && g["privilege"] == "PROCESS"
    }));
    let mut limited_cfg = f.cfg.clone();
    limited_cfg.username = Some(user.clone());
    limited_cfg.password = Some(password);
    limited_cfg.database = Some(database.clone());
    limited_cfg.max_pool_size = 1;
    let limited = admin
        .connect(&limited_cfg)
        .await
        .expect("actual least privilege tenant session");
    let estimation = admin
        .execute_command(
            &limited,
            "estimate_table_rows",
            json!({"schema":database,"table":"tracked"}),
        )
        .await
        .expect("ordinary MySQL estimate scoped to actual target");
    assert!(estimation.data["estimatedRows"].is_number());
    assert!(matches!(
        admin
            .execute_command(
                &limited,
                "create_database",
                json!({"name":format!("dz_fixture_mysql_denied_{tag}")})
            )
            .await,
        Err(DriverError::QueryFailed(_))
    ));
    let read = admin
        .query(&limited, "SELECT id FROM tracked")
        .await
        .expect("ACL allows configured scope read only");
    assert_eq!(
        read.rows,
        [vec![Some(datazen_driver_api::Value::Integer(8))]]
    );
    for (cmd, args) in [
        ("create_database", json!({})),
        ("drop_database", json!({})),
        ("create_user", json!({})),
        ("drop_user", json!({})),
        ("grant_privileges", json!({"username":user})),
        ("grant_privileges", json!({"username":user,"privileges":[]})),
        ("revoke_privileges", json!({"username":user})),
        (
            "revoke_privileges",
            json!({"username":user,"privileges":[]}),
        ),
        ("estimate_table_rows", json!({})),
    ] {
        assert!(
            matches!(
                admin.execute_command(&handle, cmd, args).await,
                Err(DriverError::InvalidConfig(_))
            ),
            "missing user-required field rejected through actual command handler: {cmd}"
        );
    }
    admin
        .execute_command(
            &handle,
            "revoke_privileges",
            json!({"username":user,"privileges":["PROCESS"]}),
        )
        .await
        .expect("global grant revocation on actual mysql");
    let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.USER_PRIVILEGES WHERE LOCATE(?,GRANTEE)>0 AND PRIVILEGE_TYPE='PROCESS'")
        .bind(&user).fetch_one(&mut f.oracle).await.expect("independently verify actual grant revocation");
    assert_eq!(exists, 0);
    let row: i32 = sqlx::query_scalar(&format!("SELECT id FROM {database}.tracked"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(
        row, 8,
        "malformed commands never mutate legitimate owned catalog data"
    );
    admin.disconnect(limited).await.unwrap();
    admin
        .execute_command(&handle, "drop_user", json!({"username":user}))
        .await
        .unwrap();
    admin
        .execute_command(&handle, "drop_database", json!({"name":database}))
        .await
        .unwrap();
    admin.disconnect(handle).await.unwrap();
}
