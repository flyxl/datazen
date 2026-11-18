//! Independent audit of the T004 column-statistics dialog.
//!
//! The coder tests cover the branches; this file pins the React shell metrics
//! and records the two base-height deviations found by the audit.

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
struct Probe {
    state: ColumnStatsState,
    open: bool,
    closes: usize,
    dark: bool,
}

fn view(probe: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k: &Kit = kit(probe.dark);
    let mut layer = Vec::new();
    if probe.open {
        ColumnStatsDialog::new(KEY, &probe.state)
            .on_close(|p: &mut Probe| p.closes += 1)
            .build(probe.dark, &mut layer);
    }
    col()
        .key("stats-audit-page")
        .w_full()
        .h_full()
        .padding(24.0)
        .fill(k.c.bg_base)
        .children(layer)
}

fn rt(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, size, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn full() -> ColumnStats {
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

fn ready_state() -> ColumnStatsState {
    ColumnStatsState {
        status: ColumnStatsStatus::Ready,
        column: Some("age".to_owned()),
        stats: Some(full()),
        error: None,
    }
}

fn ready() -> Probe {
    Probe {
        state: ready_state(),
        open: true,
        closes: 0,
        dark: true,
    }
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn maybe(rt: &Runtime<Probe>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Probe>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn press(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn the_ready_rows_match_the_react_order_and_values() {
    let runtime = rt(ready(), (960.0, 640.0));
    assert_eq!(
        text_of(&runtime, "column-stats-dialog-title"),
        format!("{} — age", t("columnStats.title"))
    );
    let labels = ["rows", "nonNull", "nulls", "distinct", "min", "max"];
    for (index, name) in labels.iter().enumerate() {
        assert_eq!(
            text_of(&runtime, &format!("{KEY}-cell-{index}-label")),
            t(&format!("columnStats.{name}"))
        );
    }
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-0-value")), "1,234,567");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-1-value")), "1,200");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-2-value")), "367");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-3-value")), "42");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-4-value")), "1");
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-5-value")), "old");
    assert_eq!(
        element(&runtime, &format!("{KEY}-row-0")).rect[3],
        32.5,
        "React py-1.5 rows"
    );
    assert!(maybe(&runtime, &format!("{KEY}-row-6")).is_none());
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-avg-note")),
        t("columnStats.avgUnsupported")
    );
}

#[test]
fn the_loading_and_error_branches_show_the_react_content() {
    let mut loading = ready();
    loading.state.status = ColumnStatsStatus::Loading;
    loading.state.stats = None;
    let runtime = rt(loading, (960.0, 640.0));
    assert_eq!(
        element(&runtime, &format!("{KEY}-spinner")).role,
        "progressbar"
    );
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-spinner-label")),
        t("columnStats.loading")
    );
    assert!(maybe(&runtime, &format!("{KEY}-row-0")).is_none());

    let mut error = ready();
    error.state.status = ColumnStatsStatus::Error;
    error.state.stats = None;
    error.state.error = Some("connection reset".to_owned());
    let runtime = rt(error, (960.0, 640.0));
    assert_eq!(
        element(&runtime, &format!("{KEY}-error")).role,
        "alert"
    );
    assert_eq!(text_of(&runtime, &format!("{KEY}-error-text")), "connection reset");
    assert!(maybe(&runtime, &format!("{KEY}-row-0")).is_none());
}

#[test]
fn the_dialog_and_every_child_stay_inside_both_windows() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = ready();
            probe.dark = dark;
            let runtime = rt(probe, (width, height));
            let panel = element(&runtime, KEY);
            assert_eq!(panel.rect[2], 576.0, "React max-w-xl");
            assert!(panel.rect[3] > 0.0);
            for key in [
                format!("{KEY}-title"),
                format!("{KEY}-close"),
                format!("{KEY}-body"),
                format!("{KEY}-stats"),
                format!("{KEY}-row-0"),
                format!("{KEY}-row-5"),
            ] {
                let e = element(&runtime, &key);
                let [x, y, w, h] = e.rect;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{width}x{height} dark={dark}: {key} leaves the window: {:?}",
                    e.rect
                );
                assert!(
                    x >= panel.rect[0] - 0.5
                        && y >= panel.rect[1] - 0.5
                        && x + w <= panel.rect[0] + panel.rect[2] + 0.5
                        && y + h <= panel.rect[1] + panel.rect[3] + 0.5,
                    "{width}x{height} dark={dark}: {key} leaves the panel: {:?}",
                    e.rect
                );
            }
        }
    }
}

#[test]
fn the_close_button_and_escape_call_the_dismissal() {
    let mut rt = rt(ready(), (960.0, 640.0));
    press(&mut rt, &format!("{KEY}-close"));
    assert_eq!(rt.state.closes, 1);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.closes, 2);
}

#[test]
fn a_zero_count_stays_zero_and_a_negative_count_is_grouped() {
    let mut probe = ready();
    probe.state.stats = Some(ColumnStats {
        null_count: Some(0),
        ..full()
    });
    let runtime = rt(probe, (960.0, 640.0));
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-2-value")), "0");

    let mut negative = ready();
    negative.state.stats = Some(ColumnStats {
        null_count: Some(-1_234_567),
        ..full()
    });
    let runtime = rt(negative, (960.0, 640.0));
    assert_eq!(text_of(&runtime, &format!("{KEY}-cell-2-value")), "-1,234,567");
}

#[test]
fn the_ready_state_without_aggregates_keeps_the_english_empty_line() {
    let mut probe = ready();
    probe.state.stats = None;
    let runtime = rt(probe, (960.0, 640.0));
    assert_eq!(
        text_of(&runtime, &format!("{KEY}-empty")),
        "No statistics reported."
    );
    assert!(maybe(&runtime, &format!("{KEY}-row-0")).is_none());
}

#[test]
#[ignore = "DEFECT a6d42efda: ColumnStatsDialog never calls Dialog::fit(), so loading/error panels inherit the 368pt form height instead of React's content height"]
fn the_loading_panel_hugs_its_content() {
    let mut probe = ready();
    probe.state.status = ColumnStatsStatus::Loading;
    probe.state.stats = None;
    let runtime = rt(probe, (960.0, 640.0));
    let panel = element(&runtime, KEY);
    assert!(
        panel.rect[3] < 200.0,
        "React auto-heights the loading panel; got {:?}",
        panel.rect
    );
}

#[test]
#[ignore = "DEFECT a6d42efda: the loading row uses py(8.0) although the comment and React py-6 say 24pt"]
fn the_loading_row_uses_the_react_vertical_padding() {
    let mut probe = ready();
    probe.state.status = ColumnStatsStatus::Loading;
    probe.state.stats = None;
    let runtime = rt(probe, (960.0, 640.0));
    let status = element(&runtime, &format!("{KEY}-status"));
    assert!(
        status.rect[3] >= 56.0,
        "React py-6 (24+24) plus the spinner is at least 56pt high; got {:?}",
        status.rect
    );
}
