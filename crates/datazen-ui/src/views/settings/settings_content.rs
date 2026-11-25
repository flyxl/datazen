//! Port of
//! `packages/frontend/src/windows/settings/SettingsContent.tsx`
//! (T390, split — the state half is `settings_content_state.rs`).
//!
//! The settings window's content shell: the grouped nav rail on the
//! left (the back button, the two groups, the twelve section buttons
//! with their icons and the active highlight) and the section body on
//! the right — the six sections the page renders itself (general,
//! data browsing, editor, behavior, logging, appearance) plus the six
//! it delegates to their own section components (AI, prompts, MCP
//! server, MCP client, tunnels, extensions).
//!
//! ## What maps where
//!
//! - `availableSections` (capabilities-driven, F4→F5 shape) is
//!   [`super::settings_content_state::available_sections`]; the nav
//!   and every capability gate (`updater`, `versionSource`) read the
//!   [`PlatformCapabilities`] slot — `None` keeps the
//!   desktop-shaped full set, exactly the pre-F5 window.
//! - The child sections are **slots**: each landed port builds its
//!   own `El` (the host wires its builder with its state and
//!   callbacks) and hands it to this shell — the same convention
//!   `AiSettingsSection` established for its sub-sections. Sections
//!   whose ports have not landed yet (Tunnels T453,
//!   TableDataSettingsSection T434, SqlSnippetsCard T426,
//!   UpdateSection T464) render nothing until their task wires the
//!   slot in.
//! - `renderSectionContributions` (the `useExtension(
//!   sqlEditorEnhancedEP)` seam) is the [`SettingsContribution`]
//!   view-model list: the host supplies the contributions (its
//!   extension registry's answer), the shell renders the groups —
//!   the `renderGroup` arm is the contribution's prebuilt `group`
//!   element, the item arms are boolean → `ToggleRow`, select →
//!   `SettingRow` + kit `Select`, number/text → `SettingRow` +
//!   `TextField`, custom → the item's prebuilt control element.
//! - `updateField` is the `on_update_field` slot (one
//!   [`SettingsField`] per control); the optimistic mirror half is
//!   [`SettingsContentState::apply_field`], the store write is the
//!   host's. `updateExtensionSetting` is the
//!   `on_extension_setting` slot over the same mirror seam.
//! - The keymap card's presets/actions live in the state module
//!   (the `lib/keymap.ts` seam, segment 2's T541 authority).
//! - `PathInput` + `pickPath` is [`crate::ui::path_input::PathInput`]
//!   with the `on_browse_log_path` slot (the host opens the picker);
//!   `dialogOptions: { directory: true }` is the host's picker
//!   concern.
//! - `SqlSyntaxPreview` is the `sql_syntax_preview` slot (segment
//!   3's T241 component); the syntax-theme select's options are the
//!   `syntax_presets` view-model (segment 2's T632 table).
//! - `settingsSectionIconId` (`lib/hostLucideMap.ts`) is
//!   [`crate::icons::settings_section_icon`].
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `settings-nav`,
//! `settings-nav-group-{id}`, `settings-nav-{id}`, `settings-back`,
//! `settings-content`, `settings-section-{id}`,
//! `settings-language-select` (the kit select's trigger is
//! `…-trigger`), `settings-connection-pool-size`,
//! `settings-toggle-enableFkPrediction`, the contribution group
//! `settings-contrib-{extensionId}` and its select
//! `settings-contrib-{ext}-{key}-select`. Unkeyed React elements
//! (the rows, the keymap card, the version line) carry structural
//! `settings-*` layout keys — hooks for the audit, never fake
//! React testids.
//!
//! Known deltas: the kit `Button` cannot stretch (`w-full
//! justify-start`), so the back button keeps its natural ghost size
//! inside the nav's full-width column; `toLocaleString` grouping is
//! the state module's fixed en-US [`SettingsContentState::
//! formatted_rows`]; the `MutationObserver` dark-mode watch is the
//! host's theme fact (the `dark` flag this shell receives).

// No production consumer yet (the settings view wiring track mounts
// the page); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, tok, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::settings_content_state::{
    available_sections, format_shortcut_for_display, get_action_shortcut,
    language_options as merge_language_options, updater_enabled, SettingsContentState,
    SettingsField, SettingsSelect, BUILTIN_LOCALE_OPTIONS, KEYMAP_ACTIONS, KEYMAP_PRESET_OPTIONS,
    LOG_LEVEL_OPTIONS, PAGE_SIZE_OPTIONS, RESULT_LIMIT_OPTIONS,
};
use super::settings_ui::{section_title, setting_row, toggle_row, ToggleRow as ToggleRowSpec};
use crate::i18n::t;
use crate::icons::{settings_section_icon, ICON_ARROW_LEFT};
use crate::platform::capabilities::PlatformCapabilities;
use crate::settings_state::{SETTINGS_NAV_GROUPS, SETTINGS_SECTIONS};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::path_input::PathInput;
use crate::ui::select::Select;
use crate::ui::slider::Slider;

/// `text-[13px]` — the nav item label.
const NAV_ITEM_TEXT: TextStyle = TextStyle::new(13.0, 400.0);
/// `text-[13px] font-medium` — the active nav item label.
const NAV_ITEM_ACTIVE: TextStyle = TextStyle::new(13.0, 500.0);
/// `text-[11px] font-semibold` — the nav group label.
const NAV_GROUP_TEXT: TextStyle = TextStyle::new(11.0, 600.0);
/// `text-xs` — the version line, the keymap rows, the back button.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-xs` semibold — the keymap card's header.
const TEXT_XS_SEMIBOLD: TextStyle = TextStyle::new(12.0, 600.0);
/// `text-[11px]` mono — the keymap input.
const TEXT_11_MONO: TextStyle = TextStyle::new(11.0, 400.0);
/// `text-[10px]` mono — the keymap kbd chip.
const TEXT_10_MONO: TextStyle = TextStyle::new(10.0, 400.0);
/// `text-sm` — the "{n}px" value and the contribution h4.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
const TEXT_SM_SEMIBOLD: TextStyle = TextStyle::new(14.0, 600.0);

/// The nav rail's width (`w-[180px]`).
const NAV_WIDTH: f32 = 180.0;
/// `max-w-2xl` — the section body's column.
const BODY_MAX_WIDTH: f32 = 672.0;

/// `renderSectionContributions`'s target sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContributionTarget {
    General,
    Editor,
    Appearance,
}

/// One contribution item's control (the React `item.type` arms).
pub enum ContributionControl {
    /// `type: 'boolean'` — a `ToggleRow`.
    Boolean(bool),
    /// `type: 'select'` — the options and the current value.
    Select { current: String, options: Vec<(String, String)> },
    /// `type: 'number'` — the current value (`Number(currentValue)`).
    Number(f64),
    /// `type: 'string'` (the fallthrough arm).
    Text(String),
    /// `type: 'custom'` — the host's prebuilt control element
    /// (React's `item.render` slot).
    Custom(El),
}

pub struct ContributionItem {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    pub control: ContributionControl,
}

/// One extension's settings contribution (the
/// `enhanced.settingsContributions` entry).
pub struct SettingsContribution {
    pub extension_id: String,
    pub target: ContributionTarget,
    pub group_title: Option<String>,
    pub group_description: Option<String>,
    /// `renderGroup` — the host's prebuilt group element; when set
    /// the shell renders it alone inside the keyed group div.
    pub group: Option<El>,
    pub items: Vec<ContributionItem>,
}

/// `SettingsContent({ initialSection, onBack })` — the shell over the
/// state, the capabilities, the view-model inputs and the host's
/// action slots.
pub struct SettingsContent<'a, S: 'static> {
    state: &'a SettingsContentState,
    dark: bool,
    /// `formatShortcutForDisplay`'s `isMac` branch (the host's
    /// platform fact).
    is_mac: bool,
    capabilities: Option<&'a PlatformCapabilities>,
    /// The language select's options (built-ins + installed packs +
    /// extension locales, deduped — the state module's helper builds
    /// them; the host owns the pack and extension lists).
    language_options: Vec<(String, String)>,
    /// `SQL_SYNTAX_PRESETS.map(p => ({ value: p.id, label: p.label }))`
    /// — segment 2's table through the view-model seam.
    syntax_presets: Vec<(String, String)>,
    contributions: Vec<SettingsContribution>,
    // the child section slots
    language_packs_card: Option<El>,
    update_section: Option<El>,
    table_data_section: Option<El>,
    sql_snippets_card: Option<El>,
    appearance_section: Option<El>,
    ai_section: Option<El>,
    prompts_section: Option<El>,
    mcp_server_section: Option<El>,
    mcp_client_section: Option<El>,
    tunnel_section: Option<El>,
    extensions_section: Option<El>,
    data_cleanup_section: Option<El>,
    sql_syntax_preview: Option<El>,
    // the host action slots
    on_back: Option<Rc<dyn Fn(&mut S)>>,
    /// The nav item press: `setActiveSection(sec.id)` — the host
    /// routes it through the state's `select_section`.
    on_select_section: Option<Rc<dyn Fn(&mut S, &'static str)>>,
    on_update_field: Option<Rc<dyn Fn(&mut S, SettingsField)>>,
    on_extension_setting: Option<Rc<dyn Fn(&mut S, String, String, String)>>,
    on_select_toggle: Option<Rc<dyn Fn(&mut S, SettingsSelect)>>,
    on_select_close: Option<Rc<dyn Fn(&mut S)>>,
    on_contribution_toggle: Option<Rc<dyn Fn(&mut S, String)>>,
    on_contribution_close: Option<Rc<dyn Fn(&mut S)>>,
    on_open_log_dir: Option<Rc<dyn Fn(&mut S)>>,
    on_browse_log_path: Option<Rc<dyn Fn(&mut S)>>,
    // the text input lenses (over the host's state)
    pool_size_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    font_family_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    log_path_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    keymap_edit: Option<Rc<dyn for<'b> Fn(&'b mut S, &'static str) -> &'b mut TextEdit>>,
    contribution_edit:
        Option<Rc<dyn for<'b, 'd> Fn(&'b mut S, &'d str, &'d str) -> &'b mut TextEdit>>,
}

impl<'a, S: 'static> SettingsContent<'a, S> {
    pub fn new(state: &'a SettingsContentState, dark: bool, is_mac: bool) -> Self {
        Self {
            state,
            dark,
            is_mac,
            capabilities: None,
            language_options: merge_language_options(
                &BUILTIN_LOCALE_OPTIONS,
                &[],
                &[],
            ),
            syntax_presets: Vec::new(),
            contributions: Vec::new(),
            language_packs_card: None,
            update_section: None,
            table_data_section: None,
            sql_snippets_card: None,
            appearance_section: None,
            ai_section: None,
            prompts_section: None,
            mcp_server_section: None,
            mcp_client_section: None,
            tunnel_section: None,
            extensions_section: None,
            data_cleanup_section: None,
            sql_syntax_preview: None,
            on_back: None,
            on_select_section: None,
            on_update_field: None,
            on_extension_setting: None,
            on_select_toggle: None,
            on_select_close: None,
            on_contribution_toggle: None,
            on_contribution_close: None,
            on_open_log_dir: None,
            on_browse_log_path: None,
            pool_size_edit: None,
            font_family_edit: None,
            log_path_edit: None,
            keymap_edit: None,
            contribution_edit: None,
        }
    }

    // ---- view-model inputs ----

    pub fn capabilities(mut self, caps: &'a PlatformCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// The language select's options: the built-ins plus the
    /// installed packs and the extension locales (deduped by value —
    /// the state module's `languageOptions` memo).
    pub fn language_options(
        mut self,
        installed_packs: &[(String, String)],
        extension_locales: &[(String, String)],
    ) -> Self {
        self.language_options = merge_language_options(
            &BUILTIN_LOCALE_OPTIONS,
            installed_packs,
            extension_locales,
        );
        self
    }

    pub fn syntax_presets(mut self, presets: Vec<(String, String)>) -> Self {
        self.syntax_presets = presets;
        self
    }

    pub fn contributions(mut self, contributions: Vec<SettingsContribution>) -> Self {
        self.contributions = contributions;
        self
    }

    // ---- the child section slots ----

    pub fn language_packs_card(mut self, section: Option<El>) -> Self {
        self.language_packs_card = section;
        self
    }
    pub fn update_section(mut self, section: Option<El>) -> Self {
        self.update_section = section;
        self
    }
    pub fn table_data_section(mut self, section: Option<El>) -> Self {
        self.table_data_section = section;
        self
    }
    pub fn sql_snippets_card(mut self, section: Option<El>) -> Self {
        self.sql_snippets_card = section;
        self
    }
    pub fn appearance_section(mut self, section: Option<El>) -> Self {
        self.appearance_section = section;
        self
    }
    pub fn ai_section(mut self, section: Option<El>) -> Self {
        self.ai_section = section;
        self
    }
    pub fn prompts_section(mut self, section: Option<El>) -> Self {
        self.prompts_section = section;
        self
    }
    pub fn mcp_server_section(mut self, section: Option<El>) -> Self {
        self.mcp_server_section = section;
        self
    }
    pub fn mcp_client_section(mut self, section: Option<El>) -> Self {
        self.mcp_client_section = section;
        self
    }
    pub fn tunnel_section(mut self, section: Option<El>) -> Self {
        self.tunnel_section = section;
        self
    }
    pub fn extensions_section(mut self, section: Option<El>) -> Self {
        self.extensions_section = section;
        self
    }
    pub fn data_cleanup_section(mut self, section: Option<El>) -> Self {
        self.data_cleanup_section = section;
        self
    }
    pub fn sql_syntax_preview(mut self, preview: Option<El>) -> Self {
        self.sql_syntax_preview = preview;
        self
    }

    // ---- the host action slots ----

    pub fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// The nav item press (`setActiveSection(sec.id)`).
    pub fn on_select_section(
        mut self,
        f: impl Fn(&mut S, &'static str) + 'static,
    ) -> Self {
        self.on_select_section = Some(Rc::new(f));
        self
    }

    /// `updateField(key, value)` — the store write half (the host
    /// also applies the optimistic mirror via
    /// [`SettingsContentState::apply_field`]).
    pub fn on_update_field(
        mut self,
        f: impl Fn(&mut S, SettingsField) + 'static,
    ) -> Self {
        self.on_update_field = Some(Rc::new(f));
        self
    }

    /// `updateExtensionSetting(extensionId, key, value)` — the
    /// contribution write.
    pub fn on_extension_setting(
        mut self,
        f: impl Fn(&mut S, String, String, String) + 'static,
    ) -> Self {
        self.on_extension_setting = Some(Rc::new(f));
        self
    }

    pub fn on_select_toggle(
        mut self,
        f: impl Fn(&mut S, SettingsSelect) + 'static,
    ) -> Self {
        self.on_select_toggle = Some(Rc::new(f));
        self
    }

    pub fn on_select_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_close = Some(Rc::new(f));
        self
    }

    /// The contribution selects' toggle — the id is
    /// `"{extensionId}/{key}"` (the state's
    /// `open_contribution_select` slot).
    pub fn on_contribution_toggle(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_contribution_toggle = Some(Rc::new(f));
        self
    }

    pub fn on_contribution_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_contribution_close = Some(Rc::new(f));
        self
    }

    /// `settingsCommands.openLogDir()` — the View Logs button.
    pub fn on_open_log_dir(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_log_dir = Some(Rc::new(f));
        self
    }

    /// `pickPath` — the log path input's browse button.
    pub fn on_browse_log_path(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_browse_log_path = Some(Rc::new(f));
        self
    }

    // ---- the text input lenses ----

    pub fn pool_size_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.pool_size_edit = Some(Rc::new(f));
        self
    }

    pub fn font_family_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.font_family_edit = Some(Rc::new(f));
        self
    }

    pub fn log_path_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.log_path_edit = Some(Rc::new(f));
        self
    }

    /// One lens for all six keymap inputs, keyed by action id.
    pub fn keymap_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S, &'static str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.keymap_edit = Some(Rc::new(f));
        self
    }

    /// One lens for every contribution text/number input, keyed
    /// `(extensionId, item key)`. The key borrows are free (only the
    /// returned edit ties to the state borrow).
    pub fn contribution_edit(
        mut self,
        f: impl for<'b, 'd> Fn(&'b mut S, &'d str, &'d str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.contribution_edit = Some(Rc::new(f));
        self
    }

    // ---- build ----

    /// The page: the nav rail and the content pane in one row.
    pub fn build(self) -> El {
        let k = kit(self.dark);
        let active = self.state.active_section;
        let sections = available_sections(self.capabilities);
        let mut page = row()
            .key("settings-page")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .gap(0.0);
        page = page.child(self.build_nav(&k, &sections, active));
        page = page.child(self.build_content(&k, &sections, active));
        page
    }

    /// The nav rail (`data-testid="settings-nav"`).
    fn build_nav(
        &self,
        k: &Kit,
        sections: &[&crate::settings_state::SettingsSectionDef],
        active: &str,
    ) -> El {
        let mut nav = col()
            .key("settings-nav")
            .w(NAV_WIDTH)
            .h_full()
            .shrink()
            .px(8.0)
            .pb(12.0)
            .gap(2.0)
            .fill(k.c.surface_weak); // bg-surface-alt

        // The back button renders only when the host supplies the
        // `onBack` prop.
        if let Some(on_back) = &self.on_back {
            let f = Rc::clone(on_back);
            let mut back = Button::new(t("common.back"))
                .icon(&ICON_ARROW_LEFT)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help(t("common.back"));
            back = back.on_press(move |s: &mut S| f(s));
            nav = nav
                .child(back.build(k).key("settings-back"))
                .child(col().key("settings-back-spacer").h(4.0)); // mb-1
        }

        for (index, (group_id, group_label)) in SETTINGS_NAV_GROUPS.iter().enumerate() {
            let mut group = col()
                .key(format!("settings-nav-group-{group_id}"))
                .w_full()
                .gap(2.0)
                // `first:pt-0` — only the later groups pad.
                .pt(if index == 0 { 0.0 } else { 10.0 });
            group = group.child(
                row()
                    .key(format!("settings-nav-group-label-{group_id}"))
                    .w_full()
                    .px(12.0)
                    .pb(2.0)
                    .child(
                        text(t(group_label).to_owned())
                            .font(NAV_GROUP_TEXT)
                            .color(k.c.text_muted), // text-fg-muted
                    ),
            );
            for section in sections
                .iter()
                .filter(|section| section.group == *group_id)
            {
                let is_active = section.id == active;
                let fill = if is_active {
                    // bg-accent/15
                    k.c.bg_accent.alpha(0.15)
                } else {
                    k.c.surface_weak
                };
                let (label_style, label_color) = if is_active {
                    (NAV_ITEM_ACTIVE, k.c.text_accent)
                } else {
                    (NAV_ITEM_TEXT, k.c.text_muted)
                };
                let mut item = row()
                    .key(format!("settings-nav-{}", section.id))
                    .w_full()
                    .h(30.0)
                    .px(12.0)
                    .gap(10.0)
                    .center_y()
                    .radius(6.0)
                    .fill(fill);
                if let Some(icon_el) = settings_section_icon(section.id) {
                    item = item.child(icon(icon_el, 16.0));
                }
                item = item.child(
                    text(t(section.label_key).to_owned())
                        .font(label_style)
                        .color(label_color),
                );
                if let Some(f) = &self.on_select_section {
                    let f = Rc::clone(f);
                    let id = section.id;
                    item = item.on_press(move |s: &mut S| f(s, id));
                }
                group = group.child(item);
            }
            nav = nav.child(group);
        }
        nav
    }

    /// The content pane (`data-testid="settings-content"`).
    fn build_content(
        &self,
        k: &Kit,
        sections: &[&crate::settings_state::SettingsSectionDef],
        active: &str,
    ) -> El {
        let mut layer = Vec::new();
        let body = match active {
            "general" => self.general_section(k, &mut layer, sections),
            "dataBrowsing" => self.data_browsing_section(k, &mut layer),
            "editor" => self.editor_section(k, &mut layer),
            "behavior" => self.behavior_section(k, &mut layer, sections),
            "logging" => self.logging_section(k, &mut layer),
            "appearance" => self.appearance_section_body(k, &mut layer),
            "ai" => self.delegated_section(k, "ai", self.ai_section.clone()),
            "prompts" => self.delegated_section(k, "prompts", self.prompts_section.clone()),
            "mcpServer" => self.delegated_section(k, "mcpServer", self.mcp_server_section.clone()),
            "mcpClient" => self.delegated_section(k, "mcpClient", self.mcp_client_section.clone()),
            "tunnels" => self.delegated_section(k, "tunnels", self.tunnel_section.clone()),
            "extensions" => self.delegated_section(k, "extensions", self.extensions_section.clone()),
            _ => self.delegated_section(k, active, None),
        };
        // The selects' open listboxes ride the overlay layer (the
        // section convention: the pane carries the layer's elements
        // as trailing children).
        col()
            .key("settings-content")
            .grow(1.0)
            .min_w(0.0)
            .h_full()
            .px(32.0)
            .py(24.0)
            .child(
                col()
                    .key(format!("settings-section-{active}"))
                    .w(BODY_MAX_WIDTH)
                    .gap(20.0)
                    .child(body),
            )
            .children(layer)
    }

    /// A delegated section: the child slot alone (an unlanded port
    /// renders its empty placeholder column — the section still
    /// exists as a nav target and a deep-link id).
    fn delegated_section(&self, k: &Kit, id: &str, slot: Option<El>) -> El {
        match slot {
            Some(el) => el,
            None => col().key(format!("settings-section-{id}")).w_full().child(
                text(t(
                    SETTINGS_SECTIONS
                        .iter()
                        .find(|section| section.id == id)
                        .map(|section| section.label_key)
                        .unwrap_or("settings.title"),
                )
                .to_owned())
                .font(tok::type_::TITLE)
                .color(k.c.text_base),
            ),
        }
    }

    /// The general section.
    fn general_section(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        _sections: &[&crate::settings_state::SettingsSectionDef],
    ) -> El {
        let mirror = &self.state.mirror;
        let mut body = col()
            .key("settings-section-general")
            .w_full()
            .gap(20.0)
            .child(section_title(
                k,
                layer,
                "settings-general-title".to_owned(),
                t("settings.general"),
                None,
            ));

        // The language select.
        let labels: Vec<String> = self
            .language_options
            .iter()
            .map(|(_, label)| label.clone())
            .collect();
        let values: Vec<String> = self
            .language_options
            .iter()
            .map(|(code, _)| code.clone())
            .collect();
        let current_label = self
            .language_options
            .iter()
            .find(|(code, _)| *code == mirror.language)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| mirror.language.clone());
        let mut select = Select::new("settings-language-select", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::Language));
        select = wire_select(
            select,
            SettingsSelect::Language,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::Language(value.to_owned()));
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-language-row".to_owned(),
            t("settings.language"),
            None,
            select_el,
        ));

        if let Some(card) = &self.language_packs_card {
            body = body.child(card.clone());
        }

        // `{(capabilities?.updater ?? true) && <UpdateSection />}`
        if updater_enabled(self.capabilities) {
            if let Some(section) = &self.update_section {
                body = body.child(section.clone());
            }
        }

        body = body.child(self.contributions_block(k, layer, ContributionTarget::General));

        // `{appVersion && <p>DataZen v{appVersion}</p>}` — a rejected
        // (or skipped — the `build-constant` gate) read never lands,
        // so the About line simply stays hidden.
        if let Some(version) = &self.state.app_version {
            body = body.child(
                col()
                    .key("settings-app-version")
                    .w_full()
                    .pt(16.0)
                    .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
                    .child(
                        text(format!("DataZen v{version}"))
                            .font(TEXT_XS)
                            .color(k.c.text_muted),
                    ),
            );
        }
        body
    }

    /// The data browsing section.
    fn data_browsing_section(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mirror = &self.state.mirror;
        let mut body = col()
            .key("settings-section-dataBrowsing")
            .w_full()
            .gap(20.0)
            .child(section_title(
                k,
                layer,
                "settings-dataBrowsing-title".to_owned(),
                t("settings.dataBrowsing"),
                None,
            ));

        // The default page size select.
        let labels: Vec<String> = PAGE_SIZE_OPTIONS
            .iter()
            .map(|v| format!("{v} {}", t("common.rows")))
            .collect();
        let values: Vec<String> = PAGE_SIZE_OPTIONS.iter().map(|v| v.to_string()).collect();
        let current = mirror.default_page_size.to_string();
        let current_label = PAGE_SIZE_OPTIONS
            .iter()
            .find(|v| v.to_string() == current)
            .map(|v| format!("{v} {}", t("common.rows")))
            .unwrap_or_else(|| current.clone());
        let mut select = Select::new("settings-default-page-size", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::PageSize));
        select = wire_select(
            select,
            SettingsSelect::PageSize,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                if let Ok(rows) = value.parse::<u32>() {
                    f(s, SettingsField::DefaultPageSize(rows));
                }
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-default-page-size-row".to_owned(),
            t("settings.defaultPageSize"),
            None,
            select_el,
        ));

        if let Some(section) = &self.table_data_section {
            body = body.child(section.clone());
        }

        // The connection pool size input (the lens + the per-edit
        // clamp commit).
        if let Some(lens) = &self.pool_size_edit {
            let lens = Rc::clone(lens);
            let mut field = TextField::new("settings-connection-pool-size", &self.state.pool_size_edit)
                .height(36.0)
                .width(96.0)
                .on_edit::<S>(move |s: &mut S| lens(s));
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                let read = Rc::clone(&self.pool_size_edit.as_ref().unwrap());
                field = field.on_change(move |s: &mut S, _tx| {
                    // Number(v); NaN guard; min(100, max(1, round(n))).
                    let text = read(s).text.trim().to_owned();
                    if let Ok(n) = text.parse::<f64>() {
                        if n.is_finite() {
                            let clamped = (n.round() as i64).clamp(1, 100) as u32;
                            f(s, SettingsField::ConnectionPoolSize(clamped));
                        }
                    }
                });
            }
            body = body.child(setting_row(
                k,
                layer,
                "settings-connection-pool-size-row".to_owned(),
                t("settings.connectionPoolSize"),
                Some(t("settings.connectionPoolSizeHint")),
                field.build(self.dark),
            ));
        }

        // The limit-select toggle.
        let mut limit_toggle = toggle_row(
            k,
            layer,
            ToggleRowSpec {
                key: "settings-limit-select".to_owned(),
                label: t("settings.limitSelect"),
                hint: None,
                checked: mirror.limit_select_results,
                disabled: false,
            },
            Rc::new(|_s: &mut S, _v: bool| {}),
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            limit_toggle = toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-limit-select".to_owned(),
                    label: t("settings.limitSelect"),
                    hint: None,
                    checked: mirror.limit_select_results,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, v: bool| f(s, SettingsField::LimitSelectResults(v))),
            );
        }
        body = body.child(limit_toggle);

        // `{settings.limitSelectResults && <SettingRow …maxRows select…>}`
        if mirror.limit_select_results {
            let labels: Vec<String> = RESULT_LIMIT_OPTIONS
                .iter()
                .map(|v| {
                    format!(
                        "{} {}",
                        SettingsContentState::formatted_rows(*v),
                        t("common.rows")
                    )
                })
                .collect();
            let values: Vec<String> = RESULT_LIMIT_OPTIONS.iter().map(|v| v.to_string()).collect();
            let current = mirror.query_result_limit.to_string();
            let current_label = RESULT_LIMIT_OPTIONS
                .iter()
                .find(|v| v.to_string() == current)
                .map(|v| {
                    format!(
                        "{} {}",
                        SettingsContentState::formatted_rows(*v),
                        t("common.rows")
                    )
                })
                .unwrap_or_else(|| current.clone());
            let mut select = Select::new("settings-query-result-limit", &labels)
                .values(&values)
                .current(Some(current_label.as_str()))
                .width(160.0)
                .open(self.state.open_select == Some(SettingsSelect::ResultLimit));
            select = wire_select(
                select,
                SettingsSelect::ResultLimit,
                &self.on_select_toggle,
                &self.on_select_close,
            );
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                    if let Ok(rows) = value.parse::<u32>() {
                        f(s, SettingsField::QueryResultLimit(rows));
                    }
                });
            }
            let select_el = select.build(k, layer);
            body = body.child(setting_row(
                k,
                layer,
                "settings-max-rows-row".to_owned(),
                t("settings.maxRows"),
                None,
                select_el,
            ));
        }

        // The auto-chart toggle.
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-auto-chart".to_owned(),
                    label: t("settings.autoChartOnQuery"),
                    hint: Some(t("settings.autoChartOnQueryHint")),
                    checked: mirror.auto_chart_on_query,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, v: bool| f(s, SettingsField::AutoChartOnQuery(v))),
            ));
        } else {
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-auto-chart".to_owned(),
                    label: t("settings.autoChartOnQuery"),
                    hint: Some(t("settings.autoChartOnQueryHint")),
                    checked: mirror.auto_chart_on_query,
                    disabled: false,
                },
                Rc::new(|_s: &mut S, _v: bool| {}),
            ));
        }

        // The workflow step result order select.
        let labels = vec![
            t("settings.workflowStepOrder.desc").to_owned(),
            t("settings.workflowStepOrder.asc").to_owned(),
        ];
        let values = vec!["desc".to_owned(), "asc".to_owned()];
        let is_asc = mirror.workflow_step_result_order_asc;
        let current_label = if is_asc {
            t("settings.workflowStepOrder.asc")
        } else {
            t("settings.workflowStepOrder.desc")
        }
        .to_owned();
        let mut select = Select::new("settings-workflow-step-order", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::WorkflowOrder));
        select = wire_select(
            select,
            SettingsSelect::WorkflowOrder,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::WorkflowStepResultOrderAsc(value == "asc"));
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-workflow-step-order-row".to_owned(),
            t("settings.workflowStepResultOrder"),
            Some(t("settings.workflowStepResultOrderHint")),
            select_el,
        ));

        body
    }

    /// The editor section.
    fn editor_section(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mirror = &self.state.mirror;
        let mut body = col()
            .key("settings-section-editor")
            .w_full()
            .gap(20.0)
            .child(section_title(
                k,
                layer,
                "settings-editor-title".to_owned(),
                t("settings.editor"),
                None,
            ));

        // The font size slider + its "{n}px" readout.
        let mut slider = Slider::<S>::new("settings-font-size", mirror.editor_font_size)
            .min(10.0)
            .max(24.0)
            .step(1.0);
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            slider = slider.on_change(move |s: &mut S, v: f64| {
                f(s, SettingsField::EditorFontSize(v));
            });
        }
        let slider_row = row()
            .key("settings-font-size-slider")
            .w_full()
            .gap(12.0)
            .center_y()
            .child(col().key("settings-font-size-track").grow(1.0).min_w(0.0).child(slider.build(self.dark)))
            .child(
                text(format!("{}px", mirror.editor_font_size as i64))
                    .key("settings-font-size-value")
                    .font(TEXT_SM)
                    .color(k.c.text_muted),
            );
        body = body.child(setting_row(
            k,
            layer,
            "settings-font-size-row".to_owned(),
            t("settings.fontSize"),
            None,
            slider_row,
        ));

        // The font family input.
        if let Some(lens) = &self.font_family_edit {
            let lens = Rc::clone(lens);
            let mut field = TextField::new("settings-font-family", &self.state.font_family_edit)
                .height(36.0)
                .on_edit::<S>(move |s: &mut S| lens(s));
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                let read = Rc::clone(&self.font_family_edit.as_ref().unwrap());
                field = field.on_change(move |s: &mut S, _tx| {
                    let text = read(s).text.clone();
                    f(s, SettingsField::EditorFontFamily(text));
                });
            }
            body = body.child(setting_row(
                k,
                layer,
                "settings-font-family-row".to_owned(),
                t("settings.fontFamily"),
                None,
                field.build(self.dark),
            ));
        }

        // The SQL syntax theme select + preview slot.
        let labels: Vec<String> = self.syntax_presets.iter().map(|(_, l)| l.clone()).collect();
        let values: Vec<String> = self.syntax_presets.iter().map(|(id, _)| id.clone()).collect();
        let current = mirror.sql_syntax_theme.clone();
        let current_label = self
            .syntax_presets
            .iter()
            .find(|(id, _)| *id == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| current.clone());
        let mut select = Select::new("settings-sql-syntax-theme", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(200.0)
            .open(self.state.open_select == Some(SettingsSelect::SyntaxTheme));
        select = wire_select(
            select,
            SettingsSelect::SyntaxTheme,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::SqlSyntaxTheme(value.to_owned()));
            });
        }
        let mut theme_content = col().key("settings-sql-syntax-theme-content").w_full().gap(8.0);
        theme_content = theme_content.child(select.build(k, layer));
        if let Some(preview) = &self.sql_syntax_preview {
            theme_content = theme_content.child(preview.clone());
        }
        body = body.child(setting_row(
            k,
            layer,
            "settings-sql-syntax-theme-row".to_owned(),
            t("settings.sqlSyntaxTheme"),
            Some(t("settings.sqlSyntaxThemeHint")),
            theme_content,
        ));

        // The completion quote policy select.
        let labels = vec![
            t("settings.quotePolicy.unquoted").to_owned(),
            t("settings.quotePolicy.both").to_owned(),
            t("settings.quotePolicy.always").to_owned(),
        ];
        let values = vec![
            "unquoted".to_owned(),
            "both".to_owned(),
            "always".to_owned(),
        ];
        let current = mirror.editor_completion_quote_policy.clone();
        let current_label = match current.as_str() {
            "both" => t("settings.quotePolicy.both"),
            "always" => t("settings.quotePolicy.always"),
            _ => t("settings.quotePolicy.unquoted"),
        }
        .to_owned();
        let mut select = Select::new("settings-quote-policy", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::QuotePolicy));
        select = wire_select(
            select,
            SettingsSelect::QuotePolicy,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::CompletionQuotePolicy(value.to_owned()));
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-quote-policy-row".to_owned(),
            t("settings.editorCompletionQuotePolicy"),
            Some(t("settings.editorCompletionQuotePolicyHint")),
            select_el,
        ));

        // The include-table-prefix toggle.
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-completion-table-prefix".to_owned(),
                    label: t("settings.editorCompletionIncludeTablePrefix"),
                    hint: None,
                    checked: mirror.editor_completion_include_table_prefix,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, v: bool| {
                    f(s, SettingsField::CompletionIncludeTablePrefix(v))
                }),
            ));
        } else {
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-completion-table-prefix".to_owned(),
                    label: t("settings.editorCompletionIncludeTablePrefix"),
                    hint: None,
                    checked: mirror.editor_completion_include_table_prefix,
                    disabled: false,
                },
                Rc::new(|_s: &mut S, _v: bool| {}),
            ));
        }

        // The FK prediction toggle (the React testid).
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-toggle-enableFkPrediction".to_owned(),
                    label: t("settings.enableFkPrediction"),
                    hint: Some(t("settings.enableFkPredictionHint")),
                    checked: mirror.enable_fk_prediction,
                    disabled: false,
                },
                Rc::new(move |s: &mut S, v: bool| f(s, SettingsField::EnableFkPrediction(v))),
            ));
        } else {
            body = body.child(toggle_row(
                k,
                layer,
                ToggleRowSpec {
                    key: "settings-toggle-enableFkPrediction".to_owned(),
                    label: t("settings.enableFkPrediction"),
                    hint: Some(t("settings.enableFkPredictionHint")),
                    checked: mirror.enable_fk_prediction,
                    disabled: false,
                },
                Rc::new(|_s: &mut S, _v: bool| {}),
            ));
        }

        // The keymap preset select.
        let labels: Vec<String> = KEYMAP_PRESET_OPTIONS
            .iter()
            .map(|(_, label_key)| t(label_key).to_owned())
            .collect();
        let values: Vec<String> = KEYMAP_PRESET_OPTIONS
            .iter()
            .map(|(value, _)| (*value).to_owned())
            .collect();
        let current = mirror.keymap_preset.clone();
        let current_label = KEYMAP_PRESET_OPTIONS
            .iter()
            .find(|(value, _)| *value == current)
            .map(|(_, label_key)| t(label_key).to_owned())
            .unwrap_or_else(|| current.clone());
        let mut select = Select::new("settings-keymap-preset", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::KeymapPreset));
        select = wire_select(
            select,
            SettingsSelect::KeymapPreset,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::KeymapPreset(value.to_owned()));
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-keymap-preset-row".to_owned(),
            t("settings.keymap.preset"),
            None,
            select_el,
        ));

        // The keymap customize card.
        body = body.child(self.keymap_card(k, layer));

        // The SQL snippets card slot.
        if let Some(card) = &self.sql_snippets_card {
            body = body.child(card.clone());
        }

        body = body.child(self.contributions_block(k, layer, ContributionTarget::Editor));

        body
    }

    /// The keymap customize card: the header with its reset button
    /// and one input + kbd chip row per `KEYMAP_ACTIONS` entry.
    fn keymap_card(&self, k: &Kit, _layer: &mut Vec<El>) -> El {
        let mirror = &self.state.mirror;
        let mut card = col()
            .key("settings-keymap-customize")
            .w_full()
            .gap(12.0)
            .padding(14.0)
            .radius(8.0)
            .fill(k.c.surface_weak) // bg-surface-alt
            .border(1.0, k.c.border_muted);

        // The header row: the label + the reset button (only while a
        // custom entry exists).
        let mut header = row()
            .key("settings-keymap-customize-header")
            .w_full()
            .justify_between()
            .center_y()
            .child(
                text(t("settings.keymap.customize").to_owned())
                    .font(TEXT_XS_SEMIBOLD)
                    .color(k.c.text_muted),
            );
        if !mirror.custom_keymap.is_empty() {
            let mut reset = Button::new(t("settings.keymap.resetCustom"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                reset = reset.on_press(move |s: &mut S| {
                    f(s, SettingsField::CustomKeymap(Vec::new()));
                });
            }
            header = header.child(reset.build(k).key("settings-keymap-reset"));
        }
        card = card.child(header);

        // One row per action: the label, the input, the kbd chip.
        let mut rows = col().key("settings-keymap-customize-rows").w_full().gap(8.0);
        for (action, label_key) in KEYMAP_ACTIONS.iter() {
            let current =
                get_action_shortcut(action, &mirror.keymap_preset, &mirror.custom_keymap);
            let display = format_shortcut_for_display(&current, self.is_mac);
            let mut row_el = row()
                .key(format!("settings-keymap-row-{action}"))
                .w_full()
                .gap(16.0)
                .justify_between()
                .center_y()
                .child(
                    text(t(label_key).to_owned())
                        .font(TEXT_XS)
                        .color(k.c.text_muted),
                );
            let mut controls = row()
                .key(format!("settings-keymap-controls-{action}"))
                .gap(8.0)
                .center_y();
            if let Some(lens) = &self.keymap_edit {
                let lens = Rc::clone(lens);
                let action_id: &'static str = action;
                let mut field = TextField::new(
                    format!("settings-keymap-input-{action}"),
                    self.state.keymap_edit(action),
                )
                .height(28.0)
                .width(128.0)
                .text_style(TEXT_11_MONO.mono())
                .placeholder("e.g. Mod-Enter")
                .on_edit::<S>(move |s: &mut S| lens(s, action_id));
                if let Some(f) = &self.on_update_field {
                    let f = Rc::clone(f);
                    let read = Rc::clone(&self.keymap_edit.as_ref().unwrap());
                    field = field.on_change(move |s: &mut S, _tx| {
                        let text = read(s, action_id).text.clone();
                        f(
                            s,
                            SettingsField::CustomKeymapEdit {
                                action: action_id,
                                input: text,
                            },
                        );
                    });
                }
                controls = controls.child(field.build(self.dark));
            }
            controls = controls.child(
                row()
                    .key(format!("settings-keymap-kbd-{action}"))
                    .min_w(40.0)
                    .px(6.0)
                    .py(2.0)
                    .radius(4.0)
                    .fill(k.c.surface_raised) // bg-surface-raised
                    .border(1.0, k.c.border_muted)
                    .center()
                    .child(
                        text(display.clone())
                            .font(TEXT_10_MONO.mono())
                            .color(k.c.text_muted),
                    ),
            );
            row_el = row_el.child(controls);
            rows = rows.child(row_el);
        }
        card = card.child(rows);
        card
    }

    /// The behavior section.
    fn behavior_section(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        sections: &[&crate::settings_state::SettingsSectionDef],
    ) -> El {
        let mirror = &self.state.mirror;
        let mut body = col()
            .key("settings-section-behavior")
            .w_full()
            .gap(20.0)
            .child(section_title(
                k,
                layer,
                "settings-behavior-title".to_owned(),
                t("settings.behavior"),
                None,
            ));

        let toggles: [(&str, &str, Option<&str>, bool, SettingsField); 4] = [
            (
                "settings-confirm-delete",
                "settings.confirmDelete",
                None,
                mirror.confirm_on_delete,
                SettingsField::ConfirmOnDelete(mirror.confirm_on_delete),
            ),
            (
                "settings-auto-commit",
                "settings.autoCommit",
                None,
                mirror.auto_commit,
                SettingsField::AutoCommit(mirror.auto_commit),
            ),
            (
                "settings-safe-mode",
                "settings.safeMode",
                Some("settings.safeModeHint"),
                mirror.safe_mode,
                SettingsField::SafeMode(mirror.safe_mode),
            ),
            (
                "settings-confirm-dangerous",
                "settings.confirmDangerousExecution",
                Some("settings.confirmDangerousExecutionHint"),
                // `confirmDangerousExecution !== false`
                mirror.confirm_dangerous_execution,
                SettingsField::ConfirmDangerousExecution(
                    mirror.confirm_dangerous_execution,
                ),
            ),
        ];
        for (key, label_key, hint_key, checked, field) in toggles {
            if let Some(f) = &self.on_update_field {
                let f = Rc::clone(f);
                body = body.child(toggle_row(
                    k,
                    layer,
                    ToggleRowSpec {
                        key: key.to_owned(),
                        label: t(label_key),
                        hint: hint_key.map(t),
                        checked,
                        disabled: false,
                    },
                    Rc::new(move |s: &mut S, v: bool| {
                        // The same field with the NEXT value.
                        let next = match &field {
                            SettingsField::ConfirmOnDelete(_) => {
                                SettingsField::ConfirmOnDelete(v)
                            }
                            SettingsField::AutoCommit(_) => SettingsField::AutoCommit(v),
                            SettingsField::SafeMode(_) => SettingsField::SafeMode(v),
                            SettingsField::ConfirmDangerousExecution(_) => {
                                SettingsField::ConfirmDangerousExecution(v)
                            }
                            other => other.clone(),
                        };
                        f(s, next);
                    }),
                ));
            } else {
                body = body.child(toggle_row(
                    k,
                    layer,
                    ToggleRowSpec {
                        key: key.to_owned(),
                        label: t(label_key),
                        hint: hint_key.map(t),
                        checked,
                        disabled: false,
                    },
                    Rc::new(|_s: &mut S, _v: bool| {}),
                ));
            }
        }

        // `{availableSections.some(s => s.id === 'logging') &&
        // <DataCleanupSection />}`
        if sections.iter().any(|section| section.id == "logging") {
            if let Some(section) = &self.data_cleanup_section {
                body = body.child(section.clone());
            }
        }

        body
    }

    /// The logging section.
    fn logging_section(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mirror = &self.state.mirror;
        let mut body = col()
            .key("settings-section-logging")
            .w_full()
            .gap(20.0)
            .child(section_title(
                k,
                layer,
                "settings-logging-title".to_owned(),
                t("settings.logging"),
                Some(t("settings.logRestartNote")),
            ));

        // The log level select (the labels are the capitalized
        // spellings, never translated).
        let labels: Vec<String> = LOG_LEVEL_OPTIONS
            .iter()
            .map(|value| {
                let mut chars = value.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let values: Vec<String> = LOG_LEVEL_OPTIONS.iter().map(|v| v.to_string()).collect();
        let current = mirror.log_level.clone();
        let current_label = labels
            .iter()
            .zip(LOG_LEVEL_OPTIONS.iter())
            .find(|(_, value)| **value == current)
            .map(|(label, _)| label.clone())
            .unwrap_or_else(|| current.clone());
        let mut select = Select::new("settings-log-level", &labels)
            .values(&values)
            .current(Some(current_label.as_str()))
            .width(160.0)
            .open(self.state.open_select == Some(SettingsSelect::LogLevel));
        select = wire_select(
            select,
            SettingsSelect::LogLevel,
            &self.on_select_toggle,
            &self.on_select_close,
        );
        if let Some(f) = &self.on_update_field {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, SettingsField::LogLevel(value.to_owned()));
            });
        }
        let select_el = select.build(k, layer);
        body = body.child(setting_row(
            k,
            layer,
            "settings-log-level-row".to_owned(),
            t("settings.logLevel"),
            None,
            select_el,
        ));

        // The log path input: the placeholder is the default path
        // (the `getLogPath` read) with the translated fallback.
        if let Some(lens) = &self.log_path_edit {
            let lens = Rc::clone(lens);
            let placeholder = self
                .state
                .default_log_path
                .clone()
                .unwrap_or_else(|| t("settings.logPathPlaceholder").to_owned());
            // The build gate requires SOME browse handler: the slot
            // when the host supplies one, a no-op otherwise (the
            // input stays editable; `pickPath` is the host's picker).
            let browse = self.on_browse_log_path.clone();
            let mut path = match browse {
                Some(f) => PathInput::new("settings-log-path", &self.state.log_path_edit)
                    .placeholder(placeholder)
                    .on_browse(move |s: &mut S| f(s)),
                None => PathInput::new("settings-log-path", &self.state.log_path_edit)
                    .placeholder(placeholder)
                    .on_browse(|_s: &mut S| {}),
            };
            path = path.on_edit(move |s: &mut S| lens(s));
            body = body.child(setting_row(
                k,
                layer,
                "settings-log-path-row".to_owned(),
                t("settings.logPath"),
                None,
                path.build(self.dark),
            ));
        }

        // The View Logs button (`variant="secondary"` → Outline).
        let mut view_logs = Button::new(t("common.viewLogs")).variant(ButtonVariant::Outline);
        if let Some(f) = &self.on_open_log_dir {
            let f = Rc::clone(f);
            view_logs = view_logs.on_press(move |s: &mut S| f(s));
        }
        body = body.child(
            row()
                .key("settings-view-logs")
                .w_full()
                .gap(12.0)
                .child(view_logs.build(k).key("settings-view-logs-button")),
        );

        body
    }

    /// The appearance section: the child slot + the appearance
    /// contributions.
    fn appearance_section_body(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut body = col()
            .key("settings-section-appearance")
            .w_full()
            .gap(20.0);
        if let Some(section) = &self.appearance_section {
            body = body.child(section.clone());
        }
        body = body.child(self.contributions_block(k, layer, ContributionTarget::Appearance));
        body
    }

    /// `renderSectionContributions(targetSection)`: the matched
    /// contributions below a divider, `None` (nothing rendered) when
    /// the list is empty.
    fn contributions_block(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        target: ContributionTarget,
    ) -> El {
        let matched: Vec<&SettingsContribution> = self
            .contributions
            .iter()
            .filter(|contribution| contribution.target == target)
            .collect();
        if matched.is_empty() {
            // React returns null: no divider, no gap.
            return col().key(format!("settings-contributions-{:?}", target));
        }

        let mut block = col()
            .key(format!("settings-contributions-{:?}", target))
            .w_full()
            .gap(24.0)
            .pt(16.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted); // border-t

        for contribution in matched {
            let mut group = col()
                .key(format!("settings-contrib-{}", contribution.extension_id))
                .w_full()
                .gap(16.0);

            // The `renderGroup` arm: the prebuilt element alone.
            if let Some(group_el) = &contribution.group {
                group = group.child(group_el.clone());
                block = block.child(group);
                continue;
            }

            // The optional h4 title (with its description hint).
            if contribution.group_title.is_some() || contribution.group_description.is_some() {
                let title = contribution
                    .group_title
                    .clone()
                    .unwrap_or_else(|| contribution.extension_id.clone());
                let mut title_row = row().key(format!(
                    "settings-contrib-{}-title",
                    contribution.extension_id
                ))
                .w_full()
                .gap(4.0)
                .center_y()
                .child(
                    text(title.clone())
                        .font(TEXT_SM_SEMIBOLD)
                        .color(k.c.text_base),
                );
                if let Some(description) = &contribution.group_description {
                    title_row = title_row.child(
                        crate::views::settings::setting_hint::SettingHint::new(
                            format!("settings-contrib-{}-title-hint", contribution.extension_id),
                            title,
                            description.clone(),
                        )
                        .build(k, layer),
                    );
                }
                group = group.child(title_row);
            }

            let mut items = col()
                .key(format!("settings-contrib-{}-items", contribution.extension_id))
                .w_full()
                .gap(12.0);
            for item in &contribution.items {
                items = items.child(self.contribution_item(k, layer, contribution, item));
            }
            group = group.child(items);
            block = block.child(group);
        }
        block
    }

    /// One contribution item (the `item.type` arms).
    fn contribution_item(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        contribution: &SettingsContribution,
        item: &ContributionItem,
    ) -> El {
        let row_key = format!(
            "settings-contrib-{}-{}",
            contribution.extension_id, item.key
        );
        match &item.control {
            ContributionControl::Boolean(checked) => {
                if let Some(f) = &self.on_extension_setting {
                    let f = Rc::clone(f);
                    let extension_id = contribution.extension_id.clone();
                    let key = item.key.clone();
                    let checked = *checked;
                    return toggle_row(
                        k,
                        layer,
                        ToggleRowSpec {
                            key: row_key,
                            label: &item.label,
                            hint: item.hint.as_deref(),
                            checked,
                            disabled: false,
                        },
                        Rc::new(move |s: &mut S, v: bool| {
                            f(s, extension_id.clone(), key.clone(), v.to_string());
                        }),
                    );
                }
                toggle_row(
                    k,
                    layer,
                    ToggleRowSpec {
                        key: row_key,
                        label: &item.label,
                        hint: item.hint.as_deref(),
                        checked: *checked,
                        disabled: false,
                    },
                    Rc::new(|_s: &mut S, _v: bool| {}),
                )
            }
            ContributionControl::Select { current, options } => {
                let labels: Vec<String> = options.iter().map(|(_, label)| label.clone()).collect();
                let values: Vec<String> = options.iter().map(|(value, _)| value.clone()).collect();
                let current_label = options
                    .iter()
                    .find(|(value, _)| value == current)
                    .map(|(_, label)| label.clone())
                    .unwrap_or_else(|| current.clone());
                let select_key = format!("{row_key}-select");
                let open_id = format!("{}/{}", contribution.extension_id, item.key);
                let mut select = Select::new(select_key, &labels)
                    .values(&values)
                    .current(Some(current_label.as_str()))
                    .width(160.0)
                    .open(self.state.open_contribution_select.as_deref() == Some(&open_id));
                {
                    let toggle = self.on_contribution_toggle.clone();
                    select = select.on_toggle(move |s: &mut S| {
                        if let Some(f) = &toggle {
                            f(s, open_id.clone());
                        }
                    });
                }
                if let Some(f) = &self.on_contribution_close {
                    let f = Rc::clone(f);
                    select = select.on_close(move |s: &mut S| f(s));
                }
                if let Some(f) = &self.on_extension_setting {
                    let f = Rc::clone(f);
                    let extension_id = contribution.extension_id.clone();
                    let key = item.key.clone();
                    select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                        f(s, extension_id.clone(), key.clone(), value.to_owned());
                    });
                }
                let select_el = select.build(k, layer);
                setting_row(
                    k,
                    layer,
                    format!("{row_key}-row"),
                    &item.label,
                    item.hint.as_deref(),
                    select_el,
                )
            }
            ContributionControl::Number(value) => {
                if let (Some(lens), Some(f)) = (&self.contribution_edit, &self.on_extension_setting)
                {
                    let lens = Rc::clone(lens);
                    let read = Rc::clone(&self.contribution_edit.as_ref().unwrap());
                    let f = Rc::clone(f);
                    let edit_ext = contribution.extension_id.clone();
                    let edit_key = item.key.clone();
                    let change_ext = contribution.extension_id.clone();
                    let change_key = item.key.clone();
                    let field = TextField::new(
                        format!("{row_key}-number"),
                        self.state.contribution_edit(&contribution.extension_id, &item.key),
                    )
                    .height(36.0)
                    .width(128.0)
                    .on_edit::<S>(move |s: &mut S| {
                        contribution_edit_at(s, &lens, &edit_ext, &edit_key)
                    })
                    .on_change(move |s: &mut S, _tx| {
                        // `Number(e.target.value)` — the empty string
                        // is 0, a non-number never parses (stays 0 in
                        // React too via NaN passthrough; the string
                        // seam keeps the last parseable value).
                        let text =
                            contribution_edit_at(s, &read, &change_ext, &change_key)
                                .text
                                .clone();
                        let number = text.trim().parse::<f64>().unwrap_or(0.0);
                        f(s, change_ext.clone(), change_key.clone(), format_number(number));
                    });
                    return setting_row(
                        k,
                        layer,
                        format!("{row_key}-row"),
                        &item.label,
                        item.hint.as_deref(),
                        field.build(self.dark),
                    );
                }
                setting_row(
                    k,
                    layer,
                    format!("{row_key}-row"),
                    &item.label,
                    item.hint.as_deref(),
                    text(format_number(*value)).font(TEXT_SM).color(k.c.text_base),
                )
            }
            ContributionControl::Text(value) => {
                if let (Some(lens), Some(f)) = (&self.contribution_edit, &self.on_extension_setting)
                {
                    let lens = Rc::clone(lens);
                    let read = Rc::clone(&self.contribution_edit.as_ref().unwrap());
                    let f = Rc::clone(f);
                    let edit_ext = contribution.extension_id.clone();
                    let edit_key = item.key.clone();
                    let change_ext = contribution.extension_id.clone();
                    let change_key = item.key.clone();
                    let field = TextField::new(
                        format!("{row_key}-text"),
                        self.state.contribution_edit(&contribution.extension_id, &item.key),
                    )
                    .height(36.0)
                    .grow()
                    .min_width(0.0)
                    .on_edit::<S>(move |s: &mut S| {
                        contribution_edit_at(s, &lens, &edit_ext, &edit_key)
                    })
                    .on_change(move |s: &mut S, _tx| {
                        let text =
                            contribution_edit_at(s, &read, &change_ext, &change_key)
                                .text
                                .clone();
                        f(s, change_ext.clone(), change_key.clone(), text);
                    });
                    return setting_row(
                        k,
                        layer,
                        format!("{row_key}-row"),
                        &item.label,
                        item.hint.as_deref(),
                        field.build(self.dark),
                    );
                }
                setting_row(
                    k,
                    layer,
                    format!("{row_key}-row"),
                    &item.label,
                    item.hint.as_deref(),
                    text(value.clone()).font(TEXT_SM).color(k.c.text_base),
                )
            }
            ContributionControl::Custom(control) => setting_row(
                k,
                layer,
                format!("{row_key}-row"),
                &item.label,
                item.hint.as_deref(),
                control.clone(),
            ),
        }
    }
}

/// `String(Number(v))` — JavaScript's number-to-string (integers
/// without a decimal part).
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// The contribution lens as a fn item (closure literals break the
/// HRTB inference — the `workflow_form::input_edit_at` convention).
/// The key borrows are free: only the state borrow ties the return.
fn contribution_edit_at<'b, S: 'static>(
    s: &'b mut S,
    lens: &Rc<dyn for<'c, 'd> Fn(&'c mut S, &'d str, &'d str) -> &'c mut TextEdit>,
    extension_id: &str,
    key: &str,
) -> &'b mut TextEdit {
    lens(s, extension_id, key)
}

/// The kit select's `on_toggle`/`on_close` pair (one helper for every
/// page select — the widget-editor-drawer convention).
fn wire_select<S: 'static>(
    select: Select,
    which: SettingsSelect,
    toggle_slot: &Option<Rc<dyn Fn(&mut S, SettingsSelect)>>,
    close_slot: &Option<Rc<dyn Fn(&mut S)>>,
) -> Select {
    let toggle = toggle_slot.clone();
    let mut select = select.on_toggle(move |s: &mut S| {
        if let Some(f) = &toggle {
            f(s, which);
        }
    });
    let close = close_slot.clone();
    select = select.on_close(move |s: &mut S| {
        if let Some(f) = &close {
            f(s);
        }
    });
    select
}
