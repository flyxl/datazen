//! Independent audit for T163 (the commands/backup.ts port).
//!
//! backup.ts has no React test of its own (it is a thin typed wrapper
//! over legacy host IPC). The audit re-derives every wire shape from the
//! React source literal: the five command names, the payload keys
//! (camelCase), the merged-IPC decisions the file's comments record
//! (save dialog + export in one call; the shared `pick_import_file`
//! command with its kind string; no `override_path` from production
//! code), and the pick receipt's two arms.

use serde_json::json;

use datazen_ui::commands::backup::{
    BackupRequest, PickedImportFile, export_app_data_request, import_app_data_request,
    pick_app_data_import_file_request, restart_app_request,
    save_encryption_key_with_dialog_request,
};

#[test]
fn every_command_name_is_the_react_literal() {
    assert_eq!(export_app_data_request("x.zip").command, "export_app_data");
    assert_eq!(pick_app_data_import_file_request().command, "pick_import_file");
    assert_eq!(import_app_data_request("/x.zip").command, "import_app_data");
    assert_eq!(
        save_encryption_key_with_dialog_request("k.key").command,
        "save_encryption_key_with_dialog"
    );
    assert_eq!(restart_app_request().command, "restart_app");
}

#[test]
fn payload_keys_match_the_react_wrappers() {
    // exportAppData(defaultFileName)
    assert_eq!(
        export_app_data_request("datazen-2026.zip").value,
        json!({ "defaultFileName": "datazen-2026.zip" })
    );
    // pickAppDataImportFile() — form-split F4c: the shared command, the
    // kind string picks the app-data ZIP filter.
    assert_eq!(pick_app_data_import_file_request().value, json!({ "kind": "app-data" }));
    // importAppData(sourcePath)
    assert_eq!(
        import_app_data_request("/backups/datazen.zip").value,
        json!({ "sourcePath": "/backups/datazen.zip" })
    );
    // saveEncryptionKeyWithDialog(defaultFileName)
    assert_eq!(
        save_encryption_key_with_dialog_request("datazen.key").value,
        json!({ "defaultFileName": "datazen.key" })
    );
    // restartApp() — no payload at all.
    assert_eq!(restart_app_request().value, json!({}));
}

#[test]
fn no_production_shaper_offers_the_override_path_hatch() {
    // The wire-level `override_path` is webdriver/E2E-only and never sent
    // from production code: no shaper here accepts or emits one.
    for request in [
        export_app_data_request("x.zip"),
        import_app_data_request("/x.zip"),
        save_encryption_key_with_dialog_request("k.key"),
    ] {
        assert!(
            request.value.get("overridePath").is_none(),
            "{request:?} must not carry override_path"
        );
    }
}

#[test]
fn the_pick_receipt_decodes_path_and_null() {
    // A path chosen in the dialog.
    let chosen: PickedImportFile =
        serde_json::from_value(json!("/home/u/backups/datazen.zip")).expect("path decodes");
    assert_eq!(chosen.0.as_deref(), Some("/home/u/backups/datazen.zip"));
    // Dismissed: null, not an error (the React `string | null` contract).
    let dismissed: PickedImportFile = serde_json::from_value(json!(null)).expect("null decodes");
    assert!(dismissed.0.is_none());
}

#[test]
fn the_request_envelope_compares_end_to_end() {
    // The audit compares whole requests, so equality must hold for every
    // field (command name and payload) at once.
    assert_eq!(
        export_app_data_request("x.zip"),
        BackupRequest { command: "export_app_data", value: json!({ "defaultFileName": "x.zip" }) }
    );
    assert_eq!(
        pick_app_data_import_file_request(),
        BackupRequest { command: "pick_import_file", value: json!({ "kind": "app-data" }) }
    );
}
