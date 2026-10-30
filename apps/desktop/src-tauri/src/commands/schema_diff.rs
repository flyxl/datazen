//! Schema Diff Deploy IPC commands.
//!
//! host-boundary F4c-3 之后，目录/快照读取、计划准备编排、复核消费与
//! 部署前验证、单表对比全部下沉 `datazen::migration::schema_diff::api`；
//! 本层只剩传输胶水（会话/作用域解析、历史记账、JobRuntime 接线）与
//! 两个端口的桌面实现。
//!
//! * [`profile_commands`] — Schema Diff profile IPC commands.
//! * [`plan_commands`] — table-level plan preparation（薄委托）.
//! * [`deploy_commands`] — deploy execution and cancellation（薄委托）.
//! * [`compare_commands`] — single-table schema comparison（薄委托）.
//! * [`reviewed_repository`] — reviewed-plan storage port implementation (desktop, single tenant).
//!
//! [`job`] and [`unified_plan`] keep their own subtrees.

pub mod job;
pub mod unified_plan;

mod compare_commands;
mod deploy_commands;
mod plan_commands;
mod profile_commands;
pub(crate) mod reviewed_repository;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use super::error::CommandError;
use super::AppState;
use crate::schema_diff::deploy::{
    execute_schema_diff_deploy_at as run_schema_diff_deploy, plan_has_destructive, DeployOptions,
    DESTRUCTIVE_CONFIRM_TOKEN,
};
use crate::schema_diff::types::{ColumnTypeOverride, SchemaDiffDeployResult, SchemaDiffPlan};
use datazen_driver_api::SqlTarget;
use tauri::State;

pub use datazen::migration::schema_diff::api as schema_diff_api;
pub(crate) use datazen::migration::schema_diff::api::{
    fetch_schema_object, is_table_missing_error, list_schema_objects, resolve_profile_table,
    schema_catalog_database,
};
use datazen::migration::schema_diff::api::{
    ComparisonPreviewHost, ComparisonSyncAdapters, PlanSyncAdapters, SchemaDiffAdapterHost,
    SchemaDiffEndpoint,
};
pub(crate) use reviewed_repository::reviewed_scope;

// Re-export the command surface so that `crate::commands::<command>` keeps
// resolving and `generate_handler!` can still reach the `__cmd__*` macros the
// `#[tauri::command]` attribute generates in each submodule.
pub use compare_commands::*;
pub use deploy_commands::*;
pub use plan_commands::*;
pub use profile_commands::*;

// ---------------------------------------------------------------- F4c-3 端口

/// sync 适配器端口：桌面挂 `state.sync_adapters`（data_transfer 注册表）。
///
/// 错误文本与下沉前宿主内联实现逐字节一致（`ensure_pair` 的注册表文本、
/// `missing source/target sync adapter`），经 api 折叠为 Validation。
pub(crate) struct AppStateSchemaDiffAdapters<'a> {
    state: &'a AppState,
}

impl SchemaDiffAdapterHost for AppStateSchemaDiffAdapters<'_> {
    fn resolve_plan_pair(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Result<PlanSyncAdapters, String> {
        self.state
            .sync_adapters
            .ensure_pair(source_type, target_type)?;
        let source = self
            .state
            .sync_adapters
            .get_source(source_type)
            .ok_or_else(|| "missing source sync adapter".to_string())?;
        let target = self
            .state
            .sync_adapters
            .get_target(target_type)
            .ok_or_else(|| "missing target sync adapter".to_string())?;
        Ok(PlanSyncAdapters { source, target })
    }

    fn comparison_adapters(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Option<ComparisonSyncAdapters> {
        self.state
            .sync_adapters
            .ensure_pair(source_type, target_type)
            .ok()?;
        Some(ComparisonSyncAdapters {
            source_of_source: self.state.sync_adapters.get_source(source_type)?,
            source_of_target: self.state.sync_adapters.get_source(target_type)?,
            target_of_source: self.state.sync_adapters.get_target(source_type)?,
            target_of_target: self.state.sync_adapters.get_target(target_type)?,
        })
    }
}

/// 对比预览端口：DDL 渲染与全类型读取由 data_transfer 提供，宿主薄转发。
pub(crate) struct DesktopComparisonPreviewHost;

#[async_trait::async_trait]
impl ComparisonPreviewHost for DesktopComparisonPreviewHost {
    async fn full_column_types(
        &self,
        adapter: &dyn datazen_driver_api::sync::SyncSourceAdapter,
        driver: &dyn datazen_driver_api::DatabaseDriver,
        handle: &datazen_driver_api::ConnectionHandle,
        table: &str,
    ) -> Option<HashMap<String, String>> {
        crate::transfer::full_types::fetch_full_column_types(adapter, driver, handle, table)
            .await
            .ok()
    }

    fn create_table_ddl(
        &self,
        ir_table: &datazen_driver_api::sync::IRTable,
        target: &dyn datazen_driver_api::sync::SyncTargetAdapter,
    ) -> String {
        crate::transfer::ddl::build_create_table_ddl(ir_table, target)
    }
}

/// 从已解析的运行时会话构造 api 端点。
pub(crate) fn schema_diff_endpoint(
    context: crate::services::migration_endpoint::RuntimeSessionContext,
) -> SchemaDiffEndpoint {
    SchemaDiffEndpoint {
        driver: context.driver,
        handle: context.handle,
        config: context.config,
    }
}

/// Read one persisted connection through the application aggregate.
///
/// Schema Diff commands often start from a runtime session and then need the
/// persisted connection configuration for read-only/capability checks. Keep
/// that owner lookup in one host-neutral port path instead of reaching into the
/// desktop store from each command subtree.
pub(crate) async fn persisted_connection(
    state: &AppState,
    connection_id: &str,
    command: &str,
) -> Result<Option<datazen_driver_api::ConnectionConfig>, CommandError> {
    let scope = state.connection_scope()?;
    let service = crate::platform::ipc::require_connection_service(state)?;
    service.get(&scope, connection_id).await.map_err(|error| {
        let err = match error {
            datazen::connection::ConnectionError::NotFound(entity) => {
                CommandError::NotFound(entity)
            }
            other => CommandError::Internal(other.to_string()),
        };
        let redacted = datazen_infra::redaction::redact_secrets_for_log(&err.to_string());
        tracing::error!(command, error = %redacted);
        err
    })
}

/// 供薄委托层使用的端口对（适配器 + 对比预览）。
pub(crate) fn adapter_ports(state: &AppState) -> AppStateSchemaDiffAdapters<'_> {
    AppStateSchemaDiffAdapters { state }
}

/// 对比预览端口的无状态桌面实现。
pub(crate) fn comparison_preview_host() -> DesktopComparisonPreviewHost {
    DesktopComparisonPreviewHost
}
