//! T060 audit: independent checks the coder tests do not cover.
//!
//! Long-line overflow (React scrolls: `overflow-x-auto` inline, CodeMirror
//! scroll fullscreen), `looks_like_sql` edges (tab / CRLF / FEFF / the
//! `postgres` tag-vs-dialect distinction), dialect survival across the
//! fullscreen swap, the stale-expiry guard in a mounted scene, and a pin of
//! the one rendering deviation found (fullscreen toggle text — see the
//! `audit_fullscreen_toggle_shows_text_label_unlike_react` test).

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::ai::ai_code_block::AiCodeBlock;
use datazen_ui::components::ai::ai_code_block_state::{
    is_sql_code_block, looks_like_sql, AiCodeBlockProps, AiCodeBlockState, SqlDialect,
};
use datazen_ui::i18n::t;

const PREFIX: &str = "ai-code-block";
const LONG_SQL: &str =
    "SELECT very_long_column_name_1, very_long_column_name_2, very_long_column_name_3 FROM some_very_long_table_name WHERE some_condition = 'some very long value' ORDER BY created_at DESC LIMIT 100";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct Scene {
    props: AiCodeBlockProps,
    state: AiCodeBlockState,
    inserts: Vec<String>,
    runs: Vec<(String, String)>,
    new_queries: Vec<String>,
    copies: Vec<(String, u64)>,
    dark: bool,
}

impl Scene {
    fn sql() -> Self {
        Self {
            props: AiCodeBlockProps::new("sql", "SELECT 1"),
            state: AiCodeBlockState::new("postgresql"),
            dark: true,
            ..Self::default()
        }
    }

    fn with_props(props: AiCodeBlockProps) -> Self {
        let dialect = props.sql_dialect.value().to_owned();
        Self {
            props,
            state: AiCodeBlockState::new(&dialect),
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let page = AiCodeBlock::<Scene>::new(
        scene.props.clone(),
        |s: &Scene| s.state.clone(),
        |s: &mut Scene| s.state.toggle_fullscreen(),
        |s: &mut Scene, dialect| s.state.set_dialect(dialect),
        |s: &mut Scene| s.state.toggle_dialect_open(),
        |s: &mut Scene| s.state.close_dialect(),
        |s: &mut Scene| s.state.copy_pressed(),
    )
    .on_insert_sql(|s: &mut Scene, code| s.inserts.push(code))
    .on_run_code(|s: &mut Scene, code, language| s.runs.push((code, language)))
    .on_new_query(|s: &mut Scene, code| s.new_queries.push(code))
    .on_copy(|s: &mut Scene, code, request| s.copies.push((code, request)))
    .build(scene, scene.dark, &mut layer);
    rune_kit::root(page, layer)
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.label).unwrap_or_default()
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.text)
        .collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !key.starts_with(PREFIX) {
            continue;
        }
        let [x, _y, ew, _eh] = e.rect;
        if x < -0.5 || x + ew > w + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- audit: inline icon-only buttons ----

#[test]
fn audit_inline_icon_only_buttons_have_no_text_label_nodes() {
    let rt = mount(Scene::sql(), (960.0, 640.0));
    // Insert is text-only in both modes (React renders the label string).
    assert_eq!(
        text_of(&rt, "ai-code-block-insert-label"),
        t("chat.insertSql")
    );
    // Run / new-query / copy / fullscreen-toggle are icon-only inline:
    // React shows only the glyph plus a native `title` tooltip.
    for action in ["run", "new-query", "copy", "fullscreen-toggle"] {
        assert!(
            !has_key(&rt, &format!("ai-code-block-{action}-label")),
            "inline {action} must not mount a text label node"
        );
    }
    // Their `title` survives as the a11y label.
    assert_eq!(label_of(&rt, "ai-code-block-run"), t("chat.runCode"));
    assert_eq!(
        label_of(&rt, "ai-code-block-new-query"),
        t("chat.newQuery")
    );
    assert_eq!(label_of(&rt, "ai-code-block-copy"), t("chat.copyCode"));
    assert_eq!(
        label_of(&rt, "ai-code-block-fullscreen-toggle"),
        t("chat.fullscreen")
    );
}

#[test]
fn audit_fullscreen_text_labels_match_react_for_four_of_five_actions() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    for (action, key) in [
        ("run", "chat.runCode"),
        ("insert", "chat.insertSql"),
        ("new-query", "chat.newQuery"),
        ("copy", "chat.copyCode"),
    ] {
        assert_eq!(
            text_of(&rt, &format!("ai-code-block-{action}-label")),
            t(key),
            "fullscreen {action} shows its text label, as React does"
        );
    }
}

// ---- audit: the one rendering deviation found ----

#[test]
fn audit_fullscreen_toggle_shows_text_label_unlike_react() {
    // KNOWN NON-BLOCKING DEVIATION (audit finding #1): React's fullscreen
    // toggle is icon-only in BOTH modes (`AiCodeBlock.tsx:131-139` renders
    // only `<Minimize2/>` with `title={t('chat.exitFullscreen')}`), but
    // Rune mounts a visible "Exit Fullscreen" text node next to the icon
    // (`ai_code_block.rs`: `toggle_text = fullscreen.then(...)`). This test
    // pins the current Rune behaviour so the fix-or-document decision is
    // visible: if the coder removes the text, flip this test to assert
    // the label node is absent.
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert_eq!(
        text_of(&rt, "ai-code-block-fullscreen-toggle-label"),
        t("chat.exitFullscreen"),
        "current Rune behaviour: fullscreen toggle shows text"
    );
    assert_eq!(
        label_of(&rt, "ai-code-block-fullscreen-toggle"),
        t("chat.exitFullscreen")
    );
}

// ---- audit: long lines ----

#[test]
fn audit_long_sql_line_does_not_overflow_inline() {
    let scene = Scene::with_props(AiCodeBlockProps::new("sql", LONG_SQL));
    let rt = mount(scene, (480.0, 320.0));
    assert_eq!(text_of(&rt, "ai-code-block-code"), LONG_SQL);
    assert_no_horizontal_overflow(&rt, 480.0, 320.0, "long sql inline 480");
    let scene = Scene::with_props(AiCodeBlockProps::new("sql", LONG_SQL));
    let rt = mount(scene, (960.0, 640.0));
    assert_no_horizontal_overflow(&rt, 960.0, 640.0, "long sql inline 960");
}

#[test]
fn audit_long_sql_line_fullscreen_at_960() {
    let mut scene = Scene::with_props(AiCodeBlockProps::new("sql", LONG_SQL));
    scene.state.fullscreen = true;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "ai-code-block-fullscreen"));
    assert_eq!(text_of(&rt, "ai-code-block-code"), LONG_SQL);
    assert_no_horizontal_overflow(&rt, 960.0, 640.0, "long sql fullscreen 960");
}

// ---- audit: SQL detection edges ----

#[test]
fn audit_sql_detection_edges_tab_crlf_and_first_line_only() {
    // A tab after the keyword is still a word boundary (`\b`).
    assert!(looks_like_sql("SELECT\t1"));
    // CRLF: `lines()` strips the `\r`; React's `split('\n')[0]` keeps it
    // but the regex matches the prefix either way.
    assert!(looks_like_sql("SELECT 1\r\nFROM t"));
    // A keyword on the second line never counts, even after blank lines.
    assert!(!looks_like_sql("\nSELECT 1"));
    assert!(!looks_like_sql(" \n SELECT 1"));
    // A non-ASCII letter after the keyword is not a word char, so `\b`
    // matches — same as React.
    assert!(looks_like_sql("SELECT\u{e9} FROM t"));
    // A digit immediately after the keyword defeats the boundary.
    assert!(!looks_like_sql("SELECT1"));
}

#[test]
fn audit_bom_prefixed_first_line_is_not_sql_in_rune() {
    // NON-BLOCKING DIVERGENCE (audit finding #2): JS `trim()` strips a
    // leading U+FEFF, so React's `looksLikeSql("\u{FEFF}SELECT 1")` is
    // true; Rust `trim_start()` (Unicode White_Space, which excludes
    // U+FEFF) leaves it, so the leading run is empty and this is false.
    // Pins current Rune behaviour; a BOM-prefixed fence is vanishingly
    // rare in AI output (cf. the T054 IANA FEFF residual, same cause).
    assert!(!looks_like_sql("\u{feff}SELECT 1"));
}

#[test]
fn audit_postgres_tag_is_sql_but_not_a_dialect_value() {
    // React's SQL-tag set includes `postgres`, but the seven-option
    // dialect select has no `postgres` value — the two vocabularies differ.
    assert!(is_sql_code_block("postgres", "print('hi')"));
    assert_eq!(SqlDialect::from_value("postgres"), None);
    // The dialect seed still falls back to PostgreSQL (deviation (9)).
    assert_eq!(
        AiCodeBlockState::new("postgres").selected_dialect,
        SqlDialect::PostgreSql
    );
}

#[test]
fn audit_empty_code_block_renders_with_code_label() {
    let scene = Scene::with_props(AiCodeBlockProps::new("", ""));
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, PREFIX));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "CODE");
    assert_eq!(text_of(&rt, "ai-code-block-code"), "");
    assert!(!has_key(&rt, "ai-code-block-run"));
    assert!(has_key(&rt, "ai-code-block-copy"));
}

// ---- audit: state survival + host contract in a mounted scene ----

#[test]
fn audit_dialect_survives_the_fullscreen_swap() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-dialect-field-trigger");
    click(&mut rt, "option1");
    assert_eq!(rt.state.state.selected_dialect, SqlDialect::MySql);
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert_eq!(rt.state.state.selected_dialect, SqlDialect::MySql);
    assert!(has_key(&rt, "ai-code-block-dialect-field-trigger"));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert_eq!(rt.state.state.selected_dialect, SqlDialect::MySql);
}

#[test]
fn audit_show_actions_false_fullscreen_still_shows_actions() {
    let mut scene =
        Scene::with_props(AiCodeBlockProps::new("sql", "SELECT 1").show_actions(false));
    scene.state.fullscreen = true;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "ai-code-block-fullscreen"));
    assert!(has_key(&rt, "ai-code-block-copy"));
    assert!(has_key(&rt, "ai-code-block-run"));
    assert!(has_key(&rt, "ai-code-block-fullscreen-toggle"));
}

#[test]
fn audit_stale_expiry_cannot_clear_a_newer_copy_in_a_mounted_scene() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-copy");
    assert!(rt.state.state.copied);
    click(&mut rt, "ai-code-block-copy");
    assert_eq!(rt.state.state.copy_request, 2);
    // The first request's timer firing now must not clear the second
    // confirmation (the host reports expiry per request id).
    assert!(!rt.state.state.expire_copy_feedback(1));
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.state.copied);
    assert!(rt.state.state.expire_copy_feedback(2));
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.state.copied);
}
