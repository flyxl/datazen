//! Port of `packages/frontend/src/commands/wapps.ts` (T440) — the
//! workspace-app command family: the list/read pair, the driver
//! command rail, the pick-then-install pair, the enable/remove
//! pair, the wapp-scoped storage triple, the package file read and
//! the best-effort audit entry.
//!
//! ## What maps where
//!
//! - The twelve `invoke` wrappers are [`WappMethod`] + the
//!   `*_request` shapers (the desktop adapter's command rail — the
//!   commands/ convention).
//! - `WAPPS_CHANGED_EVENT` mirrors the Rust command's own constant
//!   (the comment's rule): the same literal on both sides.
//! - `WappManifest` / `WappSummary` / their nested contributions
//!   are the serde wire contracts, camelCase exactly as the React
//!   interfaces spell. The view-side VMs
//!   (`views/wapps/wapp_summary.rs`, `install_wapp_dialog.rs`) stay
//!   the honest projections this rail feeds.
//! - `inspectWappPackageWithDialog`'s cancelled pick answers
//!   `null` (the host's decode); the wire-level `overridePath`
//!   escape hatch is webdriver/E2E-only — it rides the shaper
//!   absent-when-`None`, the same ruling the React `?` spells.
//! - `readWappFile`'s `number[]` (bytes) is the host's decode.
//! - `auditLog`'s fire-and-forget `.catch(() => {})` — the audit is
//!   best-effort and IPC being down must not break the call path —
//!   is the host's transport policy; the shaper only carries the
//!   entry. The detail string must never contain argument contents
//!   (callers pass only the command name and target connection id)
//!   — the React doc comment's own rule, kept here.
//!
//! Known deltas: none — every payload is key-for-key with the React
//! wire; `executeDriverCommand`'s `args` and the storage `value`
//! ride opaque JSON (`unknown`).

// No production consumer yet (the wapps management track wires the
// faces); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// `WAPPS_CHANGED_EVENT` — mirrors the Rust command's own constant
/// (`src-tauri/src/commands/wapps.rs`).
pub const WAPPS_CHANGED_EVENT: &str = "wapps:changed";

/// `WappPermission` — the closed five-string union.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WappPermission {
    #[serde(rename = "context:connections")]
    ContextConnections,
    #[serde(rename = "command:invoke")]
    CommandInvoke,
    #[serde(rename = "session:stateful")]
    SessionStateful,
    #[serde(rename = "storage:local")]
    StorageLocal,
    #[serde(rename = "ui:notify")]
    UiNotify,
}

/// `WappPageContribution` — only `workspace` is supported by this
/// host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageContribution {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Only `workspace` is supported by this host.
    pub show_in: String,
}

/// `WappThemeContribution` — a theme a wapp contributes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappThemeContribution {
    pub id: String,
    pub name: String,
    /// Package-relative path to the tokens CSS file.
    pub tokens_css: String,
    /// Non-empty subset of `light` / `dark`.
    pub modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_image: Option<String>,
    /// Optional CodeMirror color overlay (legacy ThemePack
    /// `editor.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_json: Option<String>,
    /// Optional chart series palette (legacy `charts.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charts_json: Option<String>,
    /// Optional directory of semantic icon overrides named
    /// `<semanticId>.svg|.webp|.png` (legacy `icons/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icons_dir: Option<String>,
}

/// `WappContributions`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappContributions {
    #[serde(default)]
    pub pages: Vec<WappPageContribution>,
    #[serde(default)]
    pub themes: Vec<WappThemeContribution>,
}

/// `WappAiTool` — an opt-in AI tool a manifest declares.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub read_only: bool,
    /// `'query' | 'execute'`.
    pub command: String,
    pub connection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
}

/// `WappAiContribution`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiContribution {
    #[serde(default)]
    pub tools: Vec<WappAiTool>,
}

/// `WappManifest` — the full manifest `get_wapp_manifest` answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional package-level icon (square brand image) shown in
    /// wapp lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default)]
    pub contributes: WappContributions,
    #[serde(default)]
    pub permissions: Vec<WappPermission>,
    /// Reserved for P2 backend extensions; must be null/absent in
    /// v1 (rides opaque — the reserved shape is not this host's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<Value>,
    /// Opt-in AI tools declared by the plugin manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<WappAiContribution>,
}

/// `WappPageSummary` — a page entry inside a `WappSummary` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageSummary {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// `WappThemeSummary` — a theme entry inside a `WappSummary`
/// payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappThemeSummary {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
}

/// `WappAiToolSummary`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiToolSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub read_only: bool,
}

/// `WappSummary` — an installed wapp row as `list_wapps` answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional package-level icon path (mirrors the manifest's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub permissions: Vec<WappPermission>,
    #[serde(default)]
    pub pages: Vec<WappPageSummary>,
    #[serde(default)]
    pub themes: Vec<WappThemeSummary>,
    /// Present only when the manifest declares AI tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_tools: Option<Vec<WappAiToolSummary>>,
}

/// `WappPackageKind` — which native picker the inspect opens (the
/// view-side twin lives in `views/wapps/install_wapp_dialog.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WappPackageKind {
    Zip,
    Folder,
}

impl WappPackageKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Folder => "folder",
        }
    }
}

/// `WappPackagePreview` — the preview a native pick + validate-only
/// inspect answers (no install).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPackagePreview {
    pub pick_token: String,
    pub package_label: String,
    pub manifest: WappManifest,
}

/// `executeDriverCommand`'s request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappDriverCommandRequest {
    pub wapp_id: String,
    pub connection_id: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

/// The wapp command family's invoke targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WappMethod {
    ListWapps,
    GetWappManifest,
    ExecuteDriverCommand,
    InspectWappPackageWithDialog,
    InstallWapp,
    RemoveWapp,
    SetWappEnabled,
    WappStorageGet,
    WappStorageSet,
    WappStorageRemove,
    ReadWappFile,
    WappAuditLog,
}

impl WappMethod {
    /// The invoke command.
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::ListWapps => "list_wapps",
            Self::GetWappManifest => "get_wapp_manifest",
            Self::ExecuteDriverCommand => "execute_wapp_driver_command",
            Self::InspectWappPackageWithDialog => "inspect_wapp_package_with_dialog",
            Self::InstallWapp => "install_wapp",
            Self::RemoveWapp => "remove_wapp",
            Self::SetWappEnabled => "set_wapp_enabled",
            Self::WappStorageGet => "wapp_storage_get",
            Self::WappStorageSet => "wapp_storage_set",
            Self::WappStorageRemove => "wapp_storage_remove",
            Self::ReadWappFile => "read_wapp_file",
            Self::WappAuditLog => "wapp_audit_log",
        }
    }
}

/// `listWapps()`.
pub fn list_wapps_request() -> (WappMethod, Value) {
    (WappMethod::ListWapps, json!({}))
}

/// `getWappManifest(id)`.
pub fn get_wapp_manifest_request(id: &str) -> (WappMethod, Value) {
    (WappMethod::GetWappManifest, json!({ "id": id }))
}

/// `executeDriverCommand({ wappId, connectionId, command, args })` —
/// the request rides under its own key.
pub fn execute_driver_command_request(
    request: &WappDriverCommandRequest,
) -> (WappMethod, Value) {
    (WappMethod::ExecuteDriverCommand, json!({ "request": request }))
}

/// `inspectWappPackageWithDialog(packageKind, overridePath?)` — the
/// cancelled pick answers `null` (the host's decode).
pub fn inspect_wapp_package_with_dialog_request(
    package_kind: WappPackageKind,
    override_path: Option<&str>,
) -> (WappMethod, Value) {
    let mut payload = json!({ "packageKind": package_kind.as_str() });
    if let Some(path) = override_path {
        payload["overridePath"] = json!(path);
    }
    (WappMethod::InspectWappPackageWithDialog, payload)
}

/// `installWapp(pickToken, overridePath?)`.
pub fn install_wapp_request(
    pick_token: &str,
    override_path: Option<&str>,
) -> (WappMethod, Value) {
    let mut payload = json!({ "pickToken": pick_token });
    if let Some(path) = override_path {
        payload["overridePath"] = json!(path);
    }
    (WappMethod::InstallWapp, payload)
}

/// `removeWapp(id)`.
pub fn remove_wapp_request(id: &str) -> (WappMethod, Value) {
    (WappMethod::RemoveWapp, json!({ "id": id }))
}

/// `setWappEnabled(id, enabled)`.
pub fn set_wapp_enabled_request(id: &str, enabled: bool) -> (WappMethod, Value) {
    (WappMethod::SetWappEnabled, json!({ "id": id, "enabled": enabled }))
}

/// `wappStorageGet(wappId, key)`.
pub fn wapp_storage_get_request(wapp_id: &str, key: &str) -> (WappMethod, Value) {
    (WappMethod::WappStorageGet, json!({ "wappId": wapp_id, "key": key }))
}

/// `wappStorageSet(wappId, key, value)`.
pub fn wapp_storage_set_request(
    wapp_id: &str,
    key: &str,
    value: &Value,
) -> (WappMethod, Value) {
    (
        WappMethod::WappStorageSet,
        json!({ "wappId": wapp_id, "key": key, "value": value }),
    )
}

/// `wappStorageRemove(wappId, key)`.
pub fn wapp_storage_remove_request(wapp_id: &str, key: &str) -> (WappMethod, Value) {
    (WappMethod::WappStorageRemove, json!({ "wappId": wapp_id, "key": key }))
}

/// `readWappFile(id, relativePath)` — the answer is the byte array
/// (the host's decode).
pub fn read_wapp_file_request(id: &str, relative_path: &str) -> (WappMethod, Value) {
    (
        WappMethod::ReadWappFile,
        json!({ "id": id, "relativePath": relative_path }),
    )
}

/// `auditLog(wappId, event, detail)` — fire-and-forget; the
/// transport swallows a down IPC (the host's policy). The detail
/// must never contain argument contents.
pub fn wapp_audit_log_request(
    wapp_id: &str,
    event: &str,
    detail: &str,
) -> (WappMethod, Value) {
    (
        WappMethod::WappAuditLog,
        json!({ "wappId": wapp_id, "event": event, "detail": detail }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_are_the_react_invokes() {
        assert_eq!(WappMethod::ListWapps.wire_name(), "list_wapps");
        assert_eq!(WappMethod::GetWappManifest.wire_name(), "get_wapp_manifest");
        assert_eq!(WappMethod::ExecuteDriverCommand.wire_name(), "execute_wapp_driver_command");
        assert_eq!(
            WappMethod::InspectWappPackageWithDialog.wire_name(),
            "inspect_wapp_package_with_dialog"
        );
        assert_eq!(WappMethod::InstallWapp.wire_name(), "install_wapp");
        assert_eq!(WappMethod::RemoveWapp.wire_name(), "remove_wapp");
        assert_eq!(WappMethod::SetWappEnabled.wire_name(), "set_wapp_enabled");
        assert_eq!(WappMethod::WappStorageGet.wire_name(), "wapp_storage_get");
        assert_eq!(WappMethod::WappStorageSet.wire_name(), "wapp_storage_set");
        assert_eq!(WappMethod::WappStorageRemove.wire_name(), "wapp_storage_remove");
        assert_eq!(WappMethod::ReadWappFile.wire_name(), "read_wapp_file");
        assert_eq!(WappMethod::WappAuditLog.wire_name(), "wapp_audit_log");
        assert_eq!(WAPPS_CHANGED_EVENT, "wapps:changed");
    }

    #[test]
    fn the_simple_shapers_carry_their_keys() {
        let (_, payload) = list_wapps_request();
        assert_eq!(payload, json!({}), "no payload keys");
        let (_, payload) = get_wapp_manifest_request("w-1");
        assert_eq!(payload, json!({ "id": "w-1" }));
        let (_, payload) = remove_wapp_request("w-1");
        assert_eq!(payload, json!({ "id": "w-1" }));
        let (_, payload) = set_wapp_enabled_request("w-1", true);
        assert_eq!(payload, json!({ "id": "w-1", "enabled": true }));
        let (_, payload) = wapp_storage_get_request("w-1", "k");
        assert_eq!(payload, json!({ "wappId": "w-1", "key": "k" }));
        let (_, payload) = wapp_storage_set_request("w-1", "k", &json!({ "a": 1 }));
        assert_eq!(payload["wappId"], "w-1");
        assert_eq!(payload["key"], "k");
        assert_eq!(payload["value"]["a"], 1);
        let (_, payload) = wapp_storage_remove_request("w-1", "k");
        assert_eq!(payload, json!({ "wappId": "w-1", "key": "k" }));
        let (_, payload) = read_wapp_file_request("w-1", "assets/icon.png");
        assert_eq!(payload, json!({ "id": "w-1", "relativePath": "assets/icon.png" }));
        let (_, payload) = wapp_audit_log_request("w-1", "sensitive_call", "query on c-9");
        assert_eq!(payload, json!({ "wappId": "w-1", "event": "sensitive_call", "detail": "query on c-9" }));
    }

    #[test]
    fn the_driver_command_wraps_its_request() {
        let request = WappDriverCommandRequest {
            wapp_id: "w-1".to_owned(),
            connection_id: "c-1".to_owned(),
            command: "listTables".to_owned(),
            args: Some(json!({ "schema": "public" })),
        };
        let (method, payload) = execute_driver_command_request(&request);
        assert_eq!(method, WappMethod::ExecuteDriverCommand);
        assert_eq!(payload["request"]["wappId"], "w-1");
        assert_eq!(payload["request"]["connectionId"], "c-1");
        assert_eq!(payload["request"]["command"], "listTables");
        assert_eq!(payload["request"]["args"]["schema"], "public");
        // Absent args stay absent.
        let request = WappDriverCommandRequest {
            args: None,
            wapp_id: "w-1".to_owned(),
            connection_id: "c-1".to_owned(),
            command: "ping".to_owned(),
        };
        let (_, payload) = execute_driver_command_request(&request);
        assert!(payload["request"].get("args").is_none());
    }

    #[test]
    fn the_pick_pair_rides_the_kind_and_the_escape_hatch() {
        let (_, payload) = inspect_wapp_package_with_dialog_request(WappPackageKind::Zip, None);
        assert_eq!(payload, json!({ "packageKind": "zip" }), "no override in production");
        let (_, payload) =
            inspect_wapp_package_with_dialog_request(WappPackageKind::Folder, Some("/tmp/w"));
        assert_eq!(payload["packageKind"], "folder");
        assert_eq!(payload["overridePath"], "/tmp/w");
        let (_, payload) = install_wapp_request("pick-1", None);
        assert_eq!(payload, json!({ "pickToken": "pick-1" }));
        let (_, payload) = install_wapp_request("pick-1", Some("/tmp/w"));
        assert_eq!(payload["pickToken"], "pick-1");
        assert_eq!(payload["overridePath"], "/tmp/w");
    }

    #[test]
    fn the_wire_contracts_round_trip_their_camel_case() {
        let manifest = WappManifest {
            id: "acme.tools".to_owned(),
            name: "ACME Tools".to_owned(),
            version: "1.2.0".to_owned(),
            api_version: 1,
            author: Some("ACME".to_owned()),
            description: None,
            icon: Some("assets/icon.png".to_owned()),
            entry: None,
            contributes: WappContributions {
                pages: vec![WappPageContribution {
                    id: "main".to_owned(),
                    title: "ACME".to_owned(),
                    icon: None,
                    show_in: "workspace".to_owned(),
                }],
                themes: vec![WappThemeContribution {
                    id: "acme-night".to_owned(),
                    name: "ACME Night".to_owned(),
                    tokens_css: "themes/acme.css".to_owned(),
                    modes: vec!["dark".to_owned()],
                    preview_image: None,
                    editor_json: None,
                    charts_json: None,
                    icons_dir: None,
                }],
            },
            permissions: vec![WappPermission::CommandInvoke, WappPermission::StorageLocal],
            backend: None,
            ai: None,
        };
        let wire = serde_json::to_value(&manifest).unwrap();
        assert_eq!(wire["id"], "acme.tools");
        assert_eq!(wire["apiVersion"], 1);
        assert_eq!(wire["permissions"], json!(["command:invoke", "storage:local"]));
        assert_eq!(wire["contributes"]["pages"][0]["showIn"], "workspace");
        assert_eq!(wire["contributes"]["themes"][0]["tokensCss"], "themes/acme.css");
        assert!(wire.get("description").is_none(), "absent stays absent");
        let back: WappManifest = serde_json::from_value(wire).unwrap();
        assert_eq!(back, manifest);

        // The preview wraps the manifest.
        let preview = WappPackagePreview {
            pick_token: "pick-1".to_owned(),
            package_label: "acme-tools-1.2.0.zip".to_owned(),
            manifest: manifest.clone(),
        };
        let wire = serde_json::to_value(&preview).unwrap();
        assert_eq!(wire["pickToken"], "pick-1");
        assert_eq!(wire["packageLabel"], "acme-tools-1.2.0.zip");
        assert_eq!(wire["manifest"]["name"], "ACME Tools");
        let back: WappPackagePreview = serde_json::from_value(wire).unwrap();
        assert_eq!(back, preview);
    }

    #[test]
    fn the_summary_round_trips_its_rows() {
        let summary = WappSummary {
            id: "acme.tools".to_owned(),
            name: "ACME Tools".to_owned(),
            version: "1.2.0".to_owned(),
            api_version: 1,
            author: None,
            description: Some("Tools".to_owned()),
            icon: None,
            enabled: true,
            permissions: vec![WappPermission::UiNotify],
            pages: vec![WappPageSummary {
                id: "main".to_owned(),
                title: "ACME".to_owned(),
                icon: Some("i.png".to_owned()),
            }],
            themes: Vec::new(),
            ai_tools: Some(vec![WappAiToolSummary {
                name: "explain".to_owned(),
                description: None,
                read_only: true,
            }]),
        };
        let wire = serde_json::to_value(&summary).unwrap();
        assert_eq!(wire["enabled"], true);
        assert_eq!(wire["permissions"], json!(["ui:notify"]));
        assert_eq!(wire["pages"][0]["icon"], "i.png");
        assert_eq!(wire["aiTools"][0]["readOnly"], true);
        assert_eq!(wire["themes"], json!([]), "a required list rides even empty");
        let back: WappSummary = serde_json::from_value(wire).unwrap();
        assert_eq!(back, summary);
        // The required fields refuse an absent spelling (React's
        // `enabled: boolean` is not optional; a silent `false`
        // default would hide a malformed payload).
        let mut missing = serde_json::to_value(&summary).unwrap();
        assert!(missing.as_object_mut().unwrap().remove("enabled").is_some());
        assert!(
            serde_json::from_value::<WappSummary>(missing).is_err(),
            "a summary without its enabled flag refuses the decode"
        );
    }
}
