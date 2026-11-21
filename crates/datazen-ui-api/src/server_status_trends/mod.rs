mod format;
mod history;
mod time_axis;
mod types;

pub use format::{
    card_rate_from_history, format_byte_rate, format_bytes, format_rate, latest_series_value,
};
pub use history::{number_from, update_trend_history};
pub use time_axis::{
    align_timestamp, compute_trend_time_axis, desired_tick_count, pick_tick_step,
    DEFAULT_MIN_WINDOW_MS, DEFAULT_REFRESH_PERIOD_MS, HISTORY_MAX, HISTORY_WINDOW_MS,
    TICK_STEP_CANDIDATES_MS,
};
pub use types::{
    CommandKey, StatusRecord, TrendAxisOptions, TrendKind, TrendPoint, TrendPrevSample,
    TrendSeries, TrendSeriesDef, TrendTimeAxis, TrendUpdateInput, TrendUpdateResult, COMMAND_KEYS,
    TREND_SERIES,
};
