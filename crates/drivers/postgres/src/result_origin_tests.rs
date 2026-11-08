//! W8 刀3e-1：result-origin 契约的 postgres 接线测试（无活库）。
//!
//! CI 没有 postgres 服务（ci.yml 明确登记），所以这里钉的是**接线事实**
//! 而不是目录终态：驱动已把 `describe_result_origin` 从默认的
//! 「NotImplemented」改为共享读句 + 自家目录。句柄指向不存在的池 ⇒
//! `get_tables` 失败 ⇒ `resolve_relation_kind` 收敛为 `Unknown`——
//! 「目录答不出」按只读处理，正是契约规定的诚实失败，且理由永远不再是
//! 「驱动没实现」。共享读句自身的形状/旗标/列溯源全量行为在 driver-api
//! `result_origin::tests` 单家覆盖；目录核对的终态裁决（BaseTable/View）
//! 由 sqlite 的进程内真库测试 + driver-api Mock 测试覆盖，这里不复制。

use datazen_driver_api::result_origin::{
    ResultObjectKind, ResultOrigin, ResultShape, UndeterminableReason,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, SqlTarget};

use crate::PostgresDriver;

fn dead_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "no-such-pool".to_string(),
        pool_id: "no-such-pool".to_string(),
    }
}

fn columns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// A plain single-table select is read by the shared reader; the catalog half
/// is honestly `Unknown` because the pool does not exist. The default
/// implementation would have answered `Undeterminable(NotImplemented)` —
/// that answer is gone.
#[tokio::test]
async fn a_plain_select_is_determined_with_an_unknown_kind_when_the_catalog_cannot_answer() {
    let driver = PostgresDriver::new();
    let origin = driver
        .describe_result_origin(
            &dead_handle(),
            SqlTarget::new(Some("app"), None),
            &columns(&["id", "name"]),
            "SELECT id, name FROM orders",
        )
        .await
        .expect("the contract answers, it does not fail");

    let detail = match &origin {
        ResultOrigin::Determined(detail) => detail,
        ResultOrigin::Undeterminable(reason) => {
            panic!("the driver opted in; a dead pool is not 'not implemented' ({reason:?})")
        }
    };
    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
    assert_eq!(
        detail.table.as_ref().expect("the relation is named").name,
        "orders"
    );
    assert_eq!(detail.columns.len(), 2);
    assert!(detail.columns.iter().all(|column| column.passthrough));
    // The catalog half degrades honestly — never a guess, never a refusal.
    assert_eq!(detail.object_kind, ResultObjectKind::Unknown);
}

/// This dialect's quoted spellings (double quotes, schema-qualified names)
/// stay readable: the shared reader's GenericDialect accepts them, which is
/// what makes the one-line delegation enough for this driver.
#[tokio::test]
async fn this_dialects_quoted_spellings_are_still_readable() {
    let driver = PostgresDriver::new();
    let origin = driver
        .describe_result_origin(
            &dead_handle(),
            SqlTarget::new(Some("app"), Some("public")),
            &columns(&["id"]),
            r#"SELECT "id" FROM "public"."orders""#,
        )
        .await
        .expect("the contract answers, it does not fail");
    let detail = match &origin {
        ResultOrigin::Determined(detail) => detail,
        ResultOrigin::Undeterminable(reason) => {
            panic!("double quotes are this dialect's spelling ({reason:?})")
        }
    };
    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
    let table = detail.table.as_ref().expect("the relation is named");
    assert_eq!(table.name, "orders");
    assert_eq!(table.schema.as_deref(), Some("public"));
}

/// A statement the reader cannot classify stays honestly undeterminable with
/// the dialect reason — opting in never invents an origin.
#[tokio::test]
async fn a_statement_the_reader_cannot_classify_stays_undeterminable() {
    let driver = PostgresDriver::new();
    let origin = driver
        .describe_result_origin(
            &dead_handle(),
            SqlTarget::new(Some("app"), None),
            &columns(&[]),
            "DELETE FROM orders",
        )
        .await
        .expect("the contract answers, it does not fail");
    assert_eq!(
        origin,
        ResultOrigin::Undeterminable(UndeterminableReason::UnsupportedDialect)
    );
}
