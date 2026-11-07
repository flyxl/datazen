//! Schema Diff 恢复目标的宿主侧接线。
//!
//! B2 schema_diff 域刀：**纯**投影函数（目标线 id 编解码、证据记录、表目标
//! 派生）已下沉 `datazen::migration::schema_diff::job::recovery_targets`
//! （线格式契约与实现逐字不变，测试也随迁 modules）；本文件保留**会话绑定**
//! 的两个编排——[`prepare_target_objects`]（解析运行时会话后按请求派生目标集）
//! 与 [`fingerprint_targets`]（解析会话后逐目标读结构/DDL 并计算 sha256 指纹）。
//! 它们解析运行时会话、映射宿主错误信封，因此留宿主；纯投影经
//! `pub(super) use` 原名再导出，域内调用点（`job.rs` / `job/desktop.rs`）不变。

use datazen_driver_api::{CatalogReader, LegacyCatalogReader};
use sha2::{Digest as _, Sha256};

use crate::commands::error::CommandError;
use crate::commands::schema_diff::{
    fetch_schema_object, is_table_missing_error, list_schema_objects, schema_catalog_database,
};
use crate::commands::AppState;
use crate::schema_diff::job::PrepareRequest;

// 纯投影：形态中立，随 B2 域刀下沉 modules（recovery_targets），此处原名再导出。
pub(super) use datazen::migration::schema_diff::job::recovery_targets::{
    decode_target_id, encode_target_id, record_target_objects, table_target,
    target_ids_from_evidence, targets_from_evidence,
};

pub(super) async fn prepare_target_objects(
    state: &AppState,
    request: &PrepareRequest,
) -> Result<Vec<datazen_driver_api::DatabaseObject>, CommandError> {
    let target_session = match request {
        PrepareRequest::Table {
            target_db_session_id,
            ..
        }
        | PrepareRequest::Unified {
            target_db_session_id,
            ..
        } => target_db_session_id,
    };
    let context = crate::services::migration_endpoint::runtime_session_context(
        state,
        target_session,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let config = context.config;
    let target_schema = match request {
        PrepareRequest::Table { target_schema, .. }
        | PrepareRequest::Unified { target_schema, .. } => target_schema.as_deref(),
    }
    .or(config.schema.as_deref());
    let mut targets = Vec::new();
    let mut add = |object: datazen_driver_api::DatabaseObject| {
        if !targets.contains(&object) {
            targets.push(object);
        }
    };
    match request {
        PrepareRequest::Table {
            target_table_names,
            target_only_table_names,
            ..
        }
        | PrepareRequest::Unified {
            target_table_names,
            target_only_table_names,
            ..
        } => {
            for table in target_table_names.iter().chain(target_only_table_names) {
                add(table_target(table, target_schema));
            }
        }
    }
    if let PrepareRequest::Unified {
        source_objects,
        target_objects,
        ..
    } = request
    {
        for object in target_objects {
            add(object.clone());
        }
        for object in source_objects {
            let mut target = object.clone();
            target.schema = target_schema
                .map(str::to_owned)
                .or_else(|| object.schema.clone());
            add(target);
        }
    }
    Ok(targets)
}

pub(super) async fn fingerprint_targets(
    state: &AppState,
    session_id: &str,
    database: Option<&str>,
    targets: &[datazen_driver_api::DatabaseObject],
) -> Result<String, CommandError> {
    let context = crate::services::migration_endpoint::runtime_session_context(
        state,
        session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let driver = context.driver;
    let handle = context.handle;
    let config = context.config;
    let database = database.or(config.database.as_deref()).unwrap_or_default();
    let driver_type = driver.driver_type();
    let catalog_database = schema_catalog_database(&driver_type, Some(database));
    // 表结构读取走窄契约；`driver_type()` 仍是驱动专属规则。
    let catalog = LegacyCatalogReader::new(driver.as_ref());
    let mut entries = Vec::with_capacity(targets.len());
    for target in targets {
        if datazen_driver_api::ObjectKind::parse(&target.kind)
            == Some(datazen_driver_api::ObjectKind::Table)
        {
            let snapshot = match catalog
                .get_table_schema(
                    &handle,
                    &target.name,
                    catalog_database,
                    target.schema.as_deref(),
                )
                .await
            {
                Ok(schema) => serde_json::json!({ "state": "present", "schema": schema }),
                Err(error) if is_table_missing_error(&error.to_string()) => {
                    serde_json::json!({ "state": "missing" })
                }
                Err(error) => return Err(CommandError::Driver(error)),
            };
            entries.push(serde_json::json!({ "identity": target, "snapshot": snapshot }));
            continue;
        }

        let kind = datazen_driver_api::ObjectKind::parse(&target.kind).ok_or_else(|| {
            CommandError::Validation("Recovery target kind is unsupported".into())
        })?;
        let listed = list_schema_objects(driver.as_ref(), &handle, kind).await?;
        let matches = listed
            .iter()
            .filter(|candidate| *candidate == target)
            .collect::<Vec<_>>();
        let snapshot = match matches.as_slice() {
            [] => serde_json::json!({ "state": "missing" }),
            [object] => {
                let snapshot = fetch_schema_object(driver.as_ref(), &handle, object).await?;
                serde_json::json!({ "state": "present", "ddl": snapshot.definition })
            }
            _ => {
                return Err(CommandError::Validation(
                    "Recovery target identity is ambiguous".into(),
                ));
            }
        };
        entries.push(serde_json::json!({ "identity": target, "snapshot": snapshot }));
    }
    entries.sort_by_key(|entry| serde_json::to_string(entry).unwrap_or_default());
    let bytes = serde_json::to_vec(&entries)
        .map_err(|_| CommandError::Internal("Schema Diff target fingerprint failed".into()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
