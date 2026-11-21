use serde_json::{json, Value};

use super::*;

#[test]
fn css_variables_and_defaults_are_pinned() {
    assert_eq!(CM_VARS.len(), 10);
    assert_eq!(CM_VARS[0], ("keyword", "--cm-keyword"));
    assert_eq!(CM_VARS[9], ("cursor", "--cm-cursor"));
    let defaults = default_editor_colors();
    assert_eq!(defaults.keyword, "#93c5fd");
    assert_eq!(defaults.background, "#0b0e14");
    assert_eq!(defaults.selection, "rgba(79, 195, 247, 0.25)");
}

#[test]
fn overlay_parsing_accepts_only_known_keys_with_nonblank_strings() {
    let parsed = parse_pack_editor_overlay(&json!({
        "keyword": "  #123456  ",
        "unknown": "#ffffff",
        "number": "   ",
        "string": 42
    }));
    let overlay = parsed.expect("overlay");
    assert_eq!(overlay.len(), 1);
    assert_eq!(overlay.get("keyword"), Some(&json!("#123456")));
    assert_eq!(parse_pack_editor_overlay(&json!({"unknown": "x"})), None);
    assert_eq!(parse_pack_editor_overlay(&Value::Null), None);
    assert_eq!(parse_pack_editor_overlay(&json!([])), None);
}

#[test]
fn css_var_reader_falls_back_per_slot_and_trims() {
    let colors = read_editor_colors(&|name| match name {
        "--cm-keyword" => " #abc ".to_owned(),
        "--cm-comment" => "   ".to_owned(),
        _ => String::new(),
    });
    assert_eq!(colors.keyword, "#abc");
    assert_eq!(colors.comment, "#5f6879");
    assert_eq!(colors.background, "#0b0e14");
}

#[test]
fn pack_overlay_and_json_overlay_apply_only_known_fields() {
    set_pack_editor_color_overlay(None);
    let base = default_editor_colors();
    let raw = read_editor_colors(&|_| String::new());
    assert_eq!(editor_colors_with_pack_overlay(raw.clone()), base);

    let overlay = parse_pack_editor_overlay(&json!({"keyword": "#111111", "bogus": "#222222"})).expect("overlay");
    set_pack_editor_color_overlay(Some(overlay));
    let merged = editor_colors_with_pack_overlay(raw.clone());
    assert_eq!(merged.keyword, "#111111");
    assert_eq!(merged.string, base.string);

    let from_json = editor_colors_from_json(&json!({"number": " #999 "}), &base);
    assert_eq!(from_json.number, "#999");
    assert_eq!(from_json.keyword, base.keyword);
    set_pack_editor_color_overlay(None);
}

#[test]
fn preset_table_ids_labels_and_theme_id_validation() {
    assert_eq!(SQL_SYNTAX_PRESETS.len(), 7);
    assert_eq!(SQL_SYNTAX_PRESETS[0].id, "default");
    assert_eq!(SQL_SYNTAX_PRESETS[6].id, "github-dark");
    assert!(is_sql_syntax_theme_id(Some("monokai")));
    assert!(!is_sql_syntax_theme_id(Some("nope")));
    assert!(!is_sql_syntax_theme_id(None));
}

#[test]
fn default_preset_applies_mode_specific_extended_colors() {
    let base = default_editor_colors();
    let dark = apply_sql_syntax_preset(&base, None, true);
    assert_eq!(dark.type_name, "#e5c07b");
    assert_eq!(dark.property_name, "#61afef");
    assert_eq!(dark.variable_name, "#e06c75");
    assert_eq!(dark.name, "#abb2bf");
    assert_eq!(dark.keyword, base.keyword);

    let light = apply_sql_syntax_preset(&base, Some("default"), false);
    assert_eq!(light.type_name, "#b45309");
    assert_eq!(light.name, "#111827");
}

#[test]
fn known_preset_overrides_palette_and_fills_missing_extended_slots() {
    let base = default_editor_colors();
    let dark = apply_sql_syntax_preset(&base, Some("vscode-dark"), true);
    assert_eq!(dark.keyword, "#569cd6");
    assert_eq!(dark.selection, "rgba(38, 79, 120, 0.5)");
    assert_eq!(dark.type_name, "#4EC9B0", "the preset's typeName is used");
    assert_eq!(dark.background, base.background, "unset slots stay from base");

    let dark = apply_sql_syntax_preset(&base, Some("monokai"), true);
    assert_eq!(dark.background, "#272822");
    assert_eq!(dark.variable_name, "#f8f8f2", "the preset's variableName is used");

    let dark = apply_sql_syntax_preset(&base, Some("nope"), true);
    assert_eq!(dark.comment, "#9ca3af", "unknown ids fall back to the first preset's palette");
    assert_eq!(dark.type_name, "#e5c07b");
}

#[test]
fn highlight_rules_follow_tag_order_and_mode_defaults() {
    let base = default_editor_colors();
    let dark_rules = build_editor_highlight_rules(&StyleColors { contract: &base, ext: None }, true);
    assert_eq!(dark_rules.len(), 15);
    assert_eq!(dark_rules[0].tag, "keyword");
    assert_eq!(dark_rules[0].color, base.keyword);
    assert_eq!(dark_rules[1].tag, "operatorKeyword");
    assert_eq!(dark_rules[2].tag, "typeName");
    assert_eq!(dark_rules[2].color, "#e5c07b");
    assert_eq!(dark_rules[7].tag, "comment");
    assert_eq!(dark_rules[7].font_style, Some("italic"));
    assert_eq!(dark_rules[12].tag, "functionVariableName");
    assert_eq!(dark_rules[12].color, "#61afef");
    assert_eq!(dark_rules[14].tag, "name");

    let light_rules = build_editor_highlight_rules(&StyleColors { contract: &base, ext: None }, false);
    assert_eq!(light_rules[2].color, "#b45309");

    let ext = ExtendedHighlightColors {
        type_name: "#t", property_name: "#p", variable_name: "#v", name: "#n",
    };
    let rules = build_editor_highlight_rules(&StyleColors { contract: &base, ext: Some(&ext) }, true);
    assert_eq!(rules[2].color, "#t");
    assert_eq!(rules[12].color, "#p");
}

#[test]
fn sql_property_names_require_a_preceding_dot() {
    let doc = "SELECT t.id, name FROM t";
    let nodes = vec![
        SyntaxNodeSpan { name: "Identifier".to_owned(), from: 9, to: 11 },
        SyntaxNodeSpan { name: "Identifier".to_owned(), from: 13, to: 17 },
        SyntaxNodeSpan { name: "QuotedIdentifier".to_owned(), from: 0, to: 3 },
    ];
    assert_eq!(find_sql_property_names(doc, &nodes), vec![(9, 11)]);
    let dotted = "SELECT a . b";
    let nodes = vec![
        SyntaxNodeSpan { name: "Identifier".to_owned(), from: 10, to: 11 },
        SyntaxNodeSpan { name: "Keyword".to_owned(), from: 8, to: 9 },
    ];
    assert_eq!(find_sql_property_names(dotted, &nodes), vec![(10, 11)]);
}

#[test]
fn codemirror_gaps_are_pinned() {
    assert_eq!(crate::CODEMIRROR_COLOR_GAP.len(), 5);
    assert_eq!(crate::CODEMIRROR_HIGHLIGHT_GAP.len(), 5);
    assert!(crate::CODEMIRROR_COLOR_GAP.contains(&"getComputedStyle"));
    assert!(crate::CODEMIRROR_HIGHLIGHT_GAP.contains(&"ViewPlugin.fromClass"));
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::read_editor_colors;
    let _ = crate::editor_colors_from_json;
    let _ = crate::editor_colors_with_pack_overlay;
    let _ = crate::parse_pack_editor_overlay;
    let _ = crate::set_pack_editor_color_overlay;
    let _ = crate::default_editor_colors;
    let _ = crate::apply_sql_syntax_preset;
    let _ = crate::is_sql_syntax_theme_id;
    let _ = crate::build_editor_highlight_rules;
    let _ = crate::find_sql_property_names;
    assert_eq!(crate::SQL_SYNTAX_PRESETS.len(), 7);
    assert_eq!(crate::CM_VARS.len(), 10);
    let _: Option<crate::EditorColorContract> = None;
    let _: Option<crate::EditorColors> = None;
    let _: Option<crate::ExtendedHighlightColors> = None;
    let _: Option<crate::SqlSyntaxPreset> = None;
    let _: Option<crate::Palette> = None;
    let _: Option<crate::HighlightRule> = None;
    let _: Option<crate::StyleColors<'static>> = None;
    let _: Option<crate::SyntaxNodeSpan> = None;
}
