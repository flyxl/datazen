use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
// TEMPORARY STUB - full content restore in progress
// This is NOT the final file - do not ship
#[cfg(test)]
mod tests {
    const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");
    #[test]
    fn bootstrap_path_ok() {
        assert!(BOOTSTRAP_RS.contains("commands::backup_database,"));
    }
}
