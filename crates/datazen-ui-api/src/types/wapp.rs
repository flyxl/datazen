//! Port of `packages/frontend/src/types/wapp.ts` (T295) — the Runtime
//! Workspace App (wapp) host types the frontend reads.
//!
//! The React file's own header records the twin law: it **mirrors**
//! `crates/datazen/src/wapps/manifest.rs` (serde camelCase) and the
//! `WappSummary` payload of the `list_wapps` host command. The kernel's
//! manifest struct stays the authoritative validator (id/semver/path rules,
//! `deny_unknown_fields`); this module is the **frontend's wasm-safe
//! contract mirror** — the same wire shapes decoded leniently (a TS type
//! never rejects unknown fields), one definition per interface the React
//! file declares, plus the `WAPP_API_VERSION` handshake constant the file
//! owns on its side.
//!
//! The file is NOT on the `types/index.ts` aggregate's re-export surface
//! (its importers — the wapps views and the install dialog — import
//! `'../types/wapp'` directly), so the family root does not re-export this
//! module either: reachability rides the module path
//! `datazen_ui_api::types::wapp`, the twin of React's import path.
//!
//! Pure contracts — no IO, no state — wasm32-clean like the family.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// React `WAPP_API_VERSION = 2` — the API version handshake; must match
/// `WAPP_API_VERSION` on the kernel side (`crate`-local there, mirrored
/// here). The pin below asserts the literal.
pub const WAPP_API_VERSION: u32 = 2;

/// React `WappPermission` — serialized strings of the kernel `Permission`
/// enum (serde renames): the five permissions a wapp can be granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl WappPermission {
    /// The wire spelling (the kernel enum's own `as_str`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContextConnections => "context:connections",
            Self::CommandInvoke => "command:invoke",
            Self::SessionStateful => "session:stateful",
            Self::StorageLocal => "storage:local",
            Self::UiNotify => "ui:notify",
        }
    }
}

/// React `WappPageContribution` — a page contribution from a wapp manifest.
/// Only `workspace` is supported by this host (`showIn`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageContribution {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub show_in: String,
}

/// React `WappThemeContribution` — a theme contribution from a wapp
/// manifest: the tokens CSS path, the light/dark modes, and the legacy
/// pack's optional overlays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Optional CodeMirror color overlay (legacy ThemePack `editor.json`).
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

/// React `WappContributions` — the manifest's two contribution lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappContributions {
    #[serde(default)]
    pub pages: Vec<WappPageContribution>,
    #[serde(default)]
    pub themes: Vec<WappThemeContribution>,
}

/// React `WappAiTool` — one AI tool a wapp declares. The TS `command` leg
/// is the `'query' | 'execute'` literal union; the kernel keeps the same
/// rule by string validation, the mirror keeps it as the enum the union
/// spells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub read_only: bool,
    pub command: WappAiToolCommand,
    /// Persistent connection id resolved by the host; never passed to the
    /// model.
    pub connection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// JSON Schema for the model-supplied arguments (an object).
    pub input_schema: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<BTreeMap<String, serde_json::Value>>,
}

/// The TS `'query' | 'execute'` literal union on [`WappAiTool::command`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WappAiToolCommand {
    #[serde(rename = "query")]
    Query,
    #[serde(rename = "execute")]
    Execute,
}

/// React `WappAiContribution` — the manifest's opt-in AI tools block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiContribution {
    #[serde(default)]
    pub tools: Vec<WappAiTool>,
}

/// React `WappAiToolSummary` — the summary strip a `WappSummary` carries
/// when the manifest declares AI tools.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiToolSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub read_only: bool,
}

/// React `WappManifest` — the full manifest as returned by the host's
/// `get_wapp_manifest`.
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
    /// Optional package-level icon (square brand image) shown in wapp lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default)]
    pub contributes: WappContributions,
    #[serde(default)]
    pub permissions: Vec<WappPermission>,
    /// Reserved for P2 backend extensions; must be null/absent in v1 (the
    /// kernel rejects a present value — the mirror just carries it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<serde_json::Value>,
    /// Opt-in AI tools declared by the plugin manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<WappAiContribution>,
}

/// React `WappPageSummary` — a page entry inside a `WappSummary` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageSummary {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// React `WappThemeSummary` — a theme entry inside a `WappSummary` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappThemeSummary {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
}

/// React `WappSummary` — the installed wapp row as returned by the host's
/// `list_wapps` command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Optional package-level icon path (mirrors `WappManifest.icon`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    /// The handshake literal: the frontend constant must equal the kernel's
    /// own `WAPP_API_VERSION` (the whole point of the twin).
    #[test]
    fn the_api_version_handshake_is_two() {
        assert_eq!(WAPP_API_VERSION, 2);
    }

    /// The five permission strings — the kernel enum's serde renames, the
    /// only spellings a TS union accepts.
    #[test]
    fn the_permission_literals_match_the_kernel_renames() {
        let pairs = [
            (WappPermission::ContextConnections, "context:connections"),
            (WappPermission::CommandInvoke, "command:invoke"),
            (WappPermission::SessionStateful, "session:stateful"),
            (WappPermission::StorageLocal, "storage:local"),
            (WappPermission::UiNotify, "ui:notify"),
        ];
        for (value, literal) in pairs {
            assert_eq!(value.as_str(), literal);
            let parsed: WappPermission = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    /// The manifest wire: camelCase legs, optional legs absent, the AI
    /// block's command union, and a decode that keeps unknown fields out of
    /// the TS view (a present `backend` leg rides — the kernel's rule, not
    /// the mirror's, rejects it).
    #[test]
    fn a_manifest_round_trips_the_kernel_wire_shape() {
        let manifest = WappManifest {
            id: "acme.quick-notes".into(),
            name: "Quick Notes".into(),
            version: "1.2.0".into(),
            api_version: WAPP_API_VERSION,
            author: None,
            description: Some("Notes in a workspace page".into()),
            icon: Some("assets/icon.png".into()),
            entry: Some("dist/index.html".into()),
            contributes: WappContributions {
                pages: vec![WappPageContribution {
                    id: "notes".into(),
                    title: "Notes".into(),
                    icon: None,
                    show_in: "workspace".into(),
                }],
                themes: vec![WappThemeContribution {
                    id: "slate".into(),
                    name: "Slate".into(),
                    tokens_css: "themes/slate/tokens.css".into(),
                    modes: vec!["light".into(), "dark".into()],
                    preview_image: None,
                    editor_json: Some("themes/slate/editor.json".into()),
                    charts_json: None,
                    icons_dir: None,
                }],
            },
            permissions: vec![WappPermission::ContextConnections],
            backend: None,
            ai: Some(WappAiContribution {
                tools: vec![WappAiTool {
                    name: "search_notes".into(),
                    description: Some("Search the notes".into()),
                    read_only: true,
                    command: WappAiToolCommand::Query,
                    connection: "primary".into(),
                    database: None,
                    input_schema: BTreeMap::from([(
                        "type".into(),
                        serde_json::json!("object"),
                    )]),
                    input: None,
                }],
            }),
        };
        let wire = encode(&manifest);
        // camelCase law spot pins.
        assert_eq!(wire["apiVersion"], serde_json::json!(2));
        assert_eq!(wire["contributes"]["pages"][0]["showIn"], "workspace");
        assert_eq!(wire["ai"]["tools"][0]["inputSchema"]["type"], "object");
        assert!(wire.get("author").is_none(), "an absent optional stays absent");
        let back: WappManifest = decode(wire);
        assert_eq!(back, manifest);
    }

    /// The `WappSummary` payload: the enabled row with its three summary
    /// lists (pages, themes, ai tools).
    #[test]
    fn a_summary_round_trips_with_the_enabled_flag() {
        let summary = WappSummary {
            id: "acme.quick-notes".into(),
            name: "Quick Notes".into(),
            version: "1.2.0".into(),
            api_version: WAPP_API_VERSION,
            author: Some("ACME".into()),
            description: None,
            icon: None,
            enabled: true,
            permissions: vec![WappPermission::StorageLocal, WappPermission::UiNotify],
            pages: vec![WappPageSummary {
                id: "notes".into(),
                title: "Notes".into(),
                icon: None,
            }],
            themes: vec![WappThemeSummary {
                id: "slate".into(),
                name: "Slate".into(),
                modes: vec!["dark".into()],
            }],
            ai_tools: Some(vec![WappAiToolSummary {
                name: "search_notes".into(),
                description: None,
                read_only: true,
            }]),
        };
        let wire = encode(&summary);
        assert_eq!(wire["enabled"], serde_json::json!(true));
        assert_eq!(wire["permissions"][1], "ui:notify");
        assert_eq!(wire["themes"][0]["modes"][0], "dark");
        let back: WappSummary = decode(wire);
        assert_eq!(back, summary);
    }

    /// A summary without AI tools omits the leg (the `Present only when…`
    /// law — not an explicit null).
    #[test]
    fn a_summary_without_ai_tools_omits_the_leg() {
        let summary = WappSummary {
            id: "x.y".into(),
            name: "X".into(),
            version: "0.1.0".into(),
            api_version: WAPP_API_VERSION,
            author: None,
            description: None,
            icon: None,
            enabled: false,
            permissions: Vec::new(),
            pages: Vec::new(),
            themes: Vec::new(),
            ai_tools: None,
        };
        let wire = encode(&summary);
        assert!(wire.get("aiTools").is_none());
        let back: WappSummary = decode(wire);
        assert_eq!(back, summary);
    }
}
