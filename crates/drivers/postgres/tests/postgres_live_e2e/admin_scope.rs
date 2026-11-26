use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError, Value};
use datazen_driver_postgres::PostgresDriver;
use serde_json::json;

#[tokio::test]
async fn current_database_grants_and_quoted_passwords_take_effect_on_real_postgres() {
    let mut fixture = Fixture::open().await;
    let role = format!("dz_role_{}", uuid::Uuid::new_v4().simple());
    let password = format!(r"pg fixture's; fresh \ {}", uuid::Uuid::new_v4());
    let database = fixture.config.database.clone().unwrap();
    let peer = std::env::var("TEST_PG_DATABASE_B").expect("separate fixture database");
    assert!(peer.starts_with("dz_fixture_") && peer != database);
    let driver = PostgresDriver::new();
    let owner = driver
        .connect(&fixture.config)
        .await
        .expect("real admin session");
    assert!(
        driver
            .execute_command(
                &owner,
                "create_user",
                json!({"username":role,"password":password})
            )
            .await
            .is_ok(),
        "a legitimate quoted password must survive the admin command"
    );
    let mut account = fixture.config.clone();
    account.id = uuid::Uuid::new_v4().to_string();
    account.username = Some(role.clone());
    account.password = Some(password);
    let login = driver
        .connect(&account)
        .await
        .expect("server authenticates the exact password");
    assert_eq!(
        driver
            .query(&login, "SELECT current_user::text")
            .await
            .unwrap()
            .rows,
        vec![vec![Some(Value::String(role.clone()))]]
    );
    let permission = async |oracle: &mut sqlx::PgConnection, name: &str| -> bool {
        sqlx::query_scalar("SELECT has_database_privilege($1,$2,'CREATE')")
            .bind(&role)
            .bind(name)
            .fetch_one(oracle)
            .await
            .expect("independent ACL")
    };
    assert!(!permission(&mut fixture.oracle, &database).await);
    assert!(!permission(&mut fixture.oracle, &peer).await);
    let malformed = driver
        .execute_command(
            &owner,
            "grant_privileges",
            json!({"username":role,"privileges":["BROWSE"]}),
        )
        .await;
    assert!(
        matches!(malformed, Err(DriverError::InvalidConfig(_))),
        "unknown privilege labels fail before dispatch"
    );
    assert!(
        driver
            .execute_command(
                &owner,
                "grant_privileges",
                json!({"username":role,"privileges":["CREATE"]})
            )
            .await
            .is_ok(),
        "omitted database means the current database for database privileges"
    );
    assert!(permission(&mut fixture.oracle, &database).await);
    assert!(!permission(&mut fixture.oracle, &peer).await);
    let schema = format!("dz_account_schema_{}", uuid::Uuid::new_v4().simple());
    assert!(
        driver
            .execute(&login, &format!("CREATE SCHEMA {schema}"))
            .await
            .is_ok(),
        "real user acquired the intended server permission"
    );
    driver
        .execute(&login, &format!("DROP SCHEMA {schema}"))
        .await
        .unwrap();
    assert!(driver
        .execute_command(
            &owner,
            "revoke_privileges",
            json!({"username":role,"privileges":["CREATE"]})
        )
        .await
        .is_ok());
    assert!(!permission(&mut fixture.oracle, &database).await);
    assert!(
        driver
            .execute(&login, &format!("CREATE SCHEMA {schema}"))
            .await
            .is_err(),
        "the independent real role sees the actual revocation"
    );
    driver
        .disconnect(login)
        .await
        .expect("close restricted role");
    driver
        .execute_command(&owner, "drop_user", json!({"username":role}))
        .await
        .expect("cleanup role");
    driver.disconnect(owner).await.expect("close fixture admin");
}

#[tokio::test]
async fn postgres_process_commands_do_not_claim_success_for_a_backend_that_is_gone() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let owner = driver.connect(&fixture.config).await.unwrap();
    let mut separate = fixture.config.clone();
    separate.id = uuid::Uuid::new_v4().to_string();
    let gone = driver
        .connect(&separate)
        .await
        .expect("create disposable backend");
    let pid = driver
        .query(&gone, "SELECT pg_backend_pid()")
        .await
        .unwrap();
    let [Some(Value::Integer(pid))] = pid.rows[0].as_slice() else {
        panic!("server PID not returned")
    };
    let pid: i32 = (*pid).try_into().unwrap();
    driver
        .disconnect(gone)
        .await
        .expect("close the actual backend");
    let present = async |oracle: &mut sqlx::PgConnection| -> bool {
        sqlx::query_scalar("SELECT EXISTS (SELECT FROM pg_stat_activity WHERE pid=$1)")
            .bind(pid)
            .fetch_one(oracle)
            .await
            .expect("independent PG backend presence")
    };
    for _ in 0..40 {
        if !present(&mut fixture.oracle).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(!present(&mut fixture.oracle).await);
    for force in [false, true] {
        let outcome = driver
            .execute_command(&owner, "kill_process", json!({"pid":pid,"force":force}))
            .await
            .expect("server reports process already absent");
        assert_eq!(outcome.data["ok"], false);
    }
    assert!(matches!(
        driver
            .execute_command(&owner, "kill_process", json!({"pid":0}))
            .await,
        Err(DriverError::InvalidConfig(_))
    ));
    driver
        .disconnect(owner)
        .await
        .expect("clean up fixture connection");
}
