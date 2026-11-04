//! Workflow 定时调度授权单的桌面派生实现（WR-A4 / P6 关闭项「服务身份/委托权限
//! 用于调度」的宿主半边）。
//!
//! ## 派生事实（既有持久化形状，零 schema 迁移）
//!
//! 桌面形态没有独立的授权单存储（显式授权/撤销 IPC 与前端随授权单存储迁移
//! 立项——见 `crates/datazen/src/workflow/domain/grant.rs` 模块头的 P6 剩余项登记）。
//! 授权单由两个既有事实**确定性派生**：
//!
//! 1. `workflows.definition_yaml`（`AppDb` 既有列）：定义修订 = YAML 的
//!    SHA-256（`definition_revision`）；**带 schedule 的定义保存动作就是批准
//!    事实**（`approved_by` 记 `definition-save`，不依赖 platform 身份——组织
//!    作用域文档明言「身份不可用时 workflow 命令依然可用」）。
//! 2. 连接 profile 的 `config_revision`（`DesktopProfiles`，同一
//!    `DesktopRepository` 后端）：配置已变 ⇒ 作用域不再适用 ⇒ 派生单变化 ⇒
//!    调度器的「上膛冻结 → 触发比对」拒绝一次并重新上膛。
//!
//! 确定性是**承重**的：`WorkflowScheduler` 用整单相等比对检出「撤销版本前移/
//! 被替换」。派生单的每个字段都必须是输入（定义 YAML + profile 版本）的纯
//! 函数——尤其 `expires_at` 取固定远期常量，而不是 `now + TTL`（否则两次读取
//! 必不相等，派发永远被拒绝）。
//!
//! ## 拒绝为事实（`Ok(None)`）
//!
//! 与 `WorkflowScheduleGrantReader` 端口文档同口径：以下情形返回 `Ok(None)`
//! （「没有可用授权」是事实，不是基础设施故障），调度器按「无授权单」拒绝派发：
//!
//! * 定义缺失（`AppDbError::NotFound`）；
//! * 定义解析失败（YAML 损坏，本就不可执行）；
//! * 含 AI 步骤（递归含 condition/foreach 嵌套）——服务身份不放行 AI；
//! * 任一数据步骤的目标无法固定（无连接可继承、或 profile 缺失）。
//!
//! 存储打不开等基础设施故障才返回 `Err(PortError)`（失败关闭，调度器拒绝）。

use crate::store::{AppDbError, WorkflowRecord};
use datazen::connection::domain::ProfileReader as _;
use datazen::connection::ProfileRepository;
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{ClientInstanceId, ConnectionId, OrganizationId, PrincipalId, RequestId};
use datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader;
use datazen::workflow::domain::grant::{
    definition_revision, service_principal, WorkflowScheduleGrant, WorkflowScheduleScope,
};
use datazen::workflow::domain::model::{WorkflowDefinition, WorkflowStep};
use std::collections::BTreeMap;
use std::sync::Arc;
use tauri::Manager;

/// 派生单的确定性到期时刻（2099-01-01T00:00:00Z）。
///
/// 桌面派生单无 TTL（显式授权存储迁移已登记）；「远期固定常量」既满足
/// `validate` 的 `expires_at > now` 前提，又保证两次派生逐字段相等。
fn derived_grant_expiry() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2099, 1, 1, 0, 0, 0)
        .single()
        .expect("固定远期时刻必然存在")
}

/// 派生单的批准事实标记：**带 schedule 的定义保存动作**就是批准。
///
/// 不取 platform 身份（桌面身份不可用时调度必须照常工作），也不接受调用方
/// 传入（身份由后端派生——与 `service_principal` 同一裁决）。
const DERIVED_APPROVER: &str = "definition-save";

/// 后台派生上下文（与 `schema_diff/job.rs` 的后台 `RequestContext` 同形）。
fn background_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(crate::platform::LOCAL_ORGANIZATION_ID),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new("datazen-local-client"),
        RequestId::new(format!("workflow-schedule-{}", uuid::Uuid::new_v4())),
        None,
    )
}

/// 桌面调度授权单派生器：挂在 `tauri::AppHandle` 上（与 `DesktopScheduleHost`
/// 同一装配形态），按 `AppState` 的既有存储派生授权单。
///
/// 泛型于 `R: tauri::Runtime`（与 commands 层 `ai_chat` 等同一先例）：生产
/// 走 Wry，测试走 `tauri::test::MockRuntime`。泛型成立靠的是 `AppHandle<R>`
/// 对任意 `R: Runtime` 都 `Send + Sync`（运行时句柄与 window/webview
/// dispatcher 都走 `Send + Sync` 有界 trait）——与端口
/// `WorkflowScheduleGrantReader: Send + Sync + 'static` 的注入形态相容。
pub struct DesktopScheduleGrantReader<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> DesktopScheduleGrantReader<R> {
    /// 挂到宿主的 app 句柄上（同源装配：读同一 `AppState` 的既有存储）。
    pub fn new(handle: tauri::AppHandle<R>) -> Self {
        Self(handle)
    }
}

#[async_trait::async_trait]
impl<R: tauri::Runtime> WorkflowScheduleGrantReader for DesktopScheduleGrantReader<R> {
    async fn schedule_grant(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowScheduleGrant>, PortError> {
        let state = self.0.state::<crate::commands::AppState>();
        let state = state.inner();

        // 1) 既有持久化形状：workflows.definition_yaml。缺失是事实（Ok(None)），
        //    其余存储故障如实上抛（失败关闭）。
        let record = match state.store.app_db().get_workflow(workflow_id) {
            Ok(record) => record,
            Err(AppDbError::NotFound(_)) => return Ok(None),
            Err(error) => {
                return Err(PortError::BackendUnavailable(format!(
                    "workflow store: {error}"
                )))
            }
        };

        // 2) 解析定义（与 registry 同一反序列化词汇）。损坏 ⇒ 事实。
        let definition: WorkflowDefinition = match serde_yaml::from_str(&record.definition_yaml) {
            Ok(definition) => definition,
            Err(error) => {
                tracing::warn!(workflow_id, %error, "workflow schedule grant: unparsable definition");
                return Ok(None);
            }
        };

        // 3) 作用域派生：递归收集数据步骤目标；AI 步骤 / 目标无法固定 ⇒ 事实。
        let mut targets = Vec::new();
        if !collect_targets(
            definition.connection.as_deref(),
            definition.database.as_deref(),
            &definition.steps,
            &mut targets,
        ) {
            tracing::warn!(
                workflow_id,
                "workflow schedule grant: AI step or unfreezable target"
            );
            return Ok(None);
        }
        if targets.is_empty() {
            tracing::warn!(
                workflow_id,
                "workflow schedule grant: no data targets to freeze"
            );
            return Ok(None);
        }

        // 4) 目标所属连接的配置版本（同一 DesktopRepository 后端的 profile 半边；
        //    与 platform adapter 里的 DesktopProfiles 同一实现，背景上下文读取）。
        //    profile 缺失 ⇒ 目标无法固定 ⇒ 事实。
        let profiles = Arc::new(crate::platform::repositories::DesktopProfiles::new(
            state.store.clone(),
        ));
        let ctx = background_context();
        let mut revisions: BTreeMap<String, u64> = BTreeMap::new();
        for target in &targets {
            if revisions.contains_key(&target.connection) {
                continue;
            }
            let connection_id = ConnectionId::new(target.connection.as_str());
            let Some(profile) = profiles.get(&ctx, connection_id).await? else {
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
        let revision = definition_revision(&record.definition_yaml);
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
            organization_id: state
                .organization_scope
                .organization_id()
                .as_str()
                .to_string(),
            service_principal: service_principal(workflow_id, &revision),
            approved_by: DERIVED_APPROVER.to_string(),
            scopes,
            expires_at: derived_grant_expiry(),
            revocation_version: 1,
            revoked: false,
        }))
    }
}

/// 一条待冻结目标：连接 + 继承后的数据库 + 命令。
struct ScheduleTarget {
    connection: String,
    database: Option<String>,
    command: String,
}

/// 递归收集数据步骤目标。返回 `false` = 遇到 AI 步骤或目标无法固定的步骤
/// （整单拒绝调度，失败关闭）。
fn collect_targets(
    workflow_connection: Option<&str>,
    workflow_database: Option<&str>,
    steps: &[WorkflowStep],
    out: &mut Vec<ScheduleTarget>,
) -> bool {
    for step in steps {
        match step {
            WorkflowStep::Ai { .. } => return false,
            WorkflowStep::Query {
                connection,
                database,
                ..
            } => {
                let Some(connection) = connection.as_deref().or(workflow_connection) else {
                    return false;
                };
                out.push(ScheduleTarget {
                    connection: connection.to_string(),
                    database: database
                        .clone()
                        .or_else(|| workflow_database.map(str::to_string)),
                    command: "query".to_string(),
                });
            }
            WorkflowStep::Command {
                command,
                connection,
                input,
                ..
            } => {
                let Some(connection) = connection.as_deref().or(workflow_connection) else {
                    return false;
                };
                // Command 步骤的数据库可在 input.database（继承规则见
                // `inject_inherited_database`：缺省/空串才继承）或 workflow 级默认。
                let input_database = input
                    .get("database")
                    .and_then(serde_json::Value::as_str)
                    .filter(|database| !database.trim().is_empty())
                    .map(str::to_string);
                out.push(ScheduleTarget {
                    connection: connection.to_string(),
                    database: input_database.or_else(|| workflow_database.map(str::to_string)),
                    command: command.clone(),
                });
            }
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                if !collect_targets(workflow_connection, workflow_database, then_steps, out) {
                    return false;
                }
                if let Some(else_steps) = else_steps {
                    if !collect_targets(workflow_connection, workflow_database, else_steps, out) {
                        return false;
                    }
                }
            }
            WorkflowStep::ForEach { steps, .. } => {
                if !collect_targets(workflow_connection, workflow_database, steps, out) {
                    return false;
                }
            }
            // Merge / Transform 等纯数据组装步骤没有独立连接目标。
            _ => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::WorkflowVisibility;
    use crate::testing::app_state_test_support::TestAppState;

    /// mock_app 托管 TestAppState，返回 (app, keepalive)。预置 `conn-1` profile
    /// （SCHEDULED_YAML 的目标连接）。
    async fn host() -> (
        tauri::App<tauri::test::MockRuntime>,
        crate::testing::app_state_test_support::TestStateKeepalive,
    ) {
        let test = TestAppState::new().await;
        test.save_connection("conn-1").await;
        let (state, keepalive) = test.into_state();
        let app = tauri::test::mock_app();
        app.manage(state);
        (app, keepalive)
    }

    const SCHEDULED_YAML: &str = "id: demo\nname: Demo\ndescription: d\nconnection: conn-1\ndatabase: app\nsteps:\n  - type: query\n    id: q1\n    sql: select 1\n";

    fn save(app: &tauri::App<tauri::test::MockRuntime>, id: &str, yaml: &str) {
        let state = app.state::<crate::commands::AppState>();
        state
            .store
            .app_db()
            .upsert_workflow(&WorkflowRecord {
                id: id.to_string(),
                name: "Demo".into(),
                description: "d".into(),
                visibility: WorkflowVisibility::User,
                definition_yaml: yaml.to_string(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            })
            .expect("save workflow");
    }

    fn reader(
        app: &tauri::App<tauri::test::MockRuntime>,
    ) -> DesktopScheduleGrantReader<tauri::test::MockRuntime> {
        DesktopScheduleGrantReader::new(app.handle().clone())
    }

    /// 行为钉子：同一定义两次派生**逐字段相等**——确定性是冻结比对的承重前提。
    #[tokio::test]
    async fn the_derived_grant_is_deterministic_across_reads() {
        let (app, _keepalive) = host().await;
        save(&app, "demo", SCHEDULED_YAML);
        let reader = reader(&app);
        let first: WorkflowScheduleGrant = reader
            .schedule_grant("demo")
            .await
            .expect("read")
            .expect("grant");
        let second: WorkflowScheduleGrant = reader
            .schedule_grant("demo")
            .await
            .expect("read")
            .expect("grant");
        assert_eq!(first, second, "派生单必须是输入的纯函数");
        assert_eq!(first.workflow_revision, definition_revision(SCHEDULED_YAML));
        assert!(first
            .service_principal
            .starts_with("workflow-schedule:demo:"));
        assert_eq!(first.approved_by, "definition-save");
        assert_eq!(first.revocation_version, 1);
        assert!(!first.revoked);
        assert!(
            first.validate(chrono::Utc::now()).is_ok(),
            "派生单必须过本体校验"
        );
        assert_eq!(first.scopes.len(), 1);
        assert_eq!(first.scopes[0].command, "query");
        assert_eq!(first.scopes[0].target.connection_id.as_str(), "conn-1");
        assert_eq!(
            first.scopes[0].target.namespace.database.as_deref(),
            Some("app"),
            "查询步骤未带 database 时继承 workflow 级默认"
        );
    }

    /// 行为钉子：定义一变（修订前移）派生单即变——调度器的「上膛冻结 → 触发
    /// 比对」据此拒绝一次并重新上膛。
    #[tokio::test]
    async fn a_definition_change_moves_the_derived_grant() {
        let (app, _keepalive) = host().await;
        save(&app, "demo", SCHEDULED_YAML);
        let reader = reader(&app);
        let first: WorkflowScheduleGrant = reader
            .schedule_grant("demo")
            .await
            .expect("read")
            .expect("grant");
        let changed = SCHEDULED_YAML.replace("select 1", "select 2");
        save(&app, "demo", &changed);
        let second: WorkflowScheduleGrant = reader
            .schedule_grant("demo")
            .await
            .expect("read")
            .expect("grant");
        assert_ne!(first, second, "定义变更必须换派生单");
        assert_ne!(first.workflow_revision, second.workflow_revision);
    }

    /// 行为钉子：AI 步骤（服务身份不放行）⇒ Ok(None)。
    #[tokio::test]
    async fn an_ai_step_refuses_the_schedule_grant() {
        let (app, _keepalive) = host().await;
        let yaml = "id: demo\nname: Demo\ndescription: d\nconnection: conn-1\nsteps:\n  - type: ai\n    id: a1\n    prompt: p\n";
        save(&app, "demo", yaml);
        let grant = reader(&app).schedule_grant("demo").await.expect("read");
        assert!(grant.is_none(), "AI 步骤必须让派生单缺失");
    }

    /// 行为钉子：定义缺失是事实（Ok(None)），不是基础设施故障。
    #[tokio::test]
    async fn a_missing_definition_is_a_fact_not_an_error() {
        let (app, _keepalive) = host().await;
        let grant = reader(&app).schedule_grant("missing").await.expect("read");
        assert!(grant.is_none());
    }
}
