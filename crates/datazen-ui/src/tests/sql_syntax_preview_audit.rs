//! T241 independent audit — fresh vectors against the SQL syntax
//! preview port, none shared with the coder tests: the seven presets'
//! labels and the background/foreground presence matrix, the
//! solarized twins' identical halves (the data's own quirk, verbatim),
//! fresh colour vectors on the vscode-dark and dracula palettes, the
//! span tiling's byte arithmetic on a wide and a narrow surface, and
//! the unknown-id resolution through the mounted tree.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::sql_syntax_preview::{
    background_of, foreground_of, preset_for, sample_token_spans, SqlSyntaxPreview,
    SQL_SYNTAX_PRESETS,
};

fn hex(color: &str) -> rune::Color {
    rune::color::hex(color).unwrap_or_else(|| panic!("{color} is a hex colour"))
}

// ---------------------------------------------------------------------------
// The preset table's verbatim shape
// ---------------------------------------------------------------------------

#[test]
fn every_preset_label_is_the_react_string() {
    let labels: Vec<&str> = SQL_SYNTAX_PRESETS.iter().map(|preset| preset.label).collect();
    assert_eq!(
        labels,
        vec![
            "Default (DataZen)",
            "Visual Studio Dark+",
            "Monokai",
            "Solarized Dark",
            "Solarized Light",
            "Dracula",
            "GitHub Dark",
        ]
    );
}

#[test]
fn the_background_and_foreground_presence_matches_the_react_table() {
    for preset in SQL_SYNTAX_PRESETS.iter() {
        let (bg, fg) = (&preset.dark.background, &preset.dark.foreground);
        match preset.id {
            // The default relies on both fallbacks.
            "default" => assert!(bg.is_none() && fg.is_none()),
            // VS Code dark+ carries its editor foreground but no
            // background of its own (the theme sets only ink colours).
            "vscode-dark" => assert!(bg.is_none() && fg.is_some()),
            // Every other preset paints its own surface.
            _ => assert!(bg.is_some() && fg.is_some(), "{} paints its own surface", preset.id),
        }
        // Every light half matches its dark half's presence (the table
        // never halves a preset).
        assert_eq!(
            preset.light.background.is_some(),
            preset.dark.background.is_some(),
            "{}: the light half keeps the dark's presence",
            preset.id
        );
        assert_eq!(
            preset.light.foreground.is_some(),
            preset.dark.foreground.is_some(),
            "{}: the light half keeps the dark's ink presence",
            preset.id
        );
    }
}

#[test]
fn the_solarized_twins_carry_identical_halves() {
    // The React table's quirk, verbatim: solarized-dark and
    // solarized-light hold the same dark AND the same light palettes —
    // the two entries differ only in name.
    let dark = preset_for("solarized-dark");
    let light = preset_for("solarized-light");
    assert_eq!(dark.dark.background, light.dark.background);
    assert_eq!(dark.light.background, light.light.background);
    assert_eq!(foreground_of(&dark.dark, true), foreground_of(&light.dark, true));
    assert_eq!(foreground_of(&dark.light, false), foreground_of(&light.light, false));
}

// ---------------------------------------------------------------------------
// Fresh colour vectors
// ---------------------------------------------------------------------------

#[test]
fn the_vscode_dark_and_dracula_slots_paint_the_react_hexes() {
    let vscode = preset_for("vscode-dark");
    // The VS Code dark+ family: the comment green (the italic pair is
    // the comment line, both painted by the comment slot).
    let (_, spans) = sample_token_spans(&vscode.dark, foreground_of(&vscode.dark, true));
    let italic_colours: Vec<rune::Color> = spans
        .iter()
        .filter(|span| span.italic)
        .filter_map(|span| span.color)
        .collect();
    assert_eq!(
        italic_colours,
        vec![hex("#6A9955"), hex("#6A9955")],
        "the comment line paints the VS Code comment green"
    );
    let dracula = preset_for("dracula");
    assert_eq!(background_of(&dracula.dark, true), hex("#282a36"));
    assert_eq!(background_of(&dracula.light, false), hex("#f8f8f2"));
}

#[test]
fn the_spans_tile_by_byte_range_with_the_leading_comment_pair_italic() {
    let preset = preset_for("solarized-light");
    let fg = foreground_of(&preset.light, false);
    let (source, spans) = sample_token_spans(&preset.light, fg);
    // The first two tokens are the comment line; every later span is
    // upright.
    assert!(spans[0].italic && spans[1].italic);
    assert!(!spans.iter().skip(2).any(|span| span.italic));
    // The tiling's byte arithmetic: each span ends where the next
    // begins, and the pair (2, 3) covers the comment's own text.
    assert_eq!(&source[spans[0].start as usize..spans[1].end as usize], "-- Sample query preview");
}

// ---------------------------------------------------------------------------
// The mounted tree
// ---------------------------------------------------------------------------

struct Scene {
    theme_id: String,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col()
        .key("probe-root")
        .w_full()
        .child(SqlSyntaxPreview::new(s.theme_id.clone(), s.dark).build())
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

#[test]
fn an_unknown_theme_mounts_the_default_palette() {
    // Through the mounted tree, not just the lookup: the settings key
    // may hold a stale id and the preview still paints (React's `??
    // SQL_SYNTAX_PRESETS[0]`).
    let rt = runtime(
        Scene { theme_id: "legacy-theme".to_owned(), dark: true },
        (700.0, 520.0),
    );
    let root = element(&rt, "sql-syntax-preview");
    assert_eq!(root.fill, hex("#1e1e1e"), "the fallback background");
    let pre = element(&rt, "sql-syntax-preview-pre");
    assert_eq!(pre.text_color, Some(hex("#d4d4d4")), "the fallback foreground");
}

#[test]
fn the_github_dark_preview_paints_its_own_surface_in_both_palettes() {
    for dark in [false, true] {
        let rt = runtime(
            Scene { theme_id: "github-dark".to_owned(), dark },
            (700.0, 520.0),
        );
        let root = element(&rt, "sql-syntax-preview");
        let pre = element(&rt, "sql-syntax-preview-pre");
        if dark {
            assert_eq!(root.fill, hex("#0d1117"));
            assert_eq!(pre.text_color, Some(hex("#c9d1d9")));
        } else {
            assert_eq!(root.fill, hex("#ffffff"));
            assert_eq!(pre.text_color, Some(hex("#24292f")));
        }
    }
}

#[test]
fn the_sample_wraps_at_a_narrow_surface_instead_of_clipping_wide() {
    // The React `<pre>` scrolls horizontally; the rune text wraps
    // inside the clipped rounded root, so every line stays visible
    // on a narrow surface too.
    let rt = runtime(
        Scene { theme_id: "monokai".to_owned(), dark: true },
        (240.0, 600.0),
    );
    let pre = element(&rt, "sql-syntax-preview-pre");
    let root = element(&rt, "sql-syntax-preview");
    let text = pre.text.clone().unwrap_or_default();
    assert!(text.contains("ORDER BY total DESC;"), "the whole query is one text");
    // The wrapped pre grows taller than the five 20.4pt lines.
    assert!(pre.rect[3] > 5.0 * 20.4 + 24.0, "the wrap grows the pre: {}", pre.rect[3]);
    // And it stays inside the clipped root.
    assert!(pre.rect[2] <= root.rect[2] + 0.5, "the pre stays inside the root");
}
