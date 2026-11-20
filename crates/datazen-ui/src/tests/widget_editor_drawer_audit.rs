//! Independent audit for T201 (the WidgetEditorDrawer Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below re-reads
//! `packages/frontend/src/windows/dashboard/WidgetEditorDrawer.tsx`, its
//! React test `__tests__/WidgetEditorDrawer.test.tsx`, and the
//! `types/dashboard.ts` refresh vocabulary the drawer edits with: the null
//! render while closed, the shell (the inert `bg-black/40` scrim, the
//! right-anchored `max-w-md` aside, the header/footer strips), every
//! field's edit journey (the title, the hidden-SQL pair or the workflow
//! source, the refresh preset with its value/unit interval row and the
//! dense-interval warning, the enabled checkbox, the view mode, the chart
//! type and y-axes under the chart view, the alert rule), the two save
//! guards and the normalized payload, the T146 card's mount gate over the
//! saved widget, and the close journeys (the header X, the footer cancel,
//! the handler-less scrim). Element evidence comes from
//! `Runtime::elements()` (runtime layout), never from a value the
//! component reports about itself; presses go through real pointer events
//! (move + frame before down/up), typing through Cmd+A + paste, and every
//! host-side state change through `rt.update`.
//!
//! The SqlEditor is the port's seam: the composite host below supplies a
//! real kit text field keyed `sql-editor` (the React test's own mock key)
//! through `.sql_editor(...)` and delivers its text via `set_hidden_sql`,
//! exactly as the module doc prescribes. One flagged deviation is pinned
//! deliberately in `the_closed_triggers_show_the_matched_option_label`.

use std::rc::Rc;

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime, TextEdit, Tx};

use crate::components::chart::chart_canvas::{ChartConfig, ChartType};
use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::palette;
use crate::views::dashboard::chart_widget_tile::TileViewMode;
use crate::views::dashboard::dashboard_panel::{
    HiddenSqlVm, RefreshModeVm, RefreshPolicyVm, WidgetLayoutVm, WorkflowListItemVm,
};
use crate::views::dashboard::widget_editor_drawer::{
    AlertOperatorVm, ConnectionOptionVm, IntervalUnit, RefreshPreset, WidgetDraftVm,
    WidgetEditorDrawer, WidgetEditorDrawerState, WidgetEditorSelect, WidgetSaveVm,
    MIN_REFRESH_SEC, REFRESH_WARN_BELOW_SEC, clamp_refresh_sec, default_alert,
    default_hidden_sql, normalize_refresh_policy, refresh_policy_to_preset,
    sec_to_unit_and_value, should_warn_refresh_sec,
};

/// `Date.now()` for the T146 card's builds (the sibling audits' clock).
const NOW_MS: i64 = 1_700_000_000_000;
/// The probe window: tall enough that the whole form lays out unscrolled.
const WIN_W: f32 = 960.0;
const WIN_H: f32 = 1240.0;

// ---- Fixtures (the React suite's own) ----------------------------------------

/// React's `baseWidget`: chart view, y-axes ['v'], interval 45s, enabled.
fn base_widget() -> WidgetDraftVm {
    WidgetDraftVm {
        id: "w1".to_owned(),
        title: "Tile".to_owned(),
        workflow_id: "wf-hidden".to_owned(),
        view_mode: TileViewMode::Chart,
        chart_config: Some(ChartConfig {
            chart_type: ChartType::Bar,
            y_axes: vec!["v".to_owned()],
            ..Default::default()
        }),
        layout: WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 },
        refresh: RefreshPolicyVm { mode: RefreshModeVm::Interval, refresh_sec: Some(45) },
        enabled: true,
        alert: None,
    }
}

/// `base_widget` with a different title and refresh policy.
fn widget_with(title: &str, refresh: RefreshPolicyVm) -> WidgetDraftVm {
    WidgetDraftVm { title: title.to_owned(), refresh, ..base_widget() }
}

fn interval(sec: u32) -> RefreshPolicyVm {
    RefreshPolicyVm { mode: RefreshModeVm::Interval, refresh_sec: Some(sec) }
}

fn manual() -> RefreshPolicyVm {
    RefreshPolicyVm { mode: RefreshModeVm::Manual, refresh_sec: None }
}

/// React's `hiddenSql` prop: { connectionId: 'c1', sql: 'SELECT 1 AS v' }.
fn hidden_sql() -> HiddenSqlVm {
    HiddenSqlVm { connection_id: "c1".to_owned(), sql: "SELECT 1 AS v".to_owned() }
}

/// React's `userWorkflows` prop.
fn workflows() -> Vec<WorkflowListItemVm> {
    vec![
        WorkflowListItemVm { id: "wf-1".to_owned(), name: "Workflow One".to_owned() },
        WorkflowListItemVm { id: "wf-2".to_owned(), name: "Workflow Two".to_owned() },
    ]
}

// ---- The composite host -------------------------------------------------------

#[derive(Clone)]
struct AuditState {
    drawer: WidgetEditorDrawerState,
    workflows: Vec<WorkflowListItemVm>,
    /// The SQL-editor seam's editing surface (the React test's `sql-editor`).
    sql_edit: TextEdit,
    dark: bool,
    closes: usize,
    workflow_opens: usize,
    monitor_authorizes: usize,
    monitor_revokes: usize,
    saves: Vec<WidgetSaveVm>,
}

impl AuditState {
    /// The host's open flow: the effect's re-seed plus the connections fetch
    /// landing (React's store already holds them in the test).
    fn open(
        widget: &WidgetDraftVm,
        hidden: Option<&HiddenSqlVm>,
        is_new: bool,
        dashboard: Option<&str>,
        workflows: Vec<WorkflowListItemVm>,
    ) -> Self {
        let mut drawer = WidgetEditorDrawerState::default();
        assert!(drawer.needs_resync(true, Some(widget), hidden), "the mount always re-seeds");
        drawer.begin_open(Some(widget), hidden, is_new, dashboard, "fresh-id");
        drawer.deliver_connections(vec![ConnectionOptionVm {
            id: "c1".to_owned(),
            name: "Conn 1".to_owned(),
        }]);
        Self {
            sql_edit: TextEdit::new(hidden.map_or_else(
                || default_hidden_sql().sql,
                |h| h.sql.clone(),
            )),
            drawer,
            workflows,
            dark: true,
            closes: 0,
            workflow_opens: 0,
            monitor_authorizes: 0,
            monitor_revokes: 0,
            saves: Vec::new(),
        }
    }

    /// The React suite's default scene: the baseWidget over hidden SQL.
    fn hidden() -> Self {
        Self::open(&base_widget(), Some(&hidden_sql()), false, Some("d1"), Vec::new())
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    // The seam: the host-built editing surface (React's SqlEditor), delivered
    // through the builder and painted inside the drawer's h-40 border box.
    let editor = TextField::new("sql-editor", &s.sql_edit)
        .height(160.0)
        .w_full()
        .on_edit(|s: &mut AuditState| &mut s.sql_edit)
        .on_change(|s: &mut AuditState, _tx: &mut Tx| {
            let text = s.sql_edit.text.clone();
            s.drawer.set_hidden_sql(&text);
        })
        .build(s.dark);
    WidgetEditorDrawer::<AuditState>::new(&s.drawer, &s.workflows, NOW_MS)
        .sql_editor(editor)
        .state_fn(Rc::new(|s: &mut AuditState| &mut s.drawer))
        .on_close(|s: &mut AuditState| {
            s.closes += 1;
            s.drawer.close();
        })
        .on_open_workflow_editor(|s: &mut AuditState| s.workflow_opens += 1)
        .on_save(|s: &mut AuditState, payload: WidgetSaveVm| s.saves.push(payload))
        .on_monitor_authorize(|s: &mut AuditState| {
            s.monitor_authorizes += 1;
            s.drawer.monitor.begin_change();
        })
        .on_monitor_revoke(|s: &mut AuditState| {
            s.monitor_revokes += 1;
            s.drawer.monitor.begin_change();
        })
        .build(s.dark, &mut layer);
    // The layer children ride the sized probe root so elements() sees them
    // (the drawer's scrim/aside are absolute-positioned and would collapse
    // inside an unsized column).
    let mut probe = col().key("probe").w_full().h_full();
    for child in layer {
        probe = probe.child(child);
    }
    probe
}

// ---- Runtime helpers ----------------------------------------------------------

fn runtime(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (WIN_W, WIN_H),
        Env { system_dark: true, scale: 1.0 },
    );
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// Whether some non-exiting element shows exactly `expected` (a closed
/// select's current label, the read-only field's id).
fn shows(rt: &Runtime<AuditState>, expected: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.text.as_deref() == Some(expected))
}

/// A kit button's label is its keyed `label` child under the button's path.
fn button_label(rt: &Runtime<AuditState>, key: &str) -> String {
    let Some(button) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = button.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some("label") && e.path.starts_with(&prefix))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    // Move onto the control first and let the clock run: hit-testing follows
    // the pointer, so a down without a prior move lands on nothing.
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
}

/// Replace a pre-filled field's text: click, Cmd+A, paste.
fn type_into(rt: &mut Runtime<AuditState>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods { cmd: true, ..Mods::default() },
    });
    rt.frame(1.0 / 60.0);
    rt.paste(text);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
}

// ---- The state machine (React's open effect and save handler) -----------------

#[test]
fn the_open_effect_reseeds_the_drafts_and_every_editing_surface() {
    let mut state = WidgetEditorDrawerState::default();
    assert!(state.needs_resync(true, Some(&base_widget()), None), "a closed state needs the effect");
    assert!(!state.needs_resync(false, Some(&base_widget()), None), "React's gate is `if (open)`");

    state.begin_open(Some(&base_widget()), Some(&hidden_sql()), false, Some("d1"), "fresh");
    assert!(
        !state.needs_resync(true, Some(&base_widget()), Some(&hidden_sql())),
        "the pair is served"
    );
    assert!(
        state.needs_resync(true, Some(&widget_with("Other", manual())), Some(&hidden_sql())),
        "a widget change re-runs it"
    );
    assert!(
        state.needs_resync(
            true,
            Some(&base_widget()),
            Some(&HiddenSqlVm { connection_id: "c2".to_owned(), sql: "SELECT 1 AS v".to_owned() })
        ),
        "a hidden-SQL change re-runs it"
    );

    // The effect's seeds: the drafts and every controlled display.
    assert_eq!(state.draft.title, "Tile");
    assert_eq!(state.title_edit.text, "Tile");
    assert_eq!(state.hidden_sql, hidden_sql());
    assert_eq!(state.interval_edit.text, "45", "secToUnitAndValue(45).value");
    assert_eq!(state.y_axes_edit.text, "v");
    assert_eq!(state.alert_column_edit.text, "");
    assert_eq!(state.threshold_edit.text, "0");
    assert_eq!(state.workflow_readonly_edit.text, "wf-hidden");
    assert!(!state.force_custom_refresh);
    assert_eq!(state.dashboard_id.as_deref(), Some("d1"));

    // An empty dashboard id is React's `dashboardId` falsy.
    state.begin_open(Some(&base_widget()), None, false, Some(""), "fresh");
    assert_eq!(state.dashboard_id, None);

    // A widget-less open is React's `emptyDraft()` with the host's fresh id.
    state.begin_open(None, None, true, Some("d1"), "brand-new");
    assert_eq!(state.draft.id, "brand-new");
    assert_eq!(state.draft.title, "");
    assert_eq!(state.draft.workflow_id, "");
    assert_eq!(state.draft.view_mode, TileViewMode::Chart);
    assert_eq!(
        state.draft.chart_config.as_ref().unwrap().y_axes,
        vec!["v".to_owned()],
        "DEFAULT_CHART_CONFIG with yAxes ['v']"
    );
    assert_eq!(state.draft.layout, WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 });
    assert_eq!(state.draft.refresh, RefreshPolicyVm::default(), "DEFAULT_REFRESH is manual");
    assert!(state.draft.enabled);
    assert_eq!(
        state.hidden_sql,
        default_hidden_sql(),
        "the inline default pair (connectionId '', sql 'SELECT 1 AS v')"
    );

    // The drafts survive a close; the next open re-seeds them.
    state.title_edit.replace_text("Typed".to_owned());
    state.apply_title_text();
    state.close();
    assert!(!state.open);
    assert_eq!(state.draft.title, "Typed", "React's drafts survive the close");
    state.begin_open(Some(&base_widget()), None, false, None, "fresh");
    assert_eq!(state.draft.title, "Tile", "the next open re-seeds");
}

#[test]
fn the_refresh_vocabulary_matches_types_dashboard() {
    assert_eq!(MIN_REFRESH_SEC, 30);
    assert_eq!(REFRESH_WARN_BELOW_SEC, 60);

    // clampRefreshSec.
    assert_eq!(clamp_refresh_sec(0), 30);
    assert_eq!(clamp_refresh_sec(29), 30);
    assert_eq!(clamp_refresh_sec(30), 30);
    assert_eq!(clamp_refresh_sec(45), 45);

    // normalizeRefreshPolicy: an interval keeps a clamped refreshSec (the
    // minimum standing in for a missing one); every other mode drops it.
    assert_eq!(normalize_refresh_policy(&interval(10)), interval(30));
    assert_eq!(
        normalize_refresh_policy(&RefreshPolicyVm {
            mode: RefreshModeVm::Interval,
            refresh_sec: None,
        }),
        interval(30)
    );
    assert_eq!(
        normalize_refresh_policy(&RefreshPolicyVm {
            mode: RefreshModeVm::Manual,
            refresh_sec: Some(45),
        }),
        manual()
    );
    assert_eq!(
        normalize_refresh_policy(&RefreshPolicyVm {
            mode: RefreshModeVm::OnOpen,
            refresh_sec: Some(45),
        }),
        RefreshPolicyVm { mode: RefreshModeVm::OnOpen, refresh_sec: None }
    );

    // shouldWarnRefreshSec: interval mode with a sub-warning interval.
    assert!(should_warn_refresh_sec(&interval(45)));
    assert!(should_warn_refresh_sec(&interval(30)));
    assert!(!should_warn_refresh_sec(&interval(60)));
    assert!(
        !should_warn_refresh_sec(&RefreshPolicyVm {
            mode: RefreshModeVm::Interval,
            refresh_sec: None,
        }),
        "React's refreshSec != null"
    );
    assert!(!should_warn_refresh_sec(&RefreshPolicyVm {
        mode: RefreshModeVm::Manual,
        refresh_sec: Some(45),
    }));

    // refreshPolicyToPreset.
    assert_eq!(refresh_policy_to_preset(RefreshModeVm::Manual, None), RefreshPreset::Manual);
    assert_eq!(
        refresh_policy_to_preset(RefreshModeVm::OnOpen, Some(45)),
        RefreshPreset::OnOpen
    );
    assert_eq!(
        refresh_policy_to_preset(RefreshModeVm::Interval, Some(86400)),
        RefreshPreset::Daily
    );
    assert_eq!(
        refresh_policy_to_preset(RefreshModeVm::Interval, Some(3600)),
        RefreshPreset::Hourly
    );
    assert_eq!(
        refresh_policy_to_preset(RefreshModeVm::Interval, Some(45)),
        RefreshPreset::Custom
    );
    assert_eq!(
        refresh_policy_to_preset(RefreshModeVm::Interval, None),
        RefreshPreset::Custom
    );

    // secToUnitAndValue: the largest unit that divides the interval exactly.
    assert_eq!(sec_to_unit_and_value(86400), (IntervalUnit::Days, 1));
    assert_eq!(sec_to_unit_and_value(172800), (IntervalUnit::Days, 2));
    assert_eq!(sec_to_unit_and_value(3600), (IntervalUnit::Hours, 1));
    assert_eq!(sec_to_unit_and_value(7200), (IntervalUnit::Hours, 2));
    assert_eq!(sec_to_unit_and_value(60), (IntervalUnit::Minutes, 1));
    assert_eq!(sec_to_unit_and_value(120), (IntervalUnit::Minutes, 2));
    assert_eq!(sec_to_unit_and_value(45), (IntervalUnit::Seconds, 45));
    assert_eq!(sec_to_unit_and_value(3601), (IntervalUnit::Seconds, 3601));

    // The wire spellings, multipliers and parse fallbacks.
    assert_eq!(RefreshPreset::from_str("nonsense"), RefreshPreset::Custom);
    assert_eq!(IntervalUnit::from_str("nonsense"), IntervalUnit::Seconds);
    assert_eq!(IntervalUnit::Minutes.multiplier(), 60);
    assert_eq!(IntervalUnit::Days.multiplier(), 86400);
}

#[test]
fn set_refresh_preset_applies_the_literals_and_the_custom_clamp() {
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(Some(&widget_with("Tile", interval(45))), None, false, None, "x");
    assert!(state.interval_row_visible(), "45s derives the custom preset");

    for (preset, refresh) in [
        (RefreshPreset::Manual, manual()),
        (
            RefreshPreset::OnOpen,
            RefreshPolicyVm { mode: RefreshModeVm::OnOpen, refresh_sec: None },
        ),
        (RefreshPreset::Daily, interval(86400)),
        (RefreshPreset::Hourly, interval(3600)),
    ] {
        state.set_refresh_preset(preset);
        assert_eq!(state.draft.refresh, refresh, "{preset:?}");
        assert!(!state.force_custom_refresh, "only the custom arm forces");
        assert_eq!(state.open_select, None, "choosing closes the listbox");
        assert!(!state.interval_row_visible(), "a preset hides the row");
    }

    // The custom arm clamps the standing seconds and forces the row.
    state.set_refresh_preset(RefreshPreset::Custom);
    assert!(state.force_custom_refresh);
    assert_eq!(state.draft.refresh, interval(3600), "the standing hourly seconds");
    assert!(state.interval_row_visible());

    state.draft.refresh.refresh_sec = Some(10);
    state.set_refresh_preset(RefreshPreset::Custom);
    assert_eq!(state.draft.refresh.refresh_sec, Some(30), "clampRefreshSec(10)");
}

#[test]
fn interval_typing_and_unit_switches_clamp_through_the_minimum() {
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(Some(&widget_with("Tile", interval(45))), None, false, None, "x");

    // Typing 4 seconds clamps to the minimum and re-derives the field.
    state.interval_edit.replace_text("4".to_owned());
    state.apply_interval_text();
    assert_eq!(state.draft.refresh.refresh_sec, Some(30));
    assert_eq!(state.interval_edit.text, "30", "the controlled value re-derives");

    // Math.max(1, Number(v) || 1): NaN and 0 both fall to 1 — proven against
    // the standing minutes unit (a 0 would clamp to 30, not 60).
    state.draft.refresh.refresh_sec = Some(120); // (minutes, 2)
    state.interval_edit.replace_text("abc".to_owned());
    state.apply_interval_text();
    assert_eq!(state.draft.refresh.refresh_sec, Some(60), "NaN falls to 1 minute");
    state.interval_edit.replace_text("0".to_owned());
    state.apply_interval_text();
    assert_eq!(state.draft.refresh.refresh_sec, Some(60), "0 falls to 1 minute");

    // A real value multiplies by the standing unit.
    state.interval_edit.replace_text("90".to_owned());
    state.apply_interval_text();
    assert_eq!(state.draft.refresh.refresh_sec, Some(5400), "90 minutes");
    assert_eq!(state.derived_interval(), (IntervalUnit::Minutes, 90));

    // The unit select rebases the derived value onto the new unit.
    state.set_interval_unit(IntervalUnit::Seconds);
    assert_eq!(state.draft.refresh.refresh_sec, Some(90));
    assert_eq!(state.derived_interval(), (IntervalUnit::Seconds, 90));
    state.set_interval_unit(IntervalUnit::Days);
    assert_eq!(state.draft.refresh.refresh_sec, Some(90 * 86400));
    assert_eq!(state.derived_interval(), (IntervalUnit::Days, 90));
    assert_eq!(state.open_select, None, "choosing closes the listbox");
}

#[test]
fn the_y_axes_and_threshold_parses_normalize_like_react() {
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(Some(&base_widget()), None, false, None, "x");

    // yAxes: split on commas, trim, drop blanks — then the controlled value
    // re-derives, so a trailing comma vanishes as it is typed.
    state.y_axes_edit.replace_text("v, count,".to_owned());
    state.apply_y_axes_text();
    assert_eq!(
        state.draft.chart_config.as_ref().unwrap().y_axes,
        vec!["v".to_owned(), "count".to_owned()]
    );
    assert_eq!(state.y_axes_edit.text, "v, count");
    state.y_axes_edit.replace_text("  a , , b  ".to_owned());
    state.apply_y_axes_text();
    assert_eq!(
        state.draft.chart_config.as_ref().unwrap().y_axes,
        vec!["a".to_owned(), "b".to_owned()]
    );

    // The threshold: JS Number semantics — a typed "3." normalizes to "3".
    state.set_alert_enabled(true);
    state.threshold_edit.replace_text("3.".to_owned());
    state.apply_threshold_text();
    assert_eq!(state.draft.alert.as_ref().unwrap().threshold, 3.0);
    assert_eq!(state.threshold_edit.text, "3");
    state.threshold_edit.replace_text(String::new());
    state.apply_threshold_text();
    assert_eq!(state.draft.alert.as_ref().unwrap().threshold, 0.0, "blank text is JS's 0");
    assert_eq!(state.threshold_edit.text, "0");
}

#[test]
fn the_alert_rule_seeds_its_defaults_and_edits_its_fields() {
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(Some(&base_widget()), None, false, None, "x");
    assert!(state.draft.alert.is_none());

    // On: a standing rule or React's DEFAULT_ALERT.
    state.set_alert_enabled(true);
    assert_eq!(state.draft.alert.as_ref().unwrap(), &default_alert());
    let alert = state.draft.alert.as_ref().unwrap();
    assert_eq!(alert.op.as_str(), ">");
    assert_eq!(alert.metric.kind, "column");
    assert_eq!(alert.metric.agg, None);
    assert_eq!(alert.cooldown_sec, 300);
    assert_eq!(alert.channels, vec!["app".to_owned()]);

    // Edits while armed (the unedited fields ride along, as spreads keep them).
    state.alert_column_edit.replace_text("amount".to_owned());
    state.apply_alert_column_text();
    state.set_alert_operator(AlertOperatorVm::from_str(">="));
    let alert = state.draft.alert.as_ref().unwrap();
    assert_eq!(alert.metric.column, "amount");
    assert_eq!(alert.op, AlertOperatorVm::Gte);
    assert_eq!(alert.cooldown_sec, 300);

    // Off clears the rule and parks the fields blank / 0.
    state.set_alert_enabled(false);
    assert!(state.draft.alert.is_none());
    assert_eq!(state.alert_column_edit.text, "");
    assert_eq!(state.threshold_edit.text, "0");

    // The operator parse falls back to React's default.
    assert_eq!(AlertOperatorVm::from_str("nonsense"), AlertOperatorVm::Gt);
}

#[test]
fn the_save_guards_and_the_normalized_payload() {
    // handleSave's happy path: trimmed title, clamped refresh, the pair.
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(
        Some(&widget_with("  My Tile  ", interval(10))),
        Some(&hidden_sql()),
        false,
        Some("d1"),
        "x",
    );
    let payload = state.save_payload().unwrap();
    assert_eq!(payload.widget.title, "My Tile");
    assert_eq!(payload.widget.refresh, interval(30), "normalizeRefreshPolicy clamps 10 up");
    assert_eq!(payload.widget.id, "w1");
    assert_eq!(payload.widget.layout, WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 });
    assert_eq!(payload.hidden_sql.as_ref(), Some(&hidden_sql()));
    assert!(payload.alert.is_none());

    // Guard 1: a blank title (whitespace included) saves nothing.
    for title in ["", "   "] {
        state.begin_open(Some(&widget_with(title, interval(45))), None, false, None, "x");
        assert!(state.save_payload().is_none(), "{title:?} is blank after the trim");
    }

    // Guard 2: a hidden drawer without a connection or with blank SQL.
    state.begin_open(
        Some(&base_widget()),
        Some(&HiddenSqlVm { connection_id: String::new(), sql: "SELECT 1 AS v".to_owned() }),
        false,
        None,
        "x",
    );
    assert!(state.save_payload().is_none(), "no connection");
    state.begin_open(
        Some(&base_widget()),
        Some(&HiddenSqlVm { connection_id: "c1".to_owned(), sql: "   ".to_owned() }),
        false,
        None,
        "x",
    );
    assert!(state.save_payload().is_none(), "blank SQL");

    // A workflow drawer carries no hidden pair, and manual drops the seconds.
    state.begin_open(Some(&widget_with("Tile", manual())), None, false, None, "x");
    let payload = state.save_payload().unwrap();
    assert!(payload.hidden_sql.is_none());
    assert_eq!(payload.widget.refresh, manual());
}

#[test]
fn the_dirty_guard_and_the_monitor_gate_read_the_saved_widget() {
    let mut state = WidgetEditorDrawerState::default();
    state.begin_open(Some(&base_widget()), Some(&hidden_sql()), false, Some("d1"), "x");
    assert!(!state.is_dirty(), "the seeds equal the props");

    state.title_edit.replace_text("Typed".to_owned());
    state.apply_title_text();
    assert!(state.is_dirty(), "a draft edit dirties");

    state.title_edit.replace_text("Tile".to_owned());
    state.apply_title_text();
    assert!(!state.is_dirty(), "editing back cleans");
    state.hidden_sql.sql = "SELECT 9 AS v".to_owned();
    assert!(state.is_dirty(), "the hidden pair alone dirties");
    state.hidden_sql = hidden_sql();
    assert!(!state.is_dirty());

    // A widget-less drawer is always dirty (JSON.stringify(draft) !== 'null').
    state.begin_open(None, None, true, None, "x");
    assert!(state.is_dirty());

    // React's mount gate (lines 313-323): dashboardId && widget && !isNew &&
    // widget.refresh.mode === 'interval' — the SAVED widget, not the draft.
    state.begin_open(Some(&base_widget()), Some(&hidden_sql()), false, Some("d1"), "x");
    assert_eq!(state.monitor_card_pair(), Some(("d1", "w1")));
    state.set_refresh_preset(RefreshPreset::Manual);
    assert_eq!(
        state.monitor_card_pair(),
        Some(("d1", "w1")),
        "the draft's mode does not gate the card"
    );
    state.begin_open(Some(&base_widget()), Some(&hidden_sql()), true, Some("d1"), "x");
    assert_eq!(state.monitor_card_pair(), None, "isNew gates");
    state.begin_open(Some(&base_widget()), Some(&hidden_sql()), false, None, "x");
    assert_eq!(state.monitor_card_pair(), None, "no dashboard id gates");
    state.begin_open(Some(&widget_with("Tile", manual())), Some(&hidden_sql()), false, Some("d1"), "x");
    assert_eq!(state.monitor_card_pair(), None, "a manual saved widget gates");
}

// ---- Runtime evidence ---------------------------------------------------------

#[test]
fn a_closed_drawer_builds_nothing() {
    // React test 1: `if (!open) return null`.
    let mut closed = AuditState::open(&base_widget(), Some(&hidden_sql()), false, Some("d1"), Vec::new());
    closed.drawer.close();
    let mut layer = Vec::new();
    WidgetEditorDrawer::<AuditState>::new(&closed.drawer, &closed.workflows, NOW_MS)
        .build(closed.dark, &mut layer);
    assert!(layer.is_empty(), "React renders null while closed");
}

#[test]
fn the_open_drawer_renders_the_shell_and_its_keys() {
    let rt = runtime(AuditState::hidden());
    // The React testids stay verbatim; everything else derives under the
    // root key (the module doc's table).
    assert!(has(&rt, "widget-editor-drawer"), "data-testid root");
    assert!(has(&rt, "widget-editor-drawer-scrim"));
    assert!(has(&rt, "widget-editor-drawer-panel"));
    assert!(has(&rt, "widget-editor-drawer-header"));
    assert!(has(&rt, "widget-editor-drawer-title"));
    assert!(has(&rt, "widget-editor-drawer-close"));
    assert!(has(&rt, "widget-editor-drawer-body"));
    assert!(has(&rt, "widget-editor-drawer-footer"));
    assert_eq!(text_of(&rt, "widget-editor-drawer-title"), t("dashboard.editWidget"));

    // The title field.
    assert!(has(&rt, "widget-editor-title"));
    assert!(has(&rt, "widget-editor-title-field"));
    assert_eq!(text_of(&rt, "widget-editor-title-label"), t("dashboard.widgetTitle"));

    // The hidden-SQL branch (React's hiddenSqlProp != null).
    assert!(has(&rt, "widget-editor-connection"));
    assert!(has(&rt, "widget-editor-connection-select-trigger"));
    assert_eq!(text_of(&rt, "widget-editor-connection-label"), t("dashboard.connection"));
    assert!(has(&rt, "widget-editor-sql"));
    assert!(has(&rt, "dashboard-sql-editor"), "the h-40 border box testid");
    assert!(has(&rt, "sql-editor"), "the seam's surface inside it");
    assert!(!has(&rt, "widget-editor-workflow-block"), "the workflow branch is absent");

    // The refresh policy and its custom interval row (45s).
    assert!(has(&rt, "widget-editor-refresh-mode"));
    assert!(has(&rt, "widget-editor-refresh-mode-select-trigger"));
    assert!(has(&rt, "widget-editor-refresh-interval"));
    assert!(has(&rt, "widget-editor-refresh-interval-row"));
    assert!(has(&rt, "widget-editor-interval-value"));
    assert!(has(&rt, "widget-editor-interval-unit-trigger"));

    // The T146 card under its gate, then the rest of the form.
    assert!(has(&rt, "monitor-schedule-authorization"));
    assert!(has(&rt, "widget-editor-enabled"));
    assert!(has(&rt, "widget-editor-view-mode"));
    assert!(has(&rt, "widget-editor-view-mode-select-trigger"));
    assert!(has(&rt, "widget-editor-chart-type"));
    assert!(has(&rt, "widget-editor-chart-type-select-trigger"));
    assert!(has(&rt, "widget-editor-y-axes"));
    assert!(has(&rt, "widget-editor-y-axes-field"));
    assert!(has(&rt, "widget-editor-alert"));
    assert!(has(&rt, "widget-editor-alert-enabled"));
    assert!(!has(&rt, "widget-editor-alert-fields"), "no rule is armed");
    assert!(has(&rt, "widget-editor-cancel"));
    assert!(has(&rt, "widget-editor-save"));
}

#[test]
fn the_panel_is_right_anchored_and_the_scrim_is_forty_percent_black() {
    let rt = runtime(AuditState::hidden());
    let panel = maybe(&rt, "widget-editor-drawer-panel").expect("the aside is laid out");
    let [x, y, w, h] = panel.rect;
    assert!((w - 448.0).abs() < 0.5, "React max-w-md, got {w}");
    assert!((x + w - WIN_W).abs() < 0.5, "flex justify-end anchors the right edge, got {x}");
    assert!(y.abs() < 0.5, "inset-0 top, got {y}");
    assert!((y + h - WIN_H).abs() < 0.5, "h-full, got {h}");

    let scrim = maybe(&rt, "widget-editor-drawer-scrim").expect("the scrim is laid out");
    assert_eq!(scrim.fill, rune::Color::BLACK.alpha(0.4), "bg-black/40");
    let [sx, sy, sw, sh] = scrim.rect;
    assert!(sx.abs() < 0.5 && sy.abs() < 0.5, "absolute inset-0 at {sx},{sy}");
    assert!((sw - WIN_W).abs() < 0.5 && (sh - WIN_H).abs() < 0.5, "the scrim covers the window");

    // The SQL editor box is React's h-40.
    let sql_box = maybe(&rt, "dashboard-sql-editor").expect("the box is laid out");
    assert!((sql_box.rect[3] - 160.0).abs() < 0.5, "h-40, got {}", sql_box.rect[3]);
}

#[test]
fn the_header_and_footer_follow_the_new_flag() {
    let new_rt = runtime(AuditState::open(&base_widget(), None, true, Some("d1"), workflows()));
    assert_eq!(text_of(&new_rt, "widget-editor-drawer-title"), t("dashboard.newWidget"));
    let rt = runtime(AuditState::open(&base_widget(), None, false, Some("d1"), workflows()));
    assert_eq!(text_of(&rt, "widget-editor-drawer-title"), t("dashboard.editWidget"));

    // The X's accessible label (React's aria-label, the test's own lookup).
    assert_eq!(
        maybe(&rt, "widget-editor-drawer-close").and_then(|e| e.label),
        Some(t("common.close").to_owned())
    );
    // The footer pair.
    assert_eq!(button_label(&rt, "widget-editor-cancel"), t("common.cancel"));
    assert_eq!(button_label(&rt, "widget-editor-save"), t("common.save"));
}

#[test]
fn the_dense_refresh_warning_mounts_below_the_threshold() {
    // React test 2: interval 45 < 60 shows the amber warning.
    let rt = runtime(AuditState::hidden());
    let warn = maybe(&rt, "refresh-sec-warn").expect("data-testid refresh-sec-warn");
    assert_eq!(
        warn.text.as_deref(),
        Some(t("dashboard.refreshSecWarn").replace("{sec}", "60").as_str()),
        "t('dashboard.refreshSecWarn', sec 60)"
    );
    assert_eq!(warn.text_color, Some(palette::alert_amber(true)), "React text-amber-500");
    assert_eq!(
        text_of(&rt, "widget-editor-interval-hint"),
        t("dashboard.refreshSecHint").replace("{min}", "30"),
        "t('dashboard.refreshSecHint', min 30)"
    );
}

#[test]
fn manual_and_preset_modes_hide_the_interval_row() {
    // React test 3: manual shows no warning (and no interval row at all).
    let rt = runtime(AuditState::open(&widget_with("Tile", manual()), None, false, None, workflows()));
    assert!(!has(&rt, "refresh-sec-warn"));
    assert!(!has(&rt, "widget-editor-refresh-interval"));

    // An hourly interval is a preset: no row either.
    let rt = runtime(AuditState::open(&widget_with("Tile", interval(3600)), None, false, None, Vec::new()));
    assert!(!has(&rt, "widget-editor-refresh-interval"));
    assert!(!has(&rt, "refresh-sec-warn"));

    // Choosing custom from manual mounts the row at the minimum (the manual
    // policy has no seconds: ?? MIN_REFRESH_SEC) and arms the warning.
    let mut rt = runtime(AuditState::open(&widget_with("Tile", manual()), None, false, None, workflows()));
    press(&mut rt, "widget-editor-refresh-mode-select-trigger");
    press(&mut rt, "option4"); // custom
    assert!(has(&rt, "widget-editor-refresh-interval-row"));
    assert_eq!(rt.state.drawer.draft.refresh.refresh_sec, Some(30));
    assert!(has(&rt, "refresh-sec-warn"), "30 < 60");
}

#[test]
fn saving_fires_the_slot_with_the_normalized_payload_and_hidden_sql() {
    // React test 4: onSave receives the trimmed title, the clamped refresh
    // policy and the hiddenSql pair.
    let mut rt = runtime(AuditState::open(
        &widget_with("  My Tile  ", interval(10)),
        Some(&hidden_sql()),
        false,
        Some("d1"),
        Vec::new(),
    ));
    press(&mut rt, "widget-editor-save");
    assert_eq!(rt.state.saves.len(), 1, "onSave fired once");
    let saved = rt.state.saves[0].clone();
    assert_eq!(saved.widget.title, "My Tile", "draft.title.trim()");
    assert_eq!(saved.widget.refresh.mode, RefreshModeVm::Interval);
    assert!(saved.widget.refresh.refresh_sec.unwrap() >= 30, "clamped up to the minimum");
    assert_eq!(saved.widget.refresh.refresh_sec, Some(30));
    assert_eq!(saved.hidden_sql, Some(hidden_sql()), "the pair passes through unchanged");
}

#[test]
fn every_field_edits_the_draft_and_saves() {
    // React test 5: title edit, enabled toggle, refresh mode change.
    let widget = WidgetDraftVm { workflow_id: "wf-1".to_owned(), ..widget_with("Original", manual()) };
    let mut rt = runtime(AuditState::open(&widget, None, false, None, workflows()));

    type_into(&mut rt, "widget-editor-title-field", "Updated Title");
    assert_eq!(rt.state.drawer.draft.title, "Updated Title");
    press(&mut rt, "widget-editor-enabled");
    assert!(!rt.state.drawer.draft.enabled, "the checkbox flips the draft");
    press(&mut rt, "widget-editor-refresh-mode-select-trigger");
    press(&mut rt, "option1"); // onOpen
    assert_eq!(rt.state.drawer.draft.refresh.mode, RefreshModeVm::OnOpen);

    press(&mut rt, "widget-editor-save");
    assert_eq!(rt.state.saves.len(), 1);
    let saved = rt.state.saves[0].clone();
    assert_eq!(saved.widget.title, "Updated Title");
    assert!(!saved.widget.enabled);
    assert_eq!(saved.widget.refresh.mode, RefreshModeVm::OnOpen);
    assert_eq!(saved.widget.refresh.refresh_sec, None);
    assert!(saved.hidden_sql.is_none(), "a workflow drawer passes no hidden pair");
}

#[test]
fn the_hidden_sql_seam_edits_and_its_guards_block_blank_saves() {
    // React test 6: the SQL edit rides the save's hiddenSql.
    let mut rt = runtime(AuditState::hidden());
    type_into(&mut rt, "sql-editor", "SELECT 2 AS v");
    assert_eq!(rt.state.drawer.hidden_sql.sql, "SELECT 2 AS v");
    press(&mut rt, "widget-editor-save");
    assert_eq!(rt.state.saves.len(), 1);
    assert_eq!(rt.state.saves[0].hidden_sql.as_ref().unwrap().sql, "SELECT 2 AS v");

    // React's second guard, live: no connection lands nothing.
    let mut rt = runtime(AuditState::open(
        &base_widget(),
        Some(&HiddenSqlVm { connection_id: String::new(), sql: "SELECT 1 AS v".to_owned() }),
        false,
        Some("d1"),
        Vec::new(),
    ));
    press(&mut rt, "widget-editor-save");
    assert!(rt.state.saves.is_empty(), "no connection: the early return");
    // Choosing a connection from the delivered list unblocks the save.
    press(&mut rt, "widget-editor-connection-select-trigger");
    press(&mut rt, "option0"); // Conn 1
    assert_eq!(rt.state.drawer.hidden_sql.connection_id, "c1");
    press(&mut rt, "widget-editor-save");
    assert_eq!(rt.state.saves.len(), 1, "onSave(hiddenSql) now fires");
}

#[test]
fn the_workflow_branch_selects_and_opens_the_editor() {
    // React test 7: the bound workflow shows, and the editor link fires.
    let widget = WidgetDraftVm { workflow_id: "wf-1".to_owned(), ..base_widget() };
    let mut rt = runtime(AuditState::open(&widget, None, false, None, workflows()));
    assert_eq!(rt.state.drawer.draft.workflow_id, "wf-1", "the select's value is the bound id");
    press(&mut rt, "widget-open-workflow-editor");
    assert_eq!(rt.state.workflow_opens, 1, "onOpenWorkflowEditor fired once");

    // Choosing another workflow re-binds the draft.
    press(&mut rt, "widget-editor-workflow-select-trigger");
    press(&mut rt, "option1"); // wf-2
    assert_eq!(rt.state.drawer.draft.workflow_id, "wf-2");

    // React's gate: {onOpenWorkflowEditor && draft.workflowId && …}.
    let empty = WidgetDraftVm { workflow_id: String::new(), ..base_widget() };
    let rt = runtime(AuditState::open(&empty, None, false, None, workflows()));
    assert!(!has(&rt, "widget-open-workflow-editor"), "no bound workflow: no link");
}

#[test]
fn the_workflow_readonly_field_without_user_workflows() {
    let rt = runtime(AuditState::open(&base_widget(), None, false, None, Vec::new()));
    assert!(has(&rt, "widget-editor-workflow-readonly"));
    assert!(!has(&rt, "widget-editor-workflow-select-trigger"), "no select without workflows");
    assert!(shows(&rt, "wf-hidden"), "the read-only field shows the id");
    // React: `{onOpenWorkflowEditor && draft.workflowId && …}` — a truthy id
    // shows the link even with an empty workflows list; clearing the id
    // removes it.
    assert!(has(&rt, "widget-open-workflow-editor"), "a truthy id shows the link");
    let mut rt = rt;
    rt.update(|s, _| s.drawer.draft.workflow_id.clear());
    settle(&mut rt);
    assert!(!has(&rt, "widget-open-workflow-editor"), "no id, no link");
    let field = maybe(&rt, "widget-editor-workflow-readonly").unwrap();
    assert!((field.rect[3] - 40.0).abs() < 0.5, "React Input h-10, got {}", field.rect[3]);
}

#[test]
fn enabling_the_alert_rule_mounts_fields_and_saves_the_config() {
    // React test 8: arm the rule, edit the column, save the config.
    let mut rt = runtime(AuditState::open(
        &widget_with("Alert Tile", interval(45)),
        None,
        false,
        None,
        workflows(),
    ));
    assert!(!has(&rt, "widget-editor-alert-fields"));

    press(&mut rt, "widget-editor-alert-enabled");
    assert!(has(&rt, "widget-editor-alert-fields"));
    assert!(has(&rt, "widget-editor-alert-column-field"));
    assert!(has(&rt, "widget-editor-alert-operator-select-trigger"));
    assert!(has(&rt, "widget-editor-alert-threshold-field"));
    assert_eq!(
        rt.state.drawer.draft.alert.as_ref().unwrap().op.as_str(),
        ">",
        "DEFAULT_ALERT's op"
    );

    type_into(&mut rt, "widget-editor-alert-column-field", "amount");
    type_into(&mut rt, "widget-editor-alert-threshold-field", "3.");
    assert_eq!(rt.state.drawer.draft.alert.as_ref().unwrap().threshold, 3.0);
    assert_eq!(rt.state.drawer.threshold_edit.text, "3", "the controlled value normalizes");
    press(&mut rt, "widget-editor-alert-operator-select-trigger");
    press(&mut rt, "option1"); // '>='
    assert_eq!(rt.state.drawer.draft.alert.as_ref().unwrap().op.as_str(), ">=");

    press(&mut rt, "widget-editor-save");
    let saved = rt.state.saves[0].clone();
    let alert = saved.alert.expect("the rule rides the payload");
    assert_eq!(alert.metric.column, "amount");
    assert_eq!(alert.op.as_str(), ">=");
    assert_eq!(alert.threshold, 3.0);
    assert_eq!(alert.cooldown_sec, 300, "the unedited fields ride along");
}

#[test]
fn chart_fields_hide_under_the_table_view() {
    let mut rt = runtime(AuditState::open(&widget_with("Tile", manual()), None, false, None, workflows()));
    assert!(has(&rt, "widget-editor-chart-type-select-trigger"));
    assert!(has(&rt, "widget-editor-y-axes-field"));

    // The chart-type select re-binds the standing config.
    press(&mut rt, "widget-editor-chart-type-select-trigger");
    press(&mut rt, "option1"); // line
    assert_eq!(
        rt.state.drawer.draft.chart_config.as_ref().unwrap().chart_type,
        ChartType::Line
    );

    // The y-axes field parses as it is typed.
    type_into(&mut rt, "widget-editor-y-axes-field", "v, count,");
    assert_eq!(
        rt.state.drawer.draft.chart_config.as_ref().unwrap().y_axes,
        vec!["v".to_owned(), "count".to_owned()]
    );

    // Table view hides both (React's viewMode gate).
    press(&mut rt, "widget-editor-view-mode-select-trigger");
    press(&mut rt, "option1"); // table
    assert_eq!(rt.state.drawer.draft.view_mode, TileViewMode::Table);
    assert!(!has(&rt, "widget-editor-chart-type-select-trigger"));
    assert!(!has(&rt, "widget-editor-y-axes-field"));

    // And back.
    press(&mut rt, "widget-editor-view-mode-select-trigger");
    press(&mut rt, "option0"); // chart
    assert!(has(&rt, "widget-editor-y-axes-field"));

    // React's second gate: draft.chartConfig must exist.
    let bare = WidgetDraftVm { chart_config: None, ..widget_with("Tile", manual()) };
    let rt = runtime(AuditState::open(&bare, None, false, None, workflows()));
    assert!(!has(&rt, "widget-editor-chart-type-select-trigger"), "no config, no chart fields");
}

#[test]
fn cancel_and_close_fire_onclose_and_reopen_reseeds() {
    // React test 9's cancel journey (its "backdrop" leg is the labelled X).
    let mut rt = runtime(AuditState::hidden());
    press(&mut rt, "widget-editor-cancel");
    assert_eq!(rt.state.closes, 1);
    assert!(!rt.state.drawer.open);
    assert!(!has(&rt, "widget-editor-drawer"), "unmounted on the next build");

    // The header X (React's aria-label={t('common.close')}).
    let mut rt = runtime(AuditState::hidden());
    press(&mut rt, "widget-editor-drawer-close");
    assert_eq!(rt.state.closes, 1);
    assert!(!has(&rt, "widget-editor-drawer"));

    // React's scrim is aria-hidden and handler-less: a press there closes
    // nothing (the port's own documented delta from the usual scrim-close).
    let mut rt = runtime(AuditState::hidden());
    let (x, y) = rt.find("widget-editor-drawer-scrim").expect("the scrim is laid out");
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.closes, 0, "the scrim carries no handler");
    assert!(rt.state.drawer.open);

    // Reopening re-seeds the drafts (the effect's dep array changed).
    rt.update(|s, _| {
        s.drawer.begin_open(Some(&base_widget()), Some(&hidden_sql()), false, Some("d1"), "fresh");
    });
    settle(&mut rt);
    assert!(has(&rt, "widget-editor-drawer"));
    assert_eq!(rt.state.drawer.draft.title, "Tile");
}

#[test]
fn an_empty_title_keeps_the_save_landing_nothing() {
    // React test 10: the blank title blocks onSave; the button is never
    // disabled, so the press lands nothing (the same observable nothing).
    let mut rt = runtime(AuditState::open(&widget_with("", interval(45)), None, false, None, workflows()));
    press(&mut rt, "widget-editor-save");
    assert!(rt.state.saves.is_empty(), "React's early return");
    assert!(has(&rt, "widget-editor-save"), "the button stays mounted");

    // Whitespace-only is still blank after the trim.
    type_into(&mut rt, "widget-editor-title-field", "   ");
    press(&mut rt, "widget-editor-save");
    assert!(rt.state.saves.is_empty());

    // Typing a real title unblocks the save.
    type_into(&mut rt, "widget-editor-title-field", "Revived");
    press(&mut rt, "widget-editor-save");
    assert_eq!(rt.state.saves.len(), 1);
    assert_eq!(rt.state.saves[0].widget.title, "Revived");
}

#[test]
fn the_monitor_card_mounts_under_its_gate_and_fires_its_slots() {
    // The gate: dashboardId && widget && !isNew && the saved widget's interval.
    let rt = runtime(AuditState::hidden());
    assert!(has(&rt, "monitor-schedule-authorization"));

    let rt = runtime(AuditState::open(
        &widget_with("Tile", manual()),
        Some(&hidden_sql()),
        false,
        Some("d1"),
        Vec::new(),
    ));
    assert!(!has(&rt, "monitor-schedule-authorization"), "a manual saved widget mounts no card");

    let rt = runtime(AuditState::open(&base_widget(), Some(&hidden_sql()), true, Some("d1"), Vec::new()));
    assert!(!has(&rt, "monitor-schedule-authorization"), "isNew mounts no card");

    let rt = runtime(AuditState::open(&base_widget(), Some(&hidden_sql()), false, None, Vec::new()));
    assert!(!has(&rt, "monitor-schedule-authorization"), "no dashboard id mounts no card");

    // The card's authorize press runs through the drawer's slot (T146's own
    // audit covers the card), and the draft edits drive its dirty prop.
    let mut rt = runtime(AuditState::hidden());
    assert!(!rt.state.drawer.is_dirty());
    press(&mut rt, "monitor-schedule-authorize");
    assert_eq!(rt.state.monitor_authorizes, 1);
    assert!(rt.state.drawer.monitor.busy, "the host applied the card's begin_change");
    type_into(&mut rt, "widget-editor-title-field", "Typed");
    assert!(rt.state.drawer.is_dirty());
}

#[test]
fn one_listbox_is_open_at_a_time_and_choosing_closes_it() {
    let mut rt = runtime(AuditState::hidden());
    press(&mut rt, "widget-editor-refresh-mode-select-trigger");
    assert!(has(&rt, "widget-editor-refresh-mode-select-content"), "the listbox is mounted");
    assert_eq!(rt.state.drawer.open_select, Some(WidgetEditorSelect::RefreshPreset));
    assert!(rt.find("option0").is_some(), "the preset rows are laid out");

    // The trigger toggles closed.
    press(&mut rt, "widget-editor-refresh-mode-select-trigger");
    assert!(!has(&rt, "widget-editor-refresh-mode-select-content"));

    // Another select opens its own listbox (one at a time).
    press(&mut rt, "widget-editor-view-mode-select-trigger");
    assert!(has(&rt, "widget-editor-view-mode-select-content"));
    assert_eq!(rt.state.drawer.open_select, Some(WidgetEditorSelect::ViewMode));
    assert!(!has(&rt, "widget-editor-refresh-mode-select-content"));

    // Choosing closes it (the set_* appliers).
    press(&mut rt, "option0");
    assert!(!has(&rt, "widget-editor-view-mode-select-content"));
    assert_eq!(rt.state.drawer.open_select, None);
}

#[test]
fn typing_the_interval_clamps_to_the_minimum_and_arms_the_warning() {
    let mut rt = runtime(AuditState::hidden());
    assert!(has(&rt, "widget-editor-refresh-interval-row"), "45s is custom");
    assert!(has(&rt, "refresh-sec-warn"), "45 seconds is below the 60s threshold");
    type_into(&mut rt, "widget-editor-interval-value", "4");
    assert_eq!(rt.state.drawer.draft.refresh.refresh_sec, Some(30), "clampRefreshSec(4 * 1)");
    assert_eq!(rt.state.drawer.interval_edit.text, "30", "the controlled value re-derives");
    assert!(has(&rt, "refresh-sec-warn"), "30 < 60 keeps the warning armed");
}

/// React's `ui/Select` displays the matched option's LABEL on the closed
/// trigger (`packages/ui/src/Select.tsx`: `n?.label ?? placeholder`), so a
/// 45s policy shows `自定义` and a bound workflow shows `Workflow One`.
/// The kit's field renders `Select::current` verbatim, and this port passes
/// the wire value (`custom`, `wf-1`) — these two label assertions are the
/// audit's flag of that user-visible, module-doc-undocumented deviation
/// (the chart-type and operator selects are unaffected: their labels ARE
/// the wire spellings). The listbox rows carry the localized labels either
/// way, so those assertions stand on their own.
#[test]
fn the_closed_triggers_show_the_matched_option_label() {
    let rt = runtime(AuditState::hidden());
    assert!(shows(&rt, t("dashboard.refreshPreset.custom")), "45s renders the custom label");
    assert!(shows(&rt, "bar"), "the chart type shows its label (the wire spelling)");

    let widget = WidgetDraftVm { workflow_id: "wf-1".to_owned(), ..base_widget() };
    let rt = runtime(AuditState::open(&widget, None, false, None, workflows()));
    assert!(shows(&rt, "Workflow One"), "the bound workflow's name, not its id");

    // The open listbox rows are the localized option labels.
    let mut rt = runtime(AuditState::hidden());
    press(&mut rt, "widget-editor-refresh-mode-select-trigger");
    assert_eq!(
        maybe(&rt, "option0").and_then(|e| e.label),
        Some(t("dashboard.refreshMode.manual").to_owned())
    );
    assert_eq!(
        maybe(&rt, "option4").and_then(|e| e.label),
        Some(t("dashboard.refreshPreset.custom").to_owned())
    );
}
