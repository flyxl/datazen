//! Port of `packages/frontend/src/commands/backup.ts` (T163).
//!
//! The app-data backup/import family — five host-level legacy commands
//! (Decision 3 merged IPC), shaped exactly as the React wrappers put
//! them on the wire. The host executes them against the
//! `DesktopAdapter` transport (the `schema_diff` runner / `ai::wire`
//! shaper pattern: this module is the payload layer, typed receipts
//! decode the answer).
//!
//! ## What maps where
//!
//! - `exportAppData(defaultFileName)` → `export_app_data` with
//!   `{ defaultFileName }` — the native save dialog + the ZIP export in
//!   one merged call; `false` means the dialog was dismissed (a normal
//!   exit, not an error). The wire-level `override_path` escape hatch is
//!   webdriver/E2E-only and never sent from production code, so no
//!   shaper offers it.
//! - `pickAppDataImportFile()` → `pick_import_file` with
//!   `{ kind: 'app-data' }` (form-split F4c: one shared pick command,
//!   the kind string picks the filter) — the path only, no import; the
//!   answer is the chosen path or `null` when cancelled.
//! - `importAppData(sourcePath)` → `import_app_data` with
//!   `{ sourcePath }` — the ZIP import from a path the UI already
//!   confirmed; production callers pass it after pick + web confirm.
//! - `saveEncryptionKeyWithDialog(defaultFileName)` →
//!   `save_encryption_key_with_dialog` — the same merged-IPC shape for
//!   the key file save.
//! - `restartApp()` → `restart_app` (no payload, void receipt).
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the React source's verbatim.

use serde::Deserialize;

/// One shaped legacy command: the `legacyInvoke` name plus its payload
/// (the `ai::wire::AiRequest` shape, command-specific).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackupRequest {
    /// The legacy command name.
    pub(crate) command: &'static str,
    /// The exact `args` object the React wrapper passes.
    pub(crate) value: serde_json::Value,
}

impl BackupRequest {
    fn native(command: &'static str, value: serde_json::Value) -> Self {
        Self { command, value }
    }
}

/// `backupCommands.exportAppData(defaultFileName)`: merged native save
/// dialog + ZIP export. The receipt is `false` when the dialog was
/// dismissed.
pub(crate) fn export_app_data_request(default_file_name: &str) -> BackupRequest {
    BackupRequest::native("export_app_data", serde_json::json!({ "defaultFileName": default_file_name }))
}

/// `backupCommands.pickAppDataImportFile()`: the native open dialog for
/// an app-data ZIP (path only, no import). Form-split F4c merged it into
/// the shared `pick_import_file` host command; `kind` picks the filter.
/// The receipt is the chosen path or `null` when cancelled.
pub(crate) fn pick_app_data_import_file_request() -> BackupRequest {
    BackupRequest::native("pick_import_file", serde_json::json!({ "kind": "app-data" }))
}

/// `backupCommands.importAppData(sourcePath)`: the ZIP import from a
/// path chosen (and confirmed) in the UI.
pub(crate) fn import_app_data_request(source_path: &str) -> BackupRequest {
    BackupRequest::native("import_app_data", serde_json::json!({ "sourcePath": source_path }))
}

/// `backupCommands.saveEncryptionKeyWithDialog(defaultFileName)`: the
/// merged save-dialog + key-file write.
pub(crate) fn save_encryption_key_with_dialog_request(default_file_name: &str) -> BackupRequest {
    BackupRequest::native(
        "save_encryption_key_with_dialog",
        serde_json::json!({ "defaultFileName": default_file_name }),
    )
}

/// `backupCommands.restartApp()`: restart the app (no payload).
pub(crate) fn restart_app_request() -> BackupRequest {
    BackupRequest::native("restart_app", serde_json::json!({}))
}

/// The `pick_import_file` receipt: the chosen path, or `null` when the
/// dialog was dismissed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub(crate) struct PickedImportFile(pub(crate) Option<String>);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_wraps_the_default_file_name() {
        assert_eq!(
            export_app_data_request("datazen-backup.zip"),
            BackupRequest {
                command: "export_app_data",
                value: json!({ "defaultFileName": "datazen-backup.zip" })
            }
        );
    }

    #[test]
    fn pick_is_the_shared_command_with_the_app_data_kind() {
        assert_eq!(
            pick_app_data_import_file_request(),
            BackupRequest { command: "pick_import_file", value: json!({ "kind": "app-data" }) }
        );
    }

    #[test]
    fn import_carries_the_confirmed_source_path() {
        assert_eq!(
            import_app_data_request("/backups/datazen.zip"),
            BackupRequest {
                command: "import_app_data",
                value: json!({ "sourcePath": "/backups/datazen.zip" })
            }
        );
    }

    #[test]
    fn key_save_shares_the_merged_ipc_shape() {
        assert_eq!(
            save_encryption_key_with_dialog_request("datazen.key"),
            BackupRequest {
                command: "save_encryption_key_with_dialog",
                value: json!({ "defaultFileName": "datazen.key" })
            }
        );
    }

    #[test]
    fn restart_sends_no_payload() {
        assert_eq!(restart_app_request(), BackupRequest { command: "restart_app", value: json!({}) });
    }

    #[test]
    fn the_pick_receipt_decodes_both_arms() {
        let chosen: PickedImportFile =
            serde_json::from_value(json!("/tmp/app-data.zip")).expect("a path decodes");
        assert_eq!(chosen, PickedImportFile(Some("/tmp/app-data.zip".to_string())));
        let cancelled: PickedImportFile = serde_json::from_value(json!(null)).expect("null decodes");
        assert_eq!(cancelled, PickedImportFile(None));
    }
}
