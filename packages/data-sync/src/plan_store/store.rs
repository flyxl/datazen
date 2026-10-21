//! Sync 计划存储的**端口**与随包提供的进程内实现。
//!
//! ## 为什么不再是进程级全局
//!
//! 过去计划注册表是 `static STORE`：任何形态的宿主共享同一张表，既没法按组织
//! 隔离，也没法在测试里换一份干净实例。本模块把它改成**端口 + 宿主注入**：
//!
//! * [`SyncPlanStore`] 是端口：签发、查看、绑定端点身份、认领；
//! * [`InMemorySyncPlanStore`] 是随包提供的进程内实现（规则留在包里）；
//! * 宿主在**组装期**创建实例并注入，调用点按 [`OrganizationScope`] 访问。
//!
//! ## 跨组织访问
//!
//! 越组织访问按「**不存在**」处理（CM-05）：端口方法返回与「计划不存在」完全
//! 一样的错误，**不**返回「无权限」——后者会泄漏「存在但无权」。
//!
//! ## 认领语义
//!
//! `claim` 在写入开始前把计划从注册表里摘走。被认领的计划**即使事务结果未知也
//! 视为已消费**，UI 因此永远拿不到「重试一次也许能成」的假象。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use datazen_driver_api::DatabaseDriver;
use datazen_platform_api::migration::EndpointIdentity;
use datazen_platform_api::ports::scope::OrganizationScope;

use super::{StoredSyncPlan, SyncComparisonPreview, SyncOptions, SYNC_PLAN_TTL};

/// 一次「签发计划」的请求。
///
/// 计划 id 可由调用方指定（compare 命令在 Job 之前就铸好了 id，好让预览与
/// 冻结的 ChangeSet 指向同一个计划）；不指定时由存储铸造。
pub struct SyncPlanIssue {
    /// 调用方铸定的计划 id；`None` 表示由存储铸造。
    pub plan_id: Option<String>,
    /// 选区版本。签发时恒为 1。
    pub selection_revision: u64,
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub source_schema_fingerprint: String,
    pub target_schema_fingerprint: String,
    /// 已冻结的比较存储。
    pub comparison: super::ComparisonStore,
    pub options: SyncOptions,
    pub target_read_only_at_preview: bool,
}

/// Sync 计划存储端口。
///
/// 每个方法都带 [`OrganizationScope`]：作用域只表达**组织维度**，跨组织访问按
/// 「不存在」处理。端口不抛「无权限」错误。
pub trait SyncPlanStore: Send + Sync {
    /// 签发一份不可变计划并返回摘要预览。
    fn issue(
        &self,
        scope: &OrganizationScope,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        issue: SyncPlanIssue,
    ) -> Result<SyncComparisonPreview, String>;

    /// 查看一份尚未被认领的计划。
    fn peek(&self, scope: &OrganizationScope, id: &str) -> Result<StoredSyncPlan, String>;

    /// 把稳定的端点身份绑定到已签发的计划上。
    fn attach_endpoint_identities(
        &self,
        scope: &OrganizationScope,
        id: &str,
        source: EndpointIdentity,
        target: EndpointIdentity,
    ) -> Result<(), String>;

    /// 认领计划：把它从注册表里摘走，供一次写入独占使用。
    fn claim(&self, scope: &OrganizationScope, id: &str) -> Result<StoredSyncPlan, String>;
}

/// 进程内实现：Mutex + HashMap，带有效期。
pub struct InMemorySyncPlanStore {
    plans: Mutex<HashMap<String, StoredSyncPlan>>,
}

impl InMemorySyncPlanStore {
    pub fn new() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
        }
    }

    /// 计划注册表（仅供包内测试断言过期回收）。
    #[cfg(test)]
    pub(crate) fn plans(&self) -> &Mutex<HashMap<String, StoredSyncPlan>> {
        &self.plans
    }
}

impl Default for InMemorySyncPlanStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncPlanStore for InMemorySyncPlanStore {
    fn issue(
        &self,
        scope: &OrganizationScope,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        issue: SyncPlanIssue,
    ) -> Result<SyncComparisonPreview, String> {
        let id = issue
            .plan_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let preview_tables = issue
            .comparison
            .summaries()?
            .iter()
            .map(|table| super::summary_for_table(&id, table))
            .collect();
        let conflict_policy_fingerprint =
            super::fingerprint_conflict_policy(issue.options.conflict_policy);
        let plan = StoredSyncPlan {
            organization_id: scope.organization_id().clone(),
            source_db_session_id: issue.source_db_session_id,
            target_db_session_id: issue.target_db_session_id,
            source_endpoint_identity: None,
            target_endpoint_identity: None,
            source_database: issue.source_database,
            target_database: issue.target_database,
            source_schema: issue.source_schema,
            target_schema: issue.target_schema,
            source_driver_type: source_driver.driver_type(),
            target_driver_type: target_driver.driver_type(),
            source_driver_protocol: datazen_driver_api::driver_protocol_version(source_driver),
            target_driver_protocol: datazen_driver_api::driver_protocol_version(target_driver),
            source_schema_fingerprint: issue.source_schema_fingerprint,
            target_schema_fingerprint: issue.target_schema_fingerprint,
            comparison: issue.comparison,
            options: issue.options,
            conflict_policy_fingerprint,
            selection_revision: issue.selection_revision,
            target_read_only_at_preview: issue.target_read_only_at_preview,
            expires_at: Instant::now() + SYNC_PLAN_TTL,
        };
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let now = Instant::now();
        plans.retain(|_, existing| existing.expires_at > now);
        plans.insert(id.clone(), plan);
        Ok(SyncComparisonPreview {
            contract_version: super::SYNC_COMPARISON_CONTRACT_VERSION,
            plan_id: id,
            selection_revision: issue.selection_revision,
            tables: preview_tables,
            page_size: super::SYNC_COMPARISON_PAGE_SIZE,
        })
    }

    fn peek(&self, scope: &OrganizationScope, id: &str) -> Result<StoredSyncPlan, String> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let Some(plan) = plans.get(id) else {
            return Err(unknown_plan_message());
        };
        if plan.organization_id != *scope.organization_id() {
            // 越组织访问按「不存在」处理，不泄漏「存在但无权」。
            return Err(unknown_plan_message());
        }
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err("sync plan has expired; return to comparison".into());
        }
        Ok(plan.clone())
    }

    fn attach_endpoint_identities(
        &self,
        scope: &OrganizationScope,
        id: &str,
        source: EndpointIdentity,
        target: EndpointIdentity,
    ) -> Result<(), String> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let plan = plans.get_mut(id).ok_or_else(unknown_plan_message)?;
        if plan.organization_id != *scope.organization_id() {
            return Err(unknown_plan_message());
        }
        plan.source_endpoint_identity = Some(source);
        plan.target_endpoint_identity = Some(target);
        Ok(())
    }

    fn claim(&self, scope: &OrganizationScope, id: &str) -> Result<StoredSyncPlan, String> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let Some(plan) = plans.get(id) else {
            return Err(unknown_plan_message());
        };
        if plan.organization_id != *scope.organization_id() {
            // 跨组织的计划**不能**被摘走：它对当前组织而言不存在。
            return Err(unknown_plan_message());
        }
        let plan = plans.remove(id).ok_or_else(unknown_plan_message)?;
        if plan.expires_at <= Instant::now() {
            return Err("sync plan has expired; return to comparison".into());
        }
        Ok(plan)
    }
}

/// 「计划不存在」的唯一措辞。跨组织访问与真正不存在共用它，避免泄漏存在性。
fn unknown_plan_message() -> String {
    "sync plan is unknown or has expired; return to comparison".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison_store::{default_comparison_store_root, LocalComparisonStoreRoot};
    use crate::plan_store::{
        selected_table_page, validate_selection_streaming, ComparisonStore,
        SyncComparisonPageRequest, SyncRunSelection, SyncSelectionMode, SyncTableSelection,
    };
    use crate::{ChangeOperation, ComparisonResult, RowChange, TableResult};
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::Value;
    use datazen_platform_api::id::OrganizationId;
    use std::time::Duration;

    const TEST_ORGANIZATION: &str = "datazen-data-sync-tests";

    fn test_scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(TEST_ORGANIZATION))
    }

    /// 另一个组织：用来验证越组织访问按「不存在」处理。
    fn other_scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("datazen-data-sync-other"))
    }

    fn test_root() -> LocalComparisonStoreRoot {
        LocalComparisonStoreRoot::new(
            default_comparison_store_root(),
            OrganizationId::new(TEST_ORGANIZATION),
        )
    }

    fn comparison_store(comparison: ComparisonResult) -> ComparisonStore {
        ComparisonStore::from_comparison(comparison, &test_root(), &test_scope()).unwrap()
    }

    fn comparison() -> ComparisonResult {
        let options = SyncOptions::default();
        ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![
                RowChange::insert(
                    vec![Value::Integer(1)],
                    vec![Some(Value::Integer(1))],
                    &options,
                ),
                RowChange::insert(
                    vec![Value::Integer(2)],
                    vec![Some(Value::Integer(2))],
                    &options,
                ),
            ],
        )])
    }

    fn table_scope(
        excluded_rows: Vec<crate::plan_store::SyncSelectionExclusion>,
    ) -> SyncTableSelection {
        SyncTableSelection {
            source_table: "users".into(),
            target_table: "users".into(),
            selection_mode: SyncSelectionMode::All,
            operations: vec![ChangeOperation::Insert],
            excluded_rows,
        }
    }

    fn issue(
        store: &InMemorySyncPlanStore,
        source: &MockDriver,
        target: &MockDriver,
        comparison: ComparisonResult,
        plan_id: Option<String>,
    ) -> SyncComparisonPreview {
        store
            .issue(
                &test_scope(),
                source,
                target,
                SyncPlanIssue {
                    plan_id,
                    selection_revision: 1,
                    source_db_session_id: "source-session".into(),
                    target_db_session_id: "target-session".into(),
                    source_database: "source-db".into(),
                    target_database: "target-db".into(),
                    source_schema: None,
                    target_schema: None,
                    source_schema_fingerprint: "source-fingerprint".into(),
                    target_schema_fingerprint: "target-fingerprint".into(),
                    comparison: comparison_store(comparison),
                    options: SyncOptions::default(),
                    target_read_only_at_preview: false,
                },
            )
            .unwrap()
    }

    #[test]
    fn comparison_preview_is_summary_only_and_pages_are_cursor_bound() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let options = SyncOptions::default();
        let rows = (0..205)
            .map(|value| {
                RowChange::insert(
                    vec![Value::Integer(value)],
                    vec![Some(Value::Integer(value))],
                    &options,
                )
            })
            .collect();
        let store = InMemorySyncPlanStore::new();
        let preview = issue(
            &store,
            &source,
            &target,
            ComparisonResult::new(vec![TableResult::matched("users", "users", rows)]),
            None,
        );
        let summary = &preview.tables[0];
        assert_eq!(summary.insert_count, 205);
        assert_eq!(summary.row_count, 205);
        assert!(summary.first_cursor.is_some());

        let first = super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id.clone(),
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: summary.first_cursor.clone(),
                limit: Some(2),
            },
        )
        .unwrap();
        assert_eq!(first.rows.len(), 2);
        assert!(first.has_more);
        assert_ne!(first.next_cursor, first.cursor);
        let repeated = super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id.clone(),
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: first.cursor.clone(),
                limit: Some(2),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&repeated.rows[0].key).unwrap(),
            serde_json::to_string(&first.rows[0].key).unwrap()
        );

        let forged = super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id.clone(),
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: Some("v1.999.not-a-valid-signature".into()),
                limit: Some(2),
            },
        );
        assert!(forged.is_err());
        assert!(super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id.clone(),
                source_table: "unknown".into(),
                target_table: "users".into(),
                cursor: None,
                limit: Some(2),
            },
        )
        .is_err());
        assert!(super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id,
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: None,
                limit: Some(0),
            },
        )
        .is_err());
    }

    #[test]
    fn comparison_page_rejects_claimed_plan_and_overlarge_limit() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let store = InMemorySyncPlanStore::new();
        let preview = issue(
            &store,
            &source,
            &target,
            ComparisonResult::new(vec![comparison().tables[0].clone()]),
            None,
        );
        assert!(super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id.clone(),
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: None,
                limit: Some(super::super::SYNC_COMPARISON_PAGE_MAX_LIMIT + 1),
            },
        )
        .is_err());
        let claimed = store.claim(&test_scope(), &preview.plan_id).unwrap();
        drop(claimed);
        assert!(super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id,
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: None,
                limit: Some(1),
            },
        )
        .is_err());
    }

    #[test]
    fn indexed_plan_summary_and_page_do_not_call_full_load() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large_row = RowChange::insert(
            vec![Value::Integer(7)],
            vec![Some(Value::String("x".repeat(
                crate::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
            )))],
            &SyncOptions::default(),
        );
        let store = InMemorySyncPlanStore::new();
        let preview = issue(
            &store,
            &source,
            &target,
            ComparisonResult::new(vec![
                TableResult::matched("users", "users", vec![large_row]),
                TableResult::matched(
                    "orders",
                    "orders",
                    vec![RowChange::insert(
                        vec![Value::Integer(8)],
                        vec![Some(Value::Integer(8))],
                        &SyncOptions::default(),
                    )],
                ),
            ]),
            None,
        );
        let plan = store.peek(&test_scope(), &preview.plan_id).unwrap();
        assert!(plan.comparison.is_spilled());
        assert_eq!(preview.tables[0].row_count, 1);
        let before = plan.comparison.full_load_calls();
        let page = super::super::get_comparison_page(
            &store,
            &test_scope(),
            SyncComparisonPageRequest {
                plan_id: preview.plan_id,
                source_table: "users".into(),
                target_table: "users".into(),
                cursor: None,
                limit: Some(1),
            },
        )
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(plan.comparison.full_load_calls(), before);
    }

    #[test]
    fn streaming_selection_reads_spilled_rows_in_pages_without_full_load() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let options = SyncOptions::default();
        let payload = "x".repeat(crate::comparison_store::COMPARISON_MEMORY_LIMIT + 1);
        let mut rows = Vec::with_capacity(501);
        rows.push(RowChange::insert(
            vec![Value::Integer(0)],
            vec![Some(Value::String(payload))],
            &options,
        ));
        rows.extend((1..=500).map(|key| {
            RowChange::insert(
                vec![Value::Integer(key)],
                vec![Some(Value::Integer(key))],
                &options,
            )
        }));
        let comparison = ComparisonResult::new(vec![TableResult::matched("users", "users", rows)]);
        let store = InMemorySyncPlanStore::new();
        let preview = store
            .issue(
                &test_scope(),
                source.as_ref(),
                target.as_ref(),
                SyncPlanIssue {
                    plan_id: Some("plan-stream".into()),
                    selection_revision: 1,
                    source_db_session_id: "source-session".into(),
                    target_db_session_id: "target-session".into(),
                    source_database: "source-db".into(),
                    target_database: "target-db".into(),
                    source_schema: None,
                    target_schema: None,
                    source_schema_fingerprint: "source-fingerprint".into(),
                    target_schema_fingerprint: "target-fingerprint".into(),
                    comparison: comparison_store(comparison),
                    options: options.clone(),
                    target_read_only_at_preview: false,
                },
            )
            .unwrap();
        let plan = store.peek(&test_scope(), &preview.plan_id).unwrap();
        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![table_scope(Vec::new())],
        };
        let matcher = validate_selection_streaming(&plan.comparison, &selection, &options).unwrap();
        assert_eq!(plan.comparison.full_load_calls(), 0);
        let table = plan
            .comparison
            .summaries()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let rows = plan
            .comparison
            .load_table_page("users", "users", 500, 1)
            .unwrap();
        let page = selected_table_page(&table, rows, &matcher, &options)
            .unwrap()
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(matches!(page.rows[0].key.as_slice(), [Value::Integer(500)]));
        assert_eq!(plan.comparison.full_load_calls(), 0);
    }

    #[test]
    fn expired_plan_drops_spilled_comparison_store() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large = ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::String("x".repeat(
                    crate::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
                )))],
                &SyncOptions::default(),
            )],
        )]);
        let store = InMemorySyncPlanStore::new();
        let preview = issue(&store, &source, &target, large, None);
        let path = {
            let plan = store.peek(&test_scope(), &preview.plan_id).unwrap();
            plan.comparison.path().unwrap()
        };
        assert!(path.exists());
        store
            .plans()
            .lock()
            .unwrap()
            .get_mut(&preview.plan_id)
            .unwrap()
            .expires_at = Instant::now() - Duration::from_secs(1);
        assert!(store.peek(&test_scope(), &preview.plan_id).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn claiming_plan_removes_owner_and_cleans_file_after_claimed_handle_drops() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large = ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::String("x".repeat(
                    crate::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
                )))],
                &SyncOptions::default(),
            )],
        )]);
        let store = InMemorySyncPlanStore::new();
        let preview = issue(&store, &source, &target, large, None);
        let claimed = store.claim(&test_scope(), &preview.plan_id).unwrap();
        let path = claimed.comparison.path().unwrap();
        assert!(store.peek(&test_scope(), &preview.plan_id).is_err());
        assert!(path.exists());
        drop(claimed);
        assert!(!path.exists());
    }

    /// CM-05：越组织访问按「不存在」处理——措辞与真正不存在完全一致，
    /// 既不返回「无权限」，也不泄漏「存在但无权」。
    #[test]
    fn another_organization_sees_no_plan_at_all() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let store = InMemorySyncPlanStore::new();
        let preview = issue(&store, &source, &target, comparison(), None);

        let missing = store.peek(&other_scope(), "no-such-plan").unwrap_err();
        let cross = store.peek(&other_scope(), &preview.plan_id).unwrap_err();
        assert_eq!(cross, missing, "越组织访问必须与「不存在」同词");
        assert!(!cross.to_lowercase().contains("permission"));
        assert!(!cross.to_lowercase().contains("forbidden"));

        assert!(store.claim(&other_scope(), &preview.plan_id).is_err());
        assert!(store
            .attach_endpoint_identities(
                &other_scope(),
                &preview.plan_id,
                endpoint_identity("source-connection"),
                endpoint_identity("target-connection"),
            )
            .is_err());
        // 越组织的认领不能把计划摘走：它对当前组织依然可见。
        assert!(store.peek(&test_scope(), &preview.plan_id).is_ok());
    }

    fn endpoint_identity(connection_id: &str) -> EndpointIdentity {
        EndpointIdentity {
            connection_id: datazen_platform_api::id::ConnectionId::new(connection_id),
            service_key: format!("service:{connection_id}"),
        }
    }
}
