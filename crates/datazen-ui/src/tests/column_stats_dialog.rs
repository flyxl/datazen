//! Column statistics dialog (T004): the four body branches, the em-dash rule,
//! dismissal and window bounds (`components/data_table/column_stats_dialog.rs`).
//!
//! Keys come from the widget's own contract: `<key>` panel, `<key>-title`,
//! `<key>-close`, `<key>-body`, `<key>-status`/`-spinner` while loading,
//! `<key>-error` on failure, `<key>-empty` for a ready state without
//! aggregates (an empty container: React renders no row there), and `<key>-row-<i>` / `<key>-cell-<i>-label|-value` for the
//! statistics themselves.

use super::*;

use rune::{col, Cx, El, Key, KeyEvent, Mods};
use rune_kit::Kit;

use crate::components::data_table::column_stats_dialog::{
    ColumnStats, ColumnStatsDialog, ColumnStatsState, ColumnStatsStatus, StatsValue,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const KEY: &str = "column-stats-dialog";

#[derive(Default)]
struct Scene {
    state: ColumnStatsState,
    open: bool,
    closes: usize,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let k: &Kit = kit(scene.dark);
    let mut layer = Vec::new();
    if scene.open {
        ColumnStatsDialog::new(KEY, &scene.state)
            .on_close(|scene: &mut Scene| scene.closes += 1)
            .build(scene.dark, &mut layer);
    }
    col()
        .key("stats-page")
        .w_full()
        .h_full()
        .padding(24.0)
        .child(
            col()
                .key("stats-page-body")
                .w_full()
                .h_full()
                .fill(k.c.bg_base),
        )
        .children(layer)
}

fn scene_runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let mut runtime = Runtime::new(
        scene,
        view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    runtime
}

fn element<S>(runtime: &Runtime<S>, key: &str) -> rune::ElementInfo {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn maybe<S>(runtime: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
}

fn text_of<S>(runtime: &Runtime<S>, key: &str) -> String {
    element(runtime, key).text.unwrap_or_default()
}

fn present_keys<S>(runtime: &Runtime<S>, prefix: &str) -> Vec<String> {
    runtime
        .elements()
        .into_iter()
        .filter_map(|element| element.key)
        .filter(|key| key.starts_with(prefix))
        .collect()
}

fn press<S>(runtime: &mut Runtime<S>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("{key} is pressable"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
}

fn escape<S>(runtime: &mut Runtime<S>) {
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
}

fn ready(stats: ColumnStats) -> Scene {
    Scene {
        state: ColumnStatsState {
            status: ColumnStatsStatus::Ready,
            column: Some("age".to_owned()),
            stats: Some(stats),
            error: None,
        },
        open: true,
        closes: 0,
        dark: true,
    }
}

fn full_stats() -> ColumnStats {
    ColumnStats {
        row_count: Some(1_234_567),
        non_null_count: Some(1_200),
        null_count: Some(367),
        distinct_count: Some(42),
        min: StatsValue::Number(1.0),
        max: StatsValue::Text("old".to_owned()),
        avg: StatsValue::Number(18.5),
        supports_avg: false,
    }
}

#[test]
fn the_ready_state_renders_on_every_reported_row() {
    let runtime = scene_runtime(ready(full_stats()), (1440.0, 816.0));
    assert!(maybe(&runtime, KEY).is_some(), "the dialog mounts");
    for index in 0..6 {
        assert!(
            maybe(&runtime, &format!("{KEY}-row-{index}")).is_some(),
            "row {index} is mounted"
        );
    }
    assert!(
        maybe(&runtime, &format!("{KEY}-row-6")).is_none(),
        "avg is not rendered when the column type cannot average"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-0-label")),
        t("columnStats.rows")
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-0-value")),
        "1,234,567"
    );
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-1-value")), "1,200");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-2-value")), "367");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-3-value")), "42");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-4-value")), "1");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-5-value")), "old");
    assert!(
        maybe(&runtime, &format!("{KEY}-avg-note")).is_some(),
        "the unsupported-average note renders"
    );
}

#[test]
fn an_averaged_column_adds_the_avg_row_and_drops_the_note() {
    let mut stats = full_stats();
    stats.supports_avg = true;
    let runtime = scene_runtime(ready(stats), (1440.0, 816.0));
    assert!(
        maybe(&runtime, &format!("{KEY}-row-6")).is_some(),
        "avg is the seventh row"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-6-label")),
        t("columnStats.avg")
    );
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-6-value")), "18.5");
    assert!(
        maybe(&runtime, &format!("{KEY}-avg-note")).is_none(),
        "the note explains an absence, so it leaves with the row"
    );
}

#[test]
fn an_unreported_value_is_an_em_dash_not_a_zero() {
    let mut stats = full_stats();
    stats.min = StatsValue::Null;
    stats.avg = StatsValue::Null;
    stats.supports_avg = true;
    stats.row_count = None;
    let runtime = scene_runtime(ready(stats), (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-0-value")),
        "—",
        "a count the server did not report is an em dash"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-4-value")),
        "—",
        "a min the server did not report is an em dash"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-6-value")),
        "—",
        "an avg the server did not report is an em dash"
    );
}

#[test]
fn a_zero_count_is_a_real_value_and_keeps_its_zero() {
    let mut stats = full_stats();
    stats.null_count = Some(0);
    let runtime = scene_runtime(ready(stats), (1440.0, 816.0));
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-2-value")), "0");
}

#[test]
fn an_empty_string_is_two_quotes_and_digits_are_grouped() {
    let mut stats = full_stats();
    stats.max = StatsValue::Text(String::new());
    stats.null_count = Some(-1_234_567);
    let runtime = scene_runtime(ready(stats), (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-5-value")),
        "''",
        "an empty string is a value, not a missing one"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-cell-2-value")),
        "-1,234,567"
    );
}

#[test]
fn the_loading_state_shows_one_spinner_and_no_rows() {
    let mut scene = ready(full_stats());
    scene.state.status = ColumnStatsStatus::Loading;
    scene.state.stats = None;
    let runtime = scene_runtime(scene, (1440.0, 816.0));
    let spinner = element(&runtime, &format!("{KEY}-spinner"));
    assert_eq!(spinner.role, "progressbar");
    assert_eq!(
        element(&runtime, &format!("{KEY}-spinner-label"))
            .text
            .as_deref(),
        Some(t("columnStats.loading"))
    );
    assert!(
        present_keys(&runtime, &format!("{KEY}-row-")).is_empty(),
        "loading paints no statistics"
    );
    assert!(
        maybe(&runtime, &format!("{KEY}-error")).is_none(),
        "loading is not an error"
    );
}

#[test]
fn the_error_state_prefers_the_server_message_and_falls_back() {
    let mut scene = ready(full_stats());
    scene.state.status = ColumnStatsStatus::Error;
    scene.state.stats = None;
    scene.state.error = Some("connection reset".to_owned());
    let runtime = scene_runtime(scene, (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-error-text")),
        "connection reset"
    );
    assert_eq!(
        element(&runtime, &format!("{KEY}-error")).role,
        "alert",
        "the failure is announced"
    );
    assert!(
        present_keys(&runtime, &format!("{KEY}-row-")).is_empty(),
        "an error paints no statistics"
    );

    let mut fallback = ready(full_stats());
    fallback.state.status = ColumnStatsStatus::Error;
    fallback.state.stats = None;
    let runtime = scene_runtime(fallback, (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-error-text")),
        t("columnStats.error")
    );
}

#[test]
fn the_idle_state_mounts_no_panel() {
    let scene = Scene {
        state: ColumnStatsState::default(),
        open: true,
        closes: 0,
        dark: true,
    };
    let runtime = scene_runtime(scene, (1440.0, 816.0));
    assert!(
        maybe(&runtime, KEY).is_none(),
        "idle keeps the dialog closed"
    );
}

#[test]
fn a_ready_state_without_aggregates_mounts_only_the_empty_container() {
    let mut scene = ready(full_stats());
    scene.state.stats = None;
    let runtime = scene_runtime(scene, (1440.0, 816.0));
    // React renders no row at all for a missing aggregate object, and there is
    // no `columnStats.empty` string: the container exists, with no invented
    // line inside it.
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-empty")),
        "",
        "no copy is invented for the empty state"
    );
    assert!(
        present_keys(&runtime, &format!("{KEY}-row-")).is_empty(),
        "there is nothing to list"
    );
    assert!(
        maybe(&runtime, &format!("{KEY}-stats")).is_none(),
        "the statistics list is not mounted without aggregates"
    );
}

#[test]
fn the_loading_panel_hugs_the_spinner() {
    let mut scene = ready(full_stats());
    scene.state.status = ColumnStatsStatus::Loading;
    scene.state.stats = None;
    let runtime = scene_runtime(scene, (960.0, 640.0));
    let panel = element(&runtime, KEY).rect;
    let header = element(&runtime, &format!("{KEY}-header")).rect;
    let body = element(&runtime, &format!("{KEY}-body")).rect;
    assert!(
        panel[3] < 200.0,
        "React auto-heights the loading panel; got {panel:?}"
    );
    assert!(
        (panel[3] - (header[3] + body[3])).abs() <= 2.0,
        "panel = header + body: panel {panel:?}, header {header:?}, body {body:?}"
    );
    assert!(
        element(&runtime, &format!("{KEY}-status")).rect[3] >= 56.0,
        "React's py-6 block plus the spinner is at least 56pt high"
    );
}

#[test]
fn the_ready_panel_hugs_its_rows() {
    let runtime = scene_runtime(ready(full_stats()), (1440.0, 816.0));
    let panel = element(&runtime, KEY).rect;
    let header = element(&runtime, &format!("{KEY}-header")).rect;
    let body = element(&runtime, &format!("{KEY}-body")).rect;
    let stats = element(&runtime, &format!("{KEY}-stats")).rect;
    assert!(
        (panel[3] - (header[3] + body[3])).abs() <= 2.0,
        "panel = header + body: panel {panel:?}, header {header:?}, body {body:?}"
    );
    assert!(
        (body[3] - (stats[3] + 32.0)).abs() <= 2.0,
        "body = stats + the two 16pt paddings: body {body:?}, stats {stats:?}"
    );
    assert!(
        panel[3] < 368.0,
        "a six-row list does not need the old fixed 368pt panel: {panel:?}"
    );
}

#[test]
fn the_title_names_the_column_and_drops_it_when_unknown() {
    let runtime = scene_runtime(ready(full_stats()), (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-title")),
        format!("{} — age", t("columnStats.title"))
    );

    let mut idle_column = ready(full_stats());
    idle_column.state.column = None;
    let runtime = scene_runtime(idle_column, (1440.0, 816.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-title")),
        t("columnStats.title")
    );
}

#[test]
fn the_close_control_and_escape_both_call_the_dismissal() {
    let mut rt = scene_runtime(ready(full_stats()), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-close"));
    assert_eq!(rt.state.closes, 1, "the close control dismisses");
    escape(&mut rt);
    assert_eq!(rt.state.closes, 2, "Escape dismisses through the modal");
}

#[test]
fn the_value_column_stays_inside_the_dialog_body() {
    let runtime = scene_runtime(ready(full_stats()), (1440.0, 816.0));
    let body = element(&runtime, &format!("{KEY}-body")).rect;
    let value = element(&runtime, &format!("{KEY}-cell-0-value")).rect;
    assert!(
        value[0] + value[2] <= body[0] + body[2] + 0.5,
        "the value stays inside the body: {value:?} vs {body:?}"
    );
}

#[test]
fn the_dialog_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut scene = ready(full_stats());
            scene.dark = dark;
            let runtime = scene_runtime(scene, (width, height));
            let mut outside = Vec::new();
            for element in runtime.elements() {
                let Some(key) = element.key else { continue };
                if !key.starts_with(KEY) {
                    continue;
                }
                let [x, y, w, h] = element.rect;
                if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
                    outside.push(format!("{key} [{x}, {y}, {w}, {h}]"));
                }
            }
            assert!(
                outside.is_empty(),
                "{width}x{height} dark={dark}: {} nodes leave the window: {outside:?}",
                outside.len()
            );
        }
    }
}
