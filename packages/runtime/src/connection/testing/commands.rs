//! 会话级句柄 fake 命令定义（fake-runtime-fixtures.md §9.1）。
//!
//! 服务 CM-73 / CM-74：淘汰、替换、隔离时必须先在**原 resource** 上回滚 / 关闭句柄并注销，
//! 确认后才允许释放资源。要让这条纪律可测，夹具侧必须真的提供一组「能返回会话级句柄」的
//! 命令，并通过 driver-api 的 `command_definitions()` / `execute_command()` 通道暴露。
//!
//! 依赖方向：本模块**不依赖** `fake_resource`（§2）。它只向下依赖
//! `connection::execution::SessionCommand`（命令 id 的唯一真源）与 `driver-api` 的
//! `DriverCommandDefinition`。执行侧由 provider 读这张表。
//!
//! §13 纪律：输入 schema 里不出现任何凭据字段 —— 连接密码由 driver 连接时消费，
//! 不作为命令入参，因此这里不存在可以被 journal 记录的敏感值。

use datazen_driver_api::command::{
    CommandAccessLevel, CommandCategory, DriverCommandDefinition, DriverCommandMetadata,
};
use serde_json::{json, Value as JsonValue};

use crate::connection::execution::SessionCommand;

/// `handleId` 入参。十个命令里六个都以它为第一入参（§9.1）。
fn handle_id_schema() -> JsonValue {
    json!({
        "type": "object",
        "properties": {
            "handleId": { "type": "string" }
        },
        "required": ["handleId"],
        "additionalProperties": false
    })
}

/// 句柄随 completion 交出的那几条命令：输出统一带 `sessionHandles` 数组（§9.2）。
fn handle_output_schema() -> Option<JsonValue> {
    Some(json!({
        "type": "object",
        "properties": {
            "effectOutcome": { "type": "string" },
            "sessionHandles": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "handleId": { "type": "string" },
                        "kind": { "type": "string" },
                        "resourceId": { "type": "string" },
                        "runtimeEpoch": { "type": "integer" }
                    },
                    "required": ["handleId", "kind", "resourceId", "runtimeEpoch"]
                }
            }
        },
        "required": ["effectOutcome"]
    }))
}

/// 只有终结类命令的输出才带副作用终态，且**不带**句柄。
fn outcome_output_schema() -> Option<JsonValue> {
    Some(json!({
        "type": "object",
        "properties": {
            "effectOutcome": { "type": "string" },
            "errorCode": { "type": ["string", "null"] }
        },
        "required": ["effectOutcome"]
    }))
}

/// 命令的访问级别。§9.1 表里只有 `open_session_cursor` 是 Read，其余全是 Write。
fn access_level(command: SessionCommand) -> CommandAccessLevel {
    if command.is_write() {
        CommandAccessLevel::Write
    } else {
        CommandAccessLevel::Read
    }
}

fn metadata(command: SessionCommand) -> DriverCommandMetadata {
    let category = if command.is_write() {
        CommandCategory::Mutate
    } else {
        CommandCategory::Query
    };
    // requires_connection 在 Default 里已经是 true（§9.1 L462），这里显式重述以免将来
    // Default 改动悄悄放宽门控。
    DriverCommandMetadata {
        category,
        risk: Some(access_level(command)),
        requires_connection: true,
        ..DriverCommandMetadata::default()
    }
}

fn definition(
    command: SessionCommand,
    name: &str,
    description: &str,
    input_schema: JsonValue,
    output_schema: Option<JsonValue>,
) -> DriverCommandDefinition {
    DriverCommandDefinition {
        id: command.id().to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        input_schema,
        output_schema,
        permissions: Vec::new(),
        metadata: metadata(command),
    }
}

/// §9.1 全部十条会话级句柄命令。顺序即 `SessionCommand::ALL` 的顺序。
pub fn session_handle_command_definitions() -> Vec<DriverCommandDefinition> {
    vec![
        definition(
            SessionCommand::BeginSessionTransaction,
            "begin_session_transaction",
            "开启事务并把句柄随 completion 交出（CM-73/CM-74 句柄登记基线）。",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            handle_output_schema(),
        ),
        definition(
            SessionCommand::BeginSessionTransactionHold,
            "begin_session_transaction_hold",
            "开启后不自动终结，用于驱动关闭/淘汰竞态。",
            json!({
                "type": "object",
                "properties": { "holdMs": { "type": "integer", "minimum": 0 } },
                "required": ["holdMs"],
                "additionalProperties": false
            }),
            handle_output_schema(),
        ),
        definition(
            SessionCommand::OpenSessionCursor,
            "open_session_cursor",
            "打开游标句柄；需显式关闭，存在游标时适用 5 分钟空闲事务规则。",
            json!({
                "type": "object",
                "properties": { "rows": { "type": "integer", "minimum": 0 } },
                "required": ["rows"],
                "additionalProperties": false
            }),
            handle_output_schema(),
        ),
        definition(
            SessionCommand::PrepareServerStatement,
            "prepare_server_statement",
            "服务端预处理对象；与事务句柄同等登记与注销。",
            json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"],
                "additionalProperties": false
            }),
            handle_output_schema(),
        ),
        definition(
            SessionCommand::CommitSessionTransaction,
            "commit_session_transaction",
            "在同一 resource 上提交；可注入 Unknown 副作用终态。",
            handle_id_schema(),
            outcome_output_schema(),
        ),
        definition(
            SessionCommand::RollbackSessionTransaction,
            "rollback_session_transaction",
            "回滚；可注入 RollbackFailed，届时资源被隔离且预算占用保留。",
            handle_id_schema(),
            outcome_output_schema(),
        ),
        definition(
            SessionCommand::CloseSessionCursor,
            "close_session_cursor",
            "关闭游标并注销句柄。",
            handle_id_schema(),
            outcome_output_schema(),
        ),
        definition(
            SessionCommand::BeginSessionTransactionUnregistered,
            "begin_session_transaction_unregistered",
            "反例：fake 侧建事务句柄但不写入 sessionHandles，必须被 journal 记为 orphaned。",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            outcome_output_schema(),
        ),
        definition(
            SessionCommand::CommitWithStaleHandle,
            "commit_with_stale_handle",
            "反例：携带旧 runtimeEpoch 的句柄，必须被拒绝并返回 RuntimeEpochMismatch。",
            json!({
                "type": "object",
                "properties": {
                    "handleId": { "type": "string" },
                    "runtimeEpoch": { "type": "integer" }
                },
                "required": ["handleId", "runtimeEpoch"],
                "additionalProperties": false
            }),
            outcome_output_schema(),
        ),
        definition(
            SessionCommand::HandleFromOtherResource,
            "handle_from_other_resource",
            "反例：跨 resource 复用句柄，必须被拒绝并返回 SessionLost。",
            json!({
                "type": "object",
                "properties": {
                    "handleId": { "type": "string" },
                    "resourceId": { "type": "string" }
                },
                "required": ["handleId", "resourceId"],
                "additionalProperties": false
            }),
            outcome_output_schema(),
        ),
    ]
}

/// 按 id 取一条定义。provider 的命令分发走这条路径，因此 id 与
/// `SessionCommand::from_id` 的真源保持一致。
pub fn session_handle_command_definition(id: &str) -> Option<DriverCommandDefinition> {
    session_handle_command_definitions()
        .into_iter()
        .find(|definition| definition.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_exactly_ten_commands_and_matches_the_enum() {
        let definitions = session_handle_command_definitions();
        assert_eq!(definitions.len(), 10, "§9.1 L451-460 是十条命令");
        let ids: Vec<&str> = definitions.iter().map(|d| d.id.as_str()).collect();
        let expected: Vec<&str> = SessionCommand::ALL.iter().map(|c| c.id()).collect();
        assert_eq!(ids, expected, "命令表必须与 SessionCommand::ALL 一一对应");
    }

    #[test]
    fn only_the_cursor_command_is_read_level() {
        for definition in session_handle_command_definitions() {
            let command = SessionCommand::from_id(&definition.id);
            assert!(command.is_some(), "{} 必须能反查到枚举", definition.id);
            let command = command.unwrap_or(SessionCommand::BeginSessionTransaction);
            let expected = if command.is_write() {
                CommandAccessLevel::Write
            } else {
                CommandAccessLevel::Read
            };
            assert_eq!(
                definition.metadata.risk,
                Some(expected),
                "{} 的 risk 必须与访问级别一致",
                definition.id
            );
        }
    }

    #[test]
    fn every_command_requires_a_live_connection() {
        for definition in session_handle_command_definitions() {
            assert!(
                definition.metadata.requires_connection,
                "{} 必须声明 requires_connection = true（§9.1 L462）",
                definition.id
            );
        }
    }

    #[test]
    fn handle_bearing_commands_declare_handle_output_and_others_do_not() {
        for definition in session_handle_command_definitions() {
            let returns_handle = matches!(
                SessionCommand::from_id(&definition.id),
                Some(
                    SessionCommand::BeginSessionTransaction
                        | SessionCommand::BeginSessionTransactionHold
                        | SessionCommand::OpenSessionCursor
                        | SessionCommand::PrepareServerStatement
                )
            );
            let declares_handles = definition
                .output_schema
                .as_ref()
                .and_then(|schema| schema.get("properties"))
                .and_then(|props| props.get("sessionHandles"))
                .is_some();
            assert_eq!(
                declares_handles, returns_handle,
                "{} 的输出 schema 与「是否返回句柄」必须一致",
                definition.id
            );
        }
    }

    #[test]
    fn no_input_schema_carries_a_credential_field() {
        // §13：命令入参不得含凭据字段，否则 journal 会记录到它。
        for definition in session_handle_command_definitions() {
            let properties = definition
                .input_schema
                .get("properties")
                .cloned()
                .unwrap_or(JsonValue::Null);
            if let JsonValue::Object(map) = properties {
                for key in map.keys() {
                    let lowered = key.to_ascii_lowercase();
                    assert!(
                        !lowered.contains("password")
                            && !lowered.contains("secret")
                            && !lowered.contains("token"),
                        "{} 的入参出现了疑似凭据字段 {key}",
                        definition.id
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_by_id_round_trips() {
        for command in SessionCommand::ALL {
            let found = session_handle_command_definition(command.id());
            assert!(found.is_some(), "{} 必须能查到定义", command.id());
            assert_eq!(
                found.map(|d| d.id).unwrap_or_default(),
                command.id().to_string()
            );
        }
        assert!(session_handle_command_definition("not_a_fake_session_command").is_none());
    }
}
