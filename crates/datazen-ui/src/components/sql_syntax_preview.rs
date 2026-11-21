//! Rune port of `packages/frontend/src/components/SqlSyntaxPreview.tsx`
//! (136 lines, T241): the Settings page's static SQL syntax preview —
//! the sample query painted with the selected preset's colours, no
//! editor instance needed.
//!
//! ## The carried preset data (T632's seam)
//!
//! React imports `SQL_SYNTAX_PRESETS` and its palette types from
//! `lib/themeEditorColors.ts`, whose owning ledger task is T632
//! (segment 2). Until it lands, this module carries the preset data
//! verbatim — the same shape `sql_editor/lib_helpers` holds for
//! `lib/sqlPathPrefix.ts` — and T632 will absorb the table (the
//! `lib_helpers` precedent: single source, the other side re-exports).
//! Only what the preview reads is carried: the ten syntax slots plus
//! the optional `foreground` / `background` (the `selection` colour the
//! preset also carries is editor-only, so it stays with T632).
//!
//! ## Token tagging
//!
//! `SAMPLE_TOKENS` mirrors the CodeMirror SQL parser's tagging, exactly
//! as the React table does: dotted member access (`u.name`) is
//! `name` + `propertyName`, bare identifiers and built-in functions
//! (`COUNT`) are `name`, the clauses are `keyword`, and the literals
//! / operators / punctuation are as expected.
//!
//! ## Rendering
//!
//! React's per-token `<span>`s become one `text` element with one
//! `Span` per token (the `Span` API is the rune text model's coloured
//! slice); the comment tokens carry the italic flag (`fontStyle:
//! italic`). The `<pre overflow-x-auto>` is unwrapped mono text (the
//! `ai_code_block` SQL arm's mapping) inside the rounded, clipped
//! `overflow-hidden` root; `select-text` is `El::selectable(true)`.
//!
//! ## Key convention
//!
//! React gives none of these a `data-testid`, so the keys are new and
//! documented here for the settings wiring task that mounts the preview:
//!
//! | part | key |
//! |---|---|
//! | root | `sql-syntax-preview` |
//! | pre | `sql-syntax-preview-pre` |
//!
//! No production consumer yet: the settings theme picker task is the
//! mount point (T632's own wiring lands it).

// The production consumer is the settings theme picker wiring task;
// until then the component tests are the only callers.
#![allow(dead_code)]

use rune::{col, text, Color, El, Span};

use crate::ui::kit::kit;

/// One preset's palette half (React's `Partial<EditorColorContract> &
/// Partial<ExtendedHighlightColors>` restricted to what the preview
/// reads). The ten syntax slots are present in every shipped preset, so
/// they are plain; `foreground` / `background` are the optional pair
/// the fallbacks below resolve.
pub(crate) struct SqlSyntaxPalette {
    pub keyword: &'static str,
    pub string: &'static str,
    pub number: &'static str,
    pub comment: &'static str,
    pub operator: &'static str,
    pub punctuation: &'static str,
    pub type_name: &'static str,
    pub property_name: &'static str,
    pub variable_name: &'static str,
    pub name: &'static str,
    pub foreground: Option<&'static str>,
    pub background: Option<&'static str>,
}

/// React `SqlSyntaxPreset` (the preview's read of it).
pub(crate) struct SqlSyntaxPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: SqlSyntaxPalette,
    pub light: SqlSyntaxPalette,
}

/// React `SQL_SYNTAX_PRESETS` — all seven, verbatim from
/// `themeEditorColors.ts:133–370` (T632's seam; see the module docs).
pub(crate) static SQL_SYNTAX_PRESETS: [SqlSyntaxPreset; 7] = [
    SqlSyntaxPreset {
        id: "default",
        label: "Default (DataZen)",
        dark: SqlSyntaxPalette {
            keyword: "#93c5fd",
            string: "#6ee7b7",
            number: "#fbbf24",
            comment: "#9ca3af",
            operator: "#67e8f9",
            punctuation: "#d1d5db",
            type_name: "#e5c07b",
            property_name: "#61afef",
            variable_name: "#e06c75",
            name: "#abb2bf",
            foreground: None,
            background: None,
        },
        light: SqlSyntaxPalette {
            keyword: "#2563eb",
            string: "#16a34a",
            number: "#d97706",
            comment: "#6b7280",
            operator: "#0891b2",
            punctuation: "#374151",
            type_name: "#b45309",
            property_name: "#2563eb",
            variable_name: "#dc2626",
            name: "#111827",
            foreground: None,
            background: None,
        },
    },
    SqlSyntaxPreset {
        id: "vscode-dark",
        label: "Visual Studio Dark+",
        dark: SqlSyntaxPalette {
            keyword: "#569cd6",
            string: "#ce9178",
            number: "#b5cea8",
            comment: "#6A9955",
            operator: "#d4d4d4",
            punctuation: "#808080",
            type_name: "#4EC9B0",
            property_name: "#9cdcfe",
            variable_name: "#9cdcfe",
            name: "#d4d4d4",
            foreground: Some("#d4d4d4"),
            background: None,
        },
        light: SqlSyntaxPalette {
            keyword: "#0000ff",
            string: "#a31515",
            number: "#098658",
            comment: "#008000",
            operator: "#000000",
            punctuation: "#000000",
            type_name: "#267f99",
            property_name: "#001080",
            variable_name: "#001080",
            name: "#000000",
            foreground: Some("#000000"),
            background: None,
        },
    },
    SqlSyntaxPreset {
        id: "monokai",
        label: "Monokai",
        dark: SqlSyntaxPalette {
            keyword: "#f92672",
            string: "#e6db74",
            number: "#ae81ff",
            comment: "#75715e",
            operator: "#f92672",
            punctuation: "#f8f8f2",
            type_name: "#66d9ef",
            property_name: "#a6e22e",
            variable_name: "#f8f8f2",
            name: "#f8f8f2",
            foreground: Some("#f8f8f2"),
            background: Some("#272822"),
        },
        light: SqlSyntaxPalette {
            keyword: "#e3116c",
            string: "#e6db74",
            number: "#ae81ff",
            comment: "#75715e",
            operator: "#e3116c",
            punctuation: "#333333",
            type_name: "#2b908f",
            property_name: "#a6e22e",
            variable_name: "#333333",
            name: "#333333",
            foreground: Some("#333333"),
            background: Some("#fff"),
        },
    },
    SqlSyntaxPreset {
        id: "solarized-dark",
        label: "Solarized Dark",
        dark: SqlSyntaxPalette {
            keyword: "#859900",
            string: "#2aa198",
            number: "#d33682",
            comment: "#586e75",
            operator: "#839496",
            punctuation: "#839496",
            type_name: "#b58900",
            property_name: "#268bd2",
            variable_name: "#839496",
            name: "#839496",
            foreground: Some("#839496"),
            background: Some("#002b36"),
        },
        light: SqlSyntaxPalette {
            keyword: "#859900",
            string: "#2aa198",
            number: "#d33682",
            comment: "#93a1a1",
            operator: "#657b83",
            punctuation: "#657b83",
            type_name: "#b58900",
            property_name: "#268bd2",
            variable_name: "#657b83",
            name: "#657b83",
            foreground: Some("#657b83"),
            background: Some("#fdf6e3"),
        },
    },
    SqlSyntaxPreset {
        id: "solarized-light",
        label: "Solarized Light",
        dark: SqlSyntaxPalette {
            keyword: "#859900",
            string: "#2aa198",
            number: "#d33682",
            comment: "#586e75",
            operator: "#839496",
            punctuation: "#839496",
            type_name: "#b58900",
            property_name: "#268bd2",
            variable_name: "#839496",
            name: "#839496",
            foreground: Some("#839496"),
            background: Some("#002b36"),
        },
        light: SqlSyntaxPalette {
            keyword: "#859900",
            string: "#2aa198",
            number: "#d33682",
            comment: "#93a1a1",
            operator: "#657b83",
            punctuation: "#657b83",
            type_name: "#b58900",
            property_name: "#268bd2",
            variable_name: "#657b83",
            name: "#657b83",
            foreground: Some("#657b83"),
            background: Some("#fdf6e3"),
        },
    },
    SqlSyntaxPreset {
        id: "dracula",
        label: "Dracula",
        dark: SqlSyntaxPalette {
            keyword: "#ff79c6",
            string: "#f1fa8c",
            number: "#bd93f9",
            comment: "#6272a4",
            operator: "#ff79c6",
            punctuation: "#f8f8f2",
            type_name: "#8be9fd",
            property_name: "#50fa7b",
            variable_name: "#f8f8f2",
            name: "#f8f8f2",
            foreground: Some("#f8f8f2"),
            background: Some("#282a36"),
        },
        light: SqlSyntaxPalette {
            keyword: "#e3116c",
            string: "#c18e27",
            number: "#7c3aed",
            comment: "#6272a4",
            operator: "#e3116c",
            punctuation: "#343746",
            type_name: "#0d93b3",
            property_name: "#067d17",
            variable_name: "#343746",
            name: "#343746",
            foreground: Some("#343746"),
            background: Some("#f8f8f2"),
        },
    },
    SqlSyntaxPreset {
        id: "github-dark",
        label: "GitHub Dark",
        dark: SqlSyntaxPalette {
            keyword: "#ff7b72",
            string: "#a5d6ff",
            number: "#79c0ff",
            comment: "#8b949e",
            operator: "#ff7b72",
            punctuation: "#c9d1d9",
            type_name: "#ffa657",
            property_name: "#d2a8ff",
            variable_name: "#c9d1d9",
            name: "#c9d1d9",
            foreground: Some("#c9d1d9"),
            background: Some("#0d1117"),
        },
        light: SqlSyntaxPalette {
            keyword: "#cf222e",
            string: "#0a3069",
            number: "#0550ae",
            comment: "#6e7781",
            operator: "#cf222e",
            punctuation: "#24292f",
            type_name: "#953800",
            property_name: "#8250df",
            variable_name: "#24292f",
            name: "#24292f",
            foreground: Some("#24292f"),
            background: Some("#ffffff"),
        },
    },
];

/// React's inline `lookupPreset`: the id's preset, or the first when the
/// id is unknown (`?? SQL_SYNTAX_PRESETS[0]`).
pub(crate) fn preset_for(id: &str) -> &'static SqlSyntaxPreset {
    SQL_SYNTAX_PRESETS
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or(&SQL_SYNTAX_PRESETS[0])
}

/// React's background fallback: `palette.background ?? (dark ?
/// '#1e1e1e' : '#ffffff')`.
pub(crate) fn background_of(palette: &SqlSyntaxPalette, dark: bool) -> Color {
    palette
        .background
        .and_then(|hex| rune::color::hex(hex))
        .unwrap_or(if dark {
            rune::color::hex("#1e1e1e").unwrap()
        } else {
            rune::color::hex("#ffffff").unwrap()
        })
}

/// React's foreground fallback: `palette.foreground ?? (dark ?
/// '#d4d4d4' : '#000000')`.
pub(crate) fn foreground_of(palette: &SqlSyntaxPalette, dark: bool) -> Color {
    palette
        .foreground
        .and_then(|hex| rune::color::hex(hex))
        .unwrap_or(if dark {
            rune::color::hex("#d4d4d4").unwrap()
        } else {
            rune::color::hex("#000000").unwrap()
        })
}

/// One resolved slot colour: the preset's hex, or the foreground when
/// the slot is absent (React's `palette[key] ?? fg`).
pub(crate) fn slot_of(palette: &SqlSyntaxPalette, slot: &str, fg: Color) -> Color {
    let hex = match slot {
        "keyword" => Some(palette.keyword),
        "string" => Some(palette.string),
        "number" => Some(palette.number),
        "comment" => Some(palette.comment),
        "operator" => Some(palette.operator),
        "punctuation" => Some(palette.punctuation),
        "typeName" => Some(palette.type_name),
        "propertyName" => Some(palette.property_name),
        "variableName" => Some(palette.variable_name),
        "name" => Some(palette.name),
        _ => None,
    };
    hex.and_then(|hex| rune::color::hex(hex)).unwrap_or(fg)
}

/// React `TokenDef.colorKey`: the six contract keys plus the four
/// extended slots and the foreground fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenColor {
    Comment,
    Keyword,
    String,
    Number,
    Operator,
    Punctuation,
    TypeName,
    PropertyName,
    VariableName,
    Name,
    Foreground,
}

impl TokenColor {
    /// The palette slot the key reads, or `None` for `foreground`.
    fn slot(self) -> Option<&'static str> {
        match self {
            Self::Comment => Some("comment"),
            Self::Keyword => Some("keyword"),
            Self::String => Some("string"),
            Self::Number => Some("number"),
            Self::Operator => Some("operator"),
            Self::Punctuation => Some("punctuation"),
            Self::TypeName => Some("typeName"),
            Self::PropertyName => Some("propertyName"),
            Self::VariableName => Some("variableName"),
            Self::Name => Some("name"),
            Self::Foreground => None,
        }
    }
}

/// React `TokenDef`.
struct TokenDef {
    text: &'static str,
    color: TokenColor,
}

/// React `SAMPLE_TOKENS` — the sample query covering every
/// highlightable token type, tagged as the CodeMirror SQL parser would
/// (verbatim from `SqlSyntaxPreview.tsx:43–97`).
static SAMPLE_TOKENS: [TokenDef; 53] = [
    TokenDef { text: "-- ", color: TokenColor::Comment },
    TokenDef { text: "Sample query preview", color: TokenColor::Comment },
    TokenDef { text: "\n", color: TokenColor::Foreground },
    TokenDef { text: "SELECT", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "u", color: TokenColor::Name },
    TokenDef { text: ".", color: TokenColor::Punctuation },
    TokenDef { text: "name", color: TokenColor::PropertyName },
    TokenDef { text: ",", color: TokenColor::Punctuation },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "COUNT", color: TokenColor::Name },
    TokenDef { text: "(", color: TokenColor::Punctuation },
    TokenDef { text: "o", color: TokenColor::Name },
    TokenDef { text: ".", color: TokenColor::Punctuation },
    TokenDef { text: "id", color: TokenColor::PropertyName },
    TokenDef { text: ")", color: TokenColor::Punctuation },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "AS", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "total", color: TokenColor::Name },
    TokenDef { text: "\n", color: TokenColor::Foreground },
    TokenDef { text: "FROM", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "users", color: TokenColor::Name },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "u", color: TokenColor::Name },
    TokenDef { text: "\n", color: TokenColor::Foreground },
    TokenDef { text: "WHERE", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "u", color: TokenColor::Name },
    TokenDef { text: ".", color: TokenColor::Punctuation },
    TokenDef { text: "status", color: TokenColor::PropertyName },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "=", color: TokenColor::Operator },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "'active'", color: TokenColor::String },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "AND", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "u", color: TokenColor::Name },
    TokenDef { text: ".", color: TokenColor::Punctuation },
    TokenDef { text: "age", color: TokenColor::PropertyName },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: ">", color: TokenColor::Operator },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "18", color: TokenColor::Number },
    TokenDef { text: "\n", color: TokenColor::Foreground },
    TokenDef { text: "ORDER BY", color: TokenColor::Keyword },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "total", color: TokenColor::Name },
    TokenDef { text: " ", color: TokenColor::Foreground },
    TokenDef { text: "DESC", color: TokenColor::Keyword },
    TokenDef { text: ";", color: TokenColor::Punctuation },
];

/// The preview: the sample query painted with the preset's colours.
/// Build it with the selected `themeId` and the surface's dark flag.
pub(crate) struct SqlSyntaxPreview {
    theme_id: String,
    dark: bool,
}

impl SqlSyntaxPreview {
    pub(crate) fn new(theme_id: impl Into<String>, dark: bool) -> Self {
        Self { theme_id: theme_id.into(), dark }
    }

    /// The painted sample. React's root: `mt-2 overflow-hidden
    /// rounded-lg border border-edge text-[12px] leading-[1.7]` over
    /// the preset background, the `<pre>` unwrapped mono text with one
    /// coloured span per token.
    pub(crate) fn build(&self) -> El {
        let k = kit(self.dark);
        let preset = preset_for(&self.theme_id);
        let palette = if self.dark { &preset.dark } else { &preset.light };
        let bg = background_of(palette, self.dark);
        let fg = foreground_of(palette, self.dark);

        let (source, spans) = sample_token_spans(palette, fg);

        let pre = text(source)
            .key("sql-syntax-preview-pre")
            .w_full()
            // React: `px-4 py-3 font-mono select-text`, `margin: 0`.
            .px(16.0)
            .py(12.0)
            .font(k.ty.mono.lh(20.4))
            .color(fg)
            .selectable(true)
            .spans(spans)
            .wrap();

        col()
            .key("sql-syntax-preview")
            .w_full()
            .mt(8.0)
            .radius(k.radius.lg)
            .clip_rounded(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(bg)
            .child(pre)
    }
}

/// The sample's one-text form: the tokens concatenated in order, each
/// with its coloured span (the comment tokens italic). This is React's
/// per-token `<span>` list as the text model's span list — one span
/// per token, covering the text with no gaps or overlaps.
pub(crate) fn sample_token_spans(
    palette: &SqlSyntaxPalette,
    fg: Color,
) -> (String, Vec<Span>) {
    let mut source = String::new();
    let mut spans = Vec::with_capacity(SAMPLE_TOKENS.len());
    for token in SAMPLE_TOKENS.iter() {
        let start = source.len();
        source.push_str(token.text);
        let end = source.len();
        let color = match token.color.slot() {
            Some(slot) => slot_of(palette, slot, fg),
            None => fg,
        };
        let mut span = Span::new(start, end).color(color);
        if token.color == TokenColor::Comment {
            span = span.italic();
        }
        spans.push(span);
    }
    (source, spans)
}
