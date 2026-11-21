//! Port of `packages/frontend/src/commands/settings.ts` (T388 shard /
//! T386 parity) — settings commands, form-split F4 shape: **no
//! form-factor branch**.
//!
//! The shared half rides the kernel track (`BackendClient.getSettings`
//! / `saveSettings` — the nested `SharedSettings` wire); the host half
//! rides the `HostSettingsPort` (desktop: the projecting client; web:
//! the inert port). The three `backendId === 'desktop'` ternaries the
//! file used to carry are gone — what used to be "local" is now a
//! port, and the desktop adapter owns the projection internally.
//!
//! ## What maps where
//!
//! - `flattenShared` is [`flatten_shared`], the pure JSON transform:
//!   the preferences and execution objects spread (execution over
//!   preferences — React's later spread wins), then `monitor`,
//!   `onboarding` (null → the key stays, `undefined` semantics kept by
//!   the caller dropping it), `rowCountMode`, `sqlSyntaxTheme`
//!   (preferences' own value, absent when unset), and
//!   `valuePresentation` with the issue-32 T06 default fallback
//!   (`{version: 1, renderers: {}}` — belt-and-braces against old
//!   backend payloads, `toSharedSettings` already defaults).
//! - `getSettings` is [`get_settings_request`]: BOTH halves in
//!   parallel; the extras merge **under** the shared contract (the
//!   kernel stays the authority for every field it owns) —
//!   [`merge_settings`] applies the order.
//! - `saveSettings` is [`save_settings_request`]: the host write first
//!   (desktop persists the full blob; web no-ops — either way the
//!   kernel save follows so both tracks agree), then the kernel save
//!   whose answer flattens back.
//! - `syncLanguageMenu` / `getLogPath` / `getAppExecutablePath` /
//!   `openPath` (deprecated, E2E-only) / `openLogDir` /
//!   `openWorkflowsDir` / `openContextDir` are the host-port calls
//!   (the [`SettingsMethod`] enum's wire names).
//!
//! Known deltas: the wire shapes stay opaque JSON (the `AppSettings`
//! contract is the store's own task; the commands layer passes the
//! objects through — the same ruling as the pass-throughs in
//! `commands/query.rs`).

// No production consumer yet (the settings-store track wires the
// faces); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde_json::{json, Value};

/// The settings faces (the kernel methods and the host-port calls).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsMethod {
    KernelGet,
    KernelSave,
    ReadHostExtras,
    WriteHostSettings,
    RebuildMenu,
    GetLogPath,
    GetAppExecutablePath,
    OpenPath,
    OpenLogDir,
    OpenWorkflowsDir,
    OpenContextDir,
}

impl SettingsMethod {
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::KernelGet => "getSettings",
            Self::KernelSave => "saveSettings",
            Self::ReadHostExtras => "readHostExtras",
            Self::WriteHostSettings => "writeHostSettings",
            Self::RebuildMenu => "rebuildMenu",
            Self::GetLogPath => "getLogPath",
            Self::GetAppExecutablePath => "getAppExecutablePath",
            Self::OpenPath => "openPath",
            Self::OpenLogDir => "openLogDir",
            Self::OpenWorkflowsDir => "openWorkflowsDir",
            Self::OpenContextDir => "openContextDir",
        }
    }
}

/// One shaped call.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingsRequest {
    pub method: SettingsMethod,
    pub value: Value,
}

/// React `DEFAULT_VALUE_PRESENTATION_SETTINGS` (issue-32 T06's
/// fallback): `{version: 1, renderers: {}}`.
pub(crate) fn default_value_presentation_settings() -> Value {
    json!({ "version": 1, "renderers": {} })
}

/// `flattenShared`: the nested shared wire → the store's flat view.
/// The key set is the React literal's; an absent `valuePresentation`
/// (or a non-object) falls to the default.
pub(crate) fn flatten_shared(shared: &Value) -> Value {
    let mut flat = serde_json::Map::new();
    // `{...shared.preferences, ...shared.execution}` — execution's
    // keys win (spread later).
    for half in ["preferences", "execution"] {
        if let Some(entries) = shared.get(half).and_then(Value::as_object) {
            for (key, value) in entries {
                flat.insert(key.clone(), value.clone());
            }
        }
    }
    for key in ["monitor", "onboarding", "rowCountMode"] {
        if let Some(value) = shared.get(key) {
            flat.insert(key.to_owned(), value.clone());
        }
    }
    // `sqlSyntaxTheme: shared.preferences.sqlSyntaxTheme ?? undefined`
    // — the preferences' own value (already spread above when set; a
    // null stays null, matching the explicit re-read).
    if let Some(theme) = shared.get("preferences").and_then(|p| p.get("sqlSyntaxTheme")) {
        flat.insert("sqlSyntaxTheme".to_owned(), theme.clone());
    }
    // `valuePresentation: shared.valuePresentation ?? DEFAULT` — the
    // belt-and-braces default for old backend payloads.
    let value_presentation = match shared.get("valuePresentation") {
        Some(value @ Value::Object(_)) => value.clone(),
        _ => default_value_presentation_settings(),
    };
    flat.insert("valuePresentation".to_owned(), value_presentation);
    Value::Object(flat)
}

/// `getSettings`'s plan: both halves in parallel, the extras merge
/// UNDER the shared contract (the kernel is the authority for every
/// field it owns).
pub(crate) fn get_settings_request() -> [SettingsRequest; 2] {
    [
        SettingsRequest { method: SettingsMethod::KernelGet, value: json!({}) },
        SettingsRequest { method: SettingsMethod::ReadHostExtras, value: json!({}) },
    ]
}

/// The merge order of the two answers: `{...extras, ...flattenShared(
/// shared)}` — the shared half's keys overwrite the extras'.
pub(crate) fn merge_settings(extras: &Value, shared: &Value) -> Value {
    let mut merged = serde_json::Map::new();
    if let Some(entries) = extras.as_object() {
        for (key, value) in entries {
            merged.insert(key.clone(), value.clone());
        }
    }
    if let Some(entries) = flatten_shared(shared).as_object() {
        for (key, value) in entries {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

/// `saveSettings`: the host write first, then the kernel save (the
/// answer flattens back). The wire payload is the settings blob the
/// host normalizes internally.
pub(crate) fn save_settings_request(settings: &Value) -> [SettingsRequest; 2] {
    [
        SettingsRequest {
            method: SettingsMethod::WriteHostSettings,
            value: json!({ "settings": settings }),
        },
        SettingsRequest {
            method: SettingsMethod::KernelSave,
            value: json!({ "settings": settings }),
        },
    ]
}

/// `syncLanguageMenu(language)`.
pub(crate) fn sync_language_menu_request(language: &str) -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::RebuildMenu, value: json!({ "language": language }) }
}

/// `getLogPath` (the host port read).
pub(crate) fn get_log_path_request() -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::GetLogPath, value: json!({}) }
}

/// `getAppExecutablePath`.
pub(crate) fn get_app_executable_path_request() -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::GetAppExecutablePath, value: json!({}) }
}

/// `openPath(path)` — deprecated, E2E-only; prefer the open_known_dir
/// trio below (one IPC).
pub(crate) fn open_path_request(path: &str) -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::OpenPath, value: json!({ "path": path }) }
}

/// `openLogDir`.
pub(crate) fn open_log_dir_request() -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::OpenLogDir, value: json!({}) }
}

/// `openWorkflowsDir`.
pub(crate) fn open_workflows_dir_request() -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::OpenWorkflowsDir, value: json!({}) }
}

/// `openContextDir`.
pub(crate) fn open_context_dir_request() -> SettingsRequest {
    SettingsRequest { method: SettingsMethod::OpenContextDir, value: json!({}) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flatten_spreads_execution_over_preferences_and_defaults_the_presentation() {
        let shared = json!({
            "preferences": {
                "theme": "dark",
                "language": "en",
                "sqlSyntaxTheme": null,
                "editorFontSize": 14,
            },
            "execution": {
                "autoCommit": true,
                // Execution wins the collision.
                "editorFontSize": 16,
            },
            "monitor": { "refreshPaused": false },
            "onboarding": null,
            "rowCountMode": "exact",
        });
        let flat = flatten_shared(&shared);
        assert_eq!(flat["theme"], "dark");
        assert_eq!(flat["language"], "en");
        assert_eq!(flat["autoCommit"], true);
        assert_eq!(flat["editorFontSize"], 16, "execution's spread wins");
        assert_eq!(flat["monitor"]["refreshPaused"], false);
        assert_eq!(flat["onboarding"], Value::Null, "null rides (undefined stays dropped)");
        assert_eq!(flat["rowCountMode"], "exact");
        assert_eq!(flat["sqlSyntaxTheme"], Value::Null, "the preferences' own value re-read");
        // valuePresentation absent → the issue-32 default.
        assert_eq!(flat["valuePresentation"], json!({ "version": 1, "renderers": {} }));

        // A present object rides verbatim; a non-object falls to the
        // default (the belt-and-braces).
        let shared = json!({ "valuePresentation": { "version": 2, "renderers": { "a": 1 } } });
        assert_eq!(flatten_shared(&shared)["valuePresentation"]["version"], 2);
        let shared = json!({ "valuePresentation": "corrupt" });
        assert_eq!(
            flatten_shared(&shared)["valuePresentation"],
            json!({ "version": 1, "renderers": {} })
        );
    }

    #[test]
    fn the_merge_puts_the_shared_contract_over_the_extras() {
        let extras = json!({ "nativeConfig": { "x": 1 }, "theme": "light" });
        let shared = json!({ "preferences": { "theme": "dark" } });
        let merged = merge_settings(&extras, &shared);
        assert_eq!(merged["nativeConfig"]["x"], 1, "the host extras survive");
        assert_eq!(merged["theme"], "dark", "the kernel field wins the collision");
    }

    #[test]
    fn the_faces_shape_their_payloads() {
        let [kernel, extras] = get_settings_request();
        assert_eq!(kernel.method.wire_name(), "getSettings");
        assert_eq!(extras.method.wire_name(), "readHostExtras");

        let [host, kernel] = save_settings_request(&json!({ "theme": "dark" }));
        assert_eq!(host.method.wire_name(), "writeHostSettings", "the host write first");
        assert_eq!(kernel.method.wire_name(), "saveSettings");
        assert_eq!(kernel.value["settings"]["theme"], "dark");

        assert_eq!(sync_language_menu_request("zh-CN").value["language"], "zh-CN");
        assert_eq!(get_log_path_request().method.wire_name(), "getLogPath");
        assert_eq!(get_app_executable_path_request().method.wire_name(), "getAppExecutablePath");
        assert_eq!(open_path_request("/tmp").value["path"], "/tmp");
        assert_eq!(open_log_dir_request().method.wire_name(), "openLogDir");
        assert_eq!(open_workflows_dir_request().method.wire_name(), "openWorkflowsDir");
        assert_eq!(open_context_dir_request().method.wire_name(), "openContextDir");
    }
}
