//! Port of `packages/frontend/src/commands/languagePacks.ts` (T331).
//!
//! The language-pack command family — the legacy-IPC shaper pattern
//! (`backup.rs` / `file.rs`): every call becomes a
//! [`LanguagePackRequest`] (the command name plus the exact args
//! object the React wrapper passes); the wire types are the file's
//! own, camelCase as the transport speaks.
//!
//! Two wire-level decisions the React comments record:
//!
//! - `inspectLanguagePackFileWithDialog` is a native pick +
//!   validate-only inspect — the receipt is the preview or `null`
//!   when the dialog was dismissed; the `overridePath` escape hatch
//!   is webdriver/E2E-only (the shaper still carries it — the wire
//!   accepts it — but production callers pass `None`, exactly as the
//!   React signature's optional implies).
//! - `installLanguagePack` installs what a prior inspect picked, via
//!   its `pickToken`.
//!
//! Known deltas: none — every command name and payload key is the
//! React source's verbatim. `invoke<void>` answers nothing; the
//! remove shaper carries no receipt type for the same reason.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// `LANGUAGE_PACKS_CHANGED_EVENT` (the desktop host's cross-window
/// bus event the settings card listens for).
pub const LANGUAGE_PACKS_CHANGED_EVENT: &str = "language-packs:changed";

/// One shaped legacy command (the `backup.rs` shaper shape).
#[derive(Clone, Debug, PartialEq)]
pub struct LanguagePackRequest {
    /// The legacy command name.
    pub command: &'static str,
    /// The exact args object the React wrapper passes.
    pub value: Value,
}

/// `LanguagePackManifest`: a manifest.json inside a language pack ZIP
/// (wire shape, camelCase).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackManifest {
    pub pack_version: i64,
    pub code: String,
    pub label: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `LanguagePackSummary`: one installed pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackSummary {
    pub code: String,
    pub label: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Total translated messages across all domain files.
    pub message_count: i64,
    /// Install directory name (always equals `code`).
    pub file_name: String,
}

/// `LanguagePackDomainStat`: one domain's message count in the
/// install preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackDomainStat {
    pub domain: String,
    pub message_count: i64,
}

/// `LanguagePackPreview`: the validate-only inspect's receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackPreview {
    pub pick_token: String,
    pub package_label: String,
    pub manifest: LanguagePackManifest,
    pub message_count: i64,
    pub domains: Vec<LanguagePackDomainStat>,
}

/// `LanguagePackMessages`: `read_language_pack_messages`'s receipt —
/// `label` plus the full `messages` map, everything `registerLocale`
/// needs in one IPC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackMessages {
    pub code: String,
    pub label: String,
    pub messages: std::collections::BTreeMap<String, String>,
}

/// `languagePackCommands.listLanguagePacks()`.
pub fn list_language_packs_request() -> LanguagePackRequest {
    LanguagePackRequest { command: "list_language_packs", value: json!({}) }
}

/// `languagePackCommands.readLanguagePack(code)`.
pub fn read_language_pack_request(code: &str) -> LanguagePackRequest {
    LanguagePackRequest { command: "read_language_pack_messages", value: json!({ "code": code }) }
}

/// `languagePackCommands.inspectLanguagePackFileWithDialog(overridePath?)`:
/// native picker + validate-only inspect; the receipt is the preview
/// or `null` when cancelled.
pub fn inspect_language_pack_file_with_dialog_request(
    override_path: Option<&str>,
) -> LanguagePackRequest {
    // JS drops `undefined` keys; the override rides only when given.
    let value = match override_path {
        Some(path) => json!({ "overridePath": path }),
        None => json!({}),
    };
    LanguagePackRequest { command: "inspect_language_pack_file_with_dialog", value }
}

/// `languagePackCommands.installLanguagePack(pickToken?, overridePath?)`.
pub fn install_language_pack_request(
    pick_token: Option<&str>,
    override_path: Option<&str>,
) -> LanguagePackRequest {
    let mut value = json!({});
    if let Some(token) = pick_token {
        value["pickToken"] = json!(token);
    }
    if let Some(path) = override_path {
        value["overridePath"] = json!(path);
    }
    LanguagePackRequest { command: "install_language_pack", value }
}

/// `languagePackCommands.removeLanguagePack(code)`.
pub fn remove_language_pack_request(code: &str) -> LanguagePackRequest {
    LanguagePackRequest { command: "remove_language_pack", value: json!({ "code": code }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_name_is_the_react_literal() {
        assert_eq!(list_language_packs_request().command, "list_language_packs");
        assert_eq!(read_language_pack_request("zh").command, "read_language_pack_messages");
        assert_eq!(
            inspect_language_pack_file_with_dialog_request(None).command,
            "inspect_language_pack_file_with_dialog"
        );
        assert_eq!(install_language_pack_request(None, None).command, "install_language_pack");
        assert_eq!(remove_language_pack_request("zh").command, "remove_language_pack");
    }

    #[test]
    fn payload_keys_match_the_react_wrappers() {
        // listLanguagePacks() — no payload.
        assert_eq!(list_language_packs_request().value, json!({}));
        // readLanguagePack(code).
        assert_eq!(read_language_pack_request("zh-CN").value, json!({ "code": "zh-CN" }));
        // removeLanguagePack(code).
        assert_eq!(remove_language_pack_request("zh-CN").value, json!({ "code": "zh-CN" }));
        // inspect: no override in production.
        assert_eq!(
            inspect_language_pack_file_with_dialog_request(None).value,
            json!({}),
            "undefined keys drop"
        );
        assert_eq!(
            inspect_language_pack_file_with_dialog_request(Some("/e2e/pack.zip")).value,
            json!({ "overridePath": "/e2e/pack.zip" })
        );
        // install: the token rides only when given.
        assert_eq!(install_language_pack_request(None, None).value, json!({}));
        assert_eq!(
            install_language_pack_request(Some("pick-1"), None).value,
            json!({ "pickToken": "pick-1" })
        );
        assert_eq!(
            install_language_pack_request(Some("pick-1"), Some("/e2e/pack.zip")).value,
            json!({ "pickToken": "pick-1", "overridePath": "/e2e/pack.zip" })
        );
    }

    #[test]
    fn the_changed_event_is_the_host_literal() {
        assert_eq!(LANGUAGE_PACKS_CHANGED_EVENT, "language-packs:changed");
    }

    #[test]
    fn the_wire_types_round_trip_camel_case() {
        let summary = json!({
            "code": "zh-CN",
            "label": "简体中文",
            "version": "1.2.0",
            "messageCount": 3800,
            "fileName": "zh-CN",
            "author": null
        });
        let parsed: LanguagePackSummary = serde_json::from_value(summary).expect("decodes");
        assert_eq!(parsed.code, "zh-CN");
        assert_eq!(parsed.message_count, 3800);
        assert_eq!(parsed.author, None);

        let preview = json!({
            "pickToken": "pick-1",
            "packageLabel": "zh-CN Pack.zip",
            "manifest": {
                "packVersion": 1,
                "code": "zh-CN",
                "label": "简体中文",
                "version": "1.2.0"
            },
            "messageCount": 3800,
            "domains": [{ "domain": "sync", "messageCount": 1200 }]
        });
        let parsed: LanguagePackPreview = serde_json::from_value(preview).expect("decodes");
        assert_eq!(parsed.manifest.pack_version, 1);
        assert_eq!(parsed.domains[0].domain, "sync");

        let messages = json!({
            "code": "zh-CN",
            "label": "简体中文",
            "messages": { "common.save": "保存" }
        });
        let parsed: LanguagePackMessages = serde_json::from_value(messages).expect("decodes");
        assert_eq!(parsed.messages.get("common.save").map(String::as_str), Some("保存"));
    }
}
