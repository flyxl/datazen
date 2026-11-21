use super::contract::EditorColorContract;
use super::presets::{dark_ext, light_ext, ExtendedHighlightColors};

pub const CODEMIRROR_HIGHLIGHT_GAP: [&str; 5] = [
    "HighlightStyle.define",
    "syntaxHighlighting",
    "ViewPlugin.fromClass",
    "Decoration.mark",
    "Decoration.set",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightRule {
    pub tag: &'static str,
    pub color: String,
    pub font_style: Option<&'static str>,
}

impl HighlightRule {
    fn new(tag: &'static str, color: &str) -> Self {
        Self { tag, color: color.to_owned(), font_style: None }
    }
}

pub struct StyleColors<'a> {
    pub contract: &'a EditorColorContract,
    pub ext: Option<&'a ExtendedHighlightColors>,
}

fn ext_or_default<'a>(ext: Option<&'a ExtendedHighlightColors>, dark: bool) -> ExtendedHighlightColors {
    match ext {
        Some(ext) => *ext,
        None => if dark { dark_ext() } else { light_ext() },
    }
}

pub fn build_editor_highlight_rules(colors: &StyleColors<'_>, dark: bool) -> Vec<HighlightRule> {
    let base = colors.contract;
    let ext = ext_or_default(colors.ext, dark);
    let mut rules = vec![
        HighlightRule::new("keyword", &base.keyword),
        HighlightRule::new("operatorKeyword", &base.keyword),
        HighlightRule::new("typeName", ext.type_name),
        HighlightRule::new("string", &base.string),
        HighlightRule::new("number", &base.number),
        HighlightRule::new("bool", &base.number),
        HighlightRule::new("null", &base.number),
    ];
    rules.push(HighlightRule {
        tag: "comment",
        color: base.comment.clone(),
        font_style: Some("italic"),
    });
    rules.extend([
        HighlightRule::new("punctuation", &base.punctuation),
        HighlightRule::new("bracket", &base.punctuation),
        HighlightRule::new("operator", &base.operator),
        HighlightRule::new("propertyName", ext.property_name),
        HighlightRule::new("functionVariableName", ext.property_name),
        HighlightRule::new("variableName", ext.variable_name),
        HighlightRule::new("name", ext.name),
    ]);
    rules
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxNodeSpan {
    pub name: String,
    pub from: usize,
    pub to: usize,
}

pub fn find_sql_property_names(doc: &str, nodes: &[SyntaxNodeSpan]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for node in nodes {
        if node.name != "Identifier" && node.name != "QuotedIdentifier" {
            continue;
        }
        if node.from == 0 {
            continue;
        }
        let before = match doc.get(..node.from) {
            Some(text) => text,
            None => continue,
        };
        if before.trim_end().ends_with('.') {
            ranges.push((node.from, node.to));
        }
    }
    ranges.sort_by_key(|range| range.0);
    ranges
}
