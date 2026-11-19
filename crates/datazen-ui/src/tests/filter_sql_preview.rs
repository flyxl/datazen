//! T135: `components/filter/FilterSqlPreviewDialog.tsx` ported to Rune.
//!
//! Probes mount the dialog exactly as the filter panel will, and observe it
//! through the runtime tree (keys, text, rects) and the memory clipboard — never
//! through a value the component writes for the test to read. Coverage: all
//! three statement blocks and their labels, the no-count boundary, the exact
//! copy-all payload, the real 1500ms wake/deadline, error/loading exclusivity,
//! disabled copy, Escape/Close, and two-size two-theme bounds.

use rune::{col, clipboard, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::filter::filter_sql_preview_dialog::{
    CountStrategy, FilterSqlPreviewDialog, TableReadPreviewSql,
};
use crate::i18n::{has, t};

const ROOT: &str = "filter-sql-preview-dialog";
const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

fn preview(count: Option<&str>, kind: CountStrategy) -> TableReadPreviewSql {
    TableReadPreviewSql {
        data_sql: "SELECT \"id\", \"name\" FROM \"users\" WHERE \"status\" = 'active' LIMIT 50 OFFSET 0;"
            .to_owned(),
        count_sql: count.map(str::to_owned),
        exact_count_sql: "SELECT COUNT(*) FROM \"users\" WHERE \"status\" = 'active';".to_owned(),
        count_kind: kind,
    }
}

/// Host state: the filter panel owns the copied deadline, exactly like React's
/// local `copied` plus its `setTimeout`.
#[derive(Default)]
struct Probe {
    preview: Option<TableReadPreviewSql>,
    error: Option<String>,
    closed: usize,
    copied_calls: usize,
    press_clock: Option<f64>,
    copied_until: Option<f64>,
    now: f64,
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let mut layer = Vec::new();
    FilterSqlPreviewDialog::<Probe>::new()
        .open(true)
        .preview(state.preview.as_ref())
        .error(state.error.as_deref())
        .copied(state.copied_until.map_or(false, |until| state.now < until))
        .on_close(|s: &mut Probe| s.closed += 1)
        .on_copy_tx(|s: &mut Probe, tx: &mut Tx| {
            s.copied_calls += 1;
            s.press_clock = Some(tx.clock());
            s.now = tx.clock();
            s.copied_until = Some(tx.clock() + 1.5);
        })
        .build(dark, &mut layer);
    rune_kit::root(col().w_full().h_full(), layer)
}

fn runtime(state: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env(dark));
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

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn escape(rt: &mut Runtime<Probe>) {
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
}

fn composed(p: &TableReadPreviewSql) -> String {
    let count_label = format!(
        "{} · {}",
        t("filter.previewSqlCount"),
        t(&format!("filter.previewSqlKind.{}", p.count_kind.as_str()))
    );
    let mut blocks = vec![
        format!("-- {}\n{}", t("filter.previewSqlData"), p.data_sql),
    ];
    if let Some(sql) = &p.count_sql {
        blocks.push(format!("-- {count_label}\n{sql}"));
    }
    blocks.push(format!("-- {}\n{}", t("filter.previewSqlExactCount"), p.exact_count_sql));
    blocks.join("\n\n")
}

// --- 1. statements, labels and keys -----------------------------------------

#[test]
fn shows_data_count_and_exact_count_with_their_exact_labels_and_keys() {
    let p = preview(Some("SELECT COUNT(*) FROM \"users\" LIMIT 1;"), CountStrategy::Exact);
    let rt = runtime(Probe { preview: Some(p.clone()), ..Probe::default() }, DESIGN, true);

    assert_eq!(text_of(&rt, "filter-sql-preview-data-text"), p.data_sql);
    assert_eq!(text_of(&rt, "filter-sql-preview-count-text"), p.count_sql.clone().unwrap());
    assert_eq!(text_of(&rt, "filter-sql-preview-exact-count-text"), p.exact_count_sql);

    assert_eq!(text_of(&rt, "filter-sql-preview-data-label"), t("filter.previewSqlData"));
    assert_eq!(
        text_of(&rt, "filter-sql-preview-count-label"),
        format!("{} · {}", t("filter.previewSqlCount"), t("filter.previewSqlKind.exact")),
        "the count label names its kind, as React's ` · ` separator does"
    );
    assert_eq!(text_of(&rt, "filter-sql-preview-exact-count-label"), t("filter.previewSqlExactCount"));

    assert_eq!(text_of(&rt, format!("{ROOT}-title").as_str()), t("filter.previewSqlTitle"));
    assert!(mounted(&rt, "filter-sql-preview-copy"));
    assert!(mounted(&rt, "filter-sql-preview-close"));
    assert!(!mounted(&rt, "filter-sql-preview-error"));
    assert!(!mounted(&rt, "filter-sql-preview-loading"));
}

#[test]
fn hides_the_count_statement_when_the_driver_would_not_run_one() {
    let p = preview(None, CountStrategy::Unsupported);
    let rt = runtime(Probe { preview: Some(p.clone()), ..Probe::default() }, DESIGN, true);

    assert!(!mounted(&rt, "filter-sql-preview-count"), "no count statement without countSql");
    assert!(!mounted(&rt, "filter-sql-preview-count-label"));
    assert_eq!(text_of(&rt, "filter-sql-preview-exact-count-text"), p.exact_count_sql);
    assert_eq!(text_of(&rt, "filter-sql-preview-data-text"), p.data_sql);
}

// --- 2. copy-all payload and 1500ms feedback --------------------------------

#[test]
fn copy_all_writes_the_exact_labelled_blocks_in_one_clipboard_write() {
    let p = preview(Some("SELECT COUNT(*) FROM \"users\";"), CountStrategy::Exact);
    let clipboard = clipboard::memory();
    let mut rt = runtime(Probe { preview: Some(p.clone()), ..Probe::default() }, DESIGN, true);
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    click(&mut rt, "filter-sql-preview-copy");

    let expected = composed(&p);
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some(expected.as_str()),
        "copy-all must write `-- label\\nsql` blocks joined by a blank line"
    );
    assert_eq!(rt.state.copied_calls, 1, "one press calls the copied hook once");
    assert_eq!(label_of(&rt, "filter-sql-preview-copy"), t("common.copied"));

    let press = rt.state.press_clock.expect("press clock recorded");
    let deadline = rt.wake_deadline().expect("copy press schedules a real feedback wake");
    assert!(
        (deadline - press - 1.5).abs() <= 0.01,
        "wake must be ~1500ms after the press: press={press} deadline={deadline}"
    );
}

#[test]
fn copied_feedback_reverts_at_the_1500ms_boundary_and_retimes_on_repeat() {
    let p = preview(None, CountStrategy::Unsupported);
    let mut rt = runtime(Probe { preview: Some(p.clone()), ..Probe::default() }, DESIGN, true);
    rt.frame(0.0);
    click(&mut rt, "filter-sql-preview-copy");

    let press = rt.state.press_clock.expect("first press clock");
    let until = rt.state.copied_until.expect("first copied deadline");
    assert!((until - press - 1.5).abs() <= 1e-9, "deadline is press + 1.5s");

    // Render each side of the boundary from the recorded deadline: the label is
    // computed from host state, so a fresh frame is a clean observation (and a
    // mutation of the deadline cannot hide behind a stale cached label).
    let before = runtime(
        Probe { preview: Some(p.clone()), copied_until: Some(until), now: until - 0.001, ..Probe::default() },
        DESIGN,
        true,
    );
    assert_eq!(label_of(&before, "filter-sql-preview-copy"), t("common.copied"), "just before expiry");
    let at = runtime(
        Probe { preview: Some(p.clone()), copied_until: Some(until), now: until, ..Probe::default() },
        DESIGN,
        true,
    );
    assert_eq!(label_of(&at, "filter-sql-preview-copy"), t("common.copy"), "at expiry it reverts");

    rt.frame(0.8);
    click(&mut rt, "filter-sql-preview-copy");
    let second_press = rt.state.press_clock.expect("second press clock");
    let second_until = rt.state.copied_until.expect("second deadline");
    assert!(second_press > press, "the second press is later");
    assert!(
        (second_until - second_press - 1.5).abs() <= 1e-9,
        "the second press re-times to its own clock + 1.5s"
    );
    assert!(second_until > until, "re-timing extends the deadline instead of accumulating");
}

// --- 3. error / loading exclusivity and disabled copy -----------------------

#[test]
fn error_shows_only_the_banner_and_loading_only_the_placeholder() {
    let mut rt = runtime(
        Probe { preview: None, error: Some("connection refused".to_owned()), ..Probe::default() },
        DESIGN,
        true,
    );
    assert!(mounted(&rt, "filter-sql-preview-error"));
    assert!(text_of(&rt, "filter-sql-preview-error-text").contains("connection refused"));
    assert!(!mounted(&rt, "filter-sql-preview-loading"));
    assert!(!mounted(&rt, "filter-sql-preview-data"));

    let mut rt = runtime(Probe::default(), DESIGN, true);
    assert!(mounted(&rt, "filter-sql-preview-loading"));
    assert_eq!(text_of(&rt, "filter-sql-preview-loading"), t("filter.previewSqlLoading"));
    assert!(!mounted(&rt, "filter-sql-preview-error"));
    assert!(!mounted(&rt, "filter-sql-preview-data"));

    // React renders the banner independently of the statements: a stale preview
    // stays visible with a fresh error, and neither case shows loading.
    let rt = runtime(
        Probe {
            preview: Some(preview(None, CountStrategy::Unsupported)),
            error: Some("retry failed".to_owned()),
            ..Probe::default()
        },
        DESIGN,
        true,
    );
    assert!(mounted(&rt, "filter-sql-preview-error"));
    assert!(mounted(&rt, "filter-sql-preview-data"));
    assert!(!mounted(&rt, "filter-sql-preview-loading"));
}

#[test]
fn copy_button_is_disabled_and_inert_without_a_preview() {
    let clipboard = clipboard::memory();
    let mut rt = runtime(Probe::default(), DESIGN, true);
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    assert!(!element(&rt, "filter-sql-preview-copy").focusable, "disabled copy cannot take focus");
    assert_eq!(label_of(&rt, "filter-sql-preview-copy"), t("common.copy"));
    click(&mut rt, "filter-sql-preview-copy");

    assert_eq!(clipboard.text(), None, "a disabled copy writes nothing");
    assert_eq!(rt.state.copied_calls, 0, "the copied hook never runs");
    assert!(rt.wake_deadline().is_none(), "no feedback wake without a copy");
}

// --- 4. dismissal ------------------------------------------------------------

#[test]
fn the_footer_close_and_escape_both_dismiss_the_dialog() {
    let mut rt = runtime(Probe::default(), DESIGN, true);
    click(&mut rt, "filter-sql-preview-close");
    assert_eq!(rt.state.closed, 1, "footer Close calls onClose");
    escape(&mut rt);
    assert_eq!(rt.state.closed, 2, "Escape shares the same onClose");
    click(&mut rt, format!("{ROOT}-close").as_str());
    assert_eq!(rt.state.closed, 3, "the header X shares it too");
}

// --- 5. i18n and geometry ----------------------------------------------------

#[test]
fn every_i18n_key_the_dialog_asks_for_exists_in_the_locale_pack() {
    for key in [
        "filter.previewSqlTitle",
        "filter.previewSqlDescription",
        "filter.previewSqlData",
        "filter.previewSqlCount",
        "filter.previewSqlExactCount",
        "filter.previewSqlLoading",
        "filter.previewSqlKind.exact",
        "filter.previewSqlKind.estimated",
        "filter.previewSqlKind.unsupported",
        "common.close",
        "common.copy",
        "common.copied",
    ] {
        assert!(has(key) && t(key) != key, "key {key} must resolve, not echo");
    }
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), context: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let key = e.key.as_deref().unwrap_or(e.path.as_str());
        let [x, y, w, h] = e.rect;
        if x < -0.5 || y < -0.5 || x + w > size.0 + 0.5 || y + h > size.1 + 0.5 {
            outside.push(format!("{key} rect={:?}", e.rect));
        }
    }
    assert!(outside.is_empty(), "{context}: {} node(s) cross {:?}: {outside:#?}", outside.len(), size);
}

#[test]
fn preview_error_and_loading_fit_both_window_sizes_and_themes() {
    let scenarios: Vec<(&str, Probe)> = vec![
        (
            "preview",
            Probe { preview: Some(preview(Some("SELECT COUNT(*) FROM \"users\";"), CountStrategy::Estimated)), ..Probe::default() },
        ),
        ("error", Probe { error: Some("connection refused — a long enough message to wrap across the panel".to_owned()), ..Probe::default() }),
        ("loading", Probe::default()),
    ];
    for (name, state) in scenarios {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(
                    Probe { preview: state.preview.clone(), error: state.error.clone(), ..Probe::default() },
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
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}
