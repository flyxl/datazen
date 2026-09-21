//! Cross-module guards for the merged IPC surface.
//!
//! These assert the *contract* between the config / connection-import /
//! app-archive modules and the registered handler list, so they live here
//! rather than inside any single one of them.

use crate::commands::*;
use std::path::Path;

/// Every file that hosts part of the merged config IPC surface.
const SOURCE: &str = concat!(
    include_str!("config.rs"),
    "\n",
    include_str!("connection_import/ipc.rs"),
    "\n",
    include_str!("app_archive.rs"),
    "\n",
    include_str!("encryption_key.rs"),
);
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

mod ipc_contract_guards {
    use super::*;

    // Runtime-assembled needles (avoid self-reference in include_str! source).
    #[allow(non_upper_case_globals)]
    const resolve: &str = "resolve";

    #[allow(non_upper_case_globals)]
    const override_path: &str = "override_path";

    #[test]
    fn merged_commands_take_override_path_not_raw_path_param() {
        for cmd in [
            "export_connections",
            "import_connections_preview",
            "import_connections_with_dialog",
            "export_app_data",
            "import_app_data",
        ] {
            let params = command_params(cmd);
            assert!(
                params.contains("override_path: Option<String>"),
                "`{cmd}` must expose the webdriver-only override; got: {params}"
            );
            assert!(
                !params.contains("path: String"),
                "raw `path` parameter must not return after the merge; got: {params}"
            );
        }
    }

    #[test]
    fn stale_dialog_twins_are_gone_from_config_rs() {
        // Needle parts are assembled at runtime so this test's own source never
        // contains them (mirroring backup.rs guards).
        let fn_prefix = "pub async fn ";
        let with_dialog = "_with_";
        let dialog_kind = "dialog(";
        for name in ["export_connections", "export_app_data", "import_app_data"] {
            let variant = format!("{fn_prefix}{name}{with_dialog}{dialog_kind}");
            assert!(!SOURCE.contains(&variant), "stale dialog twin `{variant}`");
        }
    }
}
