mod contract;
mod highlight;
mod presets;

#[cfg(test)]
mod tests;

pub use contract::{
    editor_colors_from_json, editor_colors_with_pack_overlay, parse_pack_editor_overlay,
    read_editor_colors, set_pack_editor_color_overlay, default_editor_colors, EditorColorContract,
    CM_VARS, CODEMIRROR_COLOR_GAP,
};
pub use highlight::{
    build_editor_highlight_rules, find_sql_property_names, HighlightRule, StyleColors,
    SyntaxNodeSpan, CODEMIRROR_HIGHLIGHT_GAP,
};
pub use presets::{
    apply_sql_syntax_preset, is_sql_syntax_theme_id, SQL_SYNTAX_PRESETS, EditorColors,
    ExtendedHighlightColors, Palette, SqlSyntaxPreset,
};
