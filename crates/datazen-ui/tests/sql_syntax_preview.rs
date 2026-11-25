//! T241: the SQL syntax preview's coder tests — the preset lookup (all
//! seven ids, the unknown-id fallback), the background / foreground
//! fallbacks (a preset without them, a preset with them), the
//! one-text span construction (the full sample text, the 52 covering
//! spans with no gaps, the comment italics, the per-slot colours), and
//! the mounted preview's palette-backed geometry and fill in both
//! palettes.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::sql_syntax_preview::{
    background_of, foreground_of, preset_for, sample_token_spans, slot_of,
    SqlSyntaxPreview, SQL_SYNTAX_PRESETS,
};
use datazen_ui::ui::kit::kit;

fn hex(color: &str) -> rune::Color {
    rune::color::hex(color).unwrap_or_else(|| panic!("{color} is a hex colour"))
}

// ---------------------------------------------------------------------------
// The preset lookup
// ---------------------------------------------------------------------------

#[test]
fn all_seven_presets_resolve_by_id() {
    for id in [
        "default",
        "vscode-dark",
        "monokai",
        "solarized-dark",
        "solarized-light",
        "dracula",
        "github-dark",
    ] {
        let preset = preset_for(id);
        assert_eq!(preset.id, id, "{id} resolves to itself");
    }
    assert_eq!(SQL_SYNTAX_PRESETS.len(), 7);
}

#[test]
fn an_unknown_id_falls_back_to_the_first_preset() {
    // React: `?? SQL_SYNTAX_PRESETS[0]` — the default palette, not an
    // error (the settings key may hold anything).
    assert_eq!(preset_for("no-such-theme").id, "default");
    assert_eq!(preset_for("").id, "default");
}

// ---------------------------------------------------------------------------
// The palette fallbacks
// ---------------------------------------------------------------------------

#[test]
fn a_preset_without_background_or_foreground_takes_the_react_fallbacks() {
    let default = preset_for("default");
    // React: `palette.background ?? (dark ? '#1e1e1e' : '#ffffff')`.
    assert_eq!(hex("#1e1e1e"), background_of(&default.dark, true));
    assert_eq!(hex("#ffffff"), background_of(&default.light, false));
    // React: `palette.foreground ?? (dark ? '#d4d4d4' : '#000000')`.
    assert_eq!(hex("#d4d4d4"), foreground_of(&default.dark, true));
    assert_eq!(hex("#000000"), foreground_of(&default.light, false));
}

#[test]
fn a_preset_with_background_and_foreground_uses_them() {
    let monokai = preset_for("monokai");
    assert_eq!(hex("#272822"), background_of(&monokai.dark, true));
    assert_eq!(hex("#fff"), background_of(&monokai.light, false));
    assert_eq!(hex("#f8f8f2"), foreground_of(&monokai.dark, true));
    assert_eq!(hex("#333333"), foreground_of(&monokai.light, false));
}

#[test]
fn a_missing_slot_falls_back_to_the_foreground() {
    // The shipped presets carry all ten slots, so the fallback arm needs
    // a palette that lacks one — an empty-hex slot models the missing
    // entry the `?? fg` contract covers.
    let preset = preset_for("default");
    let fg = foreground_of(&preset.dark, true);
    assert_eq!(slot_of(&preset.dark, "keyword", fg), hex("#93c5fd"));
    // The unknown slot name is the fallback arm's other visitor.
    assert_eq!(slot_of(&preset.dark, "not-a-slot", fg), fg);
}

// ---------------------------------------------------------------------------
// The one-text span construction
// ---------------------------------------------------------------------------

#[test]
fn the_sample_text_is_the_react_query_verbatim() {
    let preset = preset_for("default");
    let fg = foreground_of(&preset.dark, true);
    let (source, spans) = sample_token_spans(&preset.dark, fg);
    assert_eq!(
        source,
        "-- Sample query preview\nSELECT u.name, COUNT(o.id) AS total\nFROM users u\nWHERE u.status = 'active' AND u.age > 18\nORDER BY total DESC;"
    );
    assert_eq!(spans.len(), 53, "one span per React token");
}

#[test]
fn the_spans_cover_the_text_with_no_gaps_or_overlaps() {
    let preset = preset_for("dracula");
    let fg = foreground_of(&preset.dark, true);
    let (source, spans) = sample_token_spans(&preset.dark, fg);
    assert_eq!(spans[0].start as usize, 0);
    for window in spans.windows(2) {
        assert_eq!(window[0].end, window[1].start, "the spans tile the text");
    }
    assert_eq!(
        spans.last().unwrap().end as usize,
        source.len(),
        "the last span closes the text"
    );
    for span in &spans {
        assert!(span.color.is_some(), "every token is coloured");
    }
}

#[test]
fn only_the_comment_tokens_are_italic() {
    let preset = preset_for("monokai");
    let fg = foreground_of(&preset.dark, true);
    let (source, spans) = sample_token_spans(&preset.dark, fg);
    let italics: Vec<&str> = spans
        .iter()
        .filter(|span| span.italic)
        .map(|span| &source[span.start as usize..span.end as usize])
        .collect();
    assert_eq!(
        italics,
        vec!["-- ", "Sample query preview"],
        "React's `fontStyle: italic` is the comment tokens only"
    );
}

#[test]
fn the_token_colours_read_the_palette_slots() {
    let preset = preset_for("github-dark");
    let fg = foreground_of(&preset.dark, true);
    let (source, spans) = sample_token_spans(&preset.dark, fg);

    let token = |text: &str| {
        spans
            .iter()
            .find(|span| &source[span.start as usize..span.end as usize] == text)
            .unwrap_or_else(|| panic!("{text} is a token"))
    };

    // The keyword, the dotted member, the literal, the operator.
    assert_eq!(token("SELECT").color, Some(hex("#ff7b72")));
    assert_eq!(token("name").color, Some(hex("#d2a8ff")));
    assert_eq!(token("u").color, Some(hex("#c9d1d9")));
    assert_eq!(token("'active'").color, Some(hex("#a5d6ff")));
    assert_eq!(token("=").color, Some(hex("#ff7b72")));
    // The foreground tokens (the spaces) carry the pre's own colour.
    assert_eq!(token(" ").color, Some(fg));
}

// ---------------------------------------------------------------------------
// The mounted preview
// ---------------------------------------------------------------------------

struct Probe {
    theme_id: String,
    dark: bool,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .key("probe-root")
        .w_full()
        .child(SqlSyntaxPreview::new(s.theme_id.clone(), s.dark).build())
}

fn runtime(theme_id: &str, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe { theme_id: theme_id.to_owned(), dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
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
fn the_mounted_preview_paints_the_preset_background_and_foreground() {
    // The monokai preview in the dark palette: the root fills with the
    // preset's own background, the pre reads the preset's foreground.
    let rt = runtime("monokai", true, (800.0, 600.0));
    let root = element(&rt, "sql-syntax-preview");
    assert_eq!(root.fill, hex("#272822"), "the preset background fills the root");
    let pre = element(&rt, "sql-syntax-preview-pre");
    assert_eq!(pre.text_color, Some(hex("#f8f8f2")));

    // The default preview in the light palette: the fallback pair.
    let rt = runtime("default", false, (800.0, 600.0));
    let root = element(&rt, "sql-syntax-preview");
    assert_eq!(root.fill, hex("#ffffff"));
    let pre = element(&rt, "sql-syntax-preview-pre");
    assert_eq!(pre.text_color, Some(hex("#000000")));
}

#[test]
fn the_mounted_sample_text_matches_the_react_query() {
    for (theme, dark) in [("vscode-dark", true), ("solarized-light", false)] {
        let rt = runtime(theme, dark, (800.0, 600.0));
        let pre = element(&rt, "sql-syntax-preview-pre");
        let text = pre.text.clone().unwrap_or_default();
        assert!(
            text.contains("SELECT u.name, COUNT(o.id) AS total"),
            "the sample SQL is the React query, {theme} dark={dark}"
        );
        assert!(
            text.contains("WHERE u.status = 'active' AND u.age > 18"),
            "the WHERE clause is intact, {theme} dark={dark}"
        );
    }
}

#[test]
fn the_root_keeps_react_paddings_and_the_pre_paddings() {
    let size = (800.0, 600.0);
    for dark in [false, true] {
        let rt = runtime("monokai", dark, size);
        let root = element(&rt, "sql-syntax-preview");
        // React's `mt-2`: the strip starts 8 points down the probe.
        assert_eq!(root.rect[1], 8.0, "mt-2, dark={dark}");
        // React's `rounded-lg`.
        assert_eq!(root.radius, kit(dark).radius.lg, "rounded-lg, dark={dark}");

        // React's `px-4 py-3`: the padding rides inside the pre's own
        // box (the element rect spans the padding), so the py shows as
        // the box growing past its five 20.4pt lines, and the root's
        // border rides on top without taking room.
        let pre = element(&rt, "sql-syntax-preview-pre");
        let lines = 5.0 * 20.4;
        assert!(
            pre.rect[3] > lines + 23.0 && pre.rect[3] < lines + 25.1,
            "py-3 (24pt) over five 20.4pt lines: {} dark={dark}",
            pre.rect[3]
        );
        assert!(
            (root.rect[3] - pre.rect[3]).abs() < 2.0,
            "the border rides without room, dark={dark}"
        );
    }
}

#[test]
fn the_strip_stays_inside_the_surface() {
    let size = (360.0, 480.0);
    for (theme, dark) in [("dracula", true), ("default", false)] {
        let rt = runtime(theme, dark, size);
        let root = element(&rt, "sql-syntax-preview");
        assert!(
            root.rect[0] >= 0.0 && root.rect[2] <= size.0,
            "the rounded clip keeps the strip inside, {theme} dark={dark}"
        );
    }
}
