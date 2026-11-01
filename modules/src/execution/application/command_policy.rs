//! MCP 命令准备策略（自 modules::services::command_policy 归位，D6 item 4）。
//!
//! 形态无关的命令准备规则，宿主命令适配器（driver_command）与 workflow 引擎
//! 共用：权限档位映射、命令定义解析（含 schema 元数据命令合成）、调用校验、
//! SQL 参数绑定与信封目标字段注入。
//!
//! B2 driver_command 域刀（2026-10-31）：发现目录合成与请求校验管线的
//! 错误文案（`Unsupported driver command` / `requires a connection` /
//! `requires an interactive session`）是冻结的 IPC 载荷，钉在本模块的
//! 内联测试里（见文末 tests 模块），宿主侧只留端到端烟测。

//! Shape-independent command preparation shared by host command adapters.
use crate::execution::domain::permission::McpPermissionMode;
use datazen_driver_api::{CommandAccessLevel, DatabaseDriver, DriverCommandDefinition};

pub fn access_level_for_mode(mode: Option<McpPermissionMode>) -> CommandAccessLevel {
    match mode {
        None | Some(McpPermissionMode::HighRiskWrite) => CommandAccessLevel::HighRisk,
        Some(McpPermissionMode::SafeWrite) => CommandAccessLevel::Write,
        Some(McpPermissionMode::ReadOnly) => CommandAccessLevel::Read,
    }
}

/// 命令发现目录：驱动原生命令 + schema 元数据命令的合成。
///
/// schema 的合成段是 feature 门控的：本 crate 的既定依赖边是
/// `schema → execution`（schema 的实现消费 execution 词汇），execution
/// 不能反向硬引 schema，否则 `connection,execution` 这类最小腿编译不过
/// （CI 的 FEATURE_TESTS 腿之一）。没有 schema 的构建只返回驱动原生
/// 命令——元数据命令本来就不在那套词汇里。
pub fn discovery_definitions(driver: &dyn DatabaseDriver) -> Vec<DriverCommandDefinition> {
    #[cfg_attr(not(feature = "schema"), allow(unused_mut))]
    let mut definitions = driver.command_definitions();
    #[cfg(feature = "schema")]
    {
        definitions.retain(|definition| {
            !crate::schema::application::metadata::is_metadata_command(&definition.id)
        });
        definitions.extend(crate::schema::application::metadata::command_definitions());
    }
    definitions
}

/// 元数据命令的目标必须在 input 内提供；这条校验与 schema 词汇同源，
/// 同样随 feature 门控（见 [`discovery_definitions`] 的注释）。
#[cfg_attr(not(feature = "schema"), allow(unused_variables))]
pub fn resolve_definition(
    driver: &dyn DatabaseDriver,
    command: &str,
    envelope_has_target: bool,
) -> Result<DriverCommandDefinition, String> {
    let mut definitions = driver.command_definitions();
    #[cfg(feature = "schema")]
    {
        if crate::schema::application::metadata::is_metadata_command(command) {
            if envelope_has_target {
                return Err("Metadata targets must be supplied inside input".into());
            }
            definitions = discovery_definitions(driver);
        }
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

#[cfg(test)]
mod tests {
    //! 2026-10-31 自桌面 `commands/driver_command/tests.rs` 归位（B1 批次，
    //! commands-layer-review §3.3/§4）：信封目标字段注入与权限档位映射是
    //! 本模块的形态中立规则，不是宿主绑定（judgment rule：换 axum 宿主
    //! 原样通过）。宿主侧经 `mcp::application::command_policy` 旧路径再
    //! 导出消费（M1 前置刀归位 execution），测试跟逻辑走。
    //!
    //! B2 driver_command 域刀（同日）补入：请求校验管线的冻结错误文案
    //! （resolve_definition / validate_invocation）、发现目录合成规则
    //! （discovery_definitions 的元数据合并与去重）、SQL 参数绑定的原地
    //! 改写，以及 nonempty / sql_from_input / query_rows_affected 三个
    //! 请求信封辅助函数——这些原先只被宿主端到端测试间接覆盖。

    use super::*;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use std::sync::Arc;

    #[test]
    fn inject_sql_target_fields_skips_blank_values_and_non_objects() {
        let mut input = serde_json::json!({ "sql": "SELECT 1" });
        inject_sql_target_fields(&mut input, Some("  "), Some(""));
        assert_eq!(
            input,
            serde_json::json!({ "sql": "SELECT 1" }),
            "blank envelope targeting must not be injected"
        );

        let mut input = serde_json::json!({ "sql": "SELECT 1" });
        inject_sql_target_fields(&mut input, Some("db"), Some("schema_x"));
        assert_eq!(input["database"], "db");
        assert_eq!(input["schema"], "schema_x");

        let mut scalar = serde_json::Value::Null;
        inject_sql_target_fields(&mut scalar, Some("db"), None);
        assert_eq!(scalar, serde_json::Value::Null);
    }

    #[test]
    fn test_tester_access_level_for_mode_maps_mcp_permission_modes() {
        assert_eq!(access_level_for_mode(None), CommandAccessLevel::HighRisk);
        assert_eq!(
            access_level_for_mode(Some(McpPermissionMode::ReadOnly)),
            CommandAccessLevel::Read
        );
        assert_eq!(
            access_level_for_mode(Some(McpPermissionMode::SafeWrite)),
            CommandAccessLevel::Write
        );
        assert_eq!(
            access_level_for_mode(Some(McpPermissionMode::HighRiskWrite)),
            CommandAccessLevel::HighRisk
        );
    }

    // ── B2 driver_command 域刀补入的纯逻辑钉子 ────────────────────────

    #[test]
    fn nonempty_trims_and_rejects_blank_values() {
        assert_eq!(nonempty(Some(&"  x ".to_string())), Some("x"));
        assert_eq!(nonempty(Some(&"".to_string())), None);
        assert_eq!(nonempty(Some(&"   ".to_string())), None);
        assert_eq!(nonempty(None), None);
    }

    #[test]
    fn sql_from_input_requires_a_nonempty_string() {
        assert_eq!(
            sql_from_input(&serde_json::json!({ "sql": "SELECT 1" })),
            Some("SELECT 1".to_string())
        );
        // 空串、缺失与非字符串都算“没有 SQL”。
        assert_eq!(sql_from_input(&serde_json::json!({ "sql": "" })), None);
        assert_eq!(sql_from_input(&serde_json::json!({})), None);
        assert_eq!(sql_from_input(&serde_json::json!({ "sql": 42 })), None);
    }

    #[test]
    fn query_rows_affected_prefers_the_flat_field_and_sums_statement_results() {
        assert_eq!(
            query_rows_affected(&serde_json::json!({ "rowsAffected": 5 })),
            Some(5)
        );
        assert_eq!(
            query_rows_affected(&serde_json::json!({
                "results": [
                    { "rowsAffected": 2 },
                    { "rowsAffected": 3 },
                    { }
                ]
            })),
            Some(5)
        );
        assert_eq!(query_rows_affected(&serde_json::json!({})), None);
    }

    #[test]
    fn bind_sql_params_rewrites_sql_in_place_and_skips_without_params() {
        let mut input = serde_json::json!({
            "sql": "SELECT * FROM t WHERE id = :id",
            "params": { "id": 7 }
        });
        bind_sql_params(&mut input).expect("legacy object params bind");
        assert_eq!(input["sql"], "SELECT * FROM t WHERE id = 7");

        // 没有 params 时 SQL 原样保留。
        let mut untouched = serde_json::json!({ "sql": "SELECT 1" });
        bind_sql_params(&mut untouched).expect("no params is a no-op");
        assert_eq!(untouched["sql"], "SELECT 1");
    }

    fn mock_driver() -> Arc<MockDriver> {
        MockDriver::new("postgres", MockDriverOptions::default())
    }

    #[test]
    fn resolve_definition_finds_native_and_catalog_commands() {
        let driver = mock_driver();
        for command in ["query", "query_stream", "execute", "list_databases"] {
            let definition = resolve_definition(driver.as_ref(), command, false)
                .unwrap_or_else(|error| panic!("{command} must resolve: {error}"));
            assert_eq!(definition.id, command);
        }
    }

    #[test]
    fn resolve_definition_unknown_command_error_text_is_frozen() {
        let driver = mock_driver();
        // 冻结的 IPC 错误文案：桌面端 `rejects_unknown_command` 与
        // `tests/driver_command_ipc.rs` 集成测试都在前端可见载荷里匹配它。
        let error = resolve_definition(driver.as_ref(), "not-a-command", false).unwrap_err();
        assert_eq!(error, "Unsupported driver command: not-a-command");

        // 空命令 id 同样不匹配任何定义（宿主端有同形钉子）。
        let error = resolve_definition(driver.as_ref(), "", false).unwrap_err();
        assert_eq!(error, "Unsupported driver command: ");
    }

    #[test]
    fn discovery_definitions_returns_native_and_extra_commands() {
        let ping = DriverCommandDefinition {
            id: "ping".into(),
            name: "Ping".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: datazen_driver_api::DriverCommandMetadata::new(
                datazen_driver_api::CommandCategory::Observe,
                CommandAccessLevel::Read,
            )
            .unbound()
            .hide_from_workflow(),
        };
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                extra_commands: vec![ping],
                ..Default::default()
            },
        );
        let ids: Vec<String> = discovery_definitions(driver.as_ref())
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        for expected in ["query", "execute", "ping"] {
            assert!(ids.iter().any(|id| id == expected), "missing {expected}");
        }
    }

    #[test]
    fn validate_invocation_error_texts_are_frozen() {
        let connection_required = DriverCommandDefinition {
            id: "query".into(),
            name: "Query".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: Default::default(),
        };
        // 未绑定会话（bound=false）且命令声明需要连接。
        let error = validate_invocation(&connection_required, false, None, false).unwrap_err();
        assert_eq!(error, "Command 'query' requires a connection");

        let save_dialog = DriverCommandDefinition {
            id: "pull_payload".into(),
            name: "Pull Payload".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: datazen_driver_api::DriverCommandMetadata::new(
                datazen_driver_api::CommandCategory::Io,
                CommandAccessLevel::Write,
            )
            .unbound()
            .save_dialog(datazen_driver_api::DriverSaveDialogSpec {
                file_name_field: "fileName".into(),
                data_base64_field: "dataBase64".into(),
                filter_name: "SQLite Database".into(),
                extensions: vec!["db".into()],
                result_path_field: "savedPath".into(),
            }),
        };
        // 声明保存对话框的命令：非交互（无 AppHandle）与带权限档位
        //（MCP / workflow）的调用方都被拒——两条都是冻结文案。
        let error = validate_invocation(&save_dialog, false, None, false).unwrap_err();
        assert_eq!(
            error,
            "Command 'pull_payload' requires an interactive session to show its save dialog"
        );
        let error =
            validate_invocation(&save_dialog, false, Some(McpPermissionMode::ReadOnly), true)
                .unwrap_err();
        assert_eq!(
            error,
            "Command 'pull_payload' requires an interactive session to show its save dialog"
        );
        // 交互且无档位时放行。
        validate_invocation(&save_dialog, false, None, true).expect("interactive passes");
    }

    /// schema feature 开启时的发现目录合成：驱动原生命令里的**同名元数据
    /// 命令被剔除**，换上模块的权威定义（去重）；元数据命令族全部在场。
    #[cfg(feature = "schema")]
    #[test]
    fn discovery_definitions_merges_schema_metadata_and_deduplicates() {
        let fake_catalog = DriverCommandDefinition {
            id: "list_catalog".into(),
            name: "Fake Driver Catalog".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: Default::default(),
        };
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                extra_commands: vec![fake_catalog],
                ..Default::default()
            },
        );
        let definitions = discovery_definitions(driver.as_ref());

        let catalogs: Vec<_> = definitions
            .iter()
            .filter(|definition| definition.id == "list_catalog")
            .collect();
        assert_eq!(catalogs.len(), 1, "metadata commands must deduplicate");
        assert_ne!(
            catalogs[0].name, "Fake Driver Catalog",
            "the canonical modules definition must win over the driver's"
        );

        let ids: Vec<String> = definitions
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        for metadata_id in [
            "list_catalog",
            "read_relation_columns",
            "read_relation_schema",
            "refresh_schema_metadata",
            "get_structure_capabilities",
            "plan_table_structure_changes",
        ] {
            assert!(
                ids.iter().any(|id| id == metadata_id),
                "missing {metadata_id}"
            );
        }
    }

    /// 元数据命令的目标必须在 input 内提供：信封带目标直接拒（冻结文案）。
    #[cfg(feature = "schema")]
    #[test]
    fn resolve_definition_rejects_envelope_targeting_for_metadata_commands() {
        let driver = mock_driver();
        let error = resolve_definition(driver.as_ref(), "list_catalog", true).unwrap_err();
        assert_eq!(error, "Metadata targets must be supplied inside input");

        let definition = resolve_definition(driver.as_ref(), "list_catalog", false)
            .expect("resolves via synthesis");
        assert_eq!(definition.id, "list_catalog");
    }
}
