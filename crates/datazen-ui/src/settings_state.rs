//! AppState method implementations.

#![allow(unused_imports)]
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::state_def::*;
use crate::types::{Section, TableView, Tool};
use crate::views::{next_connection, workflow_yaml};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DesktopAdapter, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting,
    SettingsApi, TabKind, TablePage, Tabs, UnimplementedAdapter, Workflow, WorkflowStepResult,
    WorkspaceApi, DESKTOP_NOT_IMPLEMENTED,
};
use rune::tok::{self, type_};
use rune::{
    col, oklch, row, text, text_input, App, AppSpec, Color, Cx, El, FileDialogOptions,
    FileDialogResult, Ime, Key, KeyEvent, TextEdit, Titlebar, Tx,
};

/// A6: the 12 settings sections split across 2 nav groups, mirroring the
/// React settings page structure. The id is the deep-link key within the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsSectionDef {
    pub id: &'static str,
    pub label_key: &'static str,
    pub group: &'static str,
}

pub const SETTINGS_NAV_GROUPS: [(&str, &str); 2] = [
    ("app", "settings.nav.group.app"),
    ("integration", "settings.nav.group.integration"),
];

pub const SETTINGS_SECTIONS: [SettingsSectionDef; 12] = [
    SettingsSectionDef {
        id: "general",
        label_key: "settings.general",
        group: "app",
    },
    SettingsSectionDef {
        id: "appearance",
        label_key: "settings.appearance",
        group: "app",
    },
    SettingsSectionDef {
        id: "dataBrowsing",
        label_key: "settings.dataBrowsing",
        group: "app",
    },
    SettingsSectionDef {
        id: "editor",
        label_key: "settings.editor",
        group: "app",
    },
    SettingsSectionDef {
        id: "behavior",
        label_key: "settings.behavior",
        group: "app",
    },
    SettingsSectionDef {
        id: "logging",
        label_key: "settings.logging",
        group: "app",
    },
    SettingsSectionDef {
        id: "ai",
        label_key: "common.aiAssistant",
        group: "integration",
    },
    SettingsSectionDef {
        id: "prompts",
        label_key: "settings.prompts",
        group: "integration",
    },
    SettingsSectionDef {
        id: "mcpServer",
        label_key: "settings.mcp.title",
        group: "integration",
    },
    SettingsSectionDef {
        id: "mcpClient",
        label_key: "settings.mcpClient.title",
        group: "integration",
    },
    SettingsSectionDef {
        id: "tunnels",
        label_key: "settings.tunnels.title",
        group: "integration",
    },
    SettingsSectionDef {
        id: "extensions",
        label_key: "settings.extensions.title",
        group: "integration",
    },
];

pub fn parse_settings_section(value: &str) -> &'static str {
    SETTINGS_SECTIONS
        .iter()
        .find(|section| section.id == value)
        .map(|section| section.id)
        .unwrap_or("general")
}

impl AppState {
    /// Deep-link form (A6): the selected settings section key. Unknown keys
    /// normalize to `general` so callers can address any of the 12 sections.
    pub fn settings_section(&self) -> &'static str {
        self.settings_content.active_section
    }

    /// Select a settings section by key (A6). Accepts deep-link ids such as
    /// `appearance` or `mcpServer`; invalid ids fall back to `general`.
    pub fn set_settings_section(&mut self, section: &str) {
        self.appearance_select = None;
        self.settings_content.close_select();
        self.settings_content
            .select_section(parse_settings_section(section));
    }

    pub fn ui_settings(&self) -> Vec<Setting> {
        match self.desktop.read_ui_settings() {
            Ok(settings) => settings,
            Err(_) => self.workspace.read_settings(),
        }
    }

    /// A missing or unsupported stored locale keeps the pre-migration UI language.
    pub fn builtin_language(&self) -> datazen_ui_api::BuiltinLanguage {
        self.ui_settings()
            .into_iter()
            .find_map(|setting| match setting {
                Setting::BuiltinLanguage { locale } => Some(locale),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub(crate) fn save_builtin_language(&mut self, code: &str) {
        self.settings_content.close_select();
        let result = datazen_ui_api::BuiltinLanguage::from_code(code)
            .ok_or_else(|| format!("unsupported built-in language: {code}"))
            .and_then(|locale| self.write_ui_setting(Setting::BuiltinLanguage { locale }));
        self.settings_error = result.err();
    }

    /// The active appearance: the persisted theme setting (from the desktop
    /// adapter) or the system preference when none is saved. Memory-workspace
    /// fallback settings are not treated as a persisted theme choice.
    pub fn dark_theme(&self) -> bool {
        let saved = match self.desktop.read_ui_settings() {
            Ok(settings) => settings.into_iter().find_map(|setting| match setting {
                Setting::Theme { dark } => Some(dark),
                _ => None,
            }),
            Err(_) => None,
        };
        saved.unwrap_or_else(|| self.system_dark.get())
    }

    /// The snippets in force (T3): the host's once its file store persists
    /// them, the memory port's while it does not. The host's read answers
    /// from its loaded file — memory-speed, like the settings read.
    pub fn write_ui_setting(&mut self, setting: Setting) -> Result<(), String> {
        match self.desktop.write_ui_setting(setting.clone()) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.write_setting(setting),
            Err(error) => Err(error),
        }
    }

    /// One persisted layout size in points (L1), read from the settings in
    /// force. `None` means the region has never been resized: the caller
    /// applies its default.
    pub fn layout_size(&self, key: &str) -> Option<u32> {
        self.ui_settings()
            .into_iter()
            .find_map(|setting| match setting {
                Setting::Layout { key: region, value } if region == key => Some(value),
                _ => None,
            })
    }

    /// Persist one layout size in points (L1): the splitter stores it once
    /// the drag ends, so a restart reopens the same proportions.
    pub fn set_layout_size(&mut self, key: &str, value: u32) -> Result<(), String> {
        self.write_ui_setting(Setting::Layout {
            key: key.into(),
            value,
        })
    }

    /// L4: the rail is expanded (icon + label) by default; collapsed shows
    /// icons only. Persisted as its width in points.
    pub fn sidebar_expanded(&self) -> bool {
        self.sidebar_rail_width() > RAIL_WIDTH_COLLAPSED
    }
    pub fn sidebar_rail_width(&self) -> u32 {
        let w = self
            .layout_size(SIDEBAR_RAIL_WIDTH_KEY)
            .unwrap_or(RAIL_WIDTH_EXPANDED);
        if w <= RAIL_WIDTH_COLLAPSED {
            RAIL_WIDTH_COLLAPSED
        } else {
            RAIL_WIDTH_EXPANDED
        }
    }
    pub fn toggle_sidebar_expanded(&mut self) {
        let next = if self.sidebar_expanded() {
            RAIL_WIDTH_COLLAPSED
        } else {
            RAIL_WIDTH_EXPANDED
        };
        let _ = self.set_layout_size(SIDEBAR_RAIL_WIDTH_KEY, next);
    }

    /// L4: navigator panel width in points, clamped to its resizable range.
    pub fn navigator_width(&self) -> u32 {
        let w = self
            .layout_size(NAVIGATOR_WIDTH_KEY)
            .unwrap_or(NAVIGATOR_WIDTH_DEFAULT);
        w.clamp(NAVIGATOR_WIDTH_MIN, NAVIGATOR_WIDTH_MAX)
    }

    /// The table page size from the settings in force (fallback: the built-in 25).
    pub fn table_page_size(&self) -> u32 {
        self.ui_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::TablePageSize { rows } => Some(*rows),
                _ => None,
            })
            .unwrap_or(TABLE_PAGE_SIZE)
    }

    /// A4: scope key for per-table column width persistence.
    pub fn colw_scope(&self) -> String {
        self.desktop_connection
            .clone()
            .unwrap_or_else(|| "local".into())
    }

    /// A4: stable layout key for one column width, persisted per table.
    /// Uses a stable FNV-1a hash so the key stays within the Layout key
    /// alphabet (lowercase letters, digits, hyphens) regardless of table or
    /// column name characters.
    pub fn colw_key(&self, table: &str, column: &str) -> String {
        fn fnv1a64(s: &str) -> u64 {
            let mut h: u64 = 0xcbf29ce484222325;
            for b in s.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h
        }
        let identity = format!("{}\0{}\0{}", self.colw_scope(), table, column);
        format!("colw-{:016x}", fnv1a64(&identity))
    }

    /// A4: apply persisted column widths for the currently loaded table grid.
    pub fn apply_table_column_widths(&mut self) {
        if !self.grid.has_page() {
            return;
        }
        let table = self.grid.table_label().to_owned();
        let cols = self.grid.cols;
        for col in 0..cols {
            if let Some(name) = self.grid.column_name(col).map(str::to_owned) {
                let key = self.colw_key(&table, &name);
                if let Some(width) = self.layout_size(&key) {
                    self.grid.set_col_w(col, width);
                }
            }
        }
    }

    /// A4: set and persist one table column width (points).
    pub fn set_table_column_width(
        &mut self,
        table: &str,
        column: &str,
        width: u32,
    ) -> Result<(), String> {
        let key = self.colw_key(table, column);
        let width = width.max(40);
        self.set_layout_size(&key, width)?;
        if self.grid.has_page() && self.grid.table_label() == table {
            if let Some(col) =
                (0..self.grid.cols).find(|c| self.grid.column_name(*c) == Some(column))
            {
                self.grid.set_col_w(col, width);
            }
        }
        Ok(())
    }

    /// Set the table page size from the settings input; an invalid value keeps
    /// the old one and shows the port's error.
    pub fn set_table_page_size(&mut self) {
        let rows = self.page_size_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.write_ui_setting(Setting::TablePageSize { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("table page size must be a number".into()),
        }
    }
    pub fn set_query_max_rows(&mut self) {
        let rows = self.max_rows_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.write_ui_setting(Setting::QueryMaxRows { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("query max rows must be a number".into()),
        }
    }

    /// Save the connection form through the port. A missing address is refused
    /// and nothing is saved. E5: the six fields travel as one draft — the
    /// live host persists it through its connection use case (the password
    /// lands sealed), the memory port keeps its semantics.
    /// Flip the theme. The only write goes to the settings in force.
    pub(crate) fn save_builtin_theme(&mut self, dark: bool) {
        self.settings_error = self.write_ui_setting(Setting::Theme { dark }).err();
    }

    pub fn toggle_theme(&mut self) {
        let dark = self
            .ui_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::Theme { dark } => Some(*dark),
                _ => None,
            })
            .unwrap_or(true);
        let _ = self.write_ui_setting(Setting::Theme { dark: !dark });
    }
}

// ── settingsSections.ts (T484): the backend-facing section sets ────────────

/// `DESKTOP_SETTINGS_SECTIONS` — the sections currently supplied
/// by native host services (the F5 wiring's own table; the page's
/// available set is `settings_sections_for_backend` over the
/// capabilities).
pub const DESKTOP_SETTINGS_SECTIONS: [&str; 7] = [
    "logging",
    "ai",
    "prompts",
    "mcpServer",
    "mcpClient",
    "tunnels",
    "extensions",
];

/// `ALL_SETTINGS_SECTIONS` — every section the page knows (the
/// desktop-shaped default until F5's table): the ids of
/// [`SETTINGS_SECTIONS`], in order.
pub fn all_settings_sections() -> Vec<&'static str> {
    SETTINGS_SECTIONS.iter().map(|section| section.id).collect()
}

/// `settingsSectionsForBackend(sections)`: the sections the
/// settings page may show, given the form's section set (F5's
/// `PlatformCapabilities.settingsSections` plugs in directly;
/// callers without capabilities pass the full set).
pub fn settings_sections_for_backend(sections: &[&str]) -> Vec<&'static SettingsSectionDef> {
    SETTINGS_SECTIONS
        .iter()
        .filter(|section| sections.contains(&section.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backend_sets_match_the_react_tables() {
        // The 12-section page in order.
        let all = all_settings_sections();
        assert_eq!(all.len(), 12);
        assert_eq!(all[0], "general");
        assert_eq!(all[11], "extensions");
        // The native-host seven.
        assert_eq!(
            DESKTOP_SETTINGS_SECTIONS,
            [
                "logging",
                "ai",
                "prompts",
                "mcpServer",
                "mcpClient",
                "tunnels",
                "extensions"
            ]
        );
        // The filter keeps the page's order, not the input's.
        let web = [
            "behavior",
            "general",
            "editor",
            "dataBrowsing",
            "appearance",
        ];
        let filtered = settings_sections_for_backend(&web);
        assert_eq!(
            filtered.iter().map(|s| s.id).collect::<Vec<_>>(),
            [
                "general",
                "appearance",
                "dataBrowsing",
                "editor",
                "behavior"
            ],
            "the page order rules"
        );
        // An empty set shows nothing.
        assert!(settings_sections_for_backend(&[]).is_empty());
        // The full set shows everything.
        assert_eq!(settings_sections_for_backend(&all).len(), 12);
        // Unknown ids drop (a web build's narrower set never
        // invents sections).
        assert_eq!(settings_sections_for_backend(&["nope", "general"]).len(), 1);
    }

    #[test]
    fn parse_settings_section_rules() {
        assert_eq!(parse_settings_section("tunnels"), "tunnels");
        assert_eq!(parse_settings_section("general"), "general");
        assert_eq!(parse_settings_section(""), "general", "the fallback");
        assert_eq!(parse_settings_section("nonsense"), "general");
    }
}
