pub const SERVER_STATUS_SNAPSHOT_COMMAND: &str = "server_status_snapshot";
pub const ESTIMATE_TABLE_ROWS_COMMAND: &str = "estimate_table_rows";
pub const LIST_PROCESSES_COMMAND: &str = "list_processes";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_command_ids_keep_their_wire_spellings() {
        assert_eq!(SERVER_STATUS_SNAPSHOT_COMMAND, "server_status_snapshot");
        assert_eq!(ESTIMATE_TABLE_ROWS_COMMAND, "estimate_table_rows");
        assert_eq!(LIST_PROCESSES_COMMAND, "list_processes");
    }

    #[test]
    fn command_ids_are_distinct() {
        let ids = [SERVER_STATUS_SNAPSHOT_COMMAND, ESTIMATE_TABLE_ROWS_COMMAND, LIST_PROCESSES_COMMAND];
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::SERVER_STATUS_SNAPSHOT_COMMAND, SERVER_STATUS_SNAPSHOT_COMMAND);
        assert_eq!(crate::ESTIMATE_TABLE_ROWS_COMMAND, ESTIMATE_TABLE_ROWS_COMMAND);
        assert_eq!(crate::LIST_PROCESSES_COMMAND, LIST_PROCESSES_COMMAND);
    }
}
