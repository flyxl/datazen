//! Real Oracle MySQL 8.4.7 counterexample to account-grant-based absence proof.
//! Not a transactionBlock recipe, observer proof, or declaration of support.
//! Process environment only. Requires the standard MYSQL_MIGRATION_TEST_* gate
//! plus DATAZEN_MYSQL_PROOF_DISPOSABLE_ADMIN=1. The admin must be dedicated to a
//! disposable MySQL server: this test creates a unique account, grants it global
//! SELECT/TRIGGER, then removes only its own account and schemas. No raw SQLx
//! error, account, generated password, SQL or connection configuration is logged.
use datazen_driver_api::{namespace::NamespaceTarget, ConnectionConfig, DatabaseDriver};
use datazen_driver_mysql::MysqlDriver;
use sqlx::{mysql::MySqlConnectOptions, ConnectOptions, Connection, MySqlConnection, Row};

#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

type ProofResult<T = ()> = Result<T, &'static str>;

struct Objects {
    parent: String,
    incoming: String,
    user: String,
    password: String,
}
impl Objects {
    fn unique() -> Self {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        Self {
            parent: format!("dz_mig_txproof_{nonce}_p"),
            incoming: format!("dz_mig_txproof_{nonce}_f"),
            user: format!("dz_txproof_{}", &nonce[..16]),
            password: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
    fn account(&self) -> String {
        format!("'{}'@'%'", self.user)
    }
}
fn options(config: &ConnectionConfig) -> MySqlConnectOptions {
    let options = MySqlConnectOptions::new()
        .host(config.host.as_deref().unwrap_or(""))
        .port(config.port.unwrap_or(3306))
        .username(config.username.as_deref().unwrap_or(""))
        .password(config.password.as_deref().unwrap_or(""))
        .disable_statement_logging();
    if let Some(database) = &config.database {
        options.database(database)
    } else {
        options
    }
}
async fn statement(connection: &mut MySqlConnection, sql: &str, code: &'static str) -> ProofResult {
    sqlx::query(sql)
        .execute(connection)
        .await
        .map(|_| ())
        .map_err(|_| code)
}
fn exact_server_identity(version: &str, comment: &str) -> bool {
    matches!(version, "8.4.7" | "8.4.7-commercial")
        && matches!(
            comment,
            "MySQL Community Server - GPL" | "MySQL Enterprise Server - Commercial"
        )
}
async fn require_exact_server(admin: &mut MySqlConnection) -> ProofResult {
    let row = sqlx::query("SELECT VERSION(), @@version_comment")
        .fetch_one(admin)
        .await
        .map_err(|_| "SERVER_IDENTITY_UNAVAILABLE")?;
    let version: String = row.try_get(0).map_err(|_| "SERVER_IDENTITY_UNDECODABLE")?;
    let comment: String = row.try_get(1).map_err(|_| "SERVER_IDENTITY_UNDECODABLE")?;
    if !exact_server_identity(&version, &comment) {
        return Err("REQUIRES_ORACLE_MYSQL_8_4_7");
    }
    eprintln!("MYSQL_PROOF_SERVER=oracleMysql8.4.7");
    Ok(())
}
async fn trigger_count(connection: &mut MySqlConnection, objects: &Objects) -> ProofResult<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = ? AND EVENT_OBJECT_TABLE = 'parent'")
        .bind(&objects.parent).fetch_one(connection).await.map_err(|_| "TRIGGER_CATALOG_UNAVAILABLE")
}
async fn incoming_count(connection: &mut MySqlConnection, objects: &Objects) -> ProofResult<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.REFERENTIAL_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = ? AND UNIQUE_CONSTRAINT_SCHEMA = ? AND REFERENCED_TABLE_NAME = 'parent'")
        .bind(&objects.incoming).bind(&objects.parent).fetch_one(connection).await.map_err(|_| "INCOMING_FK_CATALOG_UNAVAILABLE")
}
async fn granted_global_count(
    connection: &mut MySqlConnection,
    objects: &Objects,
) -> ProofResult<i64> {
    // USER_PRIVILEGES reads current account ACLs; it is the intentionally
    // insufficient candidate witness under test, never an authorization input.
    sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.USER_PRIVILEGES WHERE GRANTEE = ? AND PRIVILEGE_TYPE IN ('SELECT','TRIGGER')")
        .bind(objects.account()).fetch_one(connection).await.map_err(|_| "CURRENT_GRANT_CATALOG_UNAVAILABLE")
}
fn equal(actual: i64, expected: i64, code: &'static str) -> ProofResult {
    if actual == expected {
        Ok(())
    } else {
        Err(code)
    }
}
async fn exercise(
    admin: &mut MySqlConnection,
    base: &MySqlConnectOptions,
    objects: &Objects,
    close_failures: &mut Vec<&'static str>,
) -> ProofResult {
    statement(
        admin,
        &format!("CREATE DATABASE `{}`", objects.parent),
        "PARENT_SCHEMA_CREATE_FAILED",
    )
    .await?;
    statement(
        admin,
        &format!("CREATE DATABASE `{}`", objects.incoming),
        "INCOMING_SCHEMA_CREATE_FAILED",
    )
    .await?;
    statement(
        admin,
        &format!(
            "CREATE TABLE `{}`.parent(id INT PRIMARY KEY) ENGINE=InnoDB",
            objects.parent
        ),
        "PARENT_TABLE_CREATE_FAILED",
    )
    .await?;
    statement(admin, &format!("CREATE TABLE `{}`.child(id INT PRIMARY KEY, parent_id INT, CONSTRAINT fk_parent FOREIGN KEY(parent_id) REFERENCES `{}`.parent(id)) ENGINE=InnoDB",objects.incoming,objects.parent), "INCOMING_FK_CREATE_FAILED").await?;
    statement(admin, &format!("CREATE TRIGGER `{}`.proof_trigger BEFORE INSERT ON `{}`.parent FOR EACH ROW SET NEW.id = NEW.id",objects.parent,objects.parent), "TRIGGER_CREATE_FAILED").await?;
    statement(
        admin,
        &format!(
            "CREATE USER {} IDENTIFIED BY '{}'",
            objects.account(),
            objects.password
        ),
        "ACCOUNT_CREATE_FAILED",
    )
    .await?;
    statement(
        admin,
        &format!(
            "GRANT SELECT ON `{}`.parent TO {}",
            objects.parent,
            objects.account()
        ),
        "INITIAL_TABLE_GRANT_FAILED",
    )
    .await?;
    let restricted = base
        .clone()
        .username(&objects.user)
        .password(&objects.password)
        .database(&objects.parent);
    let mut old = MySqlConnection::connect_with(&restricted)
        .await
        .map_err(|_| "OLD_SESSION_CONNECT_FAILED")?;
    let result = async {
        equal(
            trigger_count(admin, objects).await?,
            1,
            "ADMIN_TRIGGER_FIXTURE_NOT_VISIBLE",
        )?;
        equal(
            incoming_count(admin, objects).await?,
            1,
            "ADMIN_INCOMING_FIXTURE_NOT_VISIBLE",
        )?;
        equal(
            trigger_count(&mut old, objects).await?,
            0,
            "OLD_SESSION_UNEXPECTED_INITIAL_TRIGGER_VISIBILITY",
        )?;
        equal(
            incoming_count(&mut old, objects).await?,
            0,
            "OLD_SESSION_UNEXPECTED_INITIAL_FK_VISIBILITY",
        )?;
        statement(
            admin,
            &format!("GRANT SELECT, TRIGGER ON *.* TO {}", objects.account()),
            "GLOBAL_GRANT_FAILED",
        )
        .await?;
        // Do not USE/reconnect/SET ROLE on the old pinned session. A grant-table
        // snapshot that now reports both globals must not imply effective access.
        equal(
            granted_global_count(&mut old, objects).await?,
            2,
            "CURRENT_GRANTS_NOT_VISIBLE_AFTER_GRANT",
        )?;
        equal(
            trigger_count(&mut old, objects).await?,
            0,
            "OLD_SESSION_TRIGGER_CACHE_COUNTEREXAMPLE_NOT_REPRODUCED",
        )?;
        equal(
            incoming_count(&mut old, objects).await?,
            0,
            "OLD_SESSION_FK_CACHE_COUNTEREXAMPLE_NOT_REPRODUCED",
        )?;
        let mut fresh = MySqlConnection::connect_with(&restricted)
            .await
            .map_err(|_| "FRESH_SESSION_CONNECT_FAILED")?;
        let fresh_result = async {
            equal(
                trigger_count(&mut fresh, objects).await?,
                1,
                "FRESH_SESSION_TRIGGER_NOT_VISIBLE",
            )?;
            equal(
                incoming_count(&mut fresh, objects).await?,
                1,
                "FRESH_SESSION_INCOMING_FK_NOT_VISIBLE",
            )?;
            Ok(())
        }
        .await;
        let closed = fresh
            .close()
            .await
            .map_err(|_| "FRESH_SESSION_CLOSE_FAILED");
        if closed.is_err() {
            close_failures.push("FRESH_SESSION_CLOSE_FAILED");
        }
        fresh_result.and(closed)?;
        // Actual server facts still do not opt a driver into transactionBlock.
        let driver = MysqlDriver::new(false);
        let plan = driver
            .workflow_transaction_plan(
                &["UPDATE parent SET id = 1".into()],
                &NamespaceTarget::empty().with_database(&objects.parent),
            )
            .map_err(|_| "DRIVER_PROOF_GATE_FAILED")?;
        if plan.is_some() {
            return Err("TRANSACTION_BLOCK_UNEXPECTEDLY_OPTED_IN");
        }
        eprintln!("MYSQL_PROOF_COUNTEREXAMPLE=currentGrantsDoNotProveOldSessionCatalogVisibility");
        Ok(())
    }
    .await;
    let closed = old.close().await.map_err(|_| "OLD_SESSION_CLOSE_FAILED");
    if closed.is_err() {
        close_failures.push("OLD_SESSION_CLOSE_FAILED");
    }
    result.and(closed)
}
async fn cleanup(admin: &mut MySqlConnection, objects: &Objects) -> Vec<&'static str> {
    let mut failures = Vec::new();
    // Always attempt all owned names, including ambiguous CREATE acknowledgments.
    // Drop incoming before parent. IF EXISTS makes repeated cleanup safe.
    for (sql, code) in [
        (
            format!("DROP USER IF EXISTS {}", objects.account()),
            "ACCOUNT_CLEANUP_FAILED",
        ),
        (
            format!("DROP DATABASE IF EXISTS `{}`", objects.incoming),
            "INCOMING_SCHEMA_CLEANUP_FAILED",
        ),
        (
            format!("DROP DATABASE IF EXISTS `{}`", objects.parent),
            "PARENT_SCHEMA_CLEANUP_FAILED",
        ),
    ] {
        if statement(admin, &sql, code).await.is_err() {
            failures.push(code);
        }
    }
    failures
}
#[tokio::test]
async fn mysql_8_4_7_old_session_visibility_is_not_proved_by_current_global_grants() {
    const DIMENSION: &str = "workflow-transaction-session-effective-visibility";
    if std::env::var("DATAZEN_MYSQL_PROOF_DISPOSABLE_ADMIN").as_deref() != Ok("1") {
        migration_gate::unverified("mysql",DIMENSION,"explicit DATAZEN_MYSQL_PROOF_DISPOSABLE_ADMIN=1 required; dedicated disposable server/admin account only");
        return;
    }
    let Some(config) = migration_gate::require_config("mysql", DIMENSION, "mysql", "tx-proof", &[])
    else {
        return;
    };
    let base = options(&config);
    let mut admin = match MySqlConnection::connect_with(&base).await {
        Ok(connection) => connection,
        Err(_) => {
            migration_gate::unverified("mysql", DIMENSION, "ADMIN_CONNECT_FAILED");
            return;
        }
    };
    // Version/family refusal occurs before any account/schema mutation.
    if let Err(code) = require_exact_server(&mut admin).await {
        if admin.close().await.is_err() {
            panic!("MYSQL_PROOF_PRIMARY={code}; cleanup_failed=ADMIN_PRECONDITION_CLOSE_FAILED");
        }
        migration_gate::unverified("mysql", DIMENSION, code);
        return;
    }
    let objects = Objects::unique();
    let mut cleanup_failures = Vec::new();
    let primary = exercise(&mut admin, &base, &objects, &mut cleanup_failures).await;
    cleanup_failures.extend(cleanup(&mut admin, &objects).await);
    let admin_close_failed = admin.close().await.is_err();
    if let Err(code) = primary {
        panic!("MYSQL_PROOF_PRIMARY={code}; cleanup_failed={}; admin_close_failed={admin_close_failed}",cleanup_failures.join(","));
    }
    assert!(
        cleanup_failures.is_empty() && !admin_close_failed,
        "MYSQL_PROOF_CLEANUP_FAILED={}; admin_close_failed={admin_close_failed}",
        cleanup_failures.join(",")
    );
}

#[test]
fn proof_identity_gate_rejects_other_versions_forks_and_mariadb() {
    assert!(exact_server_identity(
        "8.4.7",
        "MySQL Community Server - GPL"
    ));
    assert!(exact_server_identity(
        "8.4.7-commercial",
        "MySQL Enterprise Server - Commercial"
    ));
    for (version, comment) in [
        ("8.4.6", "MySQL Community Server - GPL"),
        ("8.4.8", "MySQL Community Server - GPL"),
        ("8.4.7-custom", "MySQL Community Server - GPL"),
        ("8.4.7", "Percona Server"),
        ("11.8.4-MariaDB", "MariaDB Server"),
        ("8.4.7", "unknown"),
    ] {
        assert!(!exact_server_identity(version, comment));
    }
}

#[test]
fn proof_object_names_are_unique_and_safe_generated_identifiers() {
    let a = Objects::unique();
    let b = Objects::unique();
    assert_ne!(a.parent, b.parent);
    assert_ne!(a.user, b.user);
    assert_ne!(a.parent, a.incoming);
    for name in [&a.parent, &a.incoming, &a.user] {
        assert!(name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
    }
    assert!(a.user.len() <= 32 && a.parent.len() <= 64 && a.incoming.len() <= 64);
}
