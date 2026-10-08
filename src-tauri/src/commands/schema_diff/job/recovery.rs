use datazen_driver_api::{DatabaseObject, ObjectKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::error::CommandError;
use crate::commands::schema_diff::{
    fetch_schema_object, is_table_missing_error, list_schema_objects, schema_catalog_database,
};
use crate::commands::AppState;
use crate::schema_diff::job::PrepareRequest;

const TARGET_ID_PREFIX: &str = "obj-";
const TARGET_EVIDENCE_PREFIX: &str = "recoveryObject:";
const TARGET_JSON_PREFIX: &str = "recoveryIdentity:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryTargetEnvelope {
    pub connection_id: String,
    pub object_ids: Vec<String>,
}

pub(super) fn encode_target_id(target: &DatabaseObject) -> Option<String> {
    let json = serde_json::to_vec(target).ok()?;
    let mut id = String::with_capacity(TARGET_ID_PREFIX.len() + json.len() * 2);
    id.push_str(TARGET_ID_PREFIX);
    for byte in json {
        use std::fmt::Write;
        write!(&mut id, "{byte:02x}").ok()?;
    }
    (id.len() <= 512).then_some(id)
}

pub(super) fn decode_target_id(id: &str) -> Option<DatabaseObject> {
    let encoded = id.strip_prefix(TARGET_ID_PREFIX)?;
    if encoded.is_empty()
        || encoded.len() % 2 != 0
        || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(digits, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    serde_json::from_slice(&bytes).ok()
}

pub(super) fn target_ids_from_evidence(evidence: &[String]) -> Vec<String> {
    evidence
        .iter()
        .filter_map(|item| item.strip_prefix(TARGET_EVIDENCE_PREFIX).map(str::to_owned))
        .collect()
}

pub(super) fn targets_from_evidence(evidence: &[String]) -> Option<Vec<DatabaseObject>> {
    let raw = evidence
        .iter()
        .find_map(|item| item.strip_prefix(TARGET_JSON_PREFIX))?;
    serde_json::from_str(raw).ok()
}

pub(super) async fn prepare_target_objects(
    state: &AppState,
    request: &PrepareRequest,
) -> Result<Vec<DatabaseObject>, CommandError> {
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
    let config = state
        .connection_manager
        .get_session_config(target_session)
        .await
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    let target_schema = match request {
        PrepareRequest::Table { target_schema, .. }
        | PrepareRequest::Unified { target_schema, .. } => target_schema.as_deref(),
    }
    .or(config.schema.as_deref());
    let mut targets = Vec::new();
    let mut add = |object: DatabaseObject| {
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

pub(super) fn record_target_objects(evidence: &mut Vec<String>, targets: &[DatabaseObject]) {
    if let Ok(json) = serde_json::to_string(targets) {
        evidence.push(format!("{TARGET_JSON_PREFIX}{json}"));
    }
    let encoded = targets
        .iter()
        .map(encode_target_id)
        .collect::<Option<Vec<_>>>();
    if let Some(ids) = encoded {
        evidence.extend(
            ids.into_iter()
                .map(|id| format!("{TARGET_EVIDENCE_PREFIX}{id}")),
        );
    } else {
        evidence.push("recoveryProjectionIncomplete".into());
    }
}

pub(super) async fn fingerprint_targets(
    state: &AppState,
    session_id: &str,
    database: Option<&str>,
    targets: &[DatabaseObject],
) -> Result<String, CommandError> {
    let (driver, handle) = state
        .connection_manager
        .get_session(session_id)
        .await
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    let config = state
        .connection_manager
        .get_session_config(session_id)
        .await
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    let database = database.or(config.database.as_deref()).unwrap_or_default();
    let driver_type = driver.driver_type();
    let catalog_database = schema_catalog_database(&driver_type, Some(database));
    let mut entries = Vec::with_capacity(targets.len());
    for target in targets {
        if ObjectKind::parse(&target.kind) == Some(ObjectKind::Table) {
            let snapshot = match driver
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

        let kind = ObjectKind::parse(&target.kind).ok_or_else(|| {
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

pub(super) fn table_target(table: &str, schema: Option<&str>) -> DatabaseObject {
    let relation = table
        .trim()
        .rsplit_once('.')
        .map(|(_, name)| name)
        .unwrap_or_else(|| table.trim());
    DatabaseObject {
        kind: ObjectKind::Table.as_str().into(),
        schema: schema.map(str::to_owned),
        name: relation.to_owned(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}
