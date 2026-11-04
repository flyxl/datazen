//! Live coverage for **CM-45 — session mode lifecycle** against a real Azure
//! SQL Database instance.
//!
//! The case is stated in `docs/architecture/platform/connection-management.md`:
//!
//! - 前置：需要 identity/约束会话模式的 driver fixture
//! - 步骤：开启、写入、关闭；再注入关闭失败
//! - 断言：整个流程同资源；失败资源销毁；下一 consumer 不继承模式
//!
//! SQL Server is the driver that actually owns a session-scoped mode. It is the
//! only crate implementing `set_identity_insert`,
//! `explicit_identity_insert_requires_session_toggle` and `discard_connection`
//! — mysql overrides 0 of them, and `IDENTITY_INSERT` is a property of the
//! physical session rather than of the connection object, so the lifecycle is
//! only observable against a real server.
//!
//! **On the "inject a close failure" step.** There is no API that fails a
//! close: the mode cannot be returned to general use unless it is turned off
//! first, so the contract's response to an unusable resource is that it be
//! *destroyed* rather than reset. `discard_connection` is that path, and it is
//! what this file drives — see the step marked "失败资源销毁".
//!
//! Harness contract matches `live_write_and_ddl`: `live_config()` and
//! `write_allowed()` **skip** rather than fail. That makes this file
//! `vacuous-without-live-server` by construction — it enforces nothing when no
//! instance is configured. The non-vacuous half of CM-45 is the fact that this
//! file and this test exist at all: before it, CM-45 appeared **nowhere** in
//! the suite and nowhere in `REQUIRED_DIMENSIONS`, so the P2 exit gate's CM-45
//! clause passed by having no test to run.

mod common;

use common::{cell_i64, connect, drop_quietly, live_config, scalar, write_allowed};
use datazen_driver_api::DatabaseDriver as _;

/// `[name]` — T-SQL escapes `]` as `]]`.
fn qi(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// `[schema].[name]`.
fn qn(schema: &str, name: &str) -> String {
    format!("{}.{}", qi(schema), qi(name))
}

/// The scratch table this file uses unless it says otherwise.
fn dbo(name: &str) -> String {
    qn("dbo", name)
}

async fn count(
    driver: &datazen_driver_sqlserver::SqlServerDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    from_and_where: &str,
) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {from_and_where}");
    let value = scalar(driver, handle, &sql)
        .await
        .unwrap_or_else(|e| panic!("{sql} failed: {e}"));
    cell_i64(&value).unwrap_or_else(|| panic!("{sql} did not decode to an integer: {value:?}"))
}

/// An explicit value must be refused while `IDENTITY_INSERT` is OFF. This is
/// the observable form of "this session is not in the mode", used both as the
/// baseline on the first resource and as the inheritance check on the next one.
async fn assert_explicit_insert_refused(
    driver: &datazen_driver_sqlserver::SqlServerDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    table: &str,
    id: i32,
    value: i32,
) {
    let sql = format!("INSERT INTO {table} ([id], [v]) VALUES ({id}, {value})");
    let rejected = driver
        .execute(handle, &sql)
        .await
        .expect_err("an explicit identity value must be rejected while IDENTITY_INSERT is OFF");
    let message = format!("{rejected:?}");
    assert!(
        message.contains("544") || message.contains("IDENTITY_INSERT"),
        "expected error 544 / IDENTITY_INSERT, got: {rejected:?}"
    );
}

/// CM-45 session mode lifecycle: one resource for the whole flow, the failed
/// resource destroyed, and the next consumer not inheriting the mode.
#[tokio::test]
async fn cm45_session_mode_does_not_outlive_its_resource() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "CM-45 session mode lifecycle") {
        return;
    }

    // 前置 — the fixture must actually be a session-mode driver, or the rest of
    // the case is meaningless.
    let (driver, first) = connect(&cfg).await;
    assert!(
        driver.explicit_identity_insert_requires_session_toggle(),
        "CM-45 needs a driver whose identity insert requires a session toggle; \
         SQL Server is the only crate that declares it"
    );

    let table = cfg.scratch("cm45");
    let t = dbo(&table);
    let qualified = format!("{}.{}", qi(&cfg.schema), qi(&table));

    driver
        .execute(
            &first,
            &format!(
                "CREATE TABLE {t} ([id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, [v] INT NOT NULL)"
            ),
        )
        .await
        .expect("CREATE TABLE with an identity column");

    // Baseline on the first resource: the mode is off before anything turns it
    // on, which is what makes the later inheritance check meaningful.
    assert_explicit_insert_refused(&driver, &first, &t, 5, 1).await;

    // 开启 + 写入, both on the same resource.
    driver
        .set_identity_insert(&first, &cfg.database, Some(&cfg.schema), &table, true)
        .await
        .expect("SET IDENTITY_INSERT ON");
    driver
        .execute(
            &first,
            &format!("INSERT INTO {t} ([id], [v]) VALUES (5, 1)"),
        )
        .await
        .expect("an explicit identity value must be accepted while IDENTITY_INSERT is ON");
    assert_eq!(
        count(&driver, &first, &t).await,
        1,
        "the write must land on the resource that carries the mode"
    );

    // 关闭 on the same resource: the mode is gone from *this* session again.
    driver
        .set_identity_insert(&first, &cfg.database, Some(&cfg.schema), &table, false)
        .await
        .expect("SET IDENTITY_INSERT OFF");
    assert_explicit_insert_refused(&driver, &first, &t, 6, 2).await;

    // 失败资源销毁 — the mode-carrying resource is dropped rather than reset.
    driver
        .discard_connection(&first)
        .await
        .expect("discard_connection on the resource that carried the mode");

    // 下一 consumer 不继承模式 — a brand-new resource must come up clean.
    let (next_driver, next) = connect(&cfg).await;
    assert!(
        next.pool_id != first.pool_id,
        "the next consumer must be a different resource, or this proves nothing"
    );
    assert_explicit_insert_refused(&next_driver, &next, &t, 7, 3).await;
    assert_eq!(
        count(&next_driver, &next, &t).await,
        1,
        "only the one row written under the mode may exist"
    );

    drop_quietly(
        &next_driver,
        &next,
        &format!("DROP TABLE IF EXISTS {qualified}"),
    )
    .await;
    let _ = next_driver.disconnect(next).await;
}
