//! Port of the state half of
//! `packages/frontend/src/windows/settings/SettingsContent.tsx` (T390,
//! split — the view half is `settings_content.rs`).
//!
//! The settings page's own state: the active section (the nav's
//! selection plus the `initialSection` deep-link sync), the local
//! settings mirror (the `localSettings` optimistic copy of the store's
//! `AppSettings`), the two host reads the page kicks off on mount
//! (the app version and the default log path), and which of the
//! page's selects holds its listbox open.
//!
//! ## What maps where
//!
//! - `useState<SettingsSection>` + the `initialSection` effect are
//!   [`SettingsContentState::active_section`] with
//!   [`SettingsContentState::set_initial_section`]: a known id selects
//!   that section, an unknown (or absent) one falls back to `general`
//!   exactly as `parseSettingsSection` rules. The mount-time
//!   initializer and the prop-change effect are the same call.
//! - `settingsSectionsForBackend(capabilities?.settingsSections ??
//!   ALL_SETTINGS_SECTIONS)` is [`available_sections`]: the section
//!   table filtered by the caller's set — `None` capabilities keeps
//!   the desktop-shaped full set (the F4→F5 window's contract).
//! - `localSettings` (the `useState` mirror of the store) is
//!   [`SettingsMirror`], synced by
//!   [`SettingsContentState::sync_from_store`] (the
//!   `useEffect(setLocalSettings, [storeSettings])` line) and edited
//!   optimistically by [`SettingsContentState::apply_field`] (the
//!   `updateField` optimistic half; the store write is the host's
//!   `on_update_field` slot).
//! - `getAppVersion` / `getLogPath` (the two `.catch(() => {})`
//!   reads) are [`SettingsContentState::note_app_version`] /
//!   [`SettingsContentState::note_default_log_path`] — a rejected
//!   read simply never lands, the About line stays hidden and the
//!   placeholder falls back.
//! - `KEYMAP_ACTIONS` / `KEYMAP_PRESETS` / `getActionShortcut` /
//!   `normalizeShortcutInput` / `formatShortcutForDisplay`
//!   (`lib/keymap.ts`) are the pure seam below: the keymap card needs
//!   them today, `lib/keymap.rs` is segment 2's T541 — when that file
//!   lands it becomes the authority and these retire.
//! - The keymap input's `isMac` display branch (`Cmd` vs `Ctrl`) is
//!   the host's platform fact: [`format_shortcut_for_display`]
//!   takes it as a parameter.

use rune::TextEdit;

use crate::platform::capabilities::PlatformCapabilities;
use crate::settings_state::{SettingsSectionDef, SETTINGS_SECTIONS};

/// `BUILTIN_LOCALES` / `BUILTIN_LOCALE_LABELS` — the two locales the
/// app ships (`packages/frontend/src/locales/builtin-locales.json`,
/// the file `builtinLocales.ts` is generated from).
pub const BUILTIN_LOCALE_OPTIONS: [(&str, &str); 2] = [("en", "English"), ("zh-CN", "简体中文")];

/// `PAGE_SIZE_OPTIONS` — the default page size select's values.
pub const PAGE_SIZE_OPTIONS: [u32; 5] = [25, 50, 100, 200, 500];
/// `RESULT_LIMIT_OPTIONS` — the query result limit select's values.
pub const RESULT_LIMIT_OPTIONS: [u32; 5] = [1000, 2000, 5000, 10000, 50000];
/// `LOG_LEVEL_OPTIONS` — the values of the log level select (the
/// labels are the capitalized spellings, never translated).
pub const LOG_LEVEL_OPTIONS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];
/// `KEYMAP_PRESET_OPTIONS` — `(value, labelKey)` pairs.
pub const KEYMAP_PRESET_OPTIONS: [(&str, &str); 3] = [
    ("default", "settings.keymap.preset.default"),
    ("dbeaver", "settings.keymap.preset.dbeaver"),
    ("navicat", "settings.keymap.preset.navicat"),
];

/// `KEYMAP_ACTIONS` — `(id, labelKey)`, in the React file's order.
pub const KEYMAP_ACTIONS: [(&str, &str); 6] = [
    ("execute", "keymap.action.execute"),
    ("executeAll", "keymap.action.executeAll"),
    ("newQuery", "keymap.action.newQuery"),
    ("closeTab", "keymap.action.closeTab"),
    ("saveQuery", "keymap.action.saveQuery"),
    ("formatSql", "keymap.action.formatSql"),
];

/// `KEYMAP_PRESETS` — `[preset][action] -> shortcut`, the three
/// tables' rows in `KEYMAP_ACTIONS` order.
pub const KEYMAP_PRESET_SHORTCUTS: [(&str, [&str; 6]); 3] = [
    (
        "default",
        ["Mod-Enter", "Mod-Shift-Enter", "Mod-n", "Mod-w", "Mod-s", "Mod-Shift-f"],
    ),
    (
        "dbeaver",
        ["Mod-Enter", "Alt-x", "Ctrl-]", "Mod-w", "Mod-s", "Ctrl-Shift-f"],
    ),
    (
        "navicat",
        ["Mod-r", "Mod-Shift-r", "Mod-q", "Mod-w", "Mod-s", "Mod-Shift-f"],
    ),
];

/// The default preset's shortcut for one action (the fallback arm of
/// `getActionShortcut` when even the preset row is missing).
fn preset_shortcut(preset: &str, action: &str) -> Option<&'static str> {
    let (_, rows) = KEYMAP_PRESET_SHORTCUTS
        .iter()
        .find(|(name, _)| *name == preset)
        .unwrap_or(&KEYMAP_PRESET_SHORTCUTS[0]);
    KEYMAP_ACTIONS
        .iter()
        .position(|(id, _)| *id == action)
        .map(|idx| rows[idx])
}

/// `getActionShortcut(action, preset, custom)`: the custom map wins,
/// then the preset's row, then the default preset's row.
pub fn get_action_shortcut(
    action: &str,
    preset: &str,
    custom: &[(String, String)],
) -> String {
    if let Some((_, value)) = custom.iter().find(|(id, _)| id.as_str() == action) {
        if !value.is_empty() {
            return value.clone();
        }
    }
    preset_shortcut(preset, action)
        .or_else(|| preset_shortcut("default", action))
        .unwrap_or("")
        .to_owned()
}

/// `formatShortcutForDisplay(key)` — `Mod` renders `Cmd` on macOS and
/// `Ctrl` elsewhere, `Alt` renders `Option` on macOS; parts join with
/// `+` and F-keys uppercase.
pub fn format_shortcut_for_display(key: &str, is_mac: bool) -> String {
    key.split(['-', '+'])
        .map(|part| {
            let lower = part.trim().to_ascii_lowercase();
            match lower.as_str() {
                "mod" if is_mac => "Cmd".to_owned(),
                "mod" => "Ctrl".to_owned(),
                "ctrl" => "Ctrl".to_owned(),
                "alt" if is_mac => "Option".to_owned(),
                "alt" => "Alt".to_owned(),
                "shift" => "Shift".to_owned(),
                "enter" => "Enter".to_owned(),
                other => other.to_uppercase(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// `normalizeShortcutInput(key)` — trim, whole-word `cmd`/`command` →
/// `Mod` and `option` → `Alt` (case-insensitive), whitespace around
/// `+`/`-` separators dropped, `+` → `-`, empty parts dropped, then
/// canonical casing per part (anything else keeps its spelling —
/// only F-keys uppercase).
pub fn normalize_shortcut_input(key: &str) -> String {
    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    // \b(cmd|command)\b gi -> Mod ; \b(option)\b gi -> Alt
    fn replace_platform_words(s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < chars.len() {
            if is_word_char(chars[i]) {
                let mut j = i;
                while j < chars.len() && is_word_char(chars[j]) {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect();
                match word.to_ascii_lowercase().as_str() {
                    "cmd" | "command" => out.push_str("Mod"),
                    "option" => out.push_str("Alt"),
                    _ => out.push_str(&word),
                }
                i = j;
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    // \s*([+-])\s* -> $1 (whitespace survives only when no separator
    // follows it).
    fn collapse_separator_spaces(s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_whitespace() {
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    out.push(chars[j]);
                    let mut k = j + 1;
                    while k < chars.len() && chars[k].is_whitespace() {
                        k += 1;
                    }
                    i = k;
                } else {
                    out.extend(&chars[i..j]);
                    i = j;
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        out
    }

    let trimmed = key.trim();
    let collapsed = collapse_separator_spaces(&replace_platform_words(trimmed)).replace('+', "-");
    collapsed
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            match lower.as_str() {
                "mod" => "Mod".to_owned(),
                "ctrl" => "Ctrl".to_owned(),
                "alt" => "Alt".to_owned(),
                "shift" => "Shift".to_owned(),
                "enter" => "Enter".to_owned(),
                "space" => "Space".to_owned(),
                "escape" | "esc" => "Escape".to_owned(),
                _ => {
                    // /^f\d+$/i -> uppercase; anything else keeps its
                    // spelling (the React `return part` arm).
                    if lower.len() >= 2
                        && lower.starts_with('f')
                        && lower[1..].chars().all(|c| c.is_ascii_digit())
                    {
                        lower.to_uppercase()
                    } else {
                        part.to_owned()
                    }
                }
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// Which of the page's selects holds its listbox open (the kit select
/// takes the open flag from state; one at a time, the section
/// convention).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSelect {
    /// The general section's language select
    /// (`settings-language-select`).
    Language,
    /// The data-browsing page size select.
    PageSize,
    /// Native custom page-size numeric popover, disclosed by Page Size.
    CustomPageSize,
    /// The data-browsing query result limit select.
    ResultLimit,
    /// The native data-browsing row count select.
    RowCountMode,
    /// The native data-browsing default table sort select.
    DefaultTableSort,
    /// The data-browsing workflow step order select.
    WorkflowOrder,
    /// The native editor mono-family select.
    EditorFontFamily,
    /// The editor SQL syntax theme select.
    SyntaxTheme,
    /// The editor completion quote policy select.
    QuotePolicy,
    /// The editor keymap preset select.
    KeymapPreset,
    /// The logging level select.
    LogLevel,
}

/// `updateField(key, value)` — one edit of the settings draft. The
/// shell's controls emit these; the host applies the optimistic
/// mirror ([`SettingsContentState::apply_field`]) and persists.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsField {
    Language(String),
    DefaultPageSize(u32),
    ConnectionPoolSize(u32),
    LimitSelectResults(bool),
    QueryResultLimit(u32),
    AutoChartOnQuery(bool),
    /// `true` = `asc` (the select's two arms).
    WorkflowStepResultOrderAsc(bool),
    EditorFontSize(f64),
    EditorFontFamily(String),
    SqlSyntaxTheme(String),
    CompletionQuotePolicy(String),
    CompletionIncludeTablePrefix(bool),
    EnableFkPrediction(bool),
    KeymapPreset(String),
    /// The whole custom map (the reset emits the empty one).
    CustomKeymap(Vec<(String, String)>),
    /// One keymap input's committed edit: the raw typed text —
    /// [`SettingsContentState::apply_field`] normalizes it and merges
    /// it into the custom map (the React `newCustom` spread).
    CustomKeymapEdit {
        action: &'static str,
        input: String,
    },
    ConfirmOnDelete(bool),
    AutoCommit(bool),
    SafeMode(bool),
    ConfirmDangerousExecution(bool),
    LogLevel(String),
    LogPath(String),
    /// `updateExtensionSetting(extensionId, key, value)` — the
    /// contribution write, the value the item's control produced.
    ExtensionSetting {
        extension_id: String,
        key: String,
        value: String,
    },
}

/// The `localSettings` mirror — the `AppSettings` fields the page
/// itself renders. Defaults are `DEFAULT_SETTINGS` (the store's).
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsMirror {
    pub language: String,
    pub default_page_size: u32,
    pub connection_pool_size: u32,
    pub limit_select_results: bool,
    pub query_result_limit: u32,
    pub auto_chart_on_query: bool,
    /// `None` = `desc` is not the value (the select's fallback arm).
    pub workflow_step_result_order_asc: bool,
    pub editor_font_size: f64,
    pub editor_font_family: String,
    /// The raw preset id (the select's fallback is `default`).
    pub sql_syntax_theme: String,
    /// The raw policy (fallback `unquoted`).
    pub editor_completion_quote_policy: String,
    pub editor_completion_include_table_prefix: bool,
    pub enable_fk_prediction: bool,
    /// The raw preset (fallback `default`).
    pub keymap_preset: String,
    pub custom_keymap: Vec<(String, String)>,
    pub confirm_on_delete: bool,
    pub auto_commit: bool,
    pub safe_mode: bool,
    pub confirm_dangerous_execution: bool,
    pub log_level: String,
    pub log_path: String,
    /// `driverSettings[extensionId][key]` — the contribution values.
    pub driver_settings: Vec<(String, Vec<(String, String)>)>,
}

impl Default for SettingsMirror {
    fn default() -> Self {
        Self {
            language: "en".to_owned(),
            default_page_size: 50,
            connection_pool_size: 10,
            limit_select_results: false,
            query_result_limit: 5000,
            auto_chart_on_query: false,
            workflow_step_result_order_asc: false,
            editor_font_size: 13.0,
            editor_font_family: String::new(),
            sql_syntax_theme: "default".to_owned(),
            editor_completion_quote_policy: "unquoted".to_owned(),
            editor_completion_include_table_prefix: true,
            enable_fk_prediction: false,
            keymap_preset: "default".to_owned(),
            custom_keymap: Vec::new(),
            confirm_on_delete: true,
            auto_commit: true,
            safe_mode: true,
            confirm_dangerous_execution: true,
            log_level: "info".to_owned(),
            log_path: String::new(),
            driver_settings: Vec::new(),
        }
    }
}

impl SettingsMirror {
    /// `driverSettings[extensionId]` — the extension's current values
    /// (an absent extension answers the empty map).
    pub fn extension_values(&self, extension_id: &str) -> &[(String, String)] {
        self.driver_settings
            .iter()
            .find(|(id, _)| id == extension_id)
            .map(|(_, values)| values.as_slice())
            .unwrap_or(&[])
    }

    /// `updateExtensionSetting`'s optimistic half: the next
    /// `driverSettings` with `extensionId`'s `key` set to `value`.
    pub fn with_extension_setting(
        &self,
        extension_id: &str,
        key: &str,
        value: &str,
    ) -> Self {
        let mut next = self.clone();
        match next
            .driver_settings
            .iter_mut()
            .find(|(id, _)| id == extension_id)
        {
            Some((_, values)) => match values.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value.to_owned(),
                None => values.push((key.to_owned(), value.to_owned())),
            },
            None => next.driver_settings.push((
                extension_id.to_owned(),
                vec![(key.to_owned(), value.to_owned())],
            )),
        }
        next
    }
}

/// One contribution text/number input's `TextEdit`, keyed by its
/// `(extensionId, item key)`.
#[derive(Clone, PartialEq)]
pub struct ContributionEdit {
    pub extension_id: String,
    pub key: String,
    pub edit: TextEdit,
}

/// The settings page's state.
#[derive(Clone, PartialEq)]
pub struct SettingsContentState {
    /// The selected section id (normalized; unknown ids never land).
    pub active_section: &'static str,
    /// The nav button that armed Space, pending its key release.
    pub(crate) nav_space_target: Option<&'static str>,
    /// The `initialSection` prop's last seen value (the deep-link
    /// sync only fires when the prop actually changes).
    pub initial_section: Option<String>,
    /// The app version read (a rejected read never lands: the About
    /// line stays hidden).
    pub app_version: Option<String>,
    /// `getLogPath`'s answer — the log path input's placeholder.
    pub default_log_path: Option<String>,
    /// The local mirror of the store's settings.
    pub mirror: SettingsMirror,
    /// Which select holds its listbox open.
    pub open_select: Option<SettingsSelect>,
    /// Validation/write error scoped to the disclosed custom page-size draft.
    pub(crate) custom_page_size_error: Option<String>,
    /// Which contribution select is open (the `settings-contrib-*`
    /// selects are many; the id disambiguates — the page's own
    /// selects are the [`SettingsSelect`] enum).
    pub open_contribution_select: Option<String>,
    /// The connection pool size input (`value={connectionPoolSize ?? 10}`
    /// — controlled; [`sync_edits_from_mirror`] reseeds it).
    pub pool_size_edit: TextEdit,
    /// The editor font family input.
    pub font_family_edit: TextEdit,
    /// The log path input.
    pub log_path_edit: TextEdit,
    /// The keymap customize inputs, seeded one per `KEYMAP_ACTIONS`
    /// entry (the controlled `formatShortcutForDisplay(currentVal)`
    /// values — [`sync_edits_from_mirror`] reseeds them).
    pub keymap_edits: Vec<(&'static str, TextEdit)>,
    /// The contribution items' text/number inputs, keyed
    /// `(extensionId, key)`.
    pub contribution_edits: Vec<ContributionEdit>,
    /// The lens fallback for a control that is not rendering (never
    /// read; the closed-form arm convention).
    pub scratch_edit: TextEdit,
}

impl Default for SettingsContentState {
    /// The canonical constructor: `general` active, every input empty,
    /// the keymap inputs seeded one per action.
    fn default() -> Self {
        Self {
            active_section: "general",
            nav_space_target: None,
            initial_section: None,
            app_version: None,
            default_log_path: None,
            mirror: SettingsMirror::default(),
            open_select: None,
            custom_page_size_error: None,
            open_contribution_select: None,
            pool_size_edit: TextEdit::default(),
            font_family_edit: TextEdit::default(),
            log_path_edit: TextEdit::default(),
            keymap_edits: KEYMAP_ACTIONS
                .iter()
                .map(|(id, _)| (*id, TextEdit::default()))
                .collect(),
            contribution_edits: Vec::new(),
            scratch_edit: TextEdit::default(),
        }
    }
}

impl SettingsContentState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `parseSettingsSection` + the two `setActiveSection` call sites:
    /// a known id selects it, anything else falls back to `general`.
    pub fn set_initial_section(&mut self, initial: Option<&str>) {
        self.nav_space_target = None;
        self.initial_section = initial.map(str::to_owned);
        self.active_section = parse_settings_section(initial.unwrap_or(""));
    }

    /// The nav's own press (no `initialSection` involvement).
    pub fn select_section(&mut self, section: &'static str) {
        self.nav_space_target = None;
        self.active_section = parse_settings_section(section);
    }

    /// The store→mirror sync effect
    /// (`useEffect(() => setLocalSettings(storeSettings), [storeSettings])`).
    pub fn sync_from_store(&mut self, mirror: SettingsMirror) {
        self.mirror = mirror;
    }

    /// `getAppVersion().then(setAppVersion)`.
    pub fn note_app_version(&mut self, version: &str) {
        self.app_version = Some(version.to_owned());
    }

    /// `settingsCommands.getLogPath().then(setDefaultLogPath)`.
    pub fn note_default_log_path(&mut self, path: &str) {
        self.default_log_path = Some(path.to_owned());
    }

    /// `updateField`'s optimistic half — apply one field to the
    /// mirror (the store write is the host's). Returns `false` when
    /// the field is not the page's own (the host still persists it).
    pub fn apply_field(&mut self, field: &SettingsField) -> bool {
        let mirror = &mut self.mirror;
        match field {
            SettingsField::Language(value) => mirror.language = value.clone(),
            SettingsField::DefaultPageSize(value) => mirror.default_page_size = *value,
            SettingsField::ConnectionPoolSize(value) => {
                // The number input clamps to 1..=100 and rounds.
                mirror.connection_pool_size = (*value).clamp(1, 100);
            }
            SettingsField::LimitSelectResults(value) => mirror.limit_select_results = *value,
            SettingsField::QueryResultLimit(value) => mirror.query_result_limit = *value,
            SettingsField::AutoChartOnQuery(value) => mirror.auto_chart_on_query = *value,
            SettingsField::WorkflowStepResultOrderAsc(value) => {
                mirror.workflow_step_result_order_asc = *value;
            }
            SettingsField::EditorFontSize(value) => mirror.editor_font_size = *value,
            SettingsField::EditorFontFamily(value) => mirror.editor_font_family = value.clone(),
            SettingsField::SqlSyntaxTheme(value) => mirror.sql_syntax_theme = value.clone(),
            SettingsField::CompletionQuotePolicy(value) => {
                mirror.editor_completion_quote_policy = value.clone();
            }
            SettingsField::CompletionIncludeTablePrefix(value) => {
                mirror.editor_completion_include_table_prefix = *value;
            }
            SettingsField::EnableFkPrediction(value) => mirror.enable_fk_prediction = *value,
            SettingsField::KeymapPreset(value) => mirror.keymap_preset = value.clone(),
            SettingsField::CustomKeymap(value) => mirror.custom_keymap = value.clone(),
            SettingsField::CustomKeymapEdit { action, input } => {
                // `{ ...customKeymap, [action.id]: normalizeShortcutInput(v) }`
                // — the key is always set, even to the empty string
                // (an empty value just never wins the lookup).
                let normalized = normalize_shortcut_input(input);
                match mirror
                    .custom_keymap
                    .iter_mut()
                    .find(|(id, _)| id.as_str() == *action)
                {
                    Some(slot) => slot.1 = normalized,
                    None => mirror
                        .custom_keymap
                        .push(((*action).to_owned(), normalized)),
                }
            }
            SettingsField::ConfirmOnDelete(value) => mirror.confirm_on_delete = *value,
            SettingsField::AutoCommit(value) => mirror.auto_commit = *value,
            SettingsField::SafeMode(value) => mirror.safe_mode = *value,
            SettingsField::ConfirmDangerousExecution(value) => {
                mirror.confirm_dangerous_execution = *value;
            }
            SettingsField::LogLevel(value) => mirror.log_level = value.clone(),
            SettingsField::LogPath(value) => mirror.log_path = value.clone(),
            SettingsField::ExtensionSetting {
                extension_id,
                key,
                value,
            } => {
                mirror.driver_settings = mirror
                    .with_extension_setting(extension_id, key, value)
                    .driver_settings;
            }
        }
        true
    }

    /// The kit select's toggle (`on_toggle` / `on_close` pair).
    pub fn toggle_select(&mut self, which: SettingsSelect) {
        self.open_select = if self.open_select == Some(which) {
            None
        } else {
            Some(which)
        };
    }

    pub fn close_select(&mut self) {
        self.custom_page_size_error = None;
        self.open_select = None;
        self.open_contribution_select = None;
    }

    /// The keymap input for one action (seeded per `KEYMAP_ACTIONS`;
    /// an unknown action answers the scratch edit — it never renders).
    pub fn keymap_edit(&self, action: &str) -> &TextEdit {
        self.keymap_edits
            .iter()
            .find(|(id, _)| *id == action)
            .map(|(_, edit)| edit)
            .unwrap_or(&self.scratch_edit)
    }

    pub fn keymap_edit_mut(&mut self, action: &str) -> &mut TextEdit {
        match self
            .keymap_edits
            .iter_mut()
            .find(|(id, _)| *id == action)
        {
            Some((_, edit)) => edit,
            None => &mut self.scratch_edit,
        }
    }

    /// Seed the contribution input for `(extensionId, key)` if it is
    /// absent (the host calls this while mounting contributions).
    pub fn note_contribution(&mut self, extension_id: &str, key: &str) {
        if !self
            .contribution_edits
            .iter()
            .any(|edit| edit.extension_id == extension_id && edit.key == key)
        {
            self.contribution_edits.push(ContributionEdit {
                extension_id: extension_id.to_owned(),
                key: key.to_owned(),
                edit: TextEdit::default(),
            });
        }
    }

    pub fn contribution_edit(&self, extension_id: &str, key: &str) -> &TextEdit {
        self.contribution_edits
            .iter()
            .find(|edit| edit.extension_id == extension_id && edit.key == key)
            .map(|edit| &edit.edit)
            .unwrap_or(&self.scratch_edit)
    }

    pub fn contribution_edit_mut(
        &mut self,
        extension_id: &str,
        key: &str,
    ) -> &mut TextEdit {
        match self
            .contribution_edits
            .iter_mut()
            .find(|edit| edit.extension_id == extension_id && edit.key == key)
        {
            Some(edit) => &mut edit.edit,
            None => &mut self.scratch_edit,
        }
    }

    /// The controlled inputs' reseed: the pool input shows the
    /// clamped number, the font/log inputs their strings, and each
    /// keymap input `formatShortcutForDisplay(currentVal)` (the
    /// React controlled `value=` of each input).
    pub fn sync_edits_from_mirror(&mut self, is_mac: bool) {
        self.pool_size_edit = TextEdit::new(self.mirror.connection_pool_size.to_string());
        self.font_family_edit = TextEdit::new(self.mirror.editor_font_family.clone());
        self.log_path_edit = TextEdit::new(self.mirror.log_path.clone());
        for (action, edit) in self.keymap_edits.iter_mut() {
            let current = get_action_shortcut(action, &self.mirror.keymap_preset, &self.mirror.custom_keymap);
            *edit = TextEdit::new(format_shortcut_for_display(&current, is_mac));
        }
    }

    /// `Number(v.toLocaleString())` — the result-limit option values
    /// render with the en-US grouping commas (the label-only format).
    pub fn formatted_rows(value: u32) -> String {
        let text = value.to_string();
        let mut grouped = String::new();
        for (i, ch) in text.chars().enumerate() {
            if i > 0 && (text.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        grouped
    }
}

/// `parseSettingsSection(value)`: a listed id passes through,
/// anything else (null, undefined, unknown) is `general`.
pub fn parse_settings_section(value: &str) -> &'static str {
    SETTINGS_SECTIONS
        .iter()
        .find(|section| section.id == value)
        .map(|section| section.id)
        .unwrap_or("general")
}

/// `settingsSectionsForBackend(sections)` — the table filtered by the
/// caller's set. `None` (no capabilities yet) keeps the full
/// desktop-shaped set.
pub fn available_sections(
    capabilities: Option<&PlatformCapabilities>,
) -> Vec<&'static SettingsSectionDef> {
    match capabilities {
        None => SETTINGS_SECTIONS.iter().collect(),
        Some(caps) => SETTINGS_SECTIONS
            .iter()
            .filter(|section| caps.settings_sections.contains(&section.id))
            .collect(),
    }
}

/// `capabilities?.updater ?? true` — the UpdateSection gate.
pub fn updater_enabled(capabilities: Option<&PlatformCapabilities>) -> bool {
    capabilities.map(|caps| caps.updater).unwrap_or(true)
}

/// The version-read gate: `build-constant` forms skip the read (the
/// About line never renders); `None` capabilities read (the
/// desktop-shaped window).
pub fn version_read_enabled(capabilities: Option<&PlatformCapabilities>) -> bool {
    capabilities
        .map(|caps| caps.version_source != crate::platform::capabilities::VersionSource::BuildConstant)
        .unwrap_or(true)
}

/// The language dropdown's options: the built-ins, then the installed
/// packs, then the extension locales — every face deduped by value
/// (the `seen` set; the first spelling wins).
pub fn language_options(
    builtin: &[(&str, &str)],
    installed_packs: &[(String, String)],
    extension_locales: &[(String, String)],
) -> Vec<(String, String)> {
    let mut seen: Vec<String> = Vec::new();
    let mut options: Vec<(String, String)> = Vec::new();
    for (code, label) in builtin {
        if seen.iter().any(|c| c == code) {
            continue;
        }
        seen.push((*code).to_owned());
        options.push(((*code).to_owned(), (*label).to_owned()));
    }
    for (code, label) in installed_packs.iter().chain(extension_locales.iter()) {
        if seen.contains(code) {
            continue;
        }
        seen.push(code.clone());
        options.push((code.clone(), label.clone()));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_initial_sections_fall_back_to_general() {
        assert_eq!(parse_settings_section("appearance"), "appearance");
        assert_eq!(parse_settings_section("nope"), "general");
        assert_eq!(parse_settings_section(""), "general");
        let mut state = SettingsContentState::new();
        state.set_initial_section(Some("mcpClient"));
        assert_eq!(state.active_section, "mcpClient");
        state.set_initial_section(Some("nope"));
        assert_eq!(state.active_section, "general");
        state.set_initial_section(None);
        assert_eq!(state.active_section, "general");
    }

    #[test]
    fn available_sections_follow_the_capabilities_set() {
        assert_eq!(available_sections(None).len(), 12);
        let web = crate::platform::capabilities::WEB_CAPABILITIES;
        let ids: Vec<&str> = available_sections(Some(&web))
            .into_iter()
            .map(|section| section.id)
            .collect();
        assert_eq!(ids, ["general", "appearance", "dataBrowsing", "editor", "behavior"]);
        assert!(!updater_enabled(Some(&web)));
        assert!(updater_enabled(None));
        assert!(updater_enabled(Some(
            &crate::platform::capabilities::DESKTOP_CAPABILITIES
        )));
        assert!(!version_read_enabled(Some(&web)));
        assert!(version_read_enabled(None));
    }

    #[test]
    fn the_language_options_dedup_by_value() {
        let packs = vec![("zh-CN".to_owned(), "重复包".to_owned()), ("ja".to_owned(), "日本語".to_owned())];
        let extensions = vec![("ja".to_owned(), "重複拡張".to_owned()), ("ko".to_owned(), "한국어".to_owned())];
        let options =
            language_options(&BUILTIN_LOCALE_OPTIONS, &packs, &extensions);
        assert_eq!(
            options,
            vec![
                ("en".to_owned(), "English".to_owned()),
                ("zh-CN".to_owned(), "简体中文".to_owned()),
                ("ja".to_owned(), "日本語".to_owned()),
                ("ko".to_owned(), "한국어".to_owned()),
            ]
        );
    }

    #[test]
    fn keymap_shortcuts_resolve_custom_first() {
        assert_eq!(
            get_action_shortcut("execute", "default", &[]),
            "Mod-Enter"
        );
        assert_eq!(get_action_shortcut("executeAll", "dbeaver", &[]), "Alt-x");
        assert_eq!(get_action_shortcut("newQuery", "navicat", &[]), "Mod-q");
        assert_eq!(
            get_action_shortcut("newQuery", "navicat", &[("newQuery".to_owned(), "Ctrl-t".to_owned())]),
            "Ctrl-t"
        );
        // An empty custom value does not shadow (React's `custom[action]`
        // truthiness check).
        assert_eq!(
            get_action_shortcut(
                "newQuery",
                "navicat",
                &[("newQuery".to_owned(), String::new())]
            ),
            "Mod-q"
        );
        // An unknown preset falls back to the default table.
        assert_eq!(get_action_shortcut("closeTab", "nope", &[]), "Mod-w");
    }

    #[test]
    fn shortcut_display_and_normalize_round_trip() {
        assert_eq!(format_shortcut_for_display("Mod-Enter", false), "Ctrl+Enter");
        assert_eq!(format_shortcut_for_display("Mod-Enter", true), "Cmd+Enter");
        assert_eq!(format_shortcut_for_display("Alt-x", true), "Option+X");
        assert_eq!(format_shortcut_for_display("Alt-x", false), "Alt+X");
        assert_eq!(format_shortcut_for_display("Mod-Shift-f", false), "Ctrl+Shift+F");
        assert_eq!(normalize_shortcut_input("  cmd + enter "), "Mod-Enter");
        assert_eq!(normalize_shortcut_input("option - x"), "Alt-x");
        assert_eq!(normalize_shortcut_input("ctrl-shift-f"), "Ctrl-Shift-f");
        assert_eq!(normalize_shortcut_input(" mod - shift - f "), "Mod-Shift-f");
        assert_eq!(normalize_shortcut_input("Cmd-Enter"), "Mod-Enter");
        assert_eq!(normalize_shortcut_input("COMMAND-q"), "Mod-q");
        assert_eq!(normalize_shortcut_input("f3"), "F3");
        assert_eq!(normalize_shortcut_input("ctrl-f3"), "Ctrl-F3");
        // Non-canonical parts keep their spelling (the React
        // `return part` arm).
        assert_eq!(normalize_shortcut_input("Ctrl-q"), "Ctrl-q");
    }

    #[test]
    fn apply_field_updates_the_mirror() {
        let mut state = SettingsContentState::new();
        assert!(state.apply_field(&SettingsField::DefaultPageSize(200)));
        assert_eq!(state.mirror.default_page_size, 200);
        // The pool input clamps to the 1..=100 range.
        state.apply_field(&SettingsField::ConnectionPoolSize(999));
        assert_eq!(state.mirror.connection_pool_size, 100);
        state.apply_field(&SettingsField::ConnectionPoolSize(0));
        assert_eq!(state.mirror.connection_pool_size, 1);
        state.apply_field(&SettingsField::ExtensionSetting {
            extension_id: "ext-a".to_owned(),
            key: "k".to_owned(),
            value: "v".to_owned(),
        });
        assert_eq!(
            state.mirror.extension_values("ext-a"),
            &[("k".to_owned(), "v".to_owned())]
        );
        state.apply_field(&SettingsField::ExtensionSetting {
            extension_id: "ext-a".to_owned(),
            key: "k2".to_owned(),
            value: "v2".to_owned(),
        });
        assert_eq!(state.mirror.extension_values("ext-a").len(), 2);
    }

    #[test]
    fn result_limits_render_grouped() {
        assert_eq!(SettingsContentState::formatted_rows(1000), "1,000");
        assert_eq!(SettingsContentState::formatted_rows(50000), "50,000");
        assert_eq!(SettingsContentState::formatted_rows(500), "500");
    }
}
