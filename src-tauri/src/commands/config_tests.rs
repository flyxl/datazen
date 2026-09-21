use super::*;
use std::path::Path;

const SOURCE: &str = include_str!("config.rs");
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

fn command_params(command: &str) -> String {
    let needle = format!("pub async fn {command}(");
    let start = SOURCE
        .find(&needle)
        .unwrap_or_else(|| panic!("{command} not found"));
    let after = &SOURCE[start + needle.len()..];
    let end = after.find(')').expect("params close");
    after[..end].to_string()
}

#[test]
fn test_get_app_executable_path_returns_valid_path() {
    let exe = get_app_executable_path_impl().expect("executable path should be resolved");
    assert!(!exe.is_empty());
    assert!(Path::new(&exe).exists());
}

#[test]
fn path_is_under_matches_prefix() {
    assert!(path_is_under(
        Path::new("/data/app/logs"),
        Path::new("/data/app")
    ));
    assert!(!path_is_under(
        Path::new("/tmp/evil"),
        Path::new("/data/app")
    ));
}

#[test]
fn require_webdriver_path_ipc_gates_without_feature() {
    let result = require_webdriver_path_ipc("Direct path connection export disabled");
    if cfg!(feature = "webdriver") {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }
}

#[test]
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(BOOTSTRAP_RS.contains("commands::export_connections,"));
    assert!(BOOTSTRAP_RS.contains("commands::import_connections_with_dialog,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_tunnels,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_tunnel,"));
}

#[test]
fn merged_commands_route_through_shared_resolve_override_path() {
    // Single mechanism: config.rs must reuse the error.rs helper (F3
    // pattern) instead of redefining a local copy, and every merged
    // command body must gate through it. Needles are assembled at runtime
    // so this test's own source never contains them.
    let resolve = "resolve";
    let override_path = "override_path";
    let local_def = format!("fn {resolve}_{override_path}(");
    assert!(
        !SOURCE.contains(&local_def),
        "config.rs must not redefine resolve_override_path"
    );
    let call = format!("{resolve}_{override_path}(override_path");
    let gated_bodies = SOURCE.matches(&call).count();
    // Core file + re-exported archive module both use the helper; count is soft.
    assert!(
        gated_bodies >= 3,
        "merged commands must gate their override branch; found {gated_bodies}"
    );
}

#[test]
fn encryption_key_export_bytes_trims() {
    let bytes = encryption_key_export_bytes("  abc==  \n");
    assert_eq!(bytes, b"abc==");
}
