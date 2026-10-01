//! CM-04 基线证据 —— 见 `super` 里的说明块。
//!
//! 单独成文件是为了把 `tests.rs` 压在单文件 800 行上限内（拆分方式对齐
//! `schema_diff/unified_tests.rs` 的既有先例）。

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// ===========================================================================
// CM-04 baseline — 配置 ID 不得回退成会话 ID。
//
// Spec: `docs/architecture/platform/connection-management.md:852`（CM-04，H/W）
//   前置：存在 profileId，不存在同字符串 session。
//   步骤：向 executeInSession 传 profileId；向 executeAtTarget 传未知 profile。
//   断言：分别 SessionNotFound、不可见配置错误；driver execute 次数为 0。
//
// 落地形态：`executeInSession` / `executeAtTarget` 今天在本仓尚不存在（全仓 0
// 命中），所以基线落在它们今天共用的宿主收口：
//   * 会话侧 —— `ConnectionManager::get_session`。任何会话内执行都要先过它；
//     未命中 `connections` 时转 `reconnect`，而 `reconnect` 的第一步就是查
//     `session_owner_map`（sessions.rs:105-111），失败即 `DbSessionNotFound`，
//     早于 `store.get_connection`，更早于任何驱动调用。
//   * 配置侧 —— `get_or_connect_session` / `resolve_session_for_connection`。
//     未落盘的 profileId 在 `establish_connection` 的 `store.get_connection`
//     处失败（connections.rs:88-92），同样早于 `driver.connect`。
//
// spec 的"driver execute 次数为 0"由 `CountingDriver` 落地，并连 connect /
// query / get_server_info 一起断言为 0：只盯 execute 会漏掉"没跑语句却发生了
// 隐式重连"的回归（`get_session` 未命中就 `driver.connect` 的重连路径正是这种）。
// ===========================================================================

/// `MockDriver` 的计数外壳。
///
/// `crate::testing::mock_driver` 不是 `#[cfg(test)]` 模块（会编进生产二进制），
/// 改它就是改生产代码；而它也只有 `query_calls()` / `execute_calls()`，没有
/// 连接类计数。故照本文件里 `StubDriver` 的先例，在测试内包一层，把驱动被
/// 触碰的每一种方式都记下来。
struct CountingDriver {
    inner: Arc<MockDriver>,
    connect_calls: AtomicUsize,
    disconnect_calls: AtomicUsize,
    execute_calls: AtomicUsize,
    query_calls: AtomicUsize,
    server_info_calls: AtomicUsize,
    connected_config_ids: Mutex<Vec<String>>,
}

impl CountingDriver {
    fn new(inner: Arc<MockDriver>) -> Self {
        Self {
            inner,
            connect_calls: AtomicUsize::new(0),
            disconnect_calls: AtomicUsize::new(0),
            execute_calls: AtomicUsize::new(0),
            query_calls: AtomicUsize::new(0),
            server_info_calls: AtomicUsize::new(0),
            connected_config_ids: Mutex::new(Vec::new()),
        }
    }

    fn connect_calls(&self) -> usize {
        self.connect_calls.load(Ordering::SeqCst)
    }

    fn disconnect_calls(&self) -> usize {
        self.disconnect_calls.load(Ordering::SeqCst)
    }

    fn execute_calls(&self) -> usize {
        self.execute_calls.load(Ordering::SeqCst)
    }

    fn query_calls(&self) -> usize {
        self.query_calls.load(Ordering::SeqCst)
    }

    fn server_info_calls(&self) -> usize {
        self.server_info_calls.load(Ordering::SeqCst)
    }

    fn connected_config_ids(&self) -> Vec<String> {
        self.connected_config_ids.lock().unwrap().clone()
    }

    /// CM-04 的"driver execute 次数为 0"。id 解析被拒后驱动一次都不许被碰到
    /// ——包括没有被显式断言的那些入口。
    fn assert_driver_untouched(&self, context: &str) {
        assert_eq!(
            self.execute_calls(),
            0,
            "CM-04: {context} — driver execute 次数必须为 0"
        );
        assert_eq!(
            self.query_calls(),
            0,
            "CM-04: {context} — driver query 次数必须为 0"
        );
        assert_eq!(
            self.connect_calls(),
            0,
            "CM-04: {context} — 不得发生隐式重连（driver connect 次数必须为 0）"
        );
        assert_eq!(
            self.disconnect_calls(),
            0,
            "CM-04: {context} — driver disconnect 次数必须为 0"
        );
        assert_eq!(
            self.server_info_calls(),
            0,
            "CM-04: {context} — driver get_server_info 次数必须为 0"
        );
        assert!(
            self.connected_config_ids().is_empty(),
            "CM-04: {context} — 不得对任何配置建连，实际连了 {:?}",
            self.connected_config_ids()
        );
    }
}

#[async_trait]
impl DatabaseDriver for CountingDriver {
    fn driver_type(&self) -> DatabaseType {
        self.inner.driver_type()
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.connect_calls.fetch_add(1, Ordering::SeqCst);
        self.connected_config_ids
            .lock()
            .unwrap()
            .push(config.id.clone());
        self.inner.connect(config).await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        self.inner.test_connection(config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.disconnect_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<crate::db::TableInfo>, DriverError> {
        self.inner.get_tables(handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<crate::db::TableSchema, DriverError> {
        self.inner
            .get_table_schema(handle, table, database, schema)
            .await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<crate::db::ColumnSchema>, Vec<String>), DriverError> {
        self.inner
            .get_columns(handle, table, database, schema)
            .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<crate::db::QueryResult, DriverError> {
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<crate::db::MultiQueryResult, DriverError> {
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.query_multi(handle, sql, limit).await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[crate::db::Value],
    ) -> Result<crate::db::QueryResult, DriverError> {
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.query_with_params(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.execute(handle, sql).await
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        self.server_info_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.get_server_info(handle).await
    }
}

/// 与 `test_manager()` 同构，只是把注册的驱动换成 `CountingDriver`。
/// 额外返回内层 `MockDriver`，便于用既有计数器做交叉印证。
async fn test_manager_counting() -> (
    crate::testing::FileKeyringGuard,
    Arc<ConnectionManager>,
    Arc<Store>,
    Arc<MockDriver>,
    Arc<CountingDriver>,
) {
    let keyring = crate::testing::FileKeyringGuard::set();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
    let registry = Arc::new(DriverRegistry::new());
    let mock = MockDriver::new(
        "postgres",
        MockDriverOptions {
            server_version: "PostgreSQL 16".into(),
            ..Default::default()
        },
    );
    let counting = Arc::new(CountingDriver::new(mock.clone()));
    registry
        .register_test_driver("postgres", counting.clone())
        .await;
    let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
    (keyring, mgr, store, mock, counting)
}

/// CM-04 前置 + 第一步：存在 profileId、不存在同字符串 session 时，向会话侧
/// 传 profileId，必须报 SessionNotFound，且不得回退成"配置找不到"。
#[tokio::test]
async fn cm04_profile_id_as_db_session_id_is_db_session_not_found() {
    let (_keyring, mgr, store, mock, counting) = test_manager_counting().await;

    // 前置：profileId 已落盘；没有为它建立过任何会话。
    store
        .save_connection(sample_config("profile-cfg"))
        .await
        .unwrap();
    assert!(
        mgr.list_sessions().await.is_empty(),
        "前置：不应存在任何会话"
    );

    let err = mgr
        .get_session("profile-cfg")
        .await
        .err()
        .expect("CM-04: 把 profileId 当 dbSessionId 用必须失败，而不是凭空建出会话");

    match &err {
        ConnectionError::DbSessionNotFound(id) => {
            assert_eq!(id, "profile-cfg", "CM-04: 报错必须指名传入的会话 id")
        }
        other => panic!("CM-04: 期望 ConnectionError::DbSessionNotFound, got {other:?}"),
    }

    // 关键判据：不能报"配置不可见"。配置明明就在 store 里，报 ConnectionConfig-
    // NotFound 会把"传错了 id 种类"伪装成"配置不存在"，让调用方去建一份配置，
    // 正好绕开 CM-04 想禁掉的那条回退。
    assert!(
        !matches!(err, ConnectionError::ConnectionConfigNotFound(_)),
        "CM-04: 不得把 connectionId 回退解释成配置缺失, got {err:?}"
    );
    assert!(
        err.to_string()
            .contains("a dbSessionId is a runtime session id"),
        "CM-04: 错误信息要能区分 id 种类, got: {err}"
    );

    // driver execute 次数为 0；且不得顺带把 profileId 重连出来。
    counting.assert_driver_untouched("get_session(profileId)");
    assert_eq!(mock.execute_calls(), 0);
    assert_eq!(mock.query_calls(), 0);

    assert!(mgr.list_sessions().await.is_empty(), "被拒后不得留下会话");
    assert_eq!(mgr.session_owner_map_len().await, 0);
    assert!(!mgr.ping("profile-cfg").await);
}

/// CM-04 第二步：向配置侧传未知 profile，必须报配置不可见，且驱动一次不被触碰。
#[tokio::test]
async fn cm04_unknown_profile_id_is_connection_config_not_found() {
    let (_keyring, mgr, store, mock, counting) = test_manager_counting().await;
    store
        .save_connection(sample_config("cfg-known"))
        .await
        .unwrap();

    let err = mgr
        .get_or_connect_session("profile-unknown")
        .await
        .err()
        .expect("CM-04: 未知 profile 必须失败");
    match &err {
        ConnectionError::ConnectionConfigNotFound(id) => assert_eq!(id, "profile-unknown"),
        other => panic!("CM-04: 期望 ConnectionError::ConnectionConfigNotFound, got {other:?}"),
    }
    assert!(
        !matches!(err, ConnectionError::DbSessionNotFound(_)),
        "CM-04: 配置侧不得报会话缺失, got {err:?}"
    );

    // 同一入口的第二形态（同时拿到 driver 与 handle）。
    let err2 = mgr
        .resolve_session_for_connection("profile-unknown")
        .await
        .err()
        .expect("CM-04: 未知 profile 必须失败");
    assert!(
        matches!(err2, ConnectionError::ConnectionConfigNotFound(_)),
        "CM-04: 期望 ConnectionError::ConnectionConfigNotFound, got {err2:?}"
    );

    counting.assert_driver_untouched("get_or_connect_session(未知 profile)");
    assert_eq!(mock.execute_calls(), 0);
    assert_eq!(mgr.session_owner_map_len().await, 0);
}

/// CM-04 主体：两种 id 双向都不可互换，且互换失败时不会顺手执行语句。
#[tokio::test]
async fn cm04_profile_id_and_db_session_id_are_not_interchangeable() {
    let (_keyring, mgr, store, mock, counting) = test_manager_counting().await;
    store
        .save_connection(sample_config("cfg-cm04"))
        .await
        .unwrap();

    // 配置侧只认 connectionId：合法调用拿到的是驱动给的运行时 id。
    let db_session_id = match mgr.resolve_session_for_connection("cfg-cm04").await {
        Ok((db_session_id, _driver, handle)) => {
            assert_eq!(handle.id, db_session_id);
            db_session_id
        }
        Err(e) => panic!("CM-04: 合法调用不应失败, got {e:?}"),
    };
    assert_ne!(
        db_session_id, "cfg-cm04",
        "CM-04: dbSessionId 不得与 connectionId 同串"
    );

    // 会话侧只认 dbSessionId：把 connectionId 当会话 id 传，必须失败。
    let err = mgr
        .get_session("cfg-cm04")
        .await
        .err()
        .expect("CM-04: connectionId 回退为 dbSessionId 必须失败");
    assert!(
        matches!(err, ConnectionError::DbSessionNotFound(_)),
        "CM-04: 期望 ConnectionError::DbSessionNotFound, got {err:?}"
    );

    // 反向：把真正的 dbSessionId 当 connectionId 传，必须报配置不可见。
    let err2 = mgr
        .get_or_connect_session(&db_session_id)
        .await
        .err()
        .expect("CM-04: dbSessionId 回退为 connectionId 必须失败");
    assert!(
        matches!(err2, ConnectionError::ConnectionConfigNotFound(_)),
        "CM-04: 期望 ConnectionError::ConnectionConfigNotFound, got {err2:?}"
    );

    // 整段旅程里只发生过那一次合法建连（`connect_with_config` 会补一次
    // `get_server_info` 来记 server_version）；两次误用都没连、没执行。
    assert_eq!(counting.connect_calls(), 1);
    assert_eq!(counting.server_info_calls(), 1);
    assert_eq!(counting.execute_calls(), 0);
    assert_eq!(counting.query_calls(), 0);
    assert_eq!(mock.execute_calls(), 0);
    assert_eq!(mock.query_calls(), 0);
    assert_eq!(
        counting.connected_config_ids(),
        vec!["cfg-cm04".to_string()]
    );
}

/// CM-04 的扫尾断言：会话侧的所有 id 入口对 profileId / 未知 id 给出一致的
/// 拒绝，且没有任何一条会绕过检查碰到驱动。
#[tokio::test]
async fn cm04_rejected_ids_never_reach_the_driver_from_any_session_entry() {
    let (_keyring, mgr, store, mock, counting) = test_manager_counting().await;
    store
        .save_connection(sample_config("profile-cfg"))
        .await
        .unwrap();

    for id in ["profile-cfg", "profile-typo"] {
        for (entry, err) in [
            ("get_session", mgr.get_session(id).await.err()),
            ("get_session_config", mgr.get_session_config(id).await.err()),
            ("get_server_info", mgr.get_server_info(id).await.err()),
        ] {
            let err =
                err.unwrap_or_else(|| panic!("CM-04: {entry}({id:?}) 必须拒绝，而不是静默建连"));
            assert!(
                matches!(err, ConnectionError::DbSessionNotFound(_)),
                "CM-04: {entry}({id:?}) 期望 DbSessionNotFound, got {err:?}"
            );
        }
        assert!(!mgr.ping(id).await, "CM-04: ping({id:?}) 必须为 false");
    }

    // 关停类接口同样不得触发隐式重连。
    for id in ["profile-cfg", "profile-typo"] {
        mgr.disconnect(id).await.unwrap();
        mgr.release(id).await.unwrap();
    }

    assert!(mgr.list_sessions().await.is_empty());
    assert_eq!(mgr.session_owner_map_len().await, 0);
    counting.assert_driver_untouched("全部会话侧入口");
    assert_eq!(mock.execute_calls(), 0);
    assert_eq!(mock.query_calls(), 0);
}
