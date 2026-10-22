//! `ExecutionSettingsReader`：执行期设置项的**只读**窄端口。
//!
//! ## 它与 `SettingsRepository` 的分工
//!
//! [`SettingsRepository`](crate::ports::settings::SettingsRepository) 是**跨形态设置的
//! 读写仓储**：整份 `SharedSettings`（用户偏好 + 执行设置），并且带写方法。
//! 执行路径只关心其中**两项**，却因此要拿到一个「能改设置」的端口。
//!
//! 本端口只投影执行期**真正要读**的两项：
//!
//! | 设置项 | 桌面侧读它的地方 |
//! |---|---|
//! | `safe_mode` | 只读闸门 `sql_guard::check_sql`（`src-tauri/src/platform/session_backend/execution.rs`、`src-tauri/src/commands/driver_command/execute.rs`） |
//! | 结果集上限 | SELECT 结果封顶（`src-tauri/src/commands/driver_command/helpers.rs` 的 `result_limit()`） |
//!
//! **刻意不含**：MCP 开关、AI 出站策略、驱动私有设置、用户偏好，以及连接池大小
//! ——后者全仓只有写入方（`src-tauri/src/store/settings.rs`），**没有任何执行期读取方**。
//! 它们由 `SettingsRepository` 或各自的窄端口提供。
//!
//! ## 同一份事实的**三个**取法（别把它误判成三份重复实现）
//!
//! 集成后同一份执行设置有三个取法，**它们是同一份事实的三种投影**，不是三份实现：
//!
//! | 取法 | 位置 | 给什么 |
//! |---|---|---|
//! | `SettingsRepository::read_settings` | 本包（platform-api） | 整份 `SharedSettings`（含用户偏好）**+ 写方法** |
//! | `DatabaseToolHost::execution_settings` | `packages/feature-core`（feature-core 层的既有 trait，**不是** platform-api 端口） | 整份 `ExecutionSettings` |
//! | `ExecutionSettingsReader::read` | 本包（本端口） | 执行期两项的投影，**只读** |
//!
//! **收敛方向**是让后两者改为**经本端口取**（feature-core 那条 trait 收窄为注入本端口），
//! 但那是后续轨的事。这里写下重叠，是为了让集成者知道：**这三者不是三份重复实现，
//! 而是同一份设置的三种取用宽度**。
//!
//! ## 词汇表
//!
//! `ExecutionPolicy`：执行期开关的**投影**。它是执行期的开关集合，**不是**授权策略
//! ——授权在 [`PolicyService`](crate::ports::policy::PolicyService) / `PolicyIsolationKey`
//! 那一维度，两者不混为一谈。
//!
//! ## 边界
//!
//! * **只读**，失败一律 `Result<_, PortError>`。
//! * 端口**不重试、不鉴权、不缓存**：设置快照的取用由实现方负责，端口只给事实。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::error::PortError;

/// 执行期真正要读的那几项设置。
///
/// 投影而非原样转发：原样转发会把 MCP / AI / 驱动私有设置一并带进执行路径的依赖面，
/// 「窄」就名存实亡。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionPolicy {
    /// 安全模式：只读执行。写语句被只读闸门拒绝。
    pub safe_mode: bool,
    /// SELECT 结果集上限；`None` 表示**不封顶**（不是「上限为 0」）。
    pub result_limit: Option<u32>,
}

impl ExecutionPolicy {
    /// 从整份执行设置里投影出执行期要读的那几项。
    ///
    /// 结果上限沿用 `ExecutionSettings::result_limit()` 的判定：`limit_select_results`
    /// 关闭或上限为 0 时**不封顶**（`None`），而不是把上限解释成 0 行。
    pub fn from_settings(settings: &crate::settings::ExecutionSettings) -> Self {
        Self {
            safe_mode: settings.safe_mode,
            result_limit: settings.result_limit(),
        }
    }
}

/// 执行期设置项的只读窄端口。
#[async_trait]
pub trait ExecutionSettingsReader: Send + Sync + 'static {
    /// 读执行期要用的那几项设置。
    async fn read(&self, ctx: &RequestContext) -> Result<ExecutionPolicy, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
    use std::sync::Arc;

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（§4.1 的注入形态）。
    ///
    /// `?Sized` 是承重的：`dyn Trait` 本身不是 `Sized`，少了它这个断言退化成
    /// 「某个 `Sized` 类型满足这几个约束」，与对象安全无关。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn settings(safe_mode: bool, limit: bool, rows: u32) -> crate::settings::ExecutionSettings {
        crate::settings::ExecutionSettings {
            limit_select_results: limit,
            query_result_limit: rows,
            safe_mode,
            connection_pool_size: 10,
            mcp_disabled_tools: vec![],
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: vec![],
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        }
    }

    struct StubReader {
        policy: ExecutionPolicy,
    }

    #[async_trait]
    impl ExecutionSettingsReader for StubReader {
        async fn read(&self, _ctx: &RequestContext) -> Result<ExecutionPolicy, PortError> {
            Ok(self.policy)
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn ExecutionSettingsReader>();
        let reader: Arc<dyn ExecutionSettingsReader> = Arc::new(StubReader {
            policy: ExecutionPolicy {
                safe_mode: true,
                result_limit: Some(500),
            },
        });
        let policy = reader
            .read(&context())
            .await
            .expect("读设置失败必须是端口事实");
        assert!(policy.safe_mode);
        assert_eq!(policy.result_limit, Some(500));
    }

    #[test]
    fn the_projection_keeps_only_the_two_execution_time_switches() {
        let policy = ExecutionPolicy::from_settings(&settings(true, true, 100));
        assert!(policy.safe_mode);
        assert_eq!(policy.result_limit, Some(100));

        // 上限开关关闭 ⇒ **不封顶**（None），不是「上限 0」。
        let unlimited = ExecutionPolicy::from_settings(&settings(false, false, 0));
        assert_eq!(unlimited.result_limit, None);

        // 上限为 0 且开关打开 ⇒ 同样不封顶。
        let zero = ExecutionPolicy::from_settings(&settings(false, true, 0));
        assert_eq!(zero.result_limit, None);
    }

    #[test]
    fn the_reader_declares_no_write_path() {
        let source = code_only(include_str!("execution_settings.rs"));
        let declaration = source
            .split("pub trait ExecutionSettingsReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            1,
            "只读端口只允许一个方法：{declaration}"
        );
        for forbidden in ["write", "save", "update", "set"] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写方法 `{forbidden}`：{declaration}"
            );
        }
    }

    #[test]
    fn the_projection_does_not_leak_the_non_execution_settings() {
        // 投影里只有两项：安全模式与结果上限。MCP / AI / 驱动私有设置不在其中。
        let policy = ExecutionPolicy::from_settings(&settings(true, true, 10));
        let debug = format!("{policy:?}");
        assert!(!debug.contains("mcp"));
        assert!(!debug.contains("ai_strict_egress"));
        assert!(!debug.contains("connection_pool_size"));
    }
}
