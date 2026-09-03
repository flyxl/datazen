use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::CommandAccessLevel;

pub(crate) fn access_level_for_mode(mode: Option<McpPermissionMode>) -> CommandAccessLevel {
    match mode {
        None | Some(McpPermissionMode::HighRiskWrite) => CommandAccessLevel::HighRisk,
        Some(McpPermissionMode::SafeWrite) => CommandAccessLevel::Write,
        Some(McpPermissionMode::ReadOnly) => CommandAccessLevel::Read,
    }
}
