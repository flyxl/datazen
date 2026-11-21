use super::*;

fn translator(key: &str, _params: &[(&str, String)]) -> String {
    format!("T:{key}")
}

fn context<'a>(
    config: &'a ChartConfig,
    on_toggle_split: bool,
    on_expand: bool,
    chart_available: bool,
) -> ToolbarContext<'a> {
    ToolbarContext {
        config,
        on_toggle_split,
        on_expand,
        chart_available,
        now_ms: 1_700_000_000_000,
    }
}

#[test]
fn chart_type_labels_testids_and_classes_are_pinned() {
    let expected = [
        (ChartType::Bar, "chart.type.bar", "chart-type-bar"),
        (ChartType::Line, "chart.type.line", "chart-type-line"),
        (ChartType::Pie, "chart.type.pie", "chart-type-pie"),
        (ChartType::Scatter, "chart.type.scatter", "chart-type-scatter"),
        (ChartType::Area, "chart.type.area", "chart-type-area"),
    ];
    for (chart_type, label, test_id) in expected {
        assert_eq!(chart_type_label_key(chart_type), label);
        assert_eq!(chart_type_test_id(chart_type), test_id);
    }
    assert_eq!(CHART_TYPES.len(), 5);
    assert_eq!(ACTIVE_CLASS, "bg-accent/20 text-accent");
    assert_eq!(INACTIVE_CLASS, "text-fg-muted hover:text-fg-secondary");
    assert_eq!(TOOLBAR_SHELL_CLASS, "gap-1 px-2 py-1");
    assert_eq!(chart_type_class(true), ACTIVE_CLASS);
    assert_eq!(chart_type_class(false), INACTIVE_CLASS);
    assert_eq!(split_class(true), ACTIVE_CLASS);
    assert!(split_class(false).contains("hover:bg-surface"));
    assert_eq!(ChartExportFormat::Png.as_str(), "png");
    assert_eq!(ChartExportFormat::Svg.as_str(), "svg");
}

#[test]
fn all_translation_keys_are_pinned() {
    assert_eq!(KEY_LEGEND, "chart.legend");
    assert_eq!(KEY_GRID, "chart.grid");
    assert_eq!(KEY_VALUES, "chart.values");
    assert_eq!(KEY_SPLIT_VIEW, "chart.splitView");
    assert_eq!(KEY_EXPAND, "chart.expand");
    assert_eq!(KEY_EXPORT, "chart.export");
    let labels = resolve_toolbar_labels(&translator as &dyn Translate);
    assert_eq!(labels.legend, "T:chart.legend");
    assert_eq!(labels.grid, "T:chart.grid");
    assert_eq!(labels.values, "T:chart.values");
    assert_eq!(labels.split_view, "T:chart.splitView");
    assert_eq!(labels.expand, "T:chart.expand");
    assert_eq!(labels.export_label, "T:chart.export");
    assert_eq!(labels.chart_types.len(), 5);
    assert_eq!(labels.chart_types[0], (ChartType::Bar, "T:chart.type.bar".to_owned()));
}

#[test]
fn visible_controls_follow_the_render_guards() {
    let config = ChartConfig::default();
    let ctx = context(&config, true, true, true);
    let state = ChartToolbarState::default();
    let base = visible_controls(&ctx, &state);
    assert_eq!(base.len(), 10);
    assert_eq!(base[0], ToolbarControl::ChartType(ChartType::Bar));
    assert_eq!(base[4], ToolbarControl::ChartType(ChartType::Area));
    assert_eq!(base[5], ToolbarControl::ShowLegend(config.show_legend));
    assert_eq!(base[6], ToolbarControl::ShowGrid(config.show_grid));
    assert_eq!(base[7], ToolbarControl::ShowValues(config.show_values));
    assert!(!base.contains(&ToolbarControl::Split));
    assert_eq!(base[8], ToolbarControl::Expand);
    assert_eq!(base[9], ToolbarControl::ExportButton);

    let mut two_axes = config.clone();
    two_axes.y_axes = vec!["a".to_owned(), "b".to_owned()];
    let with_split = context(&two_axes, true, false, true);
    let controls = visible_controls(&with_split, &state);
    assert!(controls.contains(&ToolbarControl::Split));
    assert!(!controls.contains(&ToolbarControl::Expand));

    let no_callback = context(&two_axes, false, false, true);
    assert!(!visible_controls(&no_callback, &state).contains(&ToolbarControl::Split));

    let opened = ChartToolbarState { export_open: true };
    let controls = visible_controls(&ctx, &opened);
    assert_eq!(controls.last(), Some(&ToolbarControl::ExportSvg));
    assert!(controls.contains(&ToolbarControl::ExportOverlay));
    assert!(controls.contains(&ToolbarControl::ExportPng));
}

#[test]
fn chart_type_and_toggle_controls_produce_the_next_config() {
    let config = ChartConfig::default();
    let ctx = context(&config, false, false, true);
    let state = ChartToolbarState::default();

    let outcome = reduce_toolbar(&state, ToolbarControl::ChartType(ChartType::Pie), &ctx);
    let next = match outcome.config {
        Some(value) => value,
        None => panic!("chart type change must produce a config"),
    };
    assert_eq!(next.chart_type, ChartType::Pie);
    assert_eq!(next.show_legend, config.show_legend);
    assert!(outcome.effects.is_empty());
    assert_eq!(outcome.state, state);

    let legend = reduce_toolbar(&state, ToolbarControl::ShowLegend(false), &ctx);
    assert_eq!(legend.config.map(|value| value.show_legend), Some(false));
    let grid = reduce_toolbar(&state, ToolbarControl::ShowGrid(false), &ctx);
    assert_eq!(grid.config.map(|value| value.show_grid), Some(false));
    let values = reduce_toolbar(&state, ToolbarControl::ShowValues(true), &ctx);
    assert_eq!(values.config.map(|value| value.show_values), Some(true));
}

#[test]
fn split_and_expand_effects_obey_their_guards() {
    let mut config = ChartConfig::default();
    config.y_axes = vec!["a".to_owned(), "b".to_owned()];
    let allowed = context(&config, true, true, true);
    let state = ChartToolbarState::default();
    assert_eq!(
        reduce_toolbar(&state, ToolbarControl::Split, &allowed).effects,
        vec![ToolbarEffect::ToggleSplit]
    );
    assert_eq!(
        reduce_toolbar(&state, ToolbarControl::Expand, &allowed).effects,
        vec![ToolbarEffect::Expand]
    );

    let denied = context(&config, false, false, true);
    assert!(reduce_toolbar(&state, ToolbarControl::Split, &denied).effects.is_empty());
    assert!(reduce_toolbar(&state, ToolbarControl::Expand, &denied).effects.is_empty());

    let single_axis = ChartConfig::default();
    let one = context(&single_axis, true, false, true);
    assert!(reduce_toolbar(&state, ToolbarControl::Split, &one).effects.is_empty());
}

#[test]
fn export_dropdown_toggles_and_exports_only_with_an_element() {
    let config = ChartConfig::default();
    let ctx = context(&config, false, false, true);
    let state = ChartToolbarState::default();
    let opened = reduce_toolbar(&state, ToolbarControl::ExportButton, &ctx);
    assert!(opened.state.export_open);

    let png = reduce_toolbar(&opened.state, ToolbarControl::ExportPng, &ctx);
    assert!(!png.state.export_open);
    assert_eq!(
        png.effects,
        vec![ToolbarEffect::Export {
            format: ChartExportFormat::Png,
            filename: "chart-1700000000000".to_owned(),
        }]
    );
    let svg = reduce_toolbar(&state, ToolbarControl::ExportSvg, &ctx);
    assert_eq!(
        svg.effects,
        vec![ToolbarEffect::Export {
            format: ChartExportFormat::Svg,
            filename: "chart-1700000000000".to_owned(),
        }]
    );

    let closed = reduce_toolbar(&opened.state, ToolbarControl::ExportOverlay, &ctx);
    assert!(!closed.state.export_open);
    assert!(closed.effects.is_empty());

    let no_element = context(&config, false, false, false);
    let none = reduce_toolbar(&state, ToolbarControl::ExportSvg, &no_element);
    assert!(!none.state.export_open);
    assert!(none.effects.is_empty());
}

#[test]
fn crate_root_reexports_the_toolbar_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ToolbarControl>();
    resolves::<crate::ToolbarEffect>();
    resolves::<crate::ToolbarOutcome>();
    resolves::<crate::ToolbarContext>();
    resolves::<crate::ToolbarLabels>();
    assert_eq!(crate::chart_type_test_id(crate::ChartType::Bar), "chart-type-bar");
    assert_eq!(crate::ChartToolbarState::default().export_open, false);
}
