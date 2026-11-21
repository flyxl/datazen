use super::contract::EditorColorContract;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorColors {
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
    pub type_name: String,
    pub property_name: String,
    pub variable_name: String,
    pub name: String,
}

impl EditorColors {
    pub fn from_contract(base: &EditorColorContract, ext: ExtendedHighlightColors) -> Self {
        Self {
            keyword: base.keyword.clone(),
            string: base.string.clone(),
            number: base.number.clone(),
            comment: base.comment.clone(),
            operator: base.operator.clone(),
            punctuation: base.punctuation.clone(),
            foreground: base.foreground.clone(),
            background: base.background.clone(),
            selection: base.selection.clone(),
            cursor: base.cursor.clone(),
            type_name: ext.type_name.to_owned(),
            property_name: ext.property_name.to_owned(),
            variable_name: ext.variable_name.to_owned(),
            name: ext.name.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedHighlightColors {
    pub type_name: &'static str,
    pub property_name: &'static str,
    pub variable_name: &'static str,
    pub name: &'static str,
}

pub fn dark_ext() -> ExtendedHighlightColors {
    ExtendedHighlightColors {
        type_name: "#e5c07b",
        property_name: "#61afef",
        variable_name: "#e06c75",
        name: "#abb2bf",
    }
}

pub fn light_ext() -> ExtendedHighlightColors {
    ExtendedHighlightColors {
        type_name: "#b45309",
        property_name: "#2563eb",
        variable_name: "#dc2626",
        name: "#111827",
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    pub entries: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlSyntaxPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: Palette,
    pub light: Palette,
}

pub const SQL_SYNTAX_PRESETS: [SqlSyntaxPreset; 7] = [
    SqlSyntaxPreset {
        id: "default",
        label: "Default (DataZen)",
        dark: Palette { entries: &[
            ("keyword", "#93c5fd"), ("string", "#6ee7b7"), ("number", "#fbbf24"),
            ("comment", "#9ca3af"), ("operator", "#67e8f9"), ("punctuation", "#d1d5db"),
            ("typeName", "#e5c07b"), ("propertyName", "#61afef"), ("variableName", "#e06c75"),
            ("name", "#abb2bf"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#2563eb"), ("string", "#16a34a"), ("number", "#d97706"),
            ("comment", "#6b7280"), ("operator", "#0891b2"), ("punctuation", "#374151"),
            ("typeName", "#b45309"), ("propertyName", "#2563eb"), ("variableName", "#dc2626"),
            ("name", "#111827"),
        ] },
    },
    SqlSyntaxPreset {
        id: "vscode-dark",
        label: "Visual Studio Dark+",
        dark: Palette { entries: &[
            ("keyword", "#569cd6"), ("string", "#ce9178"), ("number", "#b5cea8"),
            ("comment", "#6A9955"), ("operator", "#d4d4d4"), ("punctuation", "#808080"),
            ("typeName", "#4EC9B0"), ("propertyName", "#9cdcfe"), ("variableName", "#9cdcfe"),
            ("name", "#d4d4d4"), ("foreground", "#d4d4d4"), ("selection", "rgba(38, 79, 120, 0.5)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#0000ff"), ("string", "#a31515"), ("number", "#098658"),
            ("comment", "#008000"), ("operator", "#000000"), ("punctuation", "#000000"),
            ("typeName", "#267f99"), ("propertyName", "#001080"), ("variableName", "#001080"),
            ("name", "#000000"), ("foreground", "#000000"), ("selection", "rgba(0, 0, 0, 0.14)"),
        ] },
    },
    SqlSyntaxPreset {
        id: "monokai",
        label: "Monokai",
        dark: Palette { entries: &[
            ("keyword", "#f92672"), ("string", "#e6db74"), ("number", "#ae81ff"),
            ("comment", "#75715e"), ("operator", "#f92672"), ("punctuation", "#f8f8f2"),
            ("typeName", "#66d9ef"), ("propertyName", "#a6e22e"), ("variableName", "#f8f8f2"),
            ("name", "#f8f8f2"), ("foreground", "#f8f8f2"), ("background", "#272822"),
            ("selection", "rgba(102, 102, 102, 0.5)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#e3116c"), ("string", "#e6db74"), ("number", "#ae81ff"),
            ("comment", "#75715e"), ("operator", "#e3116c"), ("punctuation", "#333333"),
            ("typeName", "#2b908f"), ("propertyName", "#a6e22e"), ("variableName", "#333333"),
            ("name", "#333333"), ("foreground", "#333333"), ("background", "#fff"),
            ("selection", "rgba(181, 137, 0, 0.2)"),
        ] },
    },
    SqlSyntaxPreset {
        id: "solarized-dark",
        label: "Solarized Dark",
        dark: Palette { entries: &[
            ("keyword", "#859900"), ("string", "#2aa198"), ("number", "#d33682"),
            ("comment", "#586e75"), ("operator", "#839496"), ("punctuation", "#839496"),
            ("typeName", "#b58900"), ("propertyName", "#268bd2"), ("variableName", "#839496"),
            ("name", "#839496"), ("foreground", "#839496"), ("background", "#002b36"),
            ("selection", "rgba(147, 161, 161, 0.3)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#859900"), ("string", "#2aa198"), ("number", "#d33682"),
            ("comment", "#93a1a1"), ("operator", "#657b83"), ("punctuation", "#657b83"),
            ("typeName", "#b58900"), ("propertyName", "#268bd2"), ("variableName", "#657b83"),
            ("name", "#657b83"), ("foreground", "#657b83"), ("background", "#fdf6e3"),
            ("selection", "rgba(147, 161, 161, 0.2)"),
        ] },
    },
    SqlSyntaxPreset {
        id: "solarized-light",
        label: "Solarized Light",
        dark: Palette { entries: &[
            ("keyword", "#859900"), ("string", "#2aa198"), ("number", "#d33682"),
            ("comment", "#586e75"), ("operator", "#839496"), ("punctuation", "#839496"),
            ("typeName", "#b58900"), ("propertyName", "#268bd2"), ("variableName", "#839496"),
            ("name", "#839496"), ("foreground", "#839496"), ("background", "#002b36"),
            ("selection", "rgba(147, 161, 161, 0.3)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#859900"), ("string", "#2aa198"), ("number", "#d33682"),
            ("comment", "#93a1a1"), ("operator", "#657b83"), ("punctuation", "#657b83"),
            ("typeName", "#b58900"), ("propertyName", "#268bd2"), ("variableName", "#657b83"),
            ("name", "#657b83"), ("foreground", "#657b83"), ("background", "#fdf6e3"),
            ("selection", "rgba(147, 161, 161, 0.2)"),
        ] },
    },
    SqlSyntaxPreset {
        id: "dracula",
        label: "Dracula",
        dark: Palette { entries: &[
            ("keyword", "#ff79c6"), ("string", "#f1fa8c"), ("number", "#bd93f9"),
            ("comment", "#6272a4"), ("operator", "#ff79c6"), ("punctuation", "#f8f8f2"),
            ("typeName", "#8be9fd"), ("propertyName", "#50fa7b"), ("variableName", "#f8f8f2"),
            ("name", "#f8f8f2"), ("foreground", "#f8f8f2"), ("background", "#282a36"),
            ("selection", "rgba(68, 71, 90, 0.6)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#e3116c"), ("string", "#c18e27"), ("number", "#7c3aed"),
            ("comment", "#6272a4"), ("operator", "#e3116c"), ("punctuation", "#343746"),
            ("typeName", "#0d93b3"), ("propertyName", "#067d17"), ("variableName", "#343746"),
            ("name", "#343746"), ("foreground", "#343746"), ("background", "#f8f8f2"),
            ("selection", "rgba(68, 71, 90, 0.2)"),
        ] },
    },
    SqlSyntaxPreset {
        id: "github-dark",
        label: "GitHub Dark",
        dark: Palette { entries: &[
            ("keyword", "#ff7b72"), ("string", "#a5d6ff"), ("number", "#79c0ff"),
            ("comment", "#8b949e"), ("operator", "#ff7b72"), ("punctuation", "#c9d1d9"),
            ("typeName", "#ffa657"), ("propertyName", "#d2a8ff"), ("variableName", "#c9d1d9"),
            ("name", "#c9d1d9"), ("foreground", "#c9d1d9"), ("background", "#0d1117"),
            ("selection", "rgba(56, 139, 253, 0.4)"),
        ] },
        light: Palette { entries: &[
            ("keyword", "#cf222e"), ("string", "#0a3069"), ("number", "#0550ae"),
            ("comment", "#6e7781"), ("operator", "#cf222e"), ("punctuation", "#24292f"),
            ("typeName", "#953800"), ("propertyName", "#8250df"), ("variableName", "#24292f"),
            ("name", "#24292f"), ("foreground", "#24292f"), ("background", "#ffffff"),
            ("selection", "rgba(56, 139, 253, 0.2)"),
        ] },
    },
];

pub fn is_sql_syntax_theme_id(value: Option<&str>) -> bool {
    match value {
        Some(value) => SQL_SYNTAX_PRESETS.iter().any(|preset| preset.id == value),
        None => false,
    }
}

fn lookup_preset(id: &str) -> &'static SqlSyntaxPreset {
    for preset in &SQL_SYNTAX_PRESETS {
        if preset.id == id {
            return preset;
        }
    }
    &SQL_SYNTAX_PRESETS[0]
}

fn palette_entry<'a>(p: &'a Palette, key: &str) -> Option<&'a str> {
    p.entries.iter().find(|(name, _)| *name == key).map(|(_, value)| *value)
}

fn overlay_palette(base: &mut EditorColors, p: &Palette) {
    for (key, value) in p.entries {
        match *key {
            "keyword" => base.keyword = (*value).to_owned(),
            "string" => base.string = (*value).to_owned(),
            "number" => base.number = (*value).to_owned(),
            "comment" => base.comment = (*value).to_owned(),
            "operator" => base.operator = (*value).to_owned(),
            "punctuation" => base.punctuation = (*value).to_owned(),
            "foreground" => base.foreground = (*value).to_owned(),
            "background" => base.background = (*value).to_owned(),
            "selection" => base.selection = (*value).to_owned(),
            "cursor" => base.cursor = (*value).to_owned(),
            _ => (),
        }
    }
}

pub fn apply_sql_syntax_preset(
    base: &EditorColorContract,
    theme_id: Option<&str>,
    dark: bool,
) -> EditorColors {
    let ext = if dark { dark_ext() } else { light_ext() };
    let mut colors = EditorColors::from_contract(base, ext);
    if theme_id.is_none() || theme_id == Some("default") {
        return colors;
    }
    let id = match theme_id {
        Some(id) => id,
        None => return colors,
    };
    let preset = lookup_preset(id);
    let p = if dark { &preset.dark } else { &preset.light };
    overlay_palette(&mut colors, p);
    if let Some(value) = palette_entry(p, "typeName") {
        colors.type_name = value.to_owned();
    }
    if let Some(value) = palette_entry(p, "propertyName") {
        colors.property_name = value.to_owned();
    }
    if let Some(value) = palette_entry(p, "variableName") {
        colors.variable_name = value.to_owned();
    }
    if let Some(value) = palette_entry(p, "name") {
        colors.name = value.to_owned();
    }
    colors
}
