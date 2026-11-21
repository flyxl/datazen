#[cfg(test)]
mod tests;

use crate::Translate;

pub const EMPTY_KEY_NO_NUMERIC_FIELD: &str = "chart.empty.noNumericField";
pub const EMPTY_KEY_NO_DATA: &str = "chart.empty.noData";
pub const EMPTY_KEY_NO_CONFIG: &str = "chart.empty.noConfig";
pub const EMPTY_CONTAINER_CLASS: &str =
    "flex flex-1 flex-col items-center justify-center gap-3 text-fg-muted";
pub const EMPTY_ICON_CLASS: &str = "h-12 w-12 opacity-30";
pub const EMPTY_TEXT_CLASS: &str = "text-sm";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartEmptyReason {
    NoNumericField,
    NoData,
    NoConfig,
}

impl ChartEmptyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ChartEmptyReason::NoNumericField => "noNumericField",
            ChartEmptyReason::NoData => "noData",
            ChartEmptyReason::NoConfig => "noConfig",
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            ChartEmptyReason::NoNumericField => EMPTY_KEY_NO_NUMERIC_FIELD,
            ChartEmptyReason::NoData => EMPTY_KEY_NO_DATA,
            ChartEmptyReason::NoConfig => EMPTY_KEY_NO_CONFIG,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartEmptyStateView {
    pub container_class: String,
    pub icon_class: String,
    pub text_class: String,
    pub message: String,
}

pub fn build_chart_empty_state(reason: ChartEmptyReason, t: &dyn Translate) -> ChartEmptyStateView {
    ChartEmptyStateView {
        container_class: EMPTY_CONTAINER_CLASS.to_owned(),
        icon_class: EMPTY_ICON_CLASS.to_owned(),
        text_class: EMPTY_TEXT_CLASS.to_owned(),
        message: t.translate(reason.message_key(), &[]),
    }
}
