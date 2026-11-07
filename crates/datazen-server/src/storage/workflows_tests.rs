//! v13 workflow 双表的存储测试（W8 刀3c-1）。
//!
//! 覆盖面：定义仓储四操作（含组织隔离与 visibility 过滤）、运行历史
//! 四操作（含过滤/截断/未知 id）、共享层 `WorkflowRegistry` 架在本件
//! 之上的登记规则回归（保存 YAML → 列表投影 → 删除）。

use std::sync::Arc;

use datazen::shared::id::OrganizationId;
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::domain::history::new_run_id;
use datazen::workflow::domain::model::WorkflowVisibility;
use datazen::workflow::domain::model::{StepExecutionResult, WorkflowExecutionResult};
use datazen::workflow::domain::repository::{
    encode_definition, list_item_for, validate_workflow_id, WorkflowDefinitionRecord,
    WorkflowDefinitionRepository,
};
use datazen::workflow::registry::WorkflowRegistry;

use crate::storage::{AdminStore, ServerWorkflowHistoryStore, ServerWorkflowStore};

const ORG: &str = "org-workflow-tests";
const OTHER_ORG: &str = "org-other";

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("admin store");
    store.migrate().expect("migrations");
    Arc::new(store)
}

fn scoped_store(store: &Arc<AdminStore>, org: &str) -> ServerWorkflowStore {
    ServerWorkflowStore::scoped(store.clone(), org)
}

fn scope(org: &str) -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(org))
}

fn sample_yaml(id: &str, name: &str) -> String {
    format!(
        "id: {id}\nname: {name}\ndescription: sample\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 1\n"
    )
}

fn record(id: &str, name: &str, visibility: WorkflowVisibility) -> WorkflowDefinitionRecord {
    WorkflowDefinitionRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        description: "sample".into(),
        visibility,
        definition_yaml: sample_yaml(id, name),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        updated_at: "2026-01-01T00:00:00.000Z".into(),
    }
}

fn execution_result(success: bool) -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success,
        final_output: "out".into(),
        steps: vec![StepExecutionResult {
            step_id: "q1".into(),
            step_type: "query".into(),
            status: if success {
                datazen::workflow::domain::model::StepStatus::Success
            } else {
                datazen::workflow::domain::model::StepStatus::Failed
            },
            result: Some(serde_json::json!({"rows": []})),
            execution_time_ms: 5,
            error: None,
            connection_name: None,
            sql_executed: Some("SELECT 1".into()),
        }],
        total_time_ms: 42,
        error: None,
    }
}

// ── 定义仓储 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn definitions_roundtrip_through_the_repository_port() {
    let store = store();
    let workflows = scoped_store(&store, ORG);
    let scope = scope(ORG);

    workflows
        .upsert_workflow(&scope, &record("wf-1", "One", WorkflowVisibility::User))
        .expect("upsert");

    let fetched = workflows
        .get_workflow(&scope, "wf-1")
        .expect("get")
        .expect("row exists");
    assert_eq!(fetched.name, "One");
    assert_eq!(fetched.definition_yaml, sample_yaml("wf-1", "One"));
    assert_eq!(fetched.visibility, WorkflowVisibility::User);

    // upsert 更新（同 id 重写）。
    workflows
        .upsert_workflow(&scope, &record("wf-1", "One-v2", WorkflowVisibility::User))
        .expect("upsert v2");
    let fetched = workflows
        .get_workflow(&scope, "wf-1")
        .expect("get")
        .expect("row exists");
    assert_eq!(fetched.name, "One-v2");

    // 删除后读取落空；再删（不存在）是幂等 no-op。
    workflows.delete_workflow(&scope, "wf-1").expect("delete");
    assert!(workflows
        .get_workflow(&scope, "wf-1")
        .expect("get")
        .is_none());
    workflows
        .delete_workflow(&scope, "wf-1")
        .expect("delete again");
}

#[tokio::test]
async fn visibility_filters_the_list_face() {
    let store = store();
    let workflows = scoped_store(&store, ORG);
    let scope = scope(ORG);

    workflows
        .upsert_workflow(&scope, &record("user-1", "U", WorkflowVisibility::User))
        .expect("upsert user");
    workflows
        .upsert_workflow(
            &scope,
            &record("hidden-1", "H", WorkflowVisibility::DashboardHidden),
        )
        .expect("upsert hidden");

    let user = workflows
        .list_workflows(&scope, WorkflowVisibility::User)
        .expect("list user");
    assert_eq!(user.len(), 1);
    assert_eq!(user[0].id, "user-1");

    let hidden = workflows
        .list_workflows(&scope, WorkflowVisibility::DashboardHidden)
        .expect("list hidden");
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].id, "hidden-1");
}

#[tokio::test]
async fn another_organization_is_invisible_on_every_face() {
    let store = store();
    let home = scoped_store(&store, ORG);
    let home_scope = scope(ORG);
    home.upsert_workflow(
        &home_scope,
        &record("wf-1", "One", WorkflowVisibility::User),
    )
    .expect("upsert");

    // 越组织读：不存在。
    let foreign_scope = scope(OTHER_ORG);
    assert!(home
        .get_workflow(&foreign_scope, "wf-1")
        .expect("get")
        .is_none());
    // 越组织列：空表。
    assert!(home
        .list_workflows(&foreign_scope, WorkflowVisibility::User)
        .expect("list")
        .is_empty());
    // 越组织删：no-op（数据一个字节没动）。
    home.delete_workflow(&foreign_scope, "wf-1")
        .expect("delete");
    assert!(home
        .get_workflow(&home_scope, "wf-1")
        .expect("get")
        .is_some());
    // 越组织写：拒绝（与桌面「workflow {id} not found」同族文案）。
    let error = home
        .upsert_workflow(
            &foreign_scope,
            &record("wf-x", "X", WorkflowVisibility::User),
        )
        .expect_err("must refuse");
    assert!(error.contains("not found"), "文案：{error}");
}

// ── 运行历史 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn run_history_roundtrips_and_filters_by_workflow() {
    let store = store();
    let history = ServerWorkflowHistoryStore::scoped(store.clone(), ORG);

    let now = chrono::Local::now();
    let id_a1 = new_run_id(now);
    history
        .record(
            &id_a1,
            "wf-a",
            "A",
            &serde_json::json!({"uid": "U001"}),
            &execution_result(true),
            "2026-01-01T00:00:01.000Z",
        )
        .await
        .expect("record a1");
    history
        .record(
            &new_run_id(now),
            "wf-b",
            "B",
            &serde_json::json!({}),
            &execution_result(false),
            "2026-01-01T00:00:02.000Z",
        )
        .await
        .expect("record b");
    history
        .record(
            &new_run_id(now),
            "wf-a",
            "A",
            &serde_json::json!({}),
            &execution_result(true),
            "2026-01-01T00:00:03.000Z",
        )
        .await
        .expect("record a2");

    let all = history.list(None).await.expect("list");
    assert_eq!(all.len(), 3);
    // created_at 降序（桌面同序）。
    assert_eq!(all[0].created_at, "2026-01-01T00:00:03.000Z");

    let of_a = history.list(Some("wf-a")).await.expect("list a");
    assert_eq!(of_a.len(), 2);
    assert!(of_a.iter().all(|item| item.workflow_id == "wf-a"));
    assert!(of_a.iter().all(|item| item.success));

    let detail = history.get(&id_a1).await.expect("get").expect("row");
    assert_eq!(detail.workflow_name, "A");
    assert_eq!(detail.variables, serde_json::json!({"uid": "U001"}));
    assert!(detail.result.success);

    // 未知 id → None；越组织 → None（同答，防枚举）。
    assert!(history.get("missing").await.expect("get").is_none());
    let foreign = ServerWorkflowHistoryStore::scoped(store.clone(), OTHER_ORG);
    assert!(foreign.get(&id_a1).await.expect("get").is_none());
    assert!(foreign.list(None).await.expect("list").is_empty());

    // 清空半边（按 workflow）返回行数；再清全部。
    let removed = history.clear(Some("wf-a")).await.expect("clear a");
    assert_eq!(removed, 2);
    assert_eq!(history.list(None).await.expect("list").len(), 1);
    let removed = history.clear(None).await.expect("clear all");
    assert_eq!(removed, 1);
    assert!(history.list(None).await.expect("list").is_empty());
}

#[tokio::test]
async fn run_history_truncates_beyond_the_shared_cap() {
    let store = store();
    let history = ServerWorkflowHistoryStore::scoped(store, ORG);
    let result = execution_result(true);
    let now = chrono::Local::now();
    for index in 0..(datazen::history::MAX_WORKFLOW_HISTORY + 25) {
        let id = new_run_id(now);
        history
            .record(
                &id,
                "wf-many",
                "Many",
                &serde_json::json!({}),
                &result,
                // created_at 单调递增：截断必须保最新。
                &format!("2026-01-01T00:{:02}:{:02}.000Z", index / 60, index % 60),
            )
            .await
            .expect("record");
    }
    let list = history.list(Some("wf-many")).await.expect("list");
    assert_eq!(
        list.len(),
        datazen::history::MAX_WORKFLOW_HISTORY,
        "追加后按组织截断到共享常量"
    );
    // 最旧的被淘汰：列表第一条是最新时刻。
    assert_eq!(
        list[0].created_at,
        format!(
            "2026-01-01T00:{:02}:{:02}.000Z",
            (datazen::history::MAX_WORKFLOW_HISTORY + 24) / 60,
            (datazen::history::MAX_WORKFLOW_HISTORY + 24) % 60
        )
    );
}

// ── 共享层 registry 架在本件之上 ────────────────────────────────────────────

#[tokio::test]
async fn the_shared_registry_rules_run_on_this_store() {
    let store = store();
    let workflows = Arc::new(scoped_store(&store, ORG));
    // 服务端不播内置样例（形态差异，模块头登记）：空播种表。
    let registry = WorkflowRegistry::new(workflows.clone(), Vec::new());
    let scope = workflows.scope();

    // 保存 YAML（解析 → 校验 → 持久化）→ 读回定义。
    let saved = registry
        .save_yaml(&scope, &sample_yaml("wf-y", "FromYaml"))
        .await
        .expect("save yaml");
    assert_eq!(saved.id, "wf-y");

    // 列表投影（list_item_for 的规则：scheduled = schedule.enabled）。
    let list = registry.list(&scope).await;
    assert_eq!(list.len(), 1);
    let item = &list[0];
    assert_eq!(item.id, "wf-y");
    assert!(!item.scheduled, "无 schedule 的定义 scheduled=false");

    // 调度列表（user 可见面）。
    let definitions = registry.list_definitions(&scope).await;
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].id, "wf-y");

    // id 词汇规则（共享层 validate_workflow_id）在保存路径上生效。
    let bad = registry
        .save_yaml(&scope, "id: bad id!\nname: Bad\nsteps: []\n")
        .await;
    assert!(bad.is_err(), "非法 id 必须被共享层规则拒绝");

    // 删除后列表清空。
    registry.delete(&scope, "wf-y").await.expect("delete");
    assert!(registry.list(&scope).await.is_empty());
}

/// 共享层纯规则的行为钉子（编码/列表投影/id 词表——防本件漂移用）。
#[test]
fn shared_rules_are_pinned_where_they_live() {
    assert!(validate_workflow_id("ok-id_1").is_ok());
    assert!(validate_workflow_id("bad id").is_err());

    let definition =
        datazen::workflow::domain::repository::decode_definition(&sample_yaml("wf-p", "P"))
            .expect("decode");
    let encoded = encode_definition(&definition).expect("encode");
    let decoded =
        datazen::workflow::domain::repository::decode_definition(&encoded).expect("decode");
    assert_eq!(decoded.id, "wf-p");
    assert_eq!(decoded.name, "P");

    let item = list_item_for(&definition);
    assert_eq!(item.id, "wf-p");
    assert!(!item.scheduled);
}
