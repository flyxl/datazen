use super::*;

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
