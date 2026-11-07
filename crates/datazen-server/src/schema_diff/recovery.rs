//! 部署恢复的目标对象派生与目录指纹（W8 刀3b-2）。
//!
//! 桌面对照 `datazen-desktop/src/commands/schema_diff/job/recovery.rs`：
//! 两个**会话绑定**编排的服务端投影——纯投影（id 编解码/证据登记/表目标
//! 派生）复用共享层 `datazen::migration::schema_diff::job::recovery_targets`，
//! 会话解析换成 [`SessionEndpoints`]（活句柄借出路径）。
//!
//! 语义逐字对齐：表目标走 `LegacyCatalogReader::get_table_schema`（missing
//! 快照 = `{"state":"missing"}`）；对象目标走 `list_schema_objects` +
//! `fetch_schema_object`（0 命中 missing / 1 命中 DDL 快照 / 多命中
//! 歧义拒绝）；条目按规范 JSON 排序后 sha256 摘要。

use datazen::migration::schema_diff::api as schema_diff_api;
use datazen::migration::schema_diff::job::recovery_targets::table_target;
use datazen::migration::schema_diff::job::PrepareRequest;
use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_api::{CatalogReader, LegacyCatalogReader};
use sha2::{Digest, Sha256};

use super::endpoints::{EndpointError, ResolvedSession, SessionEndpoints};

/// 从准备请求派生目标侧对象清单（桌面 `prepare_target_objects` 同语义）：
/// `target_table_names` + `target_only_table_names` 全部按表目标派生；
/// Unified 额外并入 `target_objects` 原样与 `source_objects`（target 侧
/// schema 覆盖）。去重保序。
pub async fn prepare_target_objects(
    endpoints: &SessionEndpoints,
    ctx: &datazen::shared::context::RequestContext,
    request: &PrepareRequest,
) -> Result<Vec<datazen_driver_api::DatabaseObject>, EndpointError> {
    let (
        target_session,
        target_table_names,
        target_only_table_names,
        target_objects,
        source_objects,
    ) = match request {
        PrepareRequest::Table {
            target_db_session_id,
            target_table_names,
            target_only_table_names,
            ..
        } => (
            target_db_session_id.clone(),
            target_table_names.clone(),
            target_only_table_names.clone(),
            Vec::new(),
            Vec::new(),
        ),
        PrepareRequest::Unified {
            target_db_session_id,
            target_table_names,
            target_only_table_names,
            target_objects,
            source_objects,
            ..
        } => (
            target_db_session_id.clone(),
            target_table_names.clone(),
            target_only_table_names.clone(),
            target_objects.clone(),
            source_objects.clone(),
        ),
    };
    let session = endpoints
        .resolve(
            ctx,
            &target_session,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await?;
    let target_schema = match request {
        PrepareRequest::Table { target_schema, .. } => target_schema.clone(),
        PrepareRequest::Unified { target_schema, .. } => target_schema.clone(),
    }
    .or_else(|| session.config.schema.clone());

    let mut targets: Vec<datazen_driver_api::DatabaseObject> = Vec::new();
    let mut add = |object: datazen_driver_api::DatabaseObject| {
        if !targets.contains(&object) {
            targets.push(object);
        }
    };
    for table in target_table_names
        .iter()
        .chain(target_only_table_names.iter())
    {
        add(table_target(table, target_schema.as_deref()));
    }
    for object in target_objects {
        add(object);
    }
    for mut object in source_objects {
        object.target_schema = target_schema
            .as_deref()
            .map(str::to_owned)
            .or_else(|| object.schema.clone());
        add(object);
    }
    Ok(targets)
}

/// 目标侧目录指纹（桌面 `fingerprint_targets` 同语义）：逐目标读目录
/// 快照（表 → `get_table_schema`；对象 → 列表命中 + DDL），条目按规范
/// JSON 排序后 sha256。
pub async fn fingerprint_targets(
    endpoints: &SessionEndpoints,
    ctx: &datazen::shared::context::RequestContext,
    session_id: &str,
    database: Option<&str>,
    targets: &[datazen_driver_api::DatabaseObject],
) -> Result<String, EndpointError> {
    let session = endpoints
        .resolve(
            ctx,
            session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await?;
    fingerprint_session_targets(&session, database, targets).await
}

/// 指纹计算的本体（会话已解析；恢复核验的 verifier 复用——它自带会话）。
pub async fn fingerprint_session_targets(
    session: &ResolvedSession,
    database: Option<&str>,
    targets: &[datazen_driver_api::DatabaseObject],
) -> Result<String, EndpointError> {
    let database = database
        .or(session.config.database.as_deref())
        .unwrap_or_default();
    let driver_type = session.driver.driver_type();
    let catalog_database = schema_diff_api::schema_catalog_database(&driver_type, Some(database));
    let catalog = LegacyCatalogReader::new(session.driver.as_ref());
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for target in targets {
        let snapshot = if ObjectKind::parse(&target.kind) == Some(ObjectKind::Table) {
            match catalog
                .get_table_schema(
                    &session.handle,
                    &target.name,
                    catalog_database,
                    target.schema.as_deref(),
                )
                .await
            {
                Ok(schema) => serde_json::json!({ "state": "present", "schema": schema }),
                Err(error) if schema_diff_api::is_table_missing_error(&error.to_string()) => {
                    serde_json::json!({ "state": "missing" })
                }
                Err(error) => {
                    return Err(EndpointError::Unavailable(error.to_string()));
                }
            }
        } else {
            let kind = ObjectKind::parse(&target.kind).ok_or_else(|| {
                EndpointError::Unavailable(format!("unknown recovery target kind {}", target.kind))
            })?;
            let listed = schema_diff_api::list_schema_objects(
                session.driver.as_ref(),
                &session.handle,
                kind,
            )
            .await
            .map_err(|error| EndpointError::Unavailable(error.to_string()))?;
            let hits = listed
                .iter()
                .filter(|candidate| *candidate == target)
                .count();
            match hits {
                0 => serde_json::json!({ "state": "missing" }),
                1 => {
                    let snapshot = schema_diff_api::fetch_schema_object(
                        session.driver.as_ref(),
                        &session.handle,
                        target,
                    )
                    .await
                    .map_err(|error| EndpointError::Unavailable(error.to_string()))?;
                    serde_json::json!({ "state": "present", "ddl": snapshot.definition })
                }
                _ => {
                    return Err(EndpointError::Unavailable(
                        "Recovery target identity is ambiguous".into(),
                    ));
                }
            }
        };
        entries.push(serde_json::json!({ "identity": target, "snapshot": snapshot }));
    }
    entries.sort_by_key(|entry| entry.to_string());
    let bytes = serde_json::to_vec(&entries)
        .map_err(|error| EndpointError::Unavailable(error.to_string()))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
