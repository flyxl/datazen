//! `WorkflowScheduleGrantReader`：workflow 定时调度授权单的**只读**窄端口。
//!
//! ## 它对照什么
//!
//! 参照 [`crate::monitor::application::MonitorGrantReader`]（monitor/** 只读
//! 先例）的形态：一个方法、只读、失败关闭、缺失是**事实**不是错误。
//!
//! ## 谁消费它
//!
//! * `engine::scheduler`（`WorkflowScheduler`）：派发前经它读当前授权单——
//!   上膛时冻结、触发时比对（撤销版本前移 → 拒绝）；
//! * 桌面宿主（`src-tauri/src/workflow/schedule_grant.rs`）：唯一实现方，按
//!   **既有持久化形状**（`workflows.definition_yaml` + 连接 profile）派生授权单。
//!
//! ## 边界
//!
//! * **只读**：签发/撤销/保存授权单都不在本端口（桌面形态的授权单由定义冻结
//!   事实派生，没有独立的签发面；显式授权存储迁移已登记不实现）。
//! * 端口**不重试、不鉴权、不缓存**。授权单缺失（未启用定时 / 定义缺失 /
//!   目标无法固定）都是事实（`Ok(None)`），不是基础设施故障。
//! * 基础设施故障（存储打不开等）才返回 `Err(PortError)`，由调用方决定
//!   拒绝派发（调度器拒绝，失败关闭）。

use async_trait::async_trait;

use crate::shared::error::PortError;
use crate::workflow::domain::grant::WorkflowScheduleGrant;

/// workflow 定时调度授权单的只读窄端口。
#[async_trait]
pub trait WorkflowScheduleGrantReader: Send + Sync + 'static {
    /// 当前登记的定时调度授权单。
    ///
    /// 没有启用定时、定义缺失、定义不可执行、目标无法固定、含服务身份下
    /// 不放行的步骤（AI）⇒ `Ok(None)`：「没有可用授权」是**事实**，不是错误。
    async fn schedule_grant(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowScheduleGrant>, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::domain::grant::{definition_revision, WorkflowScheduleScope};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（与 MonitorGrantReader
    /// 的注入形态一致）。`?Sized` 是承重的，理由见该文件同名钉子。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn grant(workflow_id: &str) -> WorkflowScheduleGrant {
        WorkflowScheduleGrant {
            workflow_id: workflow_id.into(),
            workflow_revision: definition_revision("id: demo\n"),
            organization_id: "org-1".into(),
            service_principal: "workflow-schedule:demo:abc".into(),
            approved_by: "user-1".into(),
            scopes: vec![WorkflowScheduleScope::namespace(
                "conn-1",
                Some("app".into()),
                None,
                1,
                "query",
            )],
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            revocation_version: 1,
            revoked: false,
        }
    }

    struct StubReader {
        grants: Mutex<HashMap<String, Option<WorkflowScheduleGrant>>>,
    }

    impl StubReader {
        fn with(grants: &[(&str, Option<WorkflowScheduleGrant>)]) -> Self {
            Self {
                grants: Mutex::new(
                    grants
                        .iter()
                        .map(|(id, grant)| (id.to_string(), grant.clone()))
                        .collect(),
                ),
            }
        }
    }

    #[async_trait]
    impl WorkflowScheduleGrantReader for StubReader {
        async fn schedule_grant(
            &self,
            workflow_id: &str,
        ) -> Result<Option<WorkflowScheduleGrant>, PortError> {
            Ok(self
                .grants
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(workflow_id)
                .cloned()
                .flatten())
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn WorkflowScheduleGrantReader>();
        let reader: Arc<dyn WorkflowScheduleGrantReader> = Arc::new(StubReader::with(&[(
            "demo",
            Some(grant("demo")),
        )]));
        let fetched = reader
            .schedule_grant("demo")
            .await
            .expect("读授权单失败必须是端口事实");
        let fetched = fetched.expect("应当有授权单");
        assert_eq!(fetched.workflow_id, "demo");
        assert_eq!(fetched.scopes[0].target.connection_id.as_str(), "conn-1");
    }

    /// 行为钉子：授权单缺失是事实（Ok(None)），不是基础设施故障。
    #[tokio::test]
    async fn a_missing_grant_is_a_fact_not_an_error() {
        let reader: Arc<dyn WorkflowScheduleGrantReader> =
            Arc::new(StubReader::with(&[]));
        assert!(reader
            .schedule_grant("missing")
            .await
            .expect("读失败")
            .is_none());
    }

    /// 行为钉子：只读端口只有一个方法，不得出现签发/撤销/保存等写面。
    #[test]
    fn the_reader_declares_only_the_single_read_path() {
        let source = code_only(include_str!("grant_reader.rs"));
        let declaration = source
            .split("pub trait WorkflowScheduleGrantReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            1,
            "授权单端口只允许一个方法：{declaration}"
        );
        for forbidden in ["issue", "revoke", "save", "write"] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写方法 `{forbidden}`：{declaration}"
            );
        }
    }
}
