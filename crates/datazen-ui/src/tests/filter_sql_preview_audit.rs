//! T135 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped `filter_sql_preview.rs` tests do not
//! pin: copy-all composition in the no-count case, the count-kind i18n label for
//! every strategy, the closed-dialog mount rule, React's falsy empty error
//! string, and an independent rect observation under long SQL at both sizes
//! and palettes. All assertions read the runtime element tree / memory
//! clipboard, never a value the component writes for the test.

use rune::{clipboard, col, Cx, El, Env, Runtime, Tx};

use crate::components::filter::filter_sql_preview_dialog::{
    CountStrategy, FilterSqlPreviewDialog, TableReadPreviewSql,
};
use crate::i18n::t;

const ROOT: &str = "filter-sql-preview-dialog";

#[derive(Default, Clone)]
struct AuditState {
    preview: Option<TableReadPreviewSql>,
    error: Option<String>,
    open: bool,
    copied_calls: usize,
}

fn preview_sql(count: Option<&str>, kind: CountStrategy) -> TableReadPreviewSql {
    TableReadPreviewSql {
        data_sql: "SELECT \"id\" FROM \"users\" WHERE \"status\" = 'active' LIMIT 50 OFFSET 0;"
            .to_owned(),
        count_sql: count.map(str::to_owned),
        exact_count_sql: "SELECT COUNT(*) FROM \"users\" WHERE \"status\" = 'active';".to_owned(),
        count_kind: kind,
    }
}

fn view(state: &AuditState, cx: &mut Cx<AuditState>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    FilterSqlPreviewDialog::<AuditState>::new()
        .open(state.open)
        .preview(state.preview.as_ref())
        .error(state.error.as_deref())
        .on_close(|_s: &mut AuditState| {})
        .on_copy_tx(|s: &mut AuditState, _tx: &mut Tx| s.copied_calls += 1)
        .build(dark, &mut layer);
    rune_kit::root(col().w_full().h_full(), layer)
}

fn runtime(state: AuditState, size: (f32, f32), dark: bool) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn mounted<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// 1. React composes `-- label\nsql` blocks joined by a blank line. With no
//    countSql the exact text has exactly two blocks (data + exact count).
#[test]
fn copy_all_without_a_count_statement_writes_exactly_two_labelled_blocks() {
    let p = preview_sql(None, CountStrategy::Unsupported);
    let clip = clipboard::memory();
    let mut rt = runtime(
        AuditState { preview: Some(p.clone()), open: true, ..AuditState::default() },
        (1440.0, 816.0),
        true,
    );
    rt.use_clipboard(&clip);
    rt.frame(0.0);
    click(&mut rt, "filter-sql-preview-copy");

    let expected = format!(
        "-- {}\n{}\n\n-- {}\n{}",
        t("filter.previewSqlData"),
        p.data_sql,
        t("filter.previewSqlExactCount"),
        p.exact_count_sql,
    );
    assert_eq!(
        clip.text().as_deref().map(String::as_str),
        Some(expected.as_str()),
        "no count block when countSql is None: exactly data + exact count joined by a blank line"
    );
    assert_eq!(rt.state.copied_calls, 1);
}

// 2. The count label interpolates `filter.previewSqlKind.<kind>` for every
//    strategy — the shipped test only pins the `exact` case.
#[test]
fn count_label_names_the_i18n_kind_for_every_strategy() {
    for (kind, key) in [
        (CountStrategy::Exact, "filter.previewSqlKind.exact"),
        (CountStrategy::Estimated, "filter.previewSqlKind.estimated"),
        (CountStrategy::Unsupported, "filter.previewSqlKind.unsupported"),
    ] {
        let p = preview_sql(Some("SELECT COUNT(*) FROM \"users\";"), kind);
        let rt = runtime(
            AuditState { preview: Some(p), open: true, ..AuditState::default() },
            (960.0, 640.0),
            true,
        );
        let expected = format!("{} · {}", t("filter.previewSqlCount"), t(key));
        assert_eq!(text_of(&rt, "filter-sql-preview-count-label"), expected, "kind key {key}");
    }
}

// 3. React's `<Dialog open={open}>` mounts nothing when closed.
#[test]
fn a_closed_dialog_mounts_no_keys_at_all() {
    let rt = runtime(
        AuditState {
            preview: Some(preview_sql(None, CountStrategy::Exact)),
            open: false,
            ..AuditState::default()
        },
        (960.0, 640.0),
        true,
    );
    for key in [
        ROOT,
        "filter-sql-preview-statements",
        "filter-sql-preview-data",
        "filter-sql-preview-copy",
        "filter-sql-preview-close",
    ] {
        assert!(!mounted(&rt, key), "closed dialog must not mount {key}");
    }
}

// 4. React uses `error && ...` (empty string is falsy) and
//    `!preview && !error` for loading — an empty error shows the placeholder.
#[test]
fn an_empty_error_string_is_falsy_and_shows_the_loading_placeholder() {
    let rt = runtime(
        AuditState { error: Some(String::new()), open: true, ..AuditState::default() },
        (960.0, 640.0),
        true,
    );
    assert!(!mounted(&rt, "filter-sql-preview-error"), "empty error renders no banner");
    assert!(mounted(&rt, "filter-sql-preview-loading"));
    assert_eq!(text_of(&rt, "filter-sql-preview-loading"), t("filter.previewSqlLoading"));
}

// 5. Independent rect observation with deliberately long, unbroken SQL and a
//    long error, at both sizes and palettes: nothing may cross the window and
//    the footer must stay inside the panel.
#[test]
fn long_sql_and_error_stay_inside_the_window_at_both_sizes_and_palettes() {
    let long = format!("{}x", "SELECT \"id\" FROM \"users\" WHERE \"status\" = 'active' AND ".repeat(12));
    let scenarios = [
        (
            "preview",
            AuditState {
                preview: Some(TableReadPreviewSql {
                    data_sql: long.clone(),
                    count_sql: Some(long.clone()),
                    exact_count_sql: long.clone(),
                    count_kind: CountStrategy::Estimated,
                }),
                open: true,
                ..AuditState::default()
            },
        ),
        (
            "error",
            AuditState { error: Some("x".repeat(400)), open: true, ..AuditState::default() },
        ),
    ];
    for (name, state) in scenarios {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            for dark in [true, false] {
                let rt = runtime(
                    AuditState {
                        preview: state.preview.clone(),
                        error: state.error.clone(),
                        open: true,
                        ..AuditState::default()
                    },
                    size,
                    dark,
                );
                assert!(mounted(&rt, ROOT), "{name} {size:?} dark={dark}: root mounted");
                let panel = element(&rt, ROOT).rect;
                let footer = element(&rt, format!("{ROOT}-footer").as_str()).rect;
                assert!(
                    footer[1] + footer[3] <= panel[1] + panel[3] + 0.5,
                    "{name} {size:?} dark={dark}: footer must stay inside the panel"
                );
                for e in rt.elements() {
                    // `visible` is the part of the layout rect its ancestors'
                    // clips leave on screen — the right channel for a body
                    // whose long statements legitimately scroll.
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{name} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path),
                        e.visible,
                        e.rect
                    );
                }
            }
        }
    }
}
