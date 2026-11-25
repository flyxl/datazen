//! Port of `packages/frontend/src/windows/connection/home/ShortcutFooter.tsx`.
//!
//! React reads the active keymap preset / custom keymap from the settings store
//! and shows only actions that resolve to a non-empty shortcut. Rune takes that
//! settings snapshot in [`ShortcutFooterVm`] and resolves the same presets and
//! display formatting here; no settings store or platform API is faked.
//!
//! React returns `null` when no shortcut resolves, so this builder returns
//! `Option<El>` — `None` mounts nothing at all.
//!
//! ## Keys
//!
//! Preserves the A1-era home keys `connections-shortcut-footer` and the React
//! marker `home-shortcut-footer`, plus one key per visible item
//! (`connections-shortcut-{key,save,format}`).
//!
//! Leftover L1 (removal stage): when the settings slice exposes the keymap
//! directly, feed the resolved strings from there and drop the preset/custom
//! mirrors plus the per-item allows.

use std::collections::BTreeMap;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::ui::kit::kit;

pub const FOOTER_KEY: &str = "connections-shortcut-footer";
pub const REACT_KEY: &str = "home-shortcut-footer";

/// React `KeymapAction`.
#[allow(dead_code)] // keymap mirror; consumer: the settings host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeymapAction {
    Execute,
    ExecuteAll,
    NewQuery,
    CloseTab,
    SaveQuery,
    FormatSql,
}

impl KeymapAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Execute => "execute",
            Self::ExecuteAll => "executeAll",
            Self::NewQuery => "newQuery",
            Self::CloseTab => "closeTab",
            Self::SaveQuery => "saveQuery",
            Self::FormatSql => "formatSql",
        }
    }
}

/// React `KeymapPreset`.
#[allow(dead_code)] // keymap mirror; consumer: the settings host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeymapPreset {
    #[default]
    Default,
    Dbeaver,
    Navicat,
}

impl KeymapPreset {
    pub fn from_value(value: &str) -> Self {
        match value {
            "dbeaver" => Self::Dbeaver,
            "navicat" => Self::Navicat,
            _ => Self::Default,
        }
    }
}

/// `KEYMAP_PRESETS` from React `lib/keymap.ts`.
fn preset_shortcut(preset: KeymapPreset, action: KeymapAction) -> &'static str {
    match (preset, action) {
        (KeymapPreset::Default, KeymapAction::Execute) => "Mod-Enter",
        (KeymapPreset::Default, KeymapAction::ExecuteAll) => "Mod-Shift-Enter",
        (KeymapPreset::Default, KeymapAction::NewQuery) => "Mod-n",
        (KeymapPreset::Default, KeymapAction::CloseTab) => "Mod-w",
        (KeymapPreset::Default, KeymapAction::SaveQuery) => "Mod-s",
        (KeymapPreset::Default, KeymapAction::FormatSql) => "Mod-Shift-f",
        (KeymapPreset::Dbeaver, KeymapAction::Execute) => "Mod-Enter",
        (KeymapPreset::Dbeaver, KeymapAction::ExecuteAll) => "Alt-x",
        (KeymapPreset::Dbeaver, KeymapAction::NewQuery) => "Ctrl-]",
        (KeymapPreset::Dbeaver, KeymapAction::CloseTab) => "Mod-w",
        (KeymapPreset::Dbeaver, KeymapAction::SaveQuery) => "Mod-s",
        (KeymapPreset::Dbeaver, KeymapAction::FormatSql) => "Ctrl-Shift-f",
        (KeymapPreset::Navicat, KeymapAction::Execute) => "Mod-r",
        (KeymapPreset::Navicat, KeymapAction::ExecuteAll) => "Mod-Shift-r",
        (KeymapPreset::Navicat, KeymapAction::NewQuery) => "Mod-q",
        (KeymapPreset::Navicat, KeymapAction::CloseTab) => "Mod-w",
        (KeymapPreset::Navicat, KeymapAction::SaveQuery) => "Mod-s",
        (KeymapPreset::Navicat, KeymapAction::FormatSql) => "Mod-Shift-f",
    }
}

/// React `getActionShortcut`: custom value wins, then preset, then default.
#[allow(dead_code)] // pure keymap rule; consumed by the footer builder.
pub fn get_action_shortcut(
    action: KeymapAction,
    preset: KeymapPreset,
    custom: &BTreeMap<String, String>,
) -> String {
    if let Some(value) = custom.get(action.as_str()) {
        if !value.is_empty() {
            return value.clone();
        }
    }
    preset_shortcut(preset, action).to_owned()
}

/// React `formatShortcutForDisplay`. The platform is supplied by the host
/// (React sniffs `navigator`); `is_mac = true` keeps the macOS `Cmd`/`Option`
/// copy, otherwise the Windows/Linux `Ctrl`/`Alt` copy is used.
#[allow(dead_code)] // pure keymap rule; consumed by the footer builder.
pub fn format_shortcut_for_display(key: &str, is_mac: bool) -> String {
    key.split(|c| c == '-' || c == '+')
        .map(|part| match part.trim().to_lowercase().as_str() {
            "mod" => if is_mac { "Cmd" } else { "Ctrl" }.to_owned(),
            "ctrl" => "Ctrl".to_owned(),
            "alt" => if is_mac { "Option" } else { "Alt" }.to_owned(),
            "shift" => "Shift".to_owned(),
            "enter" => "Enter".to_owned(),
            other => other.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Keymap snapshot from the settings store.
#[allow(dead_code)] // view VM; consumer: the home host (not wired).
#[derive(Clone, Debug, Default)]
pub struct ShortcutFooterVm {
    pub preset: KeymapPreset,
    /// Custom bindings keyed by `KeymapAction::as_str()`.
    pub custom: BTreeMap<String, String>,
}

/// The three candidates React's footer shows, in order.
const CANDIDATES: [(KeymapAction, &str, &str); 3] = [
    (KeymapAction::NewQuery, "keymap.action.newQuery", "connections-shortcut-key"),
    (KeymapAction::SaveQuery, "keymap.action.saveQuery", "connections-shortcut-save"),
    (KeymapAction::FormatSql, "keymap.action.formatSql", "connections-shortcut-format"),
];

#[allow(dead_code)] // view entry point; consumer: the landing page host (not wired).
pub fn shortcut_footer(vm: &ShortcutFooterVm, dark: bool, is_mac: bool) -> Option<El> {
    let k = kit(dark);
    let mut footer = row()
        .key(FOOTER_KEY)
        .child(row().key(REACT_KEY).a11y_hidden())
        .w_full()
        .gap(20.0)
        .justify_center()
        .center_y()
        .flex_wrap();

    let mut mounted = 0usize;
    for (action, label_key, item_key) in CANDIDATES {
        let shortcut = get_action_shortcut(action, vm.preset, &vm.custom);
        if shortcut.is_empty() {
            continue;
        }
        mounted += 1;
        footer = footer.child(
            row()
                .key(item_key)
                .h(22.0)
                .gap(6.0)
                .center_y()
                .child(
                    col()
                        .h(20.0)
                        .px(6.0)
                        .radius(5.0)
                        .fill(k.c.surface_base)
                        .border(1.0, k.c.border_base)
                        .center_y()
                        .child(
                            text(format_shortcut_for_display(&shortcut, is_mac))
                                .key(format!("{item_key}-kbd"))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_base),
                        ),
                )
                .child(
                    text(t(label_key))
                        .key(format!("{item_key}-label"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        );
    }
    (mounted > 0).then_some(footer)
}
