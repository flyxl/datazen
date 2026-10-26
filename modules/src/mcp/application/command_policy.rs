//! MCP 命令准备策略（自 feature-core::services::command_policy 归位，D6 item 4）。
//!
//! 形态无关的命令准备规则，宿主命令适配器（driver_command）与 workflow 引擎
//! 共用：权限档位映射、命令定义解析（含 schema 元数据命令合成）、调用校验、
//! SQL 参数绑定与信封目标字段注入。

//! Shape-independent command preparation shared by host command adapters.
use crate::mcp::domain::permission::McpPermissionMode;
use datazen_driver_api::{CommandAccessLevel, DatabaseDriver, DriverCommandDefinition};

pub fn access_level_for_mode(mode: Option<McpPermissionMode>) -> CommandAccessLevel {
    match mode {
        None | Some(McpPermissionMode::HighRiskWrite) => CommandAccessLevel::HighRisk,
        Some(McpPermissionMode::SafeWrite) => CommandAccessLevel::Write,
        Some(McpPermissionMode::ReadOnly) => CommandAccessLevel::Read,
    }
}

pub fn discovery_definitions(driver: &dyn DatabaseDriver) -> Vec<DriverCommandDefinition> {
    let mut definitions = driver.command_definitions();
    definitions.retain(|definition| {
        !crate::schema::application::metadata::is_metadata_command(&definition.id)
    });
    definitions.extend(crate::schema::application::metadata::command_definitions());
    definitions
}

pub fn resolve_definition(
    driver: &dyn DatabaseDriver,
    command: &str,
    envelope_has_target: bool,
) -> Result<DriverCommandDefinition, String> {
    let mut definitions = driver.command_definitions();
    if crate::schema::application::metadata::is_metadata_command(command) {
        if envelope_has_target {
            return Err("Metadata targets must be supplied inside input".into());
        }
        definitions = discovery_definitions(driver);
    }
    if datazen_driver_api::is_schema_catalog_command(command) {
        for definition in datazen_driver_api::schema_catalog_command_definitions() {
            if !definitions
                .iter()
                .any(|existing| existing.id == definition.id)
            {
                definitions.push(definition);
            }
        }
    }
    definitions
        .into_iter()
        .find(|definition| definition.id == command)
        .ok_or_else(|| format!("Unsupported driver command: {command}"))
}

pub fn validate_invocation(
    definition: &DriverCommandDefinition,
    bound: bool,
    permission_mode: Option<McpPermissionMode>,
    interactive: bool,
) -> Result<(), String> {
    if !bound && definition.metadata.requires_connection {
        return Err(format!("Command '{}' requires a connection", definition.id));
    }
    if definition.metadata.save_dialog.is_some() && (permission_mode.is_some() || !interactive) {
        return Err(format!(
            "Command '{}' requires an interactive session to show its save dialog",
            definition.id
        ));
    }
    Ok(())
}

pub fn bind_sql_params(input: &mut serde_json::Value) -> Result<(), String> {
    if let (Some(params), Some(sql)) = (
        input.get("params"),
        input.get("sql").and_then(serde_json::Value::as_str),
    ) {
        let bound = crate::execution::domain::sql_guard::apply_params(sql, params)?;
        input["sql"] = serde_json::Value::String(bound);
    }
    Ok(())
}

pub fn nonempty(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub fn sql_from_input(input: &serde_json::Value) -> Option<String> {
    input
        .get("sql")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// F7: copy non-blank envelope targeting fields into the command input object
/// (no-op for blank values or non-object inputs).
pub fn inject_sql_target_fields(
    input: &mut serde_json::Value,
    database: Option<&str>,
    schema: Option<&str>,
) {
    let Some(map) = input.as_object_mut() else {
        return;
    };
    let database = database.map(str::trim).filter(|s| !s.is_empty());
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    if let Some(database) = database {
        map.insert(
            "database".into(),
            serde_json::Value::String(database.into()),
        );
    }
    if let Some(schema) = schema {
        map.insert("schema".into(), serde_json::Value::String(schema.into()));
    }
}

pub fn query_rows_affected(data: &serde_json::Value) -> Option<u64> {
    if let Some(rows) = data.get("rowsAffected").and_then(|v| v.as_u64()) {
        return Some(rows);
    }
    let results = data.get("results")?.as_array()?;
    Some(
        results
            .iter()
            .filter_map(|r| r.get("rowsAffected").and_then(|v| v.as_u64()))
            .sum(),
    )
}
