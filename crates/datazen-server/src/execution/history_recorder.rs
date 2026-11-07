//! 执行面 → 查询历史落账钩子（W8 刀3a；桌面
//! `record_sql_command_outcome` 的 transport 对偶）。
//!
//! ## 这是干嘛的
//!
//! web 形态的 SQL 经 execution 域路由提交（202 异步受理），执行终态在
//! runtime 里。桌面的对等路径在命令层落账（`driver_command/helpers.rs`
//! 的 `record_sql_command_outcome`：命令返回后解析连接、记一条
//! `QueryHistoryEntry`、失败只告警）。server 的 HTTP 面在 202 就返回，
//! 落账因此挂在**受理后**：每条 SQL 受理 spawn 一个轻量轮询任务，终态
//! 后按受理方的认证链事实落一行（best-effort——落账失败绝不阻断执行
//! 应答，也绝不重试到天荒地老）。
//!
//! ## 裁决
//!
//! - **只记 SQL 命令**（`query` / `execute` / `explain`）——桌面同口径；
//!   元数据命令（list_databases 等）不进历史。
//! - **身份来源**：spawn 时刻的 `SessionIdentity.ctx`（组织 + 主体），
//!   INV-01 同源——落账行的归属就是提交者的归属。
//! - **连接解析**：executeAtTarget 从请求 target 直取；executeInSession
//!   从会话目录条目取（`SessionOwner.connection_id`，受理时的可见性闸
//!   已读过目录）。
//! - **时刻与时长**：`executed_at` = 受理时刻；`execution_time_ms` =
//!   轮询到终态的墙钟差（v1 口径：含排队；桌面记的是命令执行段——
//!   差异如实登记，不谎称相等）。
//! - **终态口径**：`success` = `ExecutionState::Succeeded`；取消/失败都
//!   是 false（错误文案取视图 `error_code` 的显示形）。
//! - **rows_affected**：视图不含行数（`ExecutionView` 无此字段）——v1
//!   记 `None`（未知），登记差异；行数面随结果投影切片补。
//! - **轮询上限**：默认 10 分钟（`POLL_DEADLINE`）——超过即放弃落账
//!   （长查询的历史缺失是如实缺口，不是谎报成功）。

use std::sync::Arc;
use std::time::Duration;

use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::domain::session_directory::SessionDirectory as _;
use datazen::history::QueryHistoryEntry;
use datazen::shared::context::RequestContext;
use datazen::shared::id::DbSessionId;

use crate::storage::query_assets::ServerQueryHistoryStore;
use crate::storage::AdminStore;

/// 轮询终态的上限（超过即放弃落账）。
const POLL_DEADLINE: Duration = Duration::from_secs(600);
/// 轮询间隔（target_runner 的 25ms 对长查询太密；落账面 500ms 足够）。
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 查询历史落账器（bootstrap 装配；`ExecutionState.history` 持有）。
#[derive(Clone)]
pub struct HistoryRecorder {
    store: Arc<AdminStore>,
    /// 缺席（`None`）= 落账关闭（测试/特殊部署显式关）。
    enabled: bool,
}

impl HistoryRecorder {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self {
            store,
            enabled: true,
        }
    }

    /// 关闭落账（测试装配面）。
    pub fn disabled() -> Self {
        Self {
            store: Arc::new(AdminStore::open_in_memory().expect("in-memory store")),
            enabled: false,
        }
    }

    /// SQL 命令判定（桌面同口径三词）。
    pub fn is_sql_command(command: &str) -> bool {
        matches!(command, "query" | "execute" | "explain")
    }

    /// 受理后 spawn 落账任务。`connection` = 连接 id（target 直取或目录
    /// 解析）；`database_hint` = 调用方已知的逻辑库（target 命名空间的
    /// database 层；会话路径为 None，落账时从目录条目的会话观察补）。
    pub fn record_on_acceptance(
        &self,
        cases: Arc<dyn ConnectionUseCases>,
        directory: Arc<dyn datazen::execution::SessionDirectory>,
        ctx: RequestContext,
        command: String,
        sql: Option<String>,
        connection: Option<String>,
        db_session: Option<DbSessionId>,
        execution_id: datazen::shared::id::ExecutionId,
    ) {
        if !self.enabled || !Self::is_sql_command(&command) {
            return;
        }
        let store = self.store.clone();
        tokio::spawn(async move {
            let started = tokio::time::Instant::now();
            let mut terminal_state = None;
            while started.elapsed() < POLL_DEADLINE {
                match cases.get_execution(&ctx, execution_id.clone()).await {
                    Ok(view) => {
                        if view.state.is_terminal() {
                            terminal_state = Some(view);
                            break;
                        }
                    }
                    Err(error) => {
                        // 执行记录读不到（目录不可用/运行时吊销）：落账
                        // 放弃，warn 如实。
                        tracing::warn!(
                            %error,
                            execution = %execution_id.as_str(),
                            "query history recording abandoned: execution unreadable"
                        );
                        return;
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            let Some(view) = terminal_state else {
                tracing::warn!(
                    execution = %execution_id.as_str(),
                    "query history recording abandoned: poll deadline exceeded"
                );
                return;
            };
            // 连接解析：target 直取的已在参数里；会话路径此刻查目录
            // （受理时已过可见性闸，此时条目可能已被关闭/吊销——查不到
            // 就放弃，不猜）。
            let connection_id = match connection {
                Some(connection) => connection,
                None => {
                    let Some(db_session) = db_session.as_ref() else {
                        return;
                    };
                    match directory.lookup(db_session.clone()).await {
                        Ok(Some(owner)) => owner.connection_id.as_str().to_owned(),
                        _ => {
                            tracing::warn!(
                                execution = %execution_id.as_str(),
                                "query history recording abandoned: session gone"
                            );
                            return;
                        }
                    }
                }
            };
            let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let success = view.state == datazen::execution::ExecutionState::Succeeded;
            let entry = QueryHistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                connection_id,
                database: String::new(),
                schema: None,
                sql: sql.unwrap_or_default(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: elapsed_ms,
                rows_affected: None,
                success,
                error_message: view.error_code.map(|code| format!("{code:?}")),
            };
            let scoped = ServerQueryHistoryStore::for_identity(
                store,
                ctx.organization_id.as_str(),
                ctx.principal_id.as_str(),
            );
            if let Err(error) = scoped.add(entry).await {
                tracing::warn!(
                    %error,
                    execution = %execution_id.as_str(),
                    "query history recording failed (best-effort, non-blocking)"
                );
            }
        });
    }
}
