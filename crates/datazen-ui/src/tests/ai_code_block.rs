//! T060: `components/ai/AiCodeBlock.tsx` port — SQL detection, dialect
//! round-trip, labels, the fullscreen / dialect / copy state machine, and
//! the mounted block (inline + fullscreen, actions, window fit).

use rune::{Cx, El, Env, Runtime};

use crate::components::ai::ai_code_block::AiCodeBlock;
use crate::components::ai::ai_code_block_state::{
    apply_dialect_choice, code_block_label, display_label, is_sql_code_block, looks_like_sql,
    run_language, AiCodeBlockProps, AiCodeBlockState, SqlDialect, COPIED_FEEDBACK_MS,
};
use crate::i18n::{has, t};

const PREFIX: &str = "ai-code-block";

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

fn view_no_callbacks(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
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
    .build(scene, scene.dark, &mut layer);
    rune_kit::root(page, layer)
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
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
    .key("custom-code-block")
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

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn own_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(PREFIX))
        .collect()
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

// ---- pure helpers: dialect, SQL detection, labels ----

#[test]
fn every_dialect_round_trips_its_value_and_label() {
    for (dialect, value, label) in [
        (SqlDialect::PostgreSql, "postgresql", "PostgreSQL"),
        (SqlDialect::MySql, "mysql", "MySQL"),
        (SqlDialect::Sqlite, "sqlite", "SQLite"),
        (SqlDialect::MariaDb, "mariadb", "MariaDB"),
        (SqlDialect::SqlServer, "sqlserver", "SQL Server"),
        (SqlDialect::Oracle, "oracle", "Oracle"),
        (SqlDialect::ClickHouse, "clickhouse", "ClickHouse"),
    ] {
        assert_eq!(dialect.value(), value);
        assert_eq!(dialect.label(), label);
        assert_eq!(SqlDialect::from_value(value), Some(dialect));
    }
    assert_eq!(
        SqlDialect::ALL.len(),
        7,
        "React's SQL_DIALECTS has 7 entries"
    );
    assert_eq!(SqlDialect::from_value("postgres"), None);
    assert_eq!(SqlDialect::from_value(""), None);
    assert_eq!(SqlDialect::from_value("PostgreSQL"), None);
    assert_eq!(SqlDialect::default(), SqlDialect::PostgreSql);
}

#[test]
fn sql_detection_follows_the_language_tag_then_the_first_line() {
    // The tag alone decides, case-insensitively — React's `it.each` table.
    for language in [
        "postgresql",
        "mysql",
        "sqlite",
        "mariadb",
        "SQL",
        "Postgres",
        "SqL",
    ] {
        assert!(
            is_sql_code_block(language, "anything at all"),
            "{language} is a SQL tag"
        );
    }
    // No SQL tag: the content decides.
    assert!(is_sql_code_block("", "SELECT 42"));
    assert!(is_sql_code_block("text", "SELECT 42"));
    assert!(is_sql_code_block("python", "SELECT 1"));
    assert!(!is_sql_code_block("python", "print('hi')"));
    assert!(!is_sql_code_block("json", "{\"a\":1}"));
    assert!(!is_sql_code_block("", ""));
}

#[test]
fn looks_like_sql_matches_the_keyword_regex() {
    for code in [
        "SELECT 1",
        "  select * from t",
        "WITH x AS (SELECT 1) SELECT * FROM x",
        "insert into t values (1)",
        "UPDATE t SET a = 1",
        "DELETE FROM t",
        "CREATE TABLE t (a INT)",
        "ALTER TABLE t ADD COLUMN b TEXT",
        "DROP TABLE t",
        "EXPLAIN SELECT 1",
        "SHOW TABLES",
        "DESCRIBE t",
        "PRAGMA table_info(t)",
        "TRUNCATE TABLE t",
        "GRANT SELECT ON t TO u",
        "REVOKE SELECT ON t FROM u",
        "BEGIN",
        "COMMIT",
        "ROLLBACK",
        "SET search_path = public",
        "USE mydb",
        "VACUUM",
        "SELECT-1",
    ] {
        assert!(looks_like_sql(code), "{code:?} starts with a SQL keyword");
    }
    // Only the first line counts.
    assert!(!looks_like_sql("hello\nSELECT 1"));
    assert!(!looks_like_sql(""));
    assert!(!looks_like_sql("   "));
    // The word boundary matters: a longer word is not the keyword.
    assert!(!looks_like_sql("SELECTED * FROM t"));
    assert!(!looks_like_sql("SELECT_1"));
    assert!(!looks_like_sql("SETTING = 1"));
    assert!(!looks_like_sql("BEGINNING"));
    // A comment first is not a statement.
    assert!(!looks_like_sql("-- SELECT 1"));
    assert!(!looks_like_sql("/* SELECT 1 */"));
}

#[test]
fn the_header_label_is_the_tag_or_sql_or_code_uppercased() {
    assert_eq!(code_block_label("sql", true), "sql");
    assert_eq!(code_block_label("Python", false), "Python");
    assert_eq!(code_block_label("", true), "sql");
    assert_eq!(code_block_label("", false), "code");
    assert_eq!(display_label("sql", true), "SQL");
    assert_eq!(display_label("Python", false), "PYTHON");
    assert_eq!(display_label("", true), "SQL");
    assert_eq!(display_label("", false), "CODE");
}

#[test]
fn run_receives_the_tag_or_sql_when_the_tag_is_empty() {
    assert_eq!(run_language("sql"), "sql");
    assert_eq!(run_language("text"), "text");
    assert_eq!(run_language(""), "sql");
}

// ---- state machine: fullscreen, dialect, copy feedback ----

#[test]
fn state_seeds_the_dialect_and_falls_back_to_postgresql() {
    assert_eq!(
        AiCodeBlockState::new("mysql").selected_dialect,
        SqlDialect::MySql
    );
    assert_eq!(
        AiCodeBlockState::new("clickhouse").selected_dialect,
        SqlDialect::ClickHouse
    );
    assert_eq!(
        AiCodeBlockState::new("bogus").selected_dialect,
        SqlDialect::PostgreSql
    );
    assert_eq!(
        AiCodeBlockState::new("").selected_dialect,
        SqlDialect::PostgreSql
    );
    let state = AiCodeBlockState::new("sqlite");
    assert!(!state.fullscreen);
    assert!(!state.dialect_open);
    assert!(!state.copied);
    assert_eq!(state.copy_request, 0);
}

#[test]
fn fullscreen_and_dialect_toggles_flip_and_close() {
    let mut state = AiCodeBlockState::new("postgresql");
    state.toggle_fullscreen();
    assert!(state.fullscreen);
    state.toggle_fullscreen();
    assert!(!state.fullscreen);

    state.toggle_dialect_open();
    assert!(state.dialect_open);
    state.toggle_dialect_open();
    assert!(!state.dialect_open);

    state.toggle_dialect_open();
    state.set_dialect(SqlDialect::Oracle);
    assert_eq!(state.selected_dialect, SqlDialect::Oracle);
    assert!(!state.dialect_open, "choosing a dialect closes the listbox");

    state.toggle_dialect_open();
    state.close_dialect();
    assert!(!state.dialect_open);
}

#[test]
fn copy_feedback_is_optimistic_and_only_the_current_request_clears_it() {
    assert_eq!(COPIED_FEEDBACK_MS, 1500);
    let mut state = AiCodeBlockState::new("postgresql");
    let first = state.copy_pressed();
    assert_eq!(first, 1);
    assert!(state.copied);

    // A second click owns a full new window.
    let second = state.copy_pressed();
    assert_eq!(second, 2);
    assert!(state.copied);

    // The stale first timer / failure cannot clear the second confirmation.
    assert!(!state.expire_copy_feedback(first));
    assert!(state.copied);
    assert!(!state.fail_copy(first));
    assert!(state.copied);

    // The current request clears, by timer or by failure.
    assert!(state.expire_copy_feedback(second));
    assert!(!state.copied);
    let third = state.copy_pressed();
    assert!(state.fail_copy(third));
    assert!(!state.copied);
    // Expiring twice is harmless: the second expiry matches but has
    // nothing left to clear.
    assert!(state.expire_copy_feedback(third));
    assert!(!state.copied);
}

#[test]
fn dialect_choice_applies_known_values_and_ignores_unknown_ones() {
    let mut scene = Scene::sql();
    scene.state.toggle_dialect_open();
    apply_dialect_choice(
        std::rc::Rc::new(|s: &mut Scene, dialect| s.state.set_dialect(dialect)),
        std::rc::Rc::new(|s: &mut Scene| s.state.close_dialect()),
        &mut scene,
        "mysql",
    );
    assert_eq!(scene.state.selected_dialect, SqlDialect::MySql);
    assert!(!scene.state.dialect_open);

    scene.state.toggle_dialect_open();
    apply_dialect_choice(
        std::rc::Rc::new(|s: &mut Scene, dialect| s.state.set_dialect(dialect)),
        std::rc::Rc::new(|s: &mut Scene| s.state.close_dialect()),
        &mut scene,
        "postgres",
    );
    assert_eq!(
        scene.state.selected_dialect,
        SqlDialect::MySql,
        "an unknown value leaves the dialect alone"
    );
    assert!(!scene.state.dialect_open, "the listbox still closes");
}

#[test]
fn every_ai_code_block_key_resolves() {
    for key in [
        "chat.insertSql",
        "chat.copyCode",
        "chat.newQuery",
        "chat.runCode",
        "chat.fullscreen",
        "chat.exitFullscreen",
        "chat.selectDialect",
    ] {
        assert!(has(key), "{key} is missing from the locale packs");
        assert_ne!(t(key), key, "{key} echoes instead of translating");
    }
}

// ---- mounted: inline tree ----

#[test]
fn sql_block_renders_label_code_dialect_and_all_actions() {
    let rt = mount(Scene::sql(), (960.0, 640.0));
    assert!(has_key(&rt, PREFIX));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "SQL");
    assert_eq!(text_of(&rt, "ai-code-block-code"), "SELECT 1");
    assert!(has_key(&rt, "ai-code-block-dialect-field-trigger"));
    assert!(has_key(&rt, "ai-code-block-run"));
    assert!(has_key(&rt, "ai-code-block-insert"));
    assert!(has_key(&rt, "ai-code-block-new-query"));
    assert!(has_key(&rt, "ai-code-block-copy"));
    assert!(has_key(&rt, "ai-code-block-fullscreen-toggle"));
    assert_eq!(
        text_of(&rt, "ai-code-block-insert-label"),
        t("chat.insertSql")
    );
    // Icon-only buttons carry their title as the a11y label.
    assert_eq!(label_of(&rt, "ai-code-block-run"), t("chat.runCode"));
    assert_eq!(label_of(&rt, "ai-code-block-copy"), t("chat.copyCode"));
    assert_eq!(
        label_of(&rt, "ai-code-block-fullscreen-toggle"),
        t("chat.fullscreen")
    );
    assert_no_horizontal_overflow(&rt, 960.0, 640.0, "sql block");
}

#[test]
fn non_sql_block_hides_dialect_and_sql_only_actions() {
    let scene = Scene::with_props(AiCodeBlockProps::new("json", "{\"a\":1}"));
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, PREFIX));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "JSON");
    assert_eq!(text_of(&rt, "ai-code-block-code"), "{\"a\":1}");
    assert!(!has_key(&rt, "ai-code-block-dialect-field-trigger"));
    assert!(!has_key(&rt, "ai-code-block-run"));
    assert!(!has_key(&rt, "ai-code-block-insert"));
    assert!(!has_key(&rt, "ai-code-block-new-query"));
    assert!(has_key(&rt, "ai-code-block-copy"));
    assert!(has_key(&rt, "ai-code-block-fullscreen-toggle"));
}

#[test]
fn sql_looking_text_block_gets_sql_actions_and_an_empty_tag_label() {
    let scene = Scene::with_props(AiCodeBlockProps::new("text", "SELECT 42"));
    let rt = mount(scene, (960.0, 640.0));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "TEXT");
    assert!(has_key(&rt, "ai-code-block-dialect-field-trigger"));
    assert!(has_key(&rt, "ai-code-block-run"));
    let untagged = Scene::with_props(AiCodeBlockProps::new("", "SELECT 42"));
    let rt = mount(untagged, (960.0, 640.0));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "SQL");
}

#[test]
fn run_insert_and_new_query_report_the_code_and_language() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-run");
    click(&mut rt, "ai-code-block-insert");
    click(&mut rt, "ai-code-block-new-query");
    assert_eq!(
        rt.state.runs,
        vec![("SELECT 1".to_string(), "sql".to_string())]
    );
    assert_eq!(rt.state.inserts, vec!["SELECT 1".to_string()]);
    assert_eq!(rt.state.new_queries, vec!["SELECT 1".to_string()]);
}

#[test]
fn run_with_an_empty_tag_reports_sql_as_the_language() {
    let scene = Scene::with_props(AiCodeBlockProps::new("", "SELECT 42"));
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "ai-code-block-run");
    assert_eq!(
        rt.state.runs,
        vec![("SELECT 42".to_string(), "sql".to_string())]
    );

    let scene = Scene::with_props(AiCodeBlockProps::new("text", "SELECT 42"));
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "ai-code-block-run");
    assert_eq!(
        rt.state.runs,
        vec![("SELECT 42".to_string(), "text".to_string())]
    );
}

#[test]
fn sql_only_buttons_are_absent_without_their_callbacks() {
    let rt = mount_with(Scene::sql(), (960.0, 640.0), view_no_callbacks);
    assert!(!has_key(&rt, "ai-code-block-run"));
    assert!(!has_key(&rt, "ai-code-block-insert"));
    assert!(!has_key(&rt, "ai-code-block-new-query"));
    // Copy without a host renders but does nothing.
    assert!(has_key(&rt, "ai-code-block-copy"));
    let mut rt = mount_with(Scene::sql(), (960.0, 640.0), view_no_callbacks);
    click(&mut rt, "ai-code-block-copy");
    assert!(!rt.state.state.copied, "no host, no claimed copy");
    assert!(rt.state.copies.is_empty());
    assert!(has_key(&rt, "ai-code-block-fullscreen-toggle"));
}

#[test]
fn copy_marks_copied_and_reports_code_with_the_request_id() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-copy");
    assert!(rt.state.state.copied);
    assert_eq!(rt.state.copies, vec![("SELECT 1".to_string(), 1)]);
    click(&mut rt, "ai-code-block-copy");
    assert_eq!(
        rt.state.copies,
        vec![("SELECT 1".to_string(), 1), ("SELECT 1".to_string(), 2),]
    );
    assert_eq!(rt.state.state.copy_request, 2);
}

#[test]
fn streaming_hides_the_actions_and_show_actions_false_too() {
    let scene = Scene::with_props(AiCodeBlockProps::new("sql", "SELECT 1").is_streaming(true));
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, PREFIX));
    assert!(!has_key(&rt, "ai-code-block-copy"));
    assert!(!has_key(&rt, "ai-code-block-insert"));
    assert!(!has_key(&rt, "ai-code-block-run"));
    assert!(!has_key(&rt, "ai-code-block-fullscreen-toggle"));
    assert!(has_key(&rt, "ai-code-block-dialect-field-trigger"));

    let scene = Scene::with_props(AiCodeBlockProps::new("sql", "SELECT 1").show_actions(false));
    let rt = mount(scene, (960.0, 640.0));
    assert!(!has_key(&rt, "ai-code-block-actions"));
    assert!(has_key(&rt, "ai-code-block-label"));
}

// ---- mounted: dialect select ----

#[test]
fn dialect_select_opens_and_choosing_mysql_updates_the_state() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    assert!(!has_key(&rt, "ai-code-block-dialect-field-content"));
    click(&mut rt, "ai-code-block-dialect-field-trigger");
    assert!(rt.state.state.dialect_open);
    assert!(has_key(&rt, "ai-code-block-dialect-field-content"));
    // The listbox rows are `option0`… in dialect order; MySQL is second.
    click(&mut rt, "option1");
    assert_eq!(rt.state.state.selected_dialect, SqlDialect::MySql);
    assert!(!rt.state.state.dialect_open);
    assert!(!has_key(&rt, "ai-code-block-dialect-field-content"));
}

#[test]
fn dialect_seed_comes_from_the_sql_dialect_prop() {
    let scene =
        Scene::with_props(AiCodeBlockProps::new("sql", "SELECT 1").sql_dialect(SqlDialect::Oracle));
    let rt = mount(scene, (960.0, 640.0));
    assert_eq!(rt.state.state.selected_dialect, SqlDialect::Oracle);
}

// ---- mounted: fullscreen ----

#[test]
fn fullscreen_toggle_swaps_the_tree_and_back() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert!(rt.state.state.fullscreen);
    assert!(has_key(&rt, "ai-code-block-fullscreen"));
    assert!(!has_key(&rt, PREFIX));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "SQL");
    assert_eq!(text_of(&rt, "ai-code-block-code"), "SELECT 1");
    assert_eq!(
        label_of(&rt, "ai-code-block-fullscreen-toggle"),
        t("chat.exitFullscreen")
    );
    // Fullscreen shows the text beside the icons.
    assert!(texts(&rt).iter().any(|text| text == t("chat.runCode")));
    assert!(texts(&rt).iter().any(|text| text == t("chat.copyCode")));
    assert!(texts(&rt).iter().any(|text| text == t("chat.newQuery")));
    assert!(texts(&rt).iter().any(|text| text == t("chat.insertSql")));
    assert!(has_key(&rt, "ai-code-block-dialect-field-trigger"));

    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert!(!rt.state.state.fullscreen);
    assert!(has_key(&rt, PREFIX));
    assert!(!has_key(&rt, "ai-code-block-fullscreen"));
}

#[test]
fn fullscreen_actions_work_and_copy_hides_its_label_while_copied() {
    let mut rt = mount(Scene::sql(), (960.0, 640.0));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    click(&mut rt, "ai-code-block-run");
    click(&mut rt, "ai-code-block-insert");
    assert_eq!(
        rt.state.runs,
        vec![("SELECT 1".to_string(), "sql".to_string())]
    );
    assert_eq!(rt.state.inserts, vec!["SELECT 1".to_string()]);

    assert!(texts(&rt).iter().any(|text| text == t("chat.copyCode")));
    click(&mut rt, "ai-code-block-copy");
    assert!(rt.state.state.copied);
    assert!(
        !texts(&rt).iter().any(|text| text == t("chat.copyCode")),
        "React renders an empty string for the copy label while copied"
    );
    assert_eq!(rt.state.copies, vec![("SELECT 1".to_string(), 1)]);
}

#[test]
fn non_sql_block_fullscreens_with_the_plain_pre() {
    let scene = Scene::with_props(AiCodeBlockProps::new("python", "print('hi')"));
    let mut rt = mount(scene, (960.0, 640.0));
    click(&mut rt, "ai-code-block-fullscreen-toggle");
    assert!(has_key(&rt, "ai-code-block-fullscreen"));
    assert_eq!(text_of(&rt, "ai-code-block-label"), "PYTHON");
    assert_eq!(text_of(&rt, "ai-code-block-code"), "print('hi')");
    assert!(!has_key(&rt, "ai-code-block-dialect-field-trigger"));
    assert!(has_key(&rt, "ai-code-block-copy"));
    assert_no_horizontal_overflow(&rt, 960.0, 640.0, "non-sql fullscreen");
}

#[test]
fn fullscreen_still_shows_actions_while_streaming() {
    let scene = Scene::with_props(AiCodeBlockProps::new("sql", "SELECT 1").is_streaming(true));
    let mut rt = mount(scene, (960.0, 640.0));
    assert!(!has_key(&rt, "ai-code-block-copy"));
    rt.state.state.fullscreen = true;
    // The toggle happened outside a handler, so the runtime needs an
    // explicit invalidate before the next frame re-runs the view.
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    assert!(has_key(&rt, "ai-code-block-fullscreen"));
    assert!(has_key(&rt, "ai-code-block-copy"));
    assert!(has_key(&rt, "ai-code-block-run"));
}

// ---- mounted: keys, fit ----

#[test]
fn custom_key_prefixes_every_part() {
    let rt = mount_with(Scene::sql(), (960.0, 640.0), view_custom_key);
    assert!(has_key(&rt, "custom-code-block"));
    assert!(has_key(&rt, "custom-code-block-copy"));
    assert!(!has_key(&rt, PREFIX));
}

#[test]
fn the_block_fits_small_and_large_windows_in_both_themes() {
    for (w, h, dark) in [
        (480.0, 320.0, true),
        (960.0, 640.0, true),
        (960.0, 640.0, false),
    ] {
        let mut scene = Scene::sql();
        scene.dark = dark;
        let rt = mount(scene, (w, h));
        assert_no_horizontal_overflow(&rt, w, h, "inline block");
        let _ = element(&rt, PREFIX);
    }
    // Fullscreen is only fitted at 960: React's fullscreen header is a
    // no-wrap flex row (label + 128pt select + five actions, ~552pt), so
    // it overflows a 480pt viewport in React too — the port does not
    // reflow what React does not reflow.
    for dark in [true, false] {
        let mut scene = Scene::sql();
        scene.dark = dark;
        scene.state.fullscreen = true;
        let rt = mount(scene, (960.0, 640.0));
        assert_no_horizontal_overflow(&rt, 960.0, 640.0, "fullscreen block");
        let _ = element(&rt, "ai-code-block-fullscreen");
    }
    // The keys a caller needs are all present, none duplicated.
    let rt = mount(Scene::sql(), (960.0, 640.0));
    let keys = own_keys(&rt);
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(keys.len(), sorted.len(), "duplicate keys: {keys:?}");
}
