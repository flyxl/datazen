//! `WorkflowScheduleGrantReader` 的 server 实现（W8 刀3c-2）。
//!
//! 桌面对照物：`datazen-desktop/src/workflow/schedule_grant.rs` 的
//! `DesktopScheduleGrantReader`。派生事实与桌面同构（零 schema 迁移，
//! 授权单由两个既有事实**确定性派生**）：
//!
//! 1. `workflows.definition_yaml`（v13 表）：定义修订 = YAML 的 SHA-256
//!    （`definition_revision`）；**带 schedule 的定义保存动作就是批准
//!    事实**（`approved_by` 记 `definition-save`）。
//! 2. 连接 profile 的 `config_revision`（`ServerProfiles`，同一
//!    `ProfileReader` 端口）：配置已变 ⇒ 作用域不再适用 ⇒ 派生单变化 ⇒
//!    调度器的「上膛冻结 → 触发比对」拒绝一次并重新上膛。
//!
//! 确定性是**承重**的：`WorkflowScheduler` 用整单相等比对检出「撤销版本
//! 前移/被替换」。派生单的每个字段都必须是输入（定义 YAML + profile
//! 版本）的纯函数——`expires_at` 取固定远期常量（2099-01-01），不是
//! `now + TTL`。
//!
//! **拒绝为事实（`Ok(None)`）**：定义缺失、YAML 损坏、含 AI 步骤（服务
//! 身份不放行）、目标无法固定（无连接可继承 / profile 缺失）、目标收集
//! 为空。存储故障才 `Err(PortError)`（失败关闭）。
//!
//! 与桌面的差异只有一处：**组织来源**——桌面读宿主解析的单组织
//! （`AppState.organization_scope`），server 读**宿主配置的组织**（v1
//! 单组织部署口径；调度器扫描的就是这个组织的定义，见 `scheduler.rs`）。

use std::collections::BTreeMap;
use std::sync::Arc;

use datazen::connection::domain::ProfileReader as _;
use datazen::shared::error::PortError;
use datazen::shared::id::ConnectionId;
use datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader;
use datazen::workflow::domain::grant::{
    collect_schedule_targets, definition_revision, service_principal, WorkflowScheduleGrant,
    WorkflowScheduleScope,
};
use datazen::workflow::domain::model::WorkflowDefinition;

use crate::storage::{ServerProfiles, ServerWorkflowStore};
use crate::workflow::host::background_context;

/// 派生单的确定性到期时刻（2099-01-01T00:00:00Z；与桌面同一常量值——
/// 两形态的派生单可比对）。
fn derived_grant_expiry() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2099, 1, 1, 0, 0, 0)
        .single()
        .expect("固定远期时刻必然存在")
}

/// 派生单的批准事实标记（桌面同词：带 schedule 的定义保存动作）。
const DERIVED_APPROVER: &str = "definition-save";

/// server 调度授权单派生器：按宿主配置的组织，从 v13 `workflows` 行 +
/// `ServerProfiles` 的配置版本确定性派生授权单。
pub struct ServerScheduleGrantReader {
    workflows: ServerWorkflowStore,
    profiles: Arc<ServerProfiles>,
    organization_id: String,
}

impl ServerScheduleGrantReader {
    /// 构造（bootstrap 装配入口；`workflows` 是调度器同组织的 scoped
    /// store）。
    pub fn new(
        workflows: ServerWorkflowStore,
        profiles: Arc<ServerProfiles>,
        organization_id: &str,
    ) -> Self {
        Self {
            workflows,
            profiles,
            organization_id: organization_id.to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl WorkflowScheduleGrantReader for ServerScheduleGrantReader {
    async fn schedule_grant(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowScheduleGrant>, PortError> {
        // 1) v13 既有持久化形状：workflows.definition_yaml。缺失是事实
        //    （Ok(None)），其余存储故障如实上抛（失败关闭）。
        let Some((definition_yaml, _visibility)) = self
            .workflows
            .raw_definition(workflow_id)
            .await
            .map_err(|error| PortError::BackendUnavailable(format!("workflow store: {error}")))?
        else {
            return Ok(None);
        };

        // 2) 解析定义（与 registry 同一反序列化词汇）。损坏 ⇒ 事实。
        let definition: WorkflowDefinition =
            match datazen::workflow::domain::repository::decode_definition(&definition_yaml) {
                Ok(definition) => definition,
                Err(error) => {
                    tracing::warn!(workflow_id, %error, "workflow schedule grant: unparsable definition");
                    return Ok(None);
                }
            };

        // 3) 作用域派生：共享层收集规则（AI 步骤 / 目标无法固定 / 无
        //    目标 ⇒ 事实）。
        let Some(targets) = collect_schedule_targets(
            definition.connection.as_deref(),
            definition.database.as_deref(),
            &definition.steps,
        ) else {
            tracing::warn!(
                workflow_id,
                "workflow schedule grant: AI step or unfreezable target"
            );
            return Ok(None);
        };
        if targets.is_empty() {
            tracing::warn!(
                workflow_id,
                "workflow schedule grant: no data targets to freeze"
            );
            return Ok(None);
        }

        // 4) 目标所属连接的配置版本（ProfileReader 只读窄端口，背景
        //    上下文读取）。profile 缺失 ⇒ 目标无法固定 ⇒ 事实。
        let ctx = background_context(&self.organization_id);
        let mut revisions: BTreeMap<String, u64> = BTreeMap::new();
        for target in &targets {
            if revisions.contains_key(&target.connection) {
                continue;
            }
            let connection_id = ConnectionId::new(target.connection.as_str());
            let Some(profile) = self.profiles.get(&ctx, connection_id).await? else {
                tracing::warn!(workflow_id, connection = %target.connection,
                    "workflow schedule grant: profile missing, target unfreezable");
                return Ok(None);
            };
            revisions.insert(
                target.connection.clone(),
                profile.config_revision.counter().get(),
            );
        }

        // 5) 组装派生单：每个字段都是（定义 YAML + profile 版本）的纯函数。
        let revision = definition_revision(&definition_yaml);
        let scopes = targets
            .into_iter()
            .map(|target| {
                WorkflowScheduleScope::namespace(
                    &target.connection,
                    target.database.clone(),
                    None,
                    revisions[&target.connection],
                    &target.command,
                )
            })
            .collect();
        Ok(Some(WorkflowScheduleGrant {
            workflow_id: workflow_id.to_string(),
            workflow_revision: revision.clone(),
            organization_id: self.organization_id.clone(),
            service_principal: service_principal(workflow_id, &revision),
            approved_by: DERIVED_APPROVER.to_string(),
            scopes,
            expires_at: derived_grant_expiry(),
            revocation_version: 1,
            revoked: false,
        }))
    }
}
