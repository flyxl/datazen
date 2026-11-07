//! Workflow 运行时双表的服务端 SQLite 适配器（W8 刀3c-1；契约 §6.14 的
//! 存储半边）。
//!
//! 词汇（[`WorkflowDefinitionRecord`] / [`WorkflowRunRecord`] /
//! [`WorkflowRunListItem`]）与登记规则（保存/列表投影/编码解码）唯一定义
//! 在共享层 `datazen::workflow`（`domain::repository` / `domain::history`）；
//! 本件只负责 v13 双表的行存取与隔离边界：
//!
//! - **组织级隔离**（CM-05）：所有查询按 `organization_id` 限定，越组织
//!   读取落空、写只落本组织（跨组织删除与「删除不存在的 id」同样按
//!   no-op 处理——端口契约注释的不可区分口径）。
//! - **`WorkflowDefinitionRepository` 的服务端实现**：trait 归共享层
//!   `workflow::domain::repository`，宿主（桌面 SQLite / 本件）提供行。
//!   共享层 `WorkflowRegistry` 的全部登记规则（visibility 过滤、YAML
//!   编码、列表投影）直接架在本件之上，不复制。
//! - **运行历史四操作**（record/list/get/clear）：记录整档 JSON 落库
//!   （结果不投影成列，桌面同款裁决）；追加后按组织截断到
//!   [`MAX_WORKFLOW_HISTORY`]（100，共享层常量）。id 由调用方铸造
//!   （共享层 `new_run_id`——本地时间戳 + 短 nonce，肉眼可排序）。
//! - **删除语义**：桌面在 `AppDb.delete_workflow` 处核 dashboard widget
//!   引用（`WorkflowInUse`）；服务端 dashboard 域（刀3d）落库前无引用
//!   可核——本件的删除不带引用闸，台账与迁移头均已登记。
//! - **异步边界**：与 `storage::query_assets` 同款——同步核心 +
//!   `spawn_blocking` 包装。

use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use datazen::shared::scope::OrganizationScope;
use datazen::workflow::domain::history::{WorkflowRunListItem, WorkflowRunRecord};
use datazen::workflow::domain::model::{WorkflowExecutionResult, WorkflowVisibility};
use datazen::workflow::registry::{WorkflowDefinitionRecord, WorkflowDefinitionRepository};

use crate::storage::AdminStore;

/// 存储面错误（路由读面 → 503 / 400 映射在域路由；派生器 → PortError）。
#[derive(Debug, thiserror::Error)]
pub enum WorkflowStoreError {
    #[error("workflow store: {0}")]
    Backend(String),
}

fn backend(error: impl std::fmt::Display) -> WorkflowStoreError {
    WorkflowStoreError::Backend(error.to_string())
}

fn to_visibility(raw: &str) -> WorkflowVisibility {
    match raw {
        "dashboardHidden" => WorkflowVisibility::DashboardHidden,
        _ => WorkflowVisibility::User,
    }
}

fn from_visibility(visibility: WorkflowVisibility) -> &'static str {
    match visibility {
        WorkflowVisibility::User => "user",
        WorkflowVisibility::DashboardHidden => "dashboardHidden",
    }
}

// ── 定义仓储（WorkflowDefinitionRepository 的服务端实现） ──────────────────

/// v13 `workflows` 表的存取（组织作用域由构造参数携带）。
#[derive(Clone)]
pub struct ServerWorkflowStore {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerWorkflowStore {
    /// 按组织边界构造（HTTP handler 按认证链事实逐请求装配；调度器按
    /// 服务配置的组织装配）。
    pub fn scoped(store: Arc<AdminStore>, organization_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
        }
    }

    /// 本边界的组织作用域（共享层 `WorkflowRegistry` 的调用参数）。
    pub fn scope(&self) -> OrganizationScope {
        OrganizationScope::new(datazen::shared::id::OrganizationId::new(
            self.organization_id.clone(),
        ))
    }

    /// 裸读一条定义行（派生授权单用——不经 registry 的解析半边，YAML
    /// 原文进派生函数）。
    pub async fn raw_definition(
        &self,
        workflow_id: &str,
    ) -> Result<Option<(String, String)>, WorkflowStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let id = workflow_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            raw_definition_sync(&conn, &org, &id)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }
}

fn raw_definition_sync(
    conn: &Connection,
    org: &str,
    workflow_id: &str,
) -> Result<Option<(String, String)>, WorkflowStoreError> {
    // (definition_yaml, visibility) —— 派生器只吃定义原文；visibility
    // 用于调度列表过滤（dashboardHidden 不进调度面）。
    conn.query_row(
        "SELECT definition_yaml, visibility FROM workflows
         WHERE organization_id = ?1 AND id = ?2",
        params![org, workflow_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(backend)
}

fn get_sync(
    conn: &Connection,
    org: &str,
    id: &str,
) -> Result<Option<WorkflowDefinitionRecord>, WorkflowStoreError> {
    conn.query_row(
        "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
         FROM workflows WHERE organization_id = ?1 AND id = ?2",
        params![org, id],
        |row| {
            Ok(WorkflowDefinitionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                visibility: to_visibility(&row.get::<_, String>(3)?),
                definition_yaml: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(backend)
}

fn list_sync(
    conn: &Connection,
    org: &str,
    visibility: WorkflowVisibility,
) -> Result<Vec<WorkflowDefinitionRecord>, WorkflowStoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
             FROM workflows WHERE organization_id = ?1 AND visibility = ?2
             ORDER BY updated_at DESC, id",
        )
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![org, from_visibility(visibility)], |row| {
            Ok(WorkflowDefinitionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                visibility: to_visibility(&row.get::<_, String>(3)?),
                definition_yaml: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(backend)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(backend)?);
    }
    Ok(records)
}

impl WorkflowDefinitionRepository for ServerWorkflowStore {
    fn get_workflow(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<Option<WorkflowDefinitionRecord>, String> {
        // 组织作用域不一致 ⇒ 不存在（CM-05：越组织按「不存在」处理）。
        if scope.organization_id().as_str() != self.organization_id {
            return Ok(None);
        }
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let id = id.to_owned();
        // 端口是同步签名（共享层 registry 同步消费）；本件是薄行存取，
        // SQLite 单行读的阻塞面可接受（与桌面 AppDb 同一形态）。
        let conn = store.lock_for_adapter().map_err(|e| e.to_string())?;
        get_sync(&conn, &org, &id).map_err(|e| e.to_string())
    }

    fn list_workflows(
        &self,
        scope: &OrganizationScope,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String> {
        if scope.organization_id().as_str() != self.organization_id {
            return Ok(Vec::new());
        }
        let conn = self.store.lock_for_adapter().map_err(|e| e.to_string())?;
        list_sync(&conn, &self.organization_id, visibility).map_err(|e| e.to_string())
    }

    fn upsert_workflow(
        &self,
        scope: &OrganizationScope,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), String> {
        if scope.organization_id().as_str() != self.organization_id {
            // 与桌面同一口径：越组织写入按「目标不存在」拒绝（错误文案
            // 与桌面 `workflow {id} not found` 同族）。
            return Err(format!("workflow {} not found", record.id));
        }
        let conn = self.store.lock_for_adapter().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO workflows (
                organization_id, id, name, description, visibility,
                definition_yaml, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (organization_id, id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                visibility = excluded.visibility,
                definition_yaml = excluded.definition_yaml,
                updated_at = excluded.updated_at",
            params![
                self.organization_id,
                record.id,
                record.name,
                record.description,
                from_visibility(record.visibility),
                record.definition_yaml,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String> {
        // 跨组织删除与删除不存在的 id 一样 no-op（端口契约注释的
        // 不可区分口径）；服务端 dashboard 引用闸随刀3d 落库时补。
        if scope.organization_id().as_str() != self.organization_id {
            return Ok(());
        }
        let conn = self.store.lock_for_adapter().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM workflows WHERE organization_id = ?1 AND id = ?2",
            params![self.organization_id, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ── 运行历史（v13 workflow_run_history 表） ─────────────────────────────────

/// 运行历史的 scoped 存取（组织边界由构造参数携带）。
#[derive(Clone)]
pub struct ServerWorkflowHistoryStore {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerWorkflowHistoryStore {
    /// 按组织边界构造。
    pub fn scoped(store: Arc<AdminStore>, organization_id: &str) -> Self {
        Self {
            store,
            organization_id: organization_id.to_owned(),
        }
    }

    /// 追加一条运行记录（id 已由调用方经共享层 `new_run_id` 铸造）。
    /// 追加后按组织截断到 [`MAX_WORKFLOW_HISTORY`]。
    pub async fn record(
        &self,
        id: &str,
        workflow_id: &str,
        workflow_name: &str,
        variables: &serde_json::Value,
        result: &WorkflowExecutionResult,
        created_at: &str,
    ) -> Result<(), WorkflowStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let row = (
            id.to_owned(),
            workflow_id.to_owned(),
            workflow_name.to_owned(),
            serde_json::to_string(variables).map_err(backend)?,
            serde_json::to_string(result).map_err(backend)?,
            created_at.to_owned(),
        );
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            record_sync(&conn, &org, &row)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 运行列表（`workflow_id` 缺省 = 全部；`created_at` 降序，桌面同序）。
    pub async fn list(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<WorkflowRunListItem>, WorkflowStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let workflow = workflow_id.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            list_runs_sync(&conn, &org, workflow.as_deref())
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 单条运行记录；未知/越组织 → `None`。
    pub async fn get(
        &self,
        history_id: &str,
    ) -> Result<Option<WorkflowRunRecord>, WorkflowStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let id = history_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            get_run_sync(&conn, &org, &id)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }

    /// 清空（`workflow_id` 缺省 = 全部）；返回移除行数（调用方据此拒绝
    /// no-op 的成功，桌面同语义）。
    pub async fn clear(&self, workflow_id: Option<&str>) -> Result<usize, WorkflowStoreError> {
        let store = self.store.clone();
        let org = self.organization_id.clone();
        let workflow = workflow_id.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let conn = store.lock_for_adapter().map_err(backend)?;
            let removed = match workflow.as_deref() {
                Some(workflow_id) => conn
                    .execute(
                        "DELETE FROM workflow_run_history
                         WHERE organization_id = ?1 AND workflow_id = ?2",
                        params![org, workflow_id],
                    )
                    .map_err(backend)?,
                None => conn
                    .execute(
                        "DELETE FROM workflow_run_history WHERE organization_id = ?1",
                        params![org],
                    )
                    .map_err(backend)?,
            };
            Ok(removed)
        })
        .await
        .map_err(|error| backend(error.to_string()))?
    }
}

type RunRow = (String, String, String, String, String, String);

fn record_sync(conn: &Connection, org: &str, row: &RunRow) -> Result<(), WorkflowStoreError> {
    conn.execute(
        "INSERT INTO workflow_run_history (
            organization_id, id, workflow_id, workflow_name,
            variables_json, result_json, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![org, row.0, row.1, row.2, row.3, row.4, row.5],
    )
    .map_err(backend)?;
    // 截断：按 created_at 升序淘汰最旧的超出部分（与桌面
    // trim_workflow_history 同语义——保最新 MAX_WORKFLOW_HISTORY 条）。
    conn.execute(
        "DELETE FROM workflow_run_history
         WHERE organization_id = ?1 AND id IN (
            SELECT id FROM workflow_run_history
            WHERE organization_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT -1 OFFSET ?2
         )",
        params![org, datazen::history::MAX_WORKFLOW_HISTORY],
    )
    .map_err(backend)?;
    Ok(())
}

fn list_runs_sync(
    conn: &Connection,
    org: &str,
    workflow_id: Option<&str>,
) -> Result<Vec<WorkflowRunListItem>, WorkflowStoreError> {
    let map_item = |row: &rusqlite::Row<'_>| -> rusqlite::Result<WorkflowRunListItem> {
        let result_json: String = row.get(3)?;
        let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok(WorkflowRunListItem {
            id: row.get(0)?,
            workflow_id: row.get(1)?,
            workflow_name: row.get(2)?,
            success: result.success,
            total_time_ms: result.total_time_ms,
            created_at: row.get(4)?,
        })
    };
    let mut items = Vec::new();
    match workflow_id {
        Some(workflow_id) => {
            let mut stmt = conn
                .prepare(
                    "SELECT id, workflow_id, workflow_name, result_json, created_at
                     FROM workflow_run_history
                     WHERE organization_id = ?1 AND workflow_id = ?2
                     ORDER BY created_at DESC, id DESC",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![org, workflow_id], map_item)
                .map_err(backend)?;
            for row in rows {
                items.push(row.map_err(backend)?);
            }
        }
        None => {
            let mut stmt = conn
                .prepare(
                    "SELECT id, workflow_id, workflow_name, result_json, created_at
                     FROM workflow_run_history
                     WHERE organization_id = ?1
                     ORDER BY created_at DESC, id DESC",
                )
                .map_err(backend)?;
            let rows = stmt.query_map(params![org], map_item).map_err(backend)?;
            for row in rows {
                items.push(row.map_err(backend)?);
            }
        }
    }
    Ok(items)
}

fn get_run_sync(
    conn: &Connection,
    org: &str,
    history_id: &str,
) -> Result<Option<WorkflowRunRecord>, WorkflowStoreError> {
    conn.query_row(
        "SELECT id, workflow_id, workflow_name, variables_json, result_json, created_at
         FROM workflow_run_history WHERE organization_id = ?1 AND id = ?2",
        params![org, history_id],
        |row| {
            let result_json: String = row.get(4)?;
            let result: WorkflowExecutionResult = serde_json::from_str(&result_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            let variables_json: String = row.get(3)?;
            let variables: serde_json::Value = serde_json::from_str(&variables_json)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok(WorkflowRunRecord {
                id: row.get(0)?,
                workflow_id: row.get(1)?,
                workflow_name: row.get(2)?,
                variables,
                result,
                created_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(backend)
}
