use std::cell::RefCell;

use serde_json::Value;

pub const CODEMIRROR_COLOR_GAP: [&str; 5] = [
    "getComputedStyle",
    "document.documentElement",
    "HighlightStyle.define",
    "syntaxHighlighting",
    "ViewPlugin.fromClass",
];

pub const CM_VARS: [(&str, &str); 10] = [
    ("keyword", "--cm-keyword"),
    ("string", "--cm-string"),
    ("number", "--cm-number"),
    ("comment", "--cm-comment"),
    ("operator", "--cm-operator"),
    ("punctuation", "--cm-punctuation"),
    ("foreground", "--cm-foreground"),
    ("background", "--cm-background"),
    ("selection", "--cm-selection"),
    ("cursor", "--cm-cursor"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorColorContract {
    pub keyword: String,
    pub string: String,
    pub number: String,
    pub comment: String,
    pub operator: String,
    pub punctuation: String,
    pub foreground: String,
    pub background: String,
    pub selection: String,
    pub cursor: String,
}

pub fn default_editor_colors() -> EditorColorContract {
    EditorColorContract {
        keyword: "#93c5fd".to_owned(),
        string: "#6ee7b7".to_owned(),
        number: "#fbbf24".to_owned(),
        comment: "#5f6879".to_owned(),
        operator: "#67e8f9".to_owned(),
        punctuation: "#8b94a7".to_owned(),
        foreground: "#e6eaf2".to_owned(),
        background: "#0b0e14".to_owned(),
        selection: "rgba(79, 195, 247, 0.25)".to_owned(),
        cursor: "#e6eaf2".to_owned(),
    }
}

thread_local! {
    static PACK_OVERLAY: RefCell<Option<serde_json::Map<String, Value>>> = const { RefCell::new(None) };
}

pub fn set_pack_editor_color_overlay(overlay: Option<serde_json::Map<String, Value>>) {
    PACK_OVERLAY.with(|slot| *slot.borrow_mut() = overlay);
}

fn pack_overlay() -> Option<serde_json::Map<String, Value>> {
    PACK_OVERLAY.with(|slot| slot.borrow().clone())
}

fn is_color_key(key: &str) -> bool {
    CM_VARS.iter().any(|(name, _)| *name == key)
}

pub fn parse_pack_editor_overlay(json: &Value) -> Option<serde_json::Map<String, Value>> {
    let object = json.as_object()?;
    let mut overlay = serde_json::Map::new();
    for (key, value) in object {
        if is_color_key(key) {
            if let Some(text) = value.as_str() {
                if !text.trim().is_empty() {
                    overlay.insert(key.clone(), Value::String(text.trim().to_owned()));
                }
            }
        }
    }
    if overlay.is_empty() {
        None
    } else {
        Some(overlay)
    }
}

fn normalize_color(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn slot(get_var: &dyn Fn(&str) -> String, css: &str, fallback: &str) -> String {
    normalize_color(&get_var(css), fallback)
}

pub fn read_editor_colors(get_var: &dyn Fn(&str) -> String) -> EditorColorContract {
    let defaults = default_editor_colors();
    EditorColorContract {
        keyword: slot(get_var, "--cm-keyword", &defaults.keyword),
        string: slot(get_var, "--cm-string", &defaults.string),
        number: slot(get_var, "--cm-number", &defaults.number),
        comment: slot(get_var, "--cm-comment", &defaults.comment),
        operator: slot(get_var, "--cm-operator", &defaults.operator),
        punctuation: slot(get_var, "--cm-punctuation", &defaults.punctuation),
        foreground: slot(get_var, "--cm-foreground", &defaults.foreground),
        background: slot(get_var, "--cm-background", &defaults.background),
        selection: slot(get_var, "--cm-selection", &defaults.selection),
        cursor: slot(get_var, "--cm-cursor", &defaults.cursor),
    }
}

fn apply_string_overlays(base: &mut EditorColorContract, overlay: &serde_json::Map<String, Value>) {
    for (key, value) in overlay {
        if let Some(text) = value.as_str() {
            if !text.trim().is_empty() {
                let trimmed = text.trim();
                match key.as_str() {
                    "keyword" => base.keyword = trimmed.to_owned(),
                    "string" => base.string = trimmed.to_owned(),
                    "number" => base.number = trimmed.to_owned(),
                    "comment" => base.comment = trimmed.to_owned(),
                    "operator" => base.operator = trimmed.to_owned(),
                    "punctuation" => base.punctuation = trimmed.to_owned(),
                    "foreground" => base.foreground = trimmed.to_owned(),
                    "background" => base.background = trimmed.to_owned(),
                    "selection" => base.selection = trimmed.to_owned(),
                    "cursor" => base.cursor = trimmed.to_owned(),
                    _ => (),
                }
            }
        }
    }
}

pub fn editor_colors_with_pack_overlay(colors: EditorColorContract) -> EditorColorContract {
    match pack_overlay() {
        Some(overlay) => {
            let mut next = colors;
            apply_string_overlays(&mut next, &overlay);
            next
        }
        None => colors,
    }
}

pub fn editor_colors_from_json(json: &Value, base: &EditorColorContract) -> EditorColorContract {
    let mut next = base.clone();
    if let Some(object) = json.as_object() {
        apply_string_overlays(&mut next, object);
    }
    next
}
