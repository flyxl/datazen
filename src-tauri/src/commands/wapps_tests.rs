use super::*;

const SOURCE: &str = include_str!("wapps.rs");
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

#[test]
fn merged_wapp_commands_gate_override_path_in_production() {
    for gone in ["commands::list_extensions,"] {
        assert!(
            !BOOTSTRAP_RS.contains(gone),
            "`{gone}` must no longer be registered"
        );
    }
    assert!(BOOTSTRAP_RS.contains("commands::list_wapps,"));
    assert!(BOOTSTRAP_RS.contains("commands::install_wapp,"));
    assert!(BOOTSTRAP_RS.contains("commands::remove_wapp,"));
}
