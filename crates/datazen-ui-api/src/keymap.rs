use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeymapPreset {
    Default,
    Dbeaver,
    Navicat,
}

impl KeymapPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Dbeaver => "dbeaver",
            Self::Navicat => "navicat",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "dbeaver" => Some(Self::Dbeaver),
            "navicat" => Some(Self::Navicat),
            _ => None,
        }
    }
}

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

pub const KEYMAP_ACTIONS: [(KeymapAction, &str); 6] = [
    (KeymapAction::Execute, "keymap.action.execute"),
    (KeymapAction::ExecuteAll, "keymap.action.executeAll"),
    (KeymapAction::NewQuery, "keymap.action.newQuery"),
    (KeymapAction::CloseTab, "keymap.action.closeTab"),
    (KeymapAction::SaveQuery, "keymap.action.saveQuery"),
    (KeymapAction::FormatSql, "keymap.action.formatSql"),
];

pub const KEYMAP_PRESETS: [(&str, [(KeymapAction, &str); 6]); 3] = [
    (
        "default",
        [
            (KeymapAction::Execute, "Mod-Enter"),
            (KeymapAction::ExecuteAll, "Mod-Shift-Enter"),
            (KeymapAction::NewQuery, "Mod-n"),
            (KeymapAction::CloseTab, "Mod-w"),
            (KeymapAction::SaveQuery, "Mod-s"),
            (KeymapAction::FormatSql, "Mod-Shift-f"),
        ],
    ),
    (
        "dbeaver",
        [
            (KeymapAction::Execute, "Mod-Enter"),
            (KeymapAction::ExecuteAll, "Alt-x"),
            (KeymapAction::NewQuery, "Ctrl-]"),
            (KeymapAction::CloseTab, "Mod-w"),
            (KeymapAction::SaveQuery, "Mod-s"),
            (KeymapAction::FormatSql, "Ctrl-Shift-f"),
        ],
    ),
    (
        "navicat",
        [
            (KeymapAction::Execute, "Mod-r"),
            (KeymapAction::ExecuteAll, "Mod-Shift-r"),
            (KeymapAction::NewQuery, "Mod-q"),
            (KeymapAction::CloseTab, "Mod-w"),
            (KeymapAction::SaveQuery, "Mod-s"),
            (KeymapAction::FormatSql, "Mod-Shift-f"),
        ],
    ),
];

fn preset_bindings(preset: &str) -> &'static [(KeymapAction, &'static str); 6] {
    KEYMAP_PRESETS
        .iter()
        .find(|(name, _)| *name == preset)
        .map(|(_, bindings)| bindings)
        .unwrap_or(&KEYMAP_PRESETS[0].1)
}

fn binding_of(bindings: &[(KeymapAction, &'static str); 6], action: KeymapAction) -> &'static str {
    bindings
        .iter()
        .find(|(candidate, _)| *candidate == action)
        .map(|(_, shortcut)| *shortcut)
        .unwrap_or(KEYMAP_PRESETS[0].1[0].1)
}

pub fn get_action_shortcut(
    action: KeymapAction,
    preset: &str,
    custom: &BTreeMap<String, String>,
) -> String {
    match custom.get(action.as_str()) {
        Some(value) if !value.is_empty() => value.clone(),
        _ => {
            let bindings = preset_bindings(preset);
            binding_of(bindings, action).to_owned()
        }
    }
}

pub fn to_shortcut_hook_format(key: &str) -> String {
    key.split('-').map(|part| part.trim().to_lowercase()).collect::<Vec<_>>().join("+")
}

fn canonical_code_mirror(part: &str) -> String {
    let lower = part.trim().to_lowercase();
    match lower.as_str() {
        "mod" => "Mod".to_owned(),
        "ctrl" => "Ctrl".to_owned(),
        "alt" => "Alt".to_owned(),
        "shift" => "Shift".to_owned(),
        "enter" => "Enter".to_owned(),
        "space" => "Space".to_owned(),
        "escape" | "esc" => "Escape".to_owned(),
        _ if lower.len() > 1 && lower.starts_with('f') && lower[1..].bytes().all(|byte| byte.is_ascii_digit()) => lower.to_uppercase(),
        _ => lower,
    }
}

pub fn to_code_mirror_key_format(key: &str) -> String {
    let parts: Vec<&str> = if key.contains('+') { key.split('+').collect() } else { key.split('-').collect() };
    parts.iter().map(|part| canonical_code_mirror(part)).collect::<Vec<_>>().join("-")
}

pub fn format_mod_key(platform: &str) -> String {
    if platform == "macos" { "⌘".to_owned() } else { "Ctrl".to_owned() }
}

pub fn format_shortcut_for_display(key: &str, is_mac: bool) -> String {
    key.split(|character| character == '-' || character == '+')
        .map(|part| {
            let lower = part.trim().to_lowercase();
            match lower.as_str() {
                "mod" => if is_mac { "Cmd".to_owned() } else { "Ctrl".to_owned() },
                "ctrl" => "Ctrl".to_owned(),
                "alt" => if is_mac { "Option".to_owned() } else { "Alt".to_owned() },
                "shift" => "Shift".to_owned(),
                "enter" => "Enter".to_owned(),
                _ if lower.len() > 1 && lower.starts_with('f') && lower[1..].bytes().all(|byte| byte.is_ascii_digit()) => lower.to_uppercase(),
                _ => lower.to_uppercase(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn collapse_platform_words(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
                index += 1;
            }
            let word = &input[start..index];
            if word.eq_ignore_ascii_case("cmd") || word.eq_ignore_ascii_case("command") {
                out.push_str("Mod");
            } else if word.eq_ignore_ascii_case("option") {
                out.push_str("Alt");
            } else {
                out.push_str(word);
            }
        } else {
            let character = input[index..].chars().next().unwrap_or('\0');
            out.push(character);
            index += character.len_utf8();
        }
    }
    out
}

fn strip_space_around_signs(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    for (index, character) in chars.iter().enumerate() {
        if character.is_whitespace()
            && (chars.get(index + 1).is_some_and(|next| *next == '+' || *next == '-')
                || index > 0 && (chars[index - 1] == '+' || chars[index - 1] == '-'))
        {
            continue;
        }
        out.push(*character);
    }
    out
}

pub fn normalize_shortcut_input(key: &str) -> String {
    let collapsed = collapse_platform_words(key.trim());
    let joined = strip_space_around_signs(&collapsed).replace('+', "-");
    joined
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lower = part.trim().to_lowercase();
            match lower.as_str() {
                "mod" => "Mod".to_owned(),
                "ctrl" => "Ctrl".to_owned(),
                "alt" => "Alt".to_owned(),
                "shift" => "Shift".to_owned(),
                "enter" => "Enter".to_owned(),
                "space" => "Space".to_owned(),
                "escape" | "esc" => "Escape".to_owned(),
                _ if lower.len() > 1 && lower.starts_with('f') && lower[1..].bytes().all(|byte| byte.is_ascii_digit()) => lower.to_uppercase(),
                _ => part.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
    }

    #[test]
    fn action_and_preset_literals() {
        for (action, literal) in [
            (KeymapAction::Execute, "execute"),
            (KeymapAction::ExecuteAll, "executeAll"),
            (KeymapAction::NewQuery, "newQuery"),
            (KeymapAction::CloseTab, "closeTab"),
            (KeymapAction::SaveQuery, "saveQuery"),
            (KeymapAction::FormatSql, "formatSql"),
        ] {
            assert_eq!(action.as_str(), literal);
        }
        assert_eq!(KeymapPreset::Default.as_str(), "default");
        assert_eq!(KeymapPreset::parse("dbeaver"), Some(KeymapPreset::Dbeaver));
        assert_eq!(KeymapPreset::parse("unknown"), None);
    }

    #[test]
    fn the_catalog_and_all_three_presets_are_exact() {
        assert_eq!(KEYMAP_ACTIONS.len(), 6);
        assert_eq!(KEYMAP_ACTIONS[0], (KeymapAction::Execute, "keymap.action.execute"));
        assert_eq!(KEYMAP_PRESETS.len(), 3);
        assert_eq!(get_action_shortcut(KeymapAction::ExecuteAll, "default", &BTreeMap::new()), "Mod-Shift-Enter");
        assert_eq!(get_action_shortcut(KeymapAction::ExecuteAll, "dbeaver", &BTreeMap::new()), "Alt-x");
        assert_eq!(get_action_shortcut(KeymapAction::NewQuery, "dbeaver", &BTreeMap::new()), "Ctrl-]");
        assert_eq!(get_action_shortcut(KeymapAction::Execute, "navicat", &BTreeMap::new()), "Mod-r");
        assert_eq!(get_action_shortcut(KeymapAction::FormatSql, "navicat", &BTreeMap::new()), "Mod-Shift-f");
    }

    #[test]
    fn custom_shortcuts_win_and_empty_custom_falls_back() {
        let overrides = custom(&[("execute", "F5")]);
        assert_eq!(get_action_shortcut(KeymapAction::Execute, "navicat", &overrides), "F5");
        let empty = custom(&[("execute", "")]);
        assert_eq!(get_action_shortcut(KeymapAction::Execute, "navicat", &empty), "Mod-r");
    }

    #[test]
    fn unknown_preset_falls_back_to_default() {
        assert_eq!(get_action_shortcut(KeymapAction::Execute, "mystery", &BTreeMap::new()), "Mod-Enter");
    }

    #[test]
    fn hook_and_code_mirror_formats() {
        assert_eq!(to_shortcut_hook_format("Mod-Shift-F12"), "mod+shift+f12");
        assert_eq!(to_code_mirror_key_format("mod+enter"), "Mod-Enter");
        assert_eq!(to_code_mirror_key_format("Ctrl-Shift-F"), "Ctrl-Shift-f");
        assert_eq!(to_code_mirror_key_format("esc"), "Escape");
        assert_eq!(to_code_mirror_key_format("f2"), "F2");
        assert_eq!(to_code_mirror_key_format("space"), "Space");
    }

    #[test]
    fn mod_key_symbol_follows_the_platform() {
        assert_eq!(format_mod_key("macos"), "⌘");
        assert_eq!(format_mod_key("windows"), "Ctrl");
        assert_eq!(format_mod_key("unknown"), "Ctrl");
    }

    #[test]
    fn display_format_is_injected_with_the_platform() {
        assert_eq!(format_shortcut_for_display("Mod-Alt-Enter", true), "Cmd+Option+Enter");
        assert_eq!(format_shortcut_for_display("mod+alt+enter", false), "Ctrl+Alt+Enter");
        assert_eq!(format_shortcut_for_display("shift+F3", false), "Shift+F3");
    }

    #[test]
    fn shortcut_input_normalizes_platform_words_and_separators() {
        assert_eq!(normalize_shortcut_input(" Cmd + Shift - Enter "), "Mod-Shift-Enter");
        assert_eq!(normalize_shortcut_input("Cmd + Option + F2"), "Mod-Alt-F2");
        assert_eq!(normalize_shortcut_input("esc"), "Escape");
        assert_eq!(normalize_shortcut_input("custom-pAst"), "custom-pAst");
    }
}
