//! Port of `packages/frontend/src/windows/settings/McpClientSection.tsx`
//! (T336, split) — Settings → AI → 外部 MCP 服务.
//!
//! The MCP client administration surface: the saved stdio server
//! configs (add / edit / delete / enable), the per-server connect /
//! reconnect actions with their error strips, and the runtime list of
//! connected servers with their expandable tool inventories.
//!
//! ## What maps where
//!
//! - `useLocaleDomains(['mcp'])` (the `localesReady` gate and its
//!   `LocaleDomainLoading` stand-in) is elided, the T018 ruling: the
//!   zh-CN `mcp.ts` table is compiled in by `build.rs`, so the section
//!   never renders half-translated keys.
//! - `useAiStore` is a host seam (the `language_packs_card` pattern):
//!   the projections (`mcpServers` / `mcpTools` / `mcpConnecting` /
//!   `mcpConnectingServerId` / `mcpError` / `mcpServerErrors`) land in
//!   [`McpClientSectionState`] as host-delivered fields, and every
//!   store action (`connectMcpServer`, `disconnectMcpServer`,
//!   `loadMcpServers`, `loadMcpTools`, `saveMcpClientServers`,
//!   `clearMcpError`, `clearMcpServerError`) fires through the
//!   builder's action slots. The mount effect is
//!   [`McpClientSectionState::on_mount`] — the
//!   `mcp_client_list_request` + `mcp_client_tools_request` pair the
//!   host runs.
//! - `savedConfigs` (`settings.mcpClientServers ?? []`) is the
//!   settings-store projection, host-delivered the same way.
//! - `EMPTY_DRAFT` / the form fields: the draft's text inputs are
//!   [`TextEdit`]s inside [`McpServerFormDraft`] (the kit's field atoms
//!   edit them through the builder's lens slots — the
//!   `model_profile_form` convention); `enabled` / `enabledForAi` stay
//!   plain booleans (default `true`, the `EMPTY_DRAFT` shape).
//! - `envToRows` / `rowsToEnv` are [`env_to_rows`] / [`rows_to_env`];
//!   an env row is [`EnvRowEdit`] (two TextEdits). `rows_to_env`
//!   trims keys, drops empty ones and lets a later duplicate row win
//!   (the React loop's last-write semantics).
//! - `isValidMcpServerId` (`^[a-zA-Z0-9_-]+$`, non-empty) is
//!   [`is_valid_mcp_server_id`], carried until the `types/index.ts`
//!   ledger task absorbs it (the desktop side's
//!   `validate_mcp_server_id` is the same rule).
//! - `handleSave` is [`McpClientSectionState::begin_save`] →
//!   [`SavePlan`]: the same validation walk, the same built config
//!   (`id` trimmed; `name` falls back to the trimmed id; `args` split
//!   on newlines, trimmed, empties dropped; `env` from the rows;
//!   `command` passes through **untrimmed** — the guard trims, the
//!   save does not). The success/failure halves are `note_save_ok` /
//!   `note_save_failed`.
//! - `handleDelete` is [`DeletePlan`] (the next list plus the
//!   per-server error clear that follows the awaited save);
//!   `handleToggleEnabled` is [`toggle_enabled_plan`].
//! - `pickPath` (the PathInput browse) is the
//!   [`McpClientSection::on_browse_command`] slot.
//! - `SectionTitle` / `SettingRow` / `ToggleRow` are the
//!   `settingsUi.tsx` port ([`crate::views::settings::settings_ui`]).
//!   The saved-config row's inline hand-drawn switch (React
//!   `role="switch"`) is the kit [`Switch`], label hidden, accessible
//!   name `t("mcpClient.enabled")`.
//! - React `Button` variants map per T022: `secondary` → `Outline`,
//!   `primary` → `Submit`.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim where they were unique:
//! `mcp-saved-configs`, `mcp-server-id`, `mcp-server-name`,
//! `mcp-server-command` (the PathInput wrapper; its input key derives
//! to `mcp-server-command-input`, the React `inputTestId`),
//! `mcp-add-env`, `mcp-save`, `mcp-add-server`. React's repeated
//! row testids (`mcp-env-key` / `mcp-env-value` on EVERY row input —
//! never unique) become the per-row keys `mcp-env-key-{i}` /
//! `mcp-env-value-{i}` (Rune keys must be unique; the index suffix is
//! the React list `key={i}`). Everything else derives from the section
//! key base `mcp-*` the way `language_packs_card` documents its table:
//! one saved config `mcp-saved-config-{id}`, its switch
//! `mcp-config-switch-{id}`, connect `mcp-config-connect-{id}`,
//! reconnect `mcp-config-reconnect-{id}`, edit `mcp-config-edit-{id}`,
//! delete `mcp-config-delete-{id}`, the per-server error strip
//! `mcp-config-error-{id}`, the global error strip `mcp-client-error`,
//! the form `mcp-server-form`, the args field `mcp-server-args`, the
//! env block `mcp-server-env`, one env row `mcp-env-row-{i}` (remove
//! `mcp-env-remove-{i}`), cancel `mcp-server-cancel`, the runtime list
//! `mcp-runtime-servers`, one server `mcp-runtime-server-{serverId}`
//! (expand `mcp-server-tools-toggle-{serverId}`, disconnect
//! `mcp-server-disconnect-{serverId}`, tools `mcp-server-tools-{serverId}`,
//! one tool `mcp-server-tool-{qualifiedName}`).
//!
//! Known deltas (documented, not silent):
//!
//! - The args `<textarea rows={3}>` renders as the kit's single-line
//!   [`TextField`] (no multiline atom yet); the value keeps the
//!   newline-joined form and the save split is identical — pasting
//!   multi-line text works, only the visual is one scrolling line.
//! - `saveError` stores the i18n KEY (`mcpClient.invalidId` /
//!   `mcpClient.duplicateId`), not the translated string React stored
//!   at set-time; the render translates. Same visible behavior,
//!   locale-switch safe.
//! - `handleSave`'s failure path: React's `await` rejection is
//!   unhandled (`void handleSave()` — no catch); the `finally` still
//!   disarms `saving` and the form stays open. [`SavePlan::note`]
//!   documents it; `note_save_failed` reproduces exactly that (the
//!   store's own `mcpError` strip is the visible failure surface).
//! - `env` maps are `BTreeMap` (sorted keys); React's
//!   `Object.entries` walked insertion order, so the edit form lists
//!   env rows sorted rather than in JSON insertion order — the same
//!   set, the `json_schema_settings_form` key-order ruling.

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, State};

use super::mcp_client_section_state::{
    EnvRowEdit, McpClientSectionState, McpClientStatus, McpServerConfigVm, McpToolInfo,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_TRASH};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::path_input::PathInput;
use crate::ui::toggle::Switch;
use crate::views::settings::setting_hint::SettingHint;
use crate::views::settings::settings_ui::{section_title, setting_row, toggle_row, ToggleRow as ToggleRowSpec};

// ---------------------------------------------------------------------------
// The component builder
// ---------------------------------------------------------------------------

/// `McpClientSection()`: the section over the state and the host's
/// action slots (the `language_packs_card` convention — every action
/// optional, `None` renders unwired).
pub(crate) struct McpClientSection<'a, S: 'static> {
    state: &'a McpClientSectionState,
    dark: bool,
    // the form's edit lenses (the model_profile_form convention)
    id_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    name_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    command_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    args_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    env_key_edit: Rc<dyn for<'x> Fn(&'x mut S, usize) -> &'x mut TextEdit>,
    env_value_edit: Rc<dyn for<'x> Fn(&'x mut S, usize) -> &'x mut TextEdit>,
    // host actions
    on_connect: Option<Rc<dyn Fn(&mut S, String)>>,
    on_disconnect: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_enabled: Option<Rc<dyn Fn(&mut S, String, bool)>>,
    /// The form's `enabled` ToggleRow flip (`setDraft`).
    on_draft_enabled: Option<Rc<dyn Fn(&mut S, bool)>>,
    /// The form's `enabledForAi` ToggleRow flip.
    on_draft_enabled_for_ai: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_edit: Option<Rc<dyn Fn(&mut S, String)>>,
    on_delete: Option<Rc<dyn Fn(&mut S, String)>>,
    on_add_server: Option<Rc<dyn Fn(&mut S)>>,
    on_save: Option<Rc<dyn Fn(&mut S)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
    on_add_env: Option<Rc<dyn Fn(&mut S)>>,
    on_remove_env: Option<Rc<dyn Fn(&mut S, usize)>>,
    on_browse_command: Option<Rc<dyn Fn(&mut S)>>,
    on_clear_mcp_error: Option<Rc<dyn Fn(&mut S)>>,
    on_clear_server_error: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_tools: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<'a, S: 'static> McpClientSection<'a, S> {
    /// The section over the state plus the six form lenses (the
    /// TextEdits live in the host's state; the field atoms edit them
    /// through these).
    pub(crate) fn new(
        state: &'a McpClientSectionState,
        dark: bool,
        id_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        name_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        command_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        args_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        env_key_edit: Rc<dyn for<'x> Fn(&'x mut S, usize) -> &'x mut TextEdit>,
        env_value_edit: Rc<dyn for<'x> Fn(&'x mut S, usize) -> &'x mut TextEdit>,
    ) -> Self {
        Self {
            state,
            dark,
            id_edit: Rc::new(id_edit),
            name_edit: Rc::new(name_edit),
            command_edit: Rc::new(command_edit),
            args_edit: Rc::new(args_edit),
            env_key_edit,
            env_value_edit,
            on_connect: None,
            on_disconnect: None,
            on_toggle_enabled: None,
            on_draft_enabled: None,
            on_draft_enabled_for_ai: None,
            on_edit: None,
            on_delete: None,
            on_add_server: None,
            on_save: None,
            on_cancel: None,
            on_add_env: None,
            on_remove_env: None,
            on_browse_command: None,
            on_clear_mcp_error: None,
            on_clear_server_error: None,
            on_toggle_tools: None,
        }
    }

    /// `connectMcpServer(id)` — connect and reconnect share it.
    pub(crate) fn on_connect(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_connect = Some(Rc::new(f));
        self
    }
    /// `disconnectMcpServer(id)`.
    pub(crate) fn on_disconnect(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_disconnect = Some(Rc::new(f));
        self
    }
    /// `handleToggleEnabled(id, next)`.
    pub(crate) fn on_toggle_enabled(
        mut self,
        f: impl Fn(&mut S, String, bool) + 'static,
    ) -> Self {
        self.on_toggle_enabled = Some(Rc::new(f));
        self
    }
    /// The form's `enabled` ToggleRow (`setDraft(d => ({...d, enabled}))`).
    pub(crate) fn on_draft_enabled(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_draft_enabled = Some(Rc::new(f));
        self
    }
    /// The form's `enabledForAi` ToggleRow.
    pub(crate) fn on_draft_enabled_for_ai(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_draft_enabled_for_ai = Some(Rc::new(f));
        self
    }
    /// `startEdit(config)`.
    pub(crate) fn on_edit(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_edit = Some(Rc::new(f));
        self
    }
    /// `handleDelete(id)`.
    pub(crate) fn on_delete(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_delete = Some(Rc::new(f));
        self
    }
    /// `startAdd`.
    pub(crate) fn on_add_server(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_add_server = Some(Rc::new(f));
        self
    }
    /// `handleSave`.
    pub(crate) fn on_save(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }
    /// Cancel (`setShowForm(false)`).
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }
    /// The env block's `+` button.
    pub(crate) fn on_add_env(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_add_env = Some(Rc::new(f));
        self
    }
    /// One env row's trash button.
    pub(crate) fn on_remove_env(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove_env = Some(Rc::new(f));
        self
    }
    /// `pickPath` — the command field's browse.
    pub(crate) fn on_browse_command(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_browse_command = Some(Rc::new(f));
        self
    }
    /// `clearMcpError`.
    pub(crate) fn on_clear_mcp_error(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_mcp_error = Some(Rc::new(f));
        self
    }
    /// `clearMcpServerError(id)`.
    pub(crate) fn on_clear_server_error(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_clear_server_error = Some(Rc::new(f));
        self
    }
    /// `toggleServerTools(id)`.
    pub(crate) fn on_toggle_tools(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_tools = Some(Rc::new(f));
        self
    }

    /// The section body.
    pub(crate) fn build(self, layer: &mut Vec<El>) -> El {
        let dark = self.dark;
        let k = kit(dark);
        let state = self.state;

        let mut body = col().key("mcp-client-section").w_full().gap(16.0);

        body = body.child(section_title(
            &k,
            layer,
            "mcp-client-title".to_owned(),
            t("mcpClient.title"),
            Some(t("mcpClient.description")),
        ));

        // The global error strip: message + the close link.
        if let Some(message) = &state.mcp_error {
            body = body.child(danger_strip::<S>(
                &k,
                "mcp-client-error".to_owned(),
                message,
                t("common.close"),
                self.on_clear_mcp_error.clone(),
            ));
        }

        // The saved-configs heading (React wraps it in the
        // `mcp-saved-configs` testid div).
        body = body.child(
            col()
                .key("mcp-saved-configs")
                .child(section_title(&k, layer, "mcp-saved-configs-title".to_owned(), t("mcpClient.savedConfigs"), None)),
        );

        if state.saved_configs.is_empty() && !state.show_form {
            body = body.child(
                text(t("mcpClient.noSavedConfigs"))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.text_muted),
            );
        }

        if !state.saved_configs.is_empty() {
            let mut list = col().key("mcp-saved-config-list").w_full().gap(4.0);
            for config in &state.saved_configs {
                list = list.child(self.saved_config_row(&k, layer, config));
            }
            body = body.child(list);
        }

        body = body.child(if state.show_form {
            self.build_form(&k, layer)
        } else {
            self.add_server_button(&k)
        });

        body = body.child(section_title(
            &k,
            layer,
            "mcp-runtime-title".to_owned(),
            t("mcpClient.runtimeStatus"),
            None,
        ));

        if state.mcp_servers.is_empty() {
            body = body.child(
                text(t("mcpClient.noServers"))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.text_muted),
            );
        } else {
            let mut list = col().key("mcp-runtime-servers").w_full().gap(4.0);
            for server in &state.mcp_servers {
                list = list.child(self.runtime_server_row(&k, layer, server));
            }
            body = body.child(list);
        }

        body
    }

    /// One saved config's card.
    fn saved_config_row(&self, k: &Kit, _layer: &mut Vec<El>, config: &McpServerConfigVm) -> El {
        let state = self.state;
        let server_error = state.server_error(&config.id);
        let is_connected = state.is_connected(&config.id);

        let mut card = col()
            .key(format!("mcp-saved-config-{}", config.id))
            .w_full()
            .px(8.0)
            .py(8.0)
            .gap(6.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);

        // The head row: name (+ the connect-failed badge), id, actions.
        let mut head = row().key(format!("mcp-saved-config-head-{}", config.id)).w_full().gap(8.0).center_y();

        let mut meta = col().key(format!("mcp-saved-config-meta-{}", config.id)).min_w(0.0).grow(1.0).gap(2.0);
        let mut name_row = row().key(format!("mcp-saved-config-name-{}", config.id)).gap(8.0).center_y();
        name_row = name_row.child(
            text(config.name.clone())
                .font(TextStyle::new(14.0, 400.0))
                .color(k.c.text_base),
        );
        if server_error.is_some() {
            // The connect-failed badge: red tint, the error text in the
            // React `title` (the SettingHint stand-in: hover carries it).
            name_row = name_row.child(
                col()
                    .key(format!("mcp-config-failed-{}", config.id))
                    .px(6.0)
                    .py(2.0)
                    .radius(4.0)
                    .fill(k.alpha(k.c.danger.fg, 0.15))
                    .child(
                        text(t("mcpClient.connectFailed"))
                            .font(TextStyle::new(10.0, 500.0))
                            .color(k.c.danger.fg),
                    ),
            );
        }
        meta = meta.child(name_row);
        meta = meta.child(
            text(config.id.clone())
                .font(TextStyle::new(12.0, 400.0))
                .color(k.c.text_muted),
        );
        head = head.child(meta);

        // The enabled switch (React's hand-drawn `role="switch"`).
        let mut switch = Switch::new(format!("mcp-config-switch-{}", config.id), config.enabled)
            .label(t("mcpClient.enabled"))
            .hide_label();
        if let Some(f) = &self.on_toggle_enabled {
            let f = Rc::clone(f);
            let id = config.id.clone();
            let next = !config.enabled;
            switch = switch.on_toggle(move |s: &mut S| f(s, id.clone(), next));
        }
        head = head.child(switch.build(k));

        // Connect / reconnect: only when NOT connected and enabled —
        // the error decides the label (`reconnect` vs `connect`).
        if !is_connected && config.enabled {
            let label = if server_error.is_some() {
                if state.is_connecting(&config.id) { t("mcpClient.connecting") } else { t("mcpClient.reconnect") }
            } else if state.is_connecting(&config.id) {
                t("mcpClient.connecting")
            } else {
                t("mcpClient.connect")
            };
            let key = if server_error.is_some() {
                format!("mcp-config-reconnect-{}", config.id)
            } else {
                format!("mcp-config-connect-{}", config.id)
            };
            let mut button = Button::new(label)
                .variant(ButtonVariant::Outline) // React secondary (T022)
                .control_size(ControlSize::Regular);
            if state.mcp_connecting {
                button = button.state(State::Disabled).disabled(true);
            } else if let Some(f) = &self.on_connect {
                let f = Rc::clone(f);
                let id = config.id.clone();
                button = button.on_press(move |s: &mut S| f(s, id.clone()));
            }
            head = head.child(button.build(k).key(key));
        }

        let mut edit = Button::new(t("mcpClient.edit"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_edit {
            let f = Rc::clone(f);
            let id = config.id.clone();
            edit = edit.on_press(move |s: &mut S| f(s, id.clone()));
        }
        head = head.child(edit.build(k).key(format!("mcp-config-edit-{}", config.id)));

        let mut delete = Button::new(t("mcpClient.delete"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_delete {
            let f = Rc::clone(f);
            let id = config.id.clone();
            delete = delete.on_press(move |s: &mut S| f(s, id.clone()));
        }
        head = head.child(delete.build(k).key(format!("mcp-config-delete-{}", config.id)));

        card = card.child(head);

        // The per-server error strip (below the head, above the fold).
        if let Some(message) = server_error {
            let on_clear = self.on_clear_server_error.clone();
            card = card.child(danger_strip::<S>(
                k,
                format!("mcp-config-error-{}", config.id),
                message,
                t("common.close"),
                on_clear.map(|f| {
                    let id = config.id.clone();
                    Rc::new(move |s: &mut S| f(s, id.clone())) as Rc<dyn Fn(&mut S)>
                }),
            ));
        }

        card
    }

    /// The add/edit form.
    fn build_form(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let dark = self.dark;

        let mut form = col()
            .key("mcp-server-form")
            .w_full()
            .px(12.0)
            .py(12.0)
            .gap(12.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01); // React bg-surface-alt

        // ID: disabled while editing; the invalid hint below.
        let id_lens = Rc::clone(&self.id_edit);
        let id_input = TextField::new("mcp-server-id", &state.draft.id)
            .placeholder("my-mcp-server")
            .w_full()
            .disabled(state.editing_id.is_some())
            .on_edit::<S>(move |s: &mut S| id_lens(s));
        let id_col = col().key("mcp-server-id-wrap").w_full().gap(4.0).child(id_input.build(dark));
        let id_col = if state.id_invalid() {
            id_col.child(
                text(t("mcpClient.invalidId"))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.danger.fg),
            )
        } else {
            id_col
        };
        form = form.child(setting_row(
            k,
            layer,
            "mcp-server-form-id".to_owned(),
            "ID",
            None,
            id_col,
        ));

        // Name.
        let name_lens = Rc::clone(&self.name_edit);
        let name_input = TextField::new("mcp-server-name", &state.draft.name)
            .placeholder("My Server")
            .w_full()
            .on_edit::<S>(move |s: &mut S| name_lens(s));
        form = form.child(setting_row(
            k,
            layer,
            "mcp-server-form-name".to_owned(),
            t("mcpClient.serverName"),
            None,
            name_input.build(dark),
        ));

        // Command: the PathInput (browse = pickPath). The type-state
        // builder changes type after `on_browse`, so the two branches
        // each build the whole value.
        let command = if let Some(f) = &self.on_browse_command {
            let f = Rc::clone(f);
            let lens = Rc::clone(&self.command_edit);
            PathInput::new("mcp-server-command", &state.draft.command)
                .placeholder("/usr/local/bin/my-mcp")
                .on_browse(move |s: &mut S| f(s))
                .on_edit(move |s: &mut S| lens(s))
        } else {
            // No picker supplied: a no-op browse keeps the input
            // editable (the build gate requires SOME browse handler).
            let lens = Rc::clone(&self.command_edit);
            PathInput::new("mcp-server-command", &state.draft.command)
                .placeholder("/usr/local/bin/my-mcp")
                .on_browse(|_s: &mut S| {})
                .on_edit(move |s: &mut S| lens(s))
        };
        form = form.child(setting_row(
            k,
            layer,
            "mcp-server-form-command".to_owned(),
            t("mcpClient.command"),
            None,
            command.build(dark),
        ));

        // Args (single-line kit field — see the module header's deltas).
        let args_lens = Rc::clone(&self.args_edit);
        let args_input = TextField::new("mcp-server-args", &state.draft.args)
            .placeholder("--flag1\n--flag2")
            .w_full()
            .on_edit::<S>(move |s: &mut S| args_lens(s));
        form = form.child(setting_row(
            k,
            layer,
            "mcp-server-form-args".to_owned(),
            t("mcpClient.args"),
            None,
            args_input.build(dark),
        ));

        // The env block.
        let mut env_head = row().key("mcp-server-env-head").w_full().gap(8.0).center_y().justify_between();
        env_head = env_head.child(
            text(t("mcpClient.envVars"))
                .font(TextStyle::new(14.0, 400.0))
                .color(k.c.text_muted),
        );
        let mut add_env = Button::new(format!("+ {}", t("mcpClient.addEnv")))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_add_env {
            let f = Rc::clone(f);
            add_env = add_env.on_press(move |s: &mut S| f(s));
        }
        env_head = env_head.child(add_env.build(k).key("mcp-add-env"));

        let mut env = col().key("mcp-server-env").w_full().gap(8.0).child(env_head);
        if state.env_rows.is_empty() {
            env = env.child(
                text(t("mcpClient.noEnvVars"))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.text_muted),
            );
        }
        for (i, row_edit) in state.env_rows.iter().enumerate() {
            let key_lens = Rc::clone(&self.env_key_edit);
            let value_lens = Rc::clone(&self.env_value_edit);
            let mut row_el = row()
                .key(format!("mcp-env-row-{i}"))
                .w_full()
                .gap(8.0)
                .center_y();
            let key_input = TextField::new(format!("mcp-env-key-{i}"), &row_edit.key)
                .placeholder(t("mcpClient.envKey"))
                .grow()
                .on_edit::<S>(move |s: &mut S| key_lens(s, i));
            let value_input = TextField::new(format!("mcp-env-value-{i}"), &row_edit.value)
                .placeholder(t("mcpClient.envValue"))
                .grow()
                .on_edit::<S>(move |s: &mut S| value_lens(s, i));
            let mut remove = Button::new("")
                .icon(&ICON_TRASH)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help(t("mcpClient.removeEnv"));
            if let Some(f) = &self.on_remove_env {
                let f = Rc::clone(f);
                remove = remove.on_press(move |s: &mut S| f(s, i));
            }
            row_el = row_el
                .child(key_input.build(dark))
                .child(value_input.build(dark))
                .child(remove.build(k).key(format!("mcp-env-remove-{i}")));
            env = env.child(row_el);
        }
        form = form.child(env);

        // The two toggles: the draft's booleans flip through the host
        // state (the section holds its state by shared reference); an
        // unwired slot renders a press that does nothing.
        let no_flip: Rc<dyn Fn(&mut S, bool)> = Rc::new(|_s: &mut S, _next: bool| {});
        form = form.child(toggle_row::<S>(
            k,
            layer,
            ToggleRowSpec {
                key: "mcp-form-enabled".to_owned(),
                label: t("mcpClient.enabled"),
                hint: None,
                checked: state.draft.enabled,
                disabled: false,
            },
            self.on_draft_enabled.clone().unwrap_or_else(|| Rc::clone(&no_flip)),
        ));
        form = form.child(toggle_row::<S>(
            k,
            layer,
            ToggleRowSpec {
                key: "mcp-form-enabled-for-ai".to_owned(),
                label: t("mcpClient.enabledForAi"),
                hint: None,
                checked: state.draft.enabled_for_ai,
                disabled: false,
            },
            self.on_draft_enabled_for_ai.clone().unwrap_or_else(|| no_flip),
        ));

        // The error line: saveError ?? duplicateId (the derived one).
        if state.save_error.is_some() || state.id_duplicate() {
            let message = state
                .save_error
                .map(|key| t(key))
                .unwrap_or_else(|| t("mcpClient.duplicateId"));
            form = form.child(
                text(message)
                    .key("mcp-server-form-error")
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.danger.fg),
            );
        }

        // Save / Cancel.
        let mut buttons = row().key("mcp-server-form-buttons").gap(8.0);
        let mut save = Button::new(if state.saving { t("mcpClient.saving") } else { t("mcpClient.save") })
            .variant(ButtonVariant::Submit) // React primary (T022)
            .control_size(ControlSize::Regular);
        if state.save_disabled() {
            save = save.state(State::Disabled).disabled(true);
        } else if let Some(f) = &self.on_save {
            let f = Rc::clone(f);
            save = save.on_press(move |s: &mut S| f(s));
        }
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_cancel {
            let f = Rc::clone(f);
            cancel = cancel.on_press(move |s: &mut S| f(s));
        }
        buttons = buttons
            .child(save.build(k).key("mcp-save"))
            .child(cancel.build(k).key("mcp-server-cancel"));
        form = form.child(buttons);

        form
    }

    /// The `+ 添加 MCP 服务` button.
    fn add_server_button(&self, k: &Kit) -> El {
        let mut add = Button::new(t("mcpClient.addServer"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_add_server {
            let f = Rc::clone(f);
            add = add.on_press(move |s: &mut S| f(s));
        }
        add.build(k).key("mcp-add-server")
    }

    /// One connected server's runtime card.
    fn runtime_server_row(&self, k: &Kit, layer: &mut Vec<El>, server: &McpClientStatus) -> El {
        let state = self.state;
        let expanded = state.expanded_servers.contains(&server.server_id);
        let server_tools = state.tools_for(&server.server_id);

        let mut card = col()
            .key(format!("mcp-runtime-server-{}", server.server_id))
            .w_full()
            .px(8.0)
            .py(8.0)
            .gap(8.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);

        let mut head = row().key(format!("mcp-runtime-head-{}", server.server_id)).w_full().gap(8.0).center_y();

        // The expand trigger: chevron + name + the tool count.
        let mut trigger = row()
            .key(format!("mcp-server-tools-toggle-{}", server.server_id))
            .min_w(0.0)
            .grow(1.0)
            .gap(4.0)
            .center_y();
        let chevron = if expanded { &ICON_CHEVRON_DOWN } else { &ICON_CHEVRON_RIGHT };
        trigger = trigger.child(icon(chevron, 14.0));
        trigger = trigger.child(
            text(server.server_name.clone())
                .font(TextStyle::new(14.0, 400.0))
                .color(k.c.text_base),
        );
        trigger = trigger.child(
            text(format!("({} {})", server.tool_count, t("mcpClient.tools")))
                .font(TextStyle::new(12.0, 400.0))
                .color(k.c.text_muted),
        );
        if let Some(f) = &self.on_toggle_tools {
            let f = Rc::clone(f);
            let id = server.server_id.clone();
            trigger = trigger.on_press(move |s: &mut S| f(s, id.clone()));
        }
        head = head.child(trigger);

        let mut disconnect = Button::new(t("mcpClient.disconnect"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_disconnect {
            let f = Rc::clone(f);
            let id = server.server_id.clone();
            disconnect = disconnect.on_press(move |s: &mut S| f(s, id.clone()));
        }
        head = head.child(disconnect.build(k).key(format!("mcp-server-disconnect-{}", server.server_id)));
        card = card.child(head);

        if expanded && !server_tools.is_empty() {
            let mut tools = col()
                .key(format!("mcp-server-tools-{}", server.server_id))
                .w_full()
                .pt(8.0)
                .gap(4.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base); // border-t
            tools = tools.child(
                text(t("mcpClient.toolList"))
                    .font(TextStyle::new(11.0, 500.0))
                    .color(k.c.text_muted),
            );
            for tool in &server_tools {
                let mut item = col()
                    .key(format!("mcp-server-tool-{}", tool.qualified_name))
                    .px(8.0)
                    .py(6.0)
                    .gap(2.0)
                    .radius(k.radius.sm)
                    .border(1.0, k.c.border_muted)
                    .fill(k.c.layer_01); // React bg-surface-alt
                let mut name_row = row().gap(4.0).center_y();
                name_row = name_row.child(
                    text(tool.tool_name.clone())
                        .font(TextStyle::new(14.0, 500.0))
                        .color(k.c.text_base),
                );
                if let Some(description) = &tool.description {
                    name_row = name_row.child(
                        SettingHint::new(
                            format!("mcp-server-tool-hint-{}", tool.qualified_name),
                            tool.tool_name.clone(),
                            description.clone(),
                        )
                        .build(k, layer),
                    );
                }
                item = item.child(name_row);
                item = item.child(
                    text(tool.qualified_name.clone())
                        .font(TextStyle::new(12.0, 400.0))
                        .color(k.c.text_muted),
                );
                tools = tools.child(item);
            }
            card = card.child(tools);
        }
        if expanded && server_tools.is_empty() {
            card = card.child(
                text(t("mcpClient.noTools"))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(k.c.text_muted),
            );
        }

        card
    }
}

/// React's two hand-drawn red strips (the global `mcpError` and the
/// per-server `serverError`): tinted background, the message, the
/// close link.
fn danger_strip<S: 'static>(
    k: &Kit,
    key: String,
    message: &str,
    close_label: &str,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
) -> El {
    let mut strip = row()
        .key(key.clone())
        .w_full()
        .px(12.0)
        .py(6.0)
        .gap(8.0)
        .center_y()
        .justify_between()
        .radius(k.radius.md)
        .border(1.0, k.alpha(k.c.danger.fg, 0.3))
        .fill(k.alpha(k.c.danger.fg, 0.1));
    strip = strip.child(
        text(message.to_owned())
            .font(TextStyle::new(12.0, 400.0))
            .color(k.c.danger.fg),
    );
    let mut close = Button::new(close_label)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small);
    if let Some(f) = on_close {
        close = close.on_press(move |s: &mut S| f(s));
    }
    strip = strip.child(close.build(k).key(format!("{key}-close")));
    strip
}
