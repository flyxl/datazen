//! W8 刀3e-1：result-origin 契约的 sqlite 接线测试（进程内真库）。
//!
//! SQLite 嵌入在本进程里，所以这里可以钉**整条链**的终态裁决：共享读句
//! 分析语句、`sqlite_master` 目录核对关系种类。pg/mysql 无活库服务的
//! 等价接线断言（死池 ⇒ kind Unknown 而非 NotImplemented）在各自 crate
//! 的 `result_origin_tests` 里；共享读句自身的形状/旗标/列溯源全量行为、
//! 以及消费侧 `evaluate_result_editability` 的 verdict 推导，在
//! driver-api `result_origin::tests` 与 datazen `table_editing` 单家
//! 覆盖，这里不复制。

use datazen_driver_api::result_origin::{
    ResultObjectKind, ResultOrigin, ResultShape, UndeterminableReason,
};
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver, SqlTarget, SslMode};

use crate::SqliteDriver;

fn memory_config() -> ConnectionConfig {
    ConnectionConfig {
        id: "sqlite-origin-test".into(),
        name: "origin".into(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some(":memory:".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
        connection_timeout: 5,
        max_pool_size: 1,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

fn columns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn detail(origin: &ResultOrigin) -> &datazen_driver_api::result_origin::ResultOriginDetail {
    match origin {
        ResultOrigin::Determined(detail) => detail,
        ResultOrigin::Undeterminable(reason) => {
            panic!("expected a determined origin, got {reason:?}")
        }
    }
}

/// A real base table: determined, single-base-table shape, passthrough
/// columns — exactly the facts `evaluate_result_editability` needs to flip
/// a grid cell writable. This is the whole point of the write-back
/// contract: the verdict stops being "the driver cannot tell".
#[tokio::test]
async fn a_real_base_table_answers_a_determined_origin() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&memory_config()).await.expect("connect");
    driver
        .execute(
            &handle,
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, name TEXT, note TEXT)",
        )
        .await
        .expect("create table");

    let origin = driver
        .describe_result_origin(
            &handle,
            SqlTarget::new(None, None),
            &columns(&["id", "name"]),
            "SELECT id, name FROM orders",
        )
        .await
        .expect("the contract answers, it does not fail");
    let detail = detail(&origin);
    assert_eq!(detail.object_kind, ResultObjectKind::BaseTable);
    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
    assert_eq!(
        detail.table.as_ref().expect("the relation is named").name,
        "orders"
    );
    assert_eq!(detail.columns.len(), 2);
    assert!(detail.columns.iter().all(|column| column.passthrough));

    driver.disconnect(handle).await.expect("disconnect");
}

/// A view is named by the statement and confirmed by the catalog: the kind is
/// `View` — the consumer-side verdict stays read-only with the view's own
/// reason, never "unknown origin".
#[tokio::test]
async fn a_view_answers_read_only_with_the_view_reason() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&memory_config()).await.expect("connect");
    driver
        .execute(
            &handle,
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, name TEXT)",
        )
        .await
        .expect("create table");
    driver
        .execute(
            &handle,
            "CREATE VIEW v_orders AS SELECT id, name FROM orders",
        )
        .await
        .expect("create view");

    let origin = driver
        .describe_result_origin(
            &handle,
            SqlTarget::new(None, None),
            &columns(&["id", "name"]),
            "SELECT id, name FROM v_orders",
        )
        .await
        .expect("the contract answers, it does not fail");
    assert_eq!(detail(&origin).object_kind, ResultObjectKind::View);

    driver.disconnect(handle).await.expect("disconnect");
}

/// A dead handle is a catalog that cannot answer: the kind degrades to
/// `Unknown` (read-only), while the statement half is still read — the driver
/// opted in, so the reason is never `NotImplemented`.
#[tokio::test]
async fn a_dead_handle_leaves_the_kind_unknown_not_unimplemented() {
    let driver = SqliteDriver::new();
    let dead = ConnectionHandle {
        id: "no-such-pool".to_string(),
        pool_id: "no-such-pool".to_string(),
    };
    let origin = driver
        .describe_result_origin(
            &dead,
            SqlTarget::new(None, None),
            &columns(&["id"]),
            "SELECT id FROM orders",
        )
        .await
        .expect("the contract answers, it does not fail");
    match &origin {
        ResultOrigin::Determined(detail) => {
            assert_eq!(detail.shape, ResultShape::SingleBaseTable);
            assert_eq!(detail.object_kind, ResultObjectKind::Unknown);
        }
        ResultOrigin::Undeterminable(reason) => {
            panic!("a dead pool is 'metadata unavailable', not {reason:?}")
        }
    }
}

/// A statement the reader cannot classify stays honestly undeterminable with
/// the dialect reason — opting in never invents an origin.
#[tokio::test]
async fn an_unreadable_statement_stays_undeterminable() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&memory_config()).await.expect("connect");
    let origin = driver
        .describe_result_origin(
            &handle,
            SqlTarget::new(None, None),
            &columns(&[]),
            "PRAGMA table_info(orders)",
        )
        .await
        .expect("the contract answers, it does not fail");
    assert_eq!(
        origin,
        ResultOrigin::Undeterminable(UndeterminableReason::UnsupportedDialect)
    );
    driver.disconnect(handle).await.expect("disconnect");
}
