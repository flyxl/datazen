//! The McpClientSection state machine and view-models (the state half
//! of `windows/settings/McpClientSection.tsx`'s port, T336 — split out
//! of [`super::mcp_client_section`] for the file-size rule, the
//! `model_profile_form` precedent): the config/tool view-models, the
//! env-row converters, the form draft, the save/delete plans and the
//! section state with its handlers. The mapping table lives in the
//! section module's header.

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use datazen_ui_api::{WebMcpConnectedServer, WebMcpToolInfo};
use rune::TextEdit;

use crate::commands::ai::AiRequest;
use crate::commands::ai::mcp::{mcp_client_list_request, mcp_client_tools_request};

/// React `McpClientStatus` (types/index.ts) — one connected server.
/// The api crate's [`WebMcpConnectedServer`] is the identical wire
/// shape (`serverId` / `serverName` / `toolsCount`).
pub type McpClientStatus = WebMcpConnectedServer;

/// React `McpToolInfo` — one tool a connected server exposes. The api
/// crate's [`WebMcpToolInfo`] is the identical wire shape.
pub type McpToolInfo = WebMcpToolInfo;

// ---------------------------------------------------------------------------
// The config view-model (React `McpServerConfig`)
// ---------------------------------------------------------------------------

/// `transport: 'stdio'` — the React union's only member today; one
/// value so a second member is an explicit change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpTransport {
    #[default]
    Stdio,
}

impl McpTransport {
    /// The React literal (`'stdio'`).
    pub fn as_str(self) -> &'static str {
        match self {
            McpTransport::Stdio => "stdio",
        }
    }
}

/// React `McpServerConfig` (types/index.ts, the settings-store shape
/// persisted under `mcpClientServers`). `command` is a plain `String`
/// (React's `command?: string` — `EMPTY_DRAFT` carries `''`).
#[derive(Clone, Debug, PartialEq)]
pub struct McpServerConfigVm {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub enabled: bool,
    /// When false, tools from this server are excluded from AI Chat.
    pub enabled_for_ai: bool,
}

impl Default for McpServerConfigVm {
    /// React `EMPTY_DRAFT` — the add form's starting shape.
    fn default() -> Self {
        McpServerConfigVm {
            id: String::new(),
            name: String::new(),
            transport: McpTransport::Stdio,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            enabled: true,
            enabled_for_ai: true,
        }
    }
}

/// `isValidMcpServerId`: `^[a-zA-Z0-9_-]+$` (non-empty) — the same
/// rule the desktop `validate_mcp_server_id` mirrors.
pub fn is_valid_mcp_server_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ---------------------------------------------------------------------------
// Env rows (React `EnvRow` + the two converters)
// ---------------------------------------------------------------------------

/// One env row: `{ key, value }` as two `TextEdit`s (the field atoms
/// edit them in place).
#[derive(Clone, Default, PartialEq)]
pub struct EnvRowEdit {
    pub key: TextEdit,
    pub value: TextEdit,
}

/// `envToRows`: the env map as rows.
pub fn env_to_rows(env: &BTreeMap<String, String>) -> Vec<EnvRowEdit> {
    env.iter()
        .map(|(key, value)| EnvRowEdit {
            key: TextEdit::new(key.clone()),
            value: TextEdit::new(value.clone()),
        })
        .collect()
}

/// `rowsToEnv`: trimmed keys, empty keys dropped, a later duplicate
/// key wins (the React loop's last write).
pub fn rows_to_env(rows: &[EnvRowEdit]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for row in rows {
        let key = row.key.text.trim().to_owned();
        if !key.is_empty() {
            env.insert(key, row.value.text.clone());
        }
    }
    env
}

// ---------------------------------------------------------------------------
// The form draft (React `draft` + `argsText`)
// ---------------------------------------------------------------------------

/// The add/edit form's draft: the four text inputs are `TextEdit`s
/// (`model_profile_form`'s convention); the toggles are plain booleans
/// (default `true`, `EMPTY_DRAFT`'s shape).
#[derive(Clone, Default, PartialEq)]
pub struct McpServerFormDraft {
    pub id: TextEdit,
    pub name: TextEdit,
    pub command: TextEdit,
    /// `argsText` — the textarea's newline-joined value.
    pub args: TextEdit,
    pub enabled: bool,
    pub enabled_for_ai: bool,
}

impl McpServerFormDraft {
    /// `EMPTY_DRAFT` (both toggles on).
    pub fn empty() -> Self {
        McpServerFormDraft { enabled: true, enabled_for_ai: true, ..Default::default() }
    }

    /// `startEdit`'s seed: the config's values (args joined with
    /// newlines, env as rows).
    pub fn from_config(config: &McpServerConfigVm) -> (Self, Vec<EnvRowEdit>) {
        (
            McpServerFormDraft {
                id: TextEdit::new(config.id.clone()),
                name: TextEdit::new(config.name.clone()),
                command: TextEdit::new(config.command.clone()),
                args: TextEdit::new(config.args.join("\n")),
                enabled: config.enabled,
                enabled_for_ai: config.enabled_for_ai,
            },
            env_to_rows(&config.env),
        )
    }
}

// ---------------------------------------------------------------------------
// The section state
// ---------------------------------------------------------------------------

/// The save walk's outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum SavePlan {
    /// The handler's first early return (`!draft.id.trim() ||
    /// !draft.command?.trim()`) — unreachable through the button (its
    /// disabled state is the same rule); carried for host parity.
    Refused,
    /// `!isValidMcpServerId(trimmedId)`: `save_error` shows
    /// `t("mcpClient.invalidId")`.
    InvalidId,
    /// A fresh config whose id already exists: `save_error` shows
    /// `t("mcpClient.duplicateId")`.
    DuplicateId,
    /// Save: the built config and the next list (replace the edited
    /// entry, append a new one). `saving` is armed; the host persists
    /// through `saveMcpClientServers` and answers `note_save_ok` /
    /// `note_save_failed`.
    Save { config: McpServerConfigVm, next: Vec<McpServerConfigVm> },
}

impl SavePlan {
    /// The error-line key the outcome arms, when it arms one.
    pub fn error_key(&self) -> Option<&'static str> {
        match self {
            SavePlan::InvalidId => Some("mcpClient.invalidId"),
            SavePlan::DuplicateId => Some("mcpClient.duplicateId"),
            _ => None,
        }
    }
}

/// `handleDelete`: the next list plus the per-server error clear that
/// follows the awaited save.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletePlan {
    pub next: Vec<McpServerConfigVm>,
    /// `clearMcpServerError(id)` — the store clear the host runs after
    /// the save lands.
    pub clear_server_error_id: String,
}

/// The section's state: the store projections (host-delivered) plus
/// React's `useState` fields.
#[derive(Clone, Default, PartialEq)]
pub struct McpClientSectionState {
    // ---- useSettingsStore / useAiStore projections ----
    /// `savedConfigs` (`settings.mcpClientServers ?? []`).
    pub saved_configs: Vec<McpServerConfigVm>,
    /// `mcpServers` — the connected runtime list.
    pub mcp_servers: Vec<McpClientStatus>,
    /// `mcpTools`.
    pub mcp_tools: Vec<McpToolInfo>,
    /// `mcpConnecting`.
    pub mcp_connecting: bool,
    /// `mcpConnectingServerId`.
    pub mcp_connecting_server_id: Option<String>,
    /// `mcpError` — the global error strip.
    pub mcp_error: Option<String>,
    /// `mcpServerErrors` — the per-server strips.
    pub mcp_server_errors: BTreeMap<String, String>,
    // ---- useState ----
    /// `expandedServers` — the runtime rows' tool-list expansion.
    pub expanded_servers: BTreeSet<String>,
    /// `showForm`.
    pub show_form: bool,
    /// `editingId`.
    pub editing_id: Option<String>,
    /// `draft` + `argsText`.
    pub draft: McpServerFormDraft,
    /// `envRows`.
    pub env_rows: Vec<EnvRowEdit>,
    /// `saving`.
    pub saving: bool,
    /// `saveError` — the i18n KEY (see the module header's deltas).
    pub save_error: Option<&'static str>,
}

impl McpClientSectionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The mount effect: the load pair the host runs (`loadMcpServers`
    /// + `loadMcpTools`, both fire-and-forget).
    pub fn on_mount(&self) -> [AiRequest; 2] {
        [mcp_client_list_request(), mcp_client_tools_request()]
    }

    /// `loadMcpServers`' answer (`set({ mcpServers })`).
    pub fn deliver_servers(&mut self, servers: Vec<McpClientStatus>) {
        self.mcp_servers = servers;
    }

    /// `loadMcpTools`' answer (`set({ mcpTools })`).
    pub fn deliver_tools(&mut self, tools: Vec<McpToolInfo>) {
        self.mcp_tools = tools;
    }

    // ---- the saved-config projections ----

    /// `trimmedDraftId`.
    pub fn trimmed_draft_id(&self) -> &str {
        self.draft.id.text.trim()
    }

    /// `idInvalid`: a non-empty draft id that fails the id rule.
    pub fn id_invalid(&self) -> bool {
        let id = self.trimmed_draft_id();
        !id.is_empty() && !is_valid_mcp_server_id(id)
    }

    /// `idDuplicate`: a FRESH config (not editing) whose trimmed id
    /// collides with a saved one.
    pub fn id_duplicate(&self) -> bool {
        self.editing_id.is_none()
            && !self.trimmed_draft_id().is_empty()
            && self.saved_configs.iter().any(|c| c.id == self.trimmed_draft_id())
    }

    /// The save button's disabled rule, verbatim.
    pub fn save_disabled(&self) -> bool {
        self.saving
            || self.trimmed_draft_id().is_empty()
            || self.draft.command.text.trim().is_empty()
            || self.id_invalid()
            || self.id_duplicate()
    }

    // ---- the form ----

    /// `startAdd`: reset every form field and open the form.
    pub fn start_add(&mut self) {
        self.editing_id = None;
        self.draft = McpServerFormDraft::empty();
        self.env_rows = Vec::new();
        self.save_error = None;
        self.show_form = true;
    }

    /// `startEdit`: seed the form from the config.
    pub fn start_edit(&mut self, config: &McpServerConfigVm) {
        self.editing_id = Some(config.id.clone());
        let (draft, rows) = McpServerFormDraft::from_config(config);
        self.draft = draft;
        self.env_rows = rows;
        self.save_error = None;
        self.show_form = true;
    }

    /// The id input's `onChange` → `setSaveError(null)` — the host
    /// calls this when its edit machinery mutates the id `TextEdit`.
    pub fn note_id_edited(&mut self) {
        self.save_error = None;
    }

    /// `ToggleRow`'s `onChange` for `enabled`.
    pub fn set_draft_enabled(&mut self, enabled: bool) {
        self.draft.enabled = enabled;
    }

    /// `ToggleRow`'s `onChange` for `enabledForAi`.
    pub fn set_draft_enabled_for_ai(&mut self, enabled_for_ai: bool) {
        self.draft.enabled_for_ai = enabled_for_ai;
    }

    /// The env block's `+` button: append an empty row.
    pub fn add_env_row(&mut self) {
        self.env_rows.push(EnvRowEdit::default());
    }

    /// One row's trash button.
    pub fn remove_env_row(&mut self, index: usize) {
        if index < self.env_rows.len() {
            self.env_rows.remove(index);
        }
    }

    /// Cancel: `setShowForm(false)` — the draft stays as it was
    /// (React keeps it; `startAdd`/`startEdit` reseed).
    pub fn close_form(&mut self) {
        self.show_form = false;
    }

    /// `handleSave`: the validation walk. `InvalidId` / `DuplicateId`
    /// arm `save_error` (the render's key); `Save` arms `saving` and
    /// clears the error. The built config keeps `command` untrimmed
    /// (the guard trims, the save does not).
    pub fn begin_save(&mut self) -> SavePlan {
        let trimmed_id = self.trimmed_draft_id().to_owned();
        if trimmed_id.is_empty() || self.draft.command.text.trim().is_empty() {
            return SavePlan::Refused;
        }
        if !is_valid_mcp_server_id(&trimmed_id) {
            self.save_error = Some("mcpClient.invalidId");
            return SavePlan::InvalidId;
        }
        if self.editing_id.is_none()
            && self.saved_configs.iter().any(|c| c.id == trimmed_id)
        {
            self.save_error = Some("mcpClient.duplicateId");
            return SavePlan::DuplicateId;
        }
        let config = McpServerConfigVm {
            id: trimmed_id.clone(),
            // `draft.name.trim() || draft.id.trim()` — the name falls
            // back to the SAME trimmed id.
            name: {
                let name = self.draft.name.text.trim().to_owned();
                if name.is_empty() { trimmed_id.clone() } else { name }
            },
            transport: McpTransport::Stdio,
            command: self.draft.command.text.clone(),
            args: self
                .draft
                .args
                .text
                .split('\n')
                .map(|line| line.trim().to_owned())
                .filter(|line| !line.is_empty())
                .collect(),
            env: rows_to_env(&self.env_rows),
            enabled: self.draft.enabled,
            enabled_for_ai: self.draft.enabled_for_ai,
        };
        let next = match &self.editing_id {
            Some(editing) => self
                .saved_configs
                .iter()
                .map(|c| if c.id == *editing { config.clone() } else { c.clone() })
                .collect::<Vec<_>>(),
            None => {
                let mut next = self.saved_configs.clone();
                next.push(config.clone());
                next
            }
        };
        self.saving = true;
        self.save_error = None;
        SavePlan::Save { config, next }
    }

    /// The save landed: close and reset the form, disarm `saving` (the
    /// `try` block plus the `finally`).
    pub fn note_save_ok(&mut self) {
        self.show_form = false;
        self.editing_id = None;
        self.draft = McpServerFormDraft::empty();
        self.env_rows = Vec::new();
        self.saving = false;
    }

    /// The save rejected: React has no `catch` — the `finally` disarms
    /// `saving` and the form stays exactly as it was (the store's own
    /// `mcpError` strip is the visible failure surface).
    pub fn note_save_failed(&mut self) {
        self.saving = false;
    }

    // ---- the list actions ----

    /// `handleDelete`: the filtered list plus the trailing
    /// `clearMcpServerError(id)`.
    pub fn delete_plan(&self, id: &str) -> DeletePlan {
        DeletePlan {
            next: self
                .saved_configs
                .iter()
                .filter(|c| c.id != id)
                .cloned()
                .collect(),
            clear_server_error_id: id.to_owned(),
        }
    }

    /// `handleToggleEnabled`: the list with the one entry's `enabled`
    /// replaced.
    pub fn toggle_enabled_plan(&self, id: &str, enabled: bool) -> Vec<McpServerConfigVm> {
        self.saved_configs
            .iter()
            .map(|c| {
                if c.id == id {
                    McpServerConfigVm { enabled, ..c.clone() }
                } else {
                    c.clone()
                }
            })
            .collect()
    }

    // ---- the runtime projections ----

    /// `toggleServerTools`: expand ↔ collapse.
    pub fn toggle_server_tools(&mut self, server_id: &str) {
        if !self.expanded_servers.remove(server_id) {
            self.expanded_servers.insert(server_id.to_owned());
        }
    }

    /// `isConnecting` for one saved config.
    pub fn is_connecting(&self, id: &str) -> bool {
        self.mcp_connecting && self.mcp_connecting_server_id.as_deref() == Some(id)
    }

    /// `connectedIds.has(config.id)`.
    pub fn is_connected(&self, id: &str) -> bool {
        self.mcp_servers.iter().any(|s| s.server_id == id)
    }

    /// `mcpServerErrors[config.id]`.
    pub fn server_error(&self, id: &str) -> Option<&str> {
        self.mcp_server_errors.get(id).map(String::as_str)
    }

    /// One runtime server's tools, in list order.
    pub fn tools_for(&self, server_id: &str) -> Vec<&McpToolInfo> {
        self.mcp_tools.iter().filter(|tool| tool.server_id == server_id).collect()
    }
}

