//! `runtime_reads` 的行为钉子：文案、访问级别与只读强制。
//!
//! 桌面集成旅程（wapp 测试、MCP golden 契约）继续在宿主 crate 覆盖
//! 真实 Runtime 路径；此处只钉共享层自己的新增语义。

use super::*;
use crate::workflow::application::resource_execution::PreparedExecution;
use std::sync::atomic::{AtomicBool, Ordering};

fn config(read_only: bool) -> ConnectionConfig {
    ConnectionConfig {
        id: "c1".to_owned(),
        name: "fixture".to_owned(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some("app".to_owned()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 30,
        max_pool_size: 10,
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
        read_only,
        pinned: false,
    }
}

/// 假宿主：只回答 profile / driver / safe_mode。
///
/// `prepare` 记录到达事实并报错——所有断言都发生更早，`prepare` 被调
/// 到本身就是回归（访问级别或只读守卫被绕过）。
struct FakeHost {
    config: ConnectionConfig,
    enabled: bool,
    safe_mode: bool,
    prepare_reached: AtomicBool,
}

impl FakeHost {
    fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            enabled: true,
            safe_mode: false,
            prepare_reached: AtomicBool::new(false),
        }
    }

    fn mock_driver() -> Arc<dyn DatabaseDriver> {
        datazen_driver_api::mock_driver::MockDriver::new(
            "sqlite",
            datazen_driver_api::mock_driver::MockDriverOptions::default(),
        )
    }
}

#[async_trait::async_trait]
impl ResourceExecutionHost for FakeHost {
    async fn profile(&self, _id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        Ok((self.config.clone(), 7, self.enabled))
    }

    async fn driver(
        &self,
        _config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        Ok(Self::mock_driver())
    }

    async fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    async fn prepare(
        &self,
        _connection_id: &str,
        _revision: u64,
        _command: &str,
        _target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        self.prepare_reached.store(true, Ordering::SeqCst);
        Err(WorkflowError::Validation(
            "prepare must not be reached by these reads".into(),
        ))
    }
}

#[tokio::test]
async fn a_command_above_read_access_is_refused_with_form_neutral_wording() {
    let host = FakeHost::new(config(false));
    let error = read_target_command(&host, "c1", "execute", serde_json::json!({ "sql": "SELECT 1" }))
        .await
        .expect_err("a write command must be refused before dispatch");
    assert_eq!(error, READ_ONLY_REFUSAL);
    assert!(
        !host.prepare_reached.load(Ordering::SeqCst),
        "the refusal must happen before any resource is prepared"
    );
}

#[tokio::test]
async fn an_unknown_command_is_reported_verbatim() {
    let host = FakeHost::new(config(false));
    let error = read_target_command(&host, "c1", "no_such_command", serde_json::json!({}))
        .await
        .expect_err("an unknown command must be reported");
    assert_eq!(error, "Unsupported command: no_such_command");
}

#[tokio::test]
async fn a_disabled_profile_is_refused_before_the_driver() {
    let mut host = FakeHost::new(config(false));
    host.enabled = false;
    let error = read_target_command(&host, "c1", "list_tables", serde_json::json!({}))
        .await
        .expect_err("a disabled connection must be refused");
    assert_eq!(error, "Connection profile is disabled");
}

#[tokio::test]
async fn a_disabled_profile_is_refused_for_driver_metadata_too() {
    let mut host = FakeHost::new(config(false));
    host.enabled = false;
    let Err(error) = driver_for_connection(&host, "c1").await else {
        panic!("a disabled connection has no driver to hand out");
    };
    assert_eq!(error, "Connection profile is disabled");
}

#[tokio::test]
async fn write_sql_through_the_read_command_is_rejected_by_the_read_only_view() {
    // The profile itself is NOT read-only: only the shared read path forces
    // the read-only view. If that forcing regresses, the DELETE slips past
    // the SQL guard and reaches prepare — which the fake host flags.
    let host = FakeHost::new(config(false));
    let error = read_target_command(
        &host,
        "c1",
        "query",
        serde_json::json!({ "sql": "DELETE FROM users", "database": "app" }),
    )
    .await
    .expect_err("a mutating statement must not ride the read path");
    assert_eq!(
        error,
        "Connection is read-only; 'DELETE' statements are not allowed"
    );
    assert!(
        !host.prepare_reached.load(Ordering::SeqCst),
        "the read-only view must reject before any resource is prepared"
    );
}

#[tokio::test]
async fn driver_metadata_errors_keep_the_host_wording() {
    // ConnectionFailed carries the host's own message; the caller must see
    // exactly that, not the enum's Display wrapper.
    struct Unavailable;
    #[async_trait::async_trait]
    impl ResourceExecutionHost for Unavailable {
        async fn profile(
            &self,
            _id: &str,
        ) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
            Err(WorkflowError::ConnectionFailed {
                connection_id: "c1".into(),
                message: "Connection profile unavailable".into(),
            })
        }
        async fn driver(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
            Err(WorkflowError::Driver("Driver unavailable".into()))
        }
        async fn safe_mode(&self) -> bool {
            false
        }
        async fn prepare(
            &self,
            _connection_id: &str,
            _revision: u64,
            _command: &str,
            _target: ExecutionTarget,
        ) -> Result<PreparedExecution, WorkflowError> {
            unreachable!("driver metadata never prepares")
        }
    }
    let Err(error) = driver_for_connection(&Unavailable, "c1").await else {
        panic!("an unavailable profile must surface its wording");
    };
    assert_eq!(error, "Connection profile unavailable");
}

// `connection_schema_context`（按 token 预算装配表结构上下文）在
// `ai::application::schema_context`：它是 list_tables 迭代 + DDL 压缩的纯
// 装配循环，依赖 ai 的 budget/format_compact_ddl，其行为由宿主侧集成旅程
// 覆盖（桌面 wapp 测试与 MCP prompts/resources 的真实 Runtime 路径）。

