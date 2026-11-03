//! Workflow 定时调度的服务身份与授权单（WR-A4 / P6 关闭项「服务身份/委托权限
//! 用于调度」的最小可代码化契约）。
//!
//! # 契约设计
//!
//! 参照 [`crate::monitor::domain::MonitorGrant`]（monitor/** 只读先例，本模块
//! 与其 feature 互相独立、无依赖边）的形态，为 workflow 定时执行定义**一份
//! 授权单 + 一个服务身份**：
//!
//! * [`WorkflowScheduleGrant`]：后端持有、可撤销的调度授权元数据。**不携带
//!   bearer 凭据、session、秘密材料**——与 MonitorGrant 同一条红线。
//! * [`WorkflowScheduleScope`]：授权单登记的一条作用域（执行目标 + 目标所属
//!   连接的配置版本 + 授权的命令）。配置已变 ⇒ 该作用域失效 ⇒ 整单重派生。
//! * [`service_principal`]：调度执行以其名义派发的服务身份。**绝不**借用
//!   桌面用户 principal，也不接受调用方传入身份（与 monitor 授权 IPC 只收
//!   dashboard/widget ID 的裁决同形：身份由后端派生，不由客户端声明）。
//!
//! # 派发前校验（钉在 [`WorkflowScheduleGrant::validate`]）
//!
//! * 无效授权单（撤销、过期、版本为 0、修订为空、作用域为空）→ 拒绝；
//! * 撤销版本前移 / 授权单被替换 → 由调度器的「上膛冻结 → 触发比对」检出
//!   （见 `engine::scheduler`），本体校验负责其余失效形态。
//!
//! # 与 MonitorGrant 的复用与差异裁决
//!
//! | 维度 | MonitorGrant | 本契约 | 裁决 |
//! | --- | --- | --- | --- |
//! | 复用 monitor 类型 | — | — | 不复用：`workflow` feature 不含 `monitor`（feature 图无环），且二者授权对象（widget 定时查询 vs 用户定时自动化）语义不同，强行共用会让两边各自长出对方的特例 |
//! | 修订算法 | `monitor::domain::revision`（SHA-256） | [`definition_revision`]（同算法） | 三行实现各持一份，行为钉子互相钉住「同输入同输出」 |
//! | 只读约束 | `read_only` + 只读 SQL AST 白名单 | 无 | workflow 定时任务是用户显式保存的自动化（可含写命令），授权单冻结**目标与定义修订**而非 SQL 白名单；SQL 仍走既有 `sql_guard` / 安全模式 |
//! | TTL | 60–86400 秒显式授权 | 字段存在，桌面派生单不设到期 | 见模块尾「剩余项」 |
//!
//! # 剩余项（环境/UX；执行上下文接线已收口）
//!
//! * live 验收与 e2e（真实驱动旅程）——可码化部分已钉在桌面
//!   `live_acceptance_tests`（fake-driver 旅程 + live 腿的 CI 策略注释）；
//! * 桌面侧显式授权/撤销 IPC 与前端（现由定义保存动作充当批准事实，派生单
//!   无 TTL、无独立撤销状态——需要授权单存储迁移，已登记不实现）；
//! * ~~调度执行以服务身份上下文派发~~ **已接线（`application::execution_identity`
//!   + `engine::scheduler` 派发路径）**：闸门放行后以 [`service_principal`]
//!   构造执行上下文（owner 归属授权单组织、审计主体 kind=Service、无委托），
//!   宿主经 `WorkflowScheduleHost::execute_workflow` 的身份参数接收。

use crate::connection::domain::NamespaceTarget;
use crate::execution::domain::target::ExecutionTarget;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 定义修订：definition YAML 的 SHA-256 十六进制串。
///
/// 与 `monitor::domain::revision` 同形（monitor feature 与 workflow feature
/// 互相独立，无法共用一份；两侧行为钉子都钉「同输入同输出、输入一变即变」）。
/// 定义修订是授权单的**冻结事实**：定义一变，修订前移，旧单作废。
pub fn definition_revision(definition: &str) -> String {
    format!("{:x}", Sha256::digest(definition.as_bytes()))
}

/// 服务身份派生：稳定、可审计、不含秘密材料。
///
/// 身份由 `(workflow_id, workflow_revision)` 确定性派生——定义或目标一变，
/// 身份随之变化，连接池与授权缓存因此不会把两个权限范围混用（与
/// `execution_identity` 的委托路径「换委托 ⇒ 换身份键」同裁决）。
pub fn service_principal(workflow_id: &str, workflow_revision: &str) -> String {
    let prefix = &workflow_revision[..workflow_revision.len().min(16)];
    format!("workflow-schedule:{workflow_id}:{prefix}")
}

/// 授权单登记的一条作用域（与 `MonitorScope` 同形：目标 + 配置版本 + 命令）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowScheduleScope {
    /// 授权触达的执行目标（连接 + 命名空间）。
    pub target: ExecutionTarget,
    /// 目标所属连接的配置版本：配置已变 ⇒ 本作用域不再适用。
    pub config_revision: u64,
    /// 授权的命令（`query` / `execute` / 驱动命令 id …）。
    pub command: String,
}

impl WorkflowScheduleScope {
    /// 命名空间便捷构造（数据步骤的冻结目标）。
    pub fn namespace(
        connection_id: &str,
        database: Option<String>,
        schema: Option<String>,
        config_revision: u64,
        command: &str,
    ) -> Self {
        Self {
            target: ExecutionTarget::new(
                crate::shared::id::ConnectionId::new(connection_id),
                NamespaceTarget::new(database, None, schema, vec![]),
            ),
            config_revision,
            command: command.to_string(),
        }
    }
}

/// Workflow 定时调度的授权单：后端持有、可撤销的元数据。无 bearer 凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowScheduleGrant {
    pub workflow_id: String,
    /// 冻结的定义修订。定义变更 ⇒ 修订前移 ⇒ 本单作废（重新派生新单）。
    pub workflow_revision: String,
    pub organization_id: String,
    /// 服务身份：调度执行以其名义派发，绝不复用桌面用户 principal。
    pub service_principal: String,
    /// 批准者（审计事实；桌面形态为本地用户 principal）。
    pub approved_by: String,
    pub scopes: Vec<WorkflowScheduleScope>,
    pub expires_at: DateTime<Utc>,
    /// 撤销版本。0 = 未初始化（无效单）；重发/撤销必须前移。
    pub revocation_version: u64,
    pub revoked: bool,
}

impl WorkflowScheduleGrant {
    /// 派发前的授权单本体校验。
    ///
    /// 故意**不接收** [`crate::shared::context::RequestContext`]：本体校验只
    /// 回答「这一份授权单本身还有效吗」——撤销、过期、版本、修订与作用域
    /// 完备性。派发期的身份事实由闸门放行后的
    /// `application::execution_identity` 从**本单**派生（服务身份上下文），
    /// 不经本方法；若将来服务端签发流引入请求侧上下文一致性校验，再在此挂
    /// （monitor 的 `validate(ctx, now)` 是它的先例）。
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), &'static str> {
        if self.revoked
            || self.expires_at <= now
            || self.revocation_version == 0
            || self.workflow_revision.is_empty()
            || self.service_principal.is_empty()
            || self.scopes.is_empty()
        {
            return Err("workflow schedule grant revoked, expired or invalid");
        }
        Ok(())
    }

    /// 本单是否冻结着这份定义修订。
    ///
    /// 调度器在触发时用它比对「上膛时冻结的单」与「当前派生单」：不一致即
    /// 撤销/重发/定义变更（版本前移），拒绝本次派发。
    pub fn covers_definition(&self, workflow_revision: &str) -> bool {
        self.workflow_revision == workflow_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::Counter;

    const DEFINITION: &str = "id: demo\nsteps: []\n";

    fn scope() -> WorkflowScheduleScope {
        WorkflowScheduleScope::namespace("conn-1", Some("app".into()), None, 3, "query")
    }

    fn grant() -> WorkflowScheduleGrant {
        WorkflowScheduleGrant {
            workflow_id: "demo".into(),
            workflow_revision: definition_revision(DEFINITION),
            organization_id: "org-1".into(),
            service_principal: service_principal("demo", &definition_revision(DEFINITION)),
            approved_by: "user-1".into(),
            scopes: vec![scope()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
            revocation_version: 1,
            revoked: false,
        }
    }

    /// 行为钉子：有效授权单通过本体校验，作用域冻结目标、配置版本与命令。
    #[test]
    fn a_valid_grant_passes_validation_and_freezes_scope_facts() {
        let grant = grant();
        assert!(grant.validate(Utc::now()).is_ok());
        assert!(grant.covers_definition(&definition_revision(DEFINITION)));
        assert!(!grant.covers_definition(&definition_revision("id: changed\n")));
        assert_eq!(grant.scopes[0].config_revision, 3);
        assert_eq!(grant.scopes[0].command, "query");
        assert_eq!(
            grant.scopes[0].target.namespace.database.as_deref(),
            Some("app")
        );
    }

    /// 行为钉子：撤销 / 过期 / 版本为 0 / 修订为空 / 作用域为空一律拒绝。
    #[test]
    fn revoked_expired_or_invalid_grants_are_rejected() {
        let now = Utc::now();
        let mut revoked = grant();
        revoked.revoked = true;
        assert!(revoked.validate(now).is_err(), "已撤销的单必须拒绝");

        let mut expired = grant();
        expired.expires_at = now - chrono::Duration::seconds(1);
        assert!(expired.validate(now).is_err(), "过期的单必须拒绝");

        let mut uninitialized = grant();
        uninitialized.revocation_version = 0;
        assert!(
            uninitialized.validate(now).is_err(),
            "版本为 0 的单必须拒绝"
        );

        let mut no_revision = grant();
        no_revision.workflow_revision = String::new();
        assert!(no_revision.validate(now).is_err(), "无定义修订的单必须拒绝");

        let mut no_scopes = grant();
        no_scopes.scopes.clear();
        assert!(no_scopes.validate(now).is_err(), "无作用域的单必须拒绝");

        let mut anonymous = grant();
        anonymous.service_principal = String::new();
        assert!(anonymous.validate(now).is_err(), "无服务身份的单必须拒绝");
    }

    /// 行为钉子：定义修订 = definition YAML 的 SHA-256，输入一变即变。
    #[test]
    fn the_definition_revision_moves_when_the_definition_changes() {
        let original = definition_revision(DEFINITION);
        assert_eq!(original, definition_revision(DEFINITION), "同输入同修订");
        assert_ne!(original, definition_revision("id: other\n"));
        // 与 monitor 同形：同算法（SHA-256 hex），跨家族语义一致。
        assert_eq!(original.len(), 64);
    }

    /// 行为钉子：服务身份由 (workflow_id, revision) 确定性派生，且声明归属。
    #[test]
    fn the_service_principal_is_deterministic_and_declares_its_origin() {
        let revision = definition_revision(DEFINITION);
        let principal = service_principal("demo", &revision);
        assert_eq!(principal, service_principal("demo", &revision));
        assert!(principal.starts_with("workflow-schedule:demo:"));
        assert_ne!(
            principal,
            service_principal("other", &revision),
            "不同工作流不得共享服务身份"
        );
        assert_ne!(
            principal,
            service_principal("demo", &definition_revision("id: changed\n")),
            "定义变更必须换服务身份"
        );
        // Debug 形态不含秘密材料（本就没有）：不出现 password/token 字样。
        let grant = grant();
        let debug = format!("{grant:?}");
        assert!(!debug.to_ascii_lowercase().contains("password"));
        assert!(!debug.to_ascii_lowercase().contains("token"));
    }

    /// 行为钉子：作用域构造把目标/配置版本/命令冻结进一条记录。
    #[test]
    fn a_scope_record_carries_target_revision_and_command() {
        let scope = WorkflowScheduleScope::namespace(
            "conn-1",
            None,
            Some("public".into()),
            Counter::new(7).get(),
            "execute",
        );
        assert_eq!(
            scope.target.connection_id.as_str(),
            "conn-1",
            "目标必须携带连接"
        );
        assert_eq!(scope.target.namespace.database, None);
        assert_eq!(scope.target.namespace.schema.as_deref(), Some("public"));
        assert_eq!(scope.config_revision, 7);
        assert_eq!(scope.command, "execute");
    }
}
