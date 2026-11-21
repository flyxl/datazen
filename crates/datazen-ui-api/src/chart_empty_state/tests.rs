use super::*;

fn translator(key: &str, _params: &[(&str, String)]) -> String {
    format!("T:{key}")
}

#[test]
fn reason_literals_and_message_keys_are_pinned() {
    assert_eq!(ChartEmptyReason::NoNumericField.as_str(), "noNumericField");
    assert_eq!(ChartEmptyReason::NoData.as_str(), "noData");
    assert_eq!(ChartEmptyReason::NoConfig.as_str(), "noConfig");
    assert_eq!(
        ChartEmptyReason::NoNumericField.message_key(),
        "chart.empty.noNumericField"
    );
    assert_eq!(ChartEmptyReason::NoData.message_key(), "chart.empty.noData");
    assert_eq!(
        ChartEmptyReason::NoConfig.message_key(),
        "chart.empty.noConfig"
    );
    assert_eq!(EMPTY_KEY_NO_NUMERIC_FIELD, "chart.empty.noNumericField");
    assert_eq!(EMPTY_KEY_NO_DATA, "chart.empty.noData");
    assert_eq!(EMPTY_KEY_NO_CONFIG, "chart.empty.noConfig");
}

#[test]
fn the_view_keeps_the_container_icon_and_text_contract() {
    let view = build_chart_empty_state(ChartEmptyReason::NoData, &translator as &dyn Translate);
    assert_eq!(
        view.container_class,
        "flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted"
    );
    assert_eq!(view.icon_class, "h-12 w-12 opacity-30");
    assert_eq!(view.text_class, "text-sm");
    assert_eq!(view.message, "T:chart.empty.noData");
}

#[test]
fn each_reason_resolves_its_own_message() {
    for reason in [
        ChartEmptyReason::NoNumericField,
        ChartEmptyReason::NoData,
        ChartEmptyReason::NoConfig,
    ] {
        let view = build_chart_empty_state(reason, &translator as &dyn Translate);
        assert_eq!(view.message, format!("T:{}", reason.message_key()));
    }
}

#[test]
fn crate_root_reexports_the_empty_state_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ChartEmptyReason>();
    resolves::<crate::ChartEmptyStateView>();
    assert_eq!(crate::ChartEmptyReason::NoData.as_str(), "noData");
}
