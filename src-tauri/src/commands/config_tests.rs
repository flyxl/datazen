use super::*;
use std::path::Path;

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
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(BOOTSTRAP_RS.contains("commands::export_connections,"));
    assert!(BOOTSTRAP_RS.contains("commands::import_connections_with_dialog,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_settings,"));
}
