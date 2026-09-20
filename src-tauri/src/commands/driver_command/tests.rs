use std::any::type_name;
use std::sync::Arc;
use std::sync::Mutex;

use super::access::access_level_for_mode;
use super::helpers::{inject_sql_target_fields, nonempty};
use super::resolve::resolve_command_driver;
use super::*;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    CommandAccessLevel, QueryExecutionId, QueryStreamCallback, QueryStreamEvent,
};

// NOTE: Full test suite temporarily reduced while restoring from PLACEHOLDER.
// Bootstrap registration guard kept; remaining tests re-added in follow-up.

#[test]
fn test_tester_ipc_commands_registered_in_bootstrap() {
    let bootstrap = include_str!("../../bootstrap/run.rs");
    for cmd in [
        "get_driver_commands",
        "get_connection_commands",
        "execute_driver_command",
        "execute_driver_command_stream",
    ] {
        assert!(
            bootstrap.contains(&format!("crate::commands::{cmd}")),
            "bootstrap invoke handler must register `{cmd}`",
        );
    }
}
