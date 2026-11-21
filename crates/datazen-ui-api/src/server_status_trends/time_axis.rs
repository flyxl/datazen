use super::types::{TrendAxisOptions, TrendTimeAxis};

pub const HISTORY_WINDOW_MS: i64 = 3_600_000;
pub const HISTORY_MAX: usize = 720;
pub const DEFAULT_MIN_WINDOW_MS: i64 = 60_000;
pub const DEFAULT_REFRESH_PERIOD_MS: i64 = 5_000;
pub const TICK_STEP_CANDIDATES_MS: [i64; 11] = [
    1_000, 5_000, 10_000, 30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000, 3_600_000,
];

pub fn desired_tick_count(window_ms: i64) -> i64 {
    let w = window_ms.max(0);
    let min_window = DEFAULT_MIN_WINDOW_MS as f64;
    let max_window = HISTORY_WINDOW_MS as f64;
    let dense = 10.0;
    let sparse = 4.0;
    let t = ((w as f64 - min_window) / (max_window - min_window)).clamp(0.0, 1.0);
    (dense + (sparse - dense) * t).round() as i64
}

pub fn pick_tick_step(window_ms: i64) -> i64 {
    let target = (window_ms.max(0) as f64) / (desired_tick_count(window_ms) as f64);
    for step in TICK_STEP_CANDIDATES_MS {
        if (step as f64) >= target {
            return step;
        }
    }
    TICK_STEP_CANDIDATES_MS
        .last()
        .copied()
        .unwrap_or(HISTORY_WINDOW_MS)
}

pub fn compute_trend_time_axis(
    data_earliest: i64,
    last_sample: i64,
    opts: &TrendAxisOptions,
) -> TrendTimeAxis {
    let min_window = opts.min_window_ms.unwrap_or(DEFAULT_MIN_WINDOW_MS);
    let max_window = opts.max_window_ms.unwrap_or(HISTORY_WINDOW_MS);
    let span = last_sample - data_earliest;
    let window = span.clamp(min_window, max_window);
    let left = last_sample - window;
    let right = last_sample;

    let step = pick_tick_step(window);
    let first = ((left as f64) / (step as f64)).ceil() as i64 * step;
    let mut ticks = Vec::new();
    let mut t = first;
    while t <= right {
        ticks.push(t);
        t += step;
    }
    TrendTimeAxis {
        domain: (left, right),
        ticks,
    }
}

pub fn align_timestamp(now: i64, period_ms: i64) -> i64 {
    if period_ms <= 0 {
        return now;
    }
    ((now as f64) / (period_ms as f64)).round() as i64 * period_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_pinned() {
        assert_eq!(HISTORY_WINDOW_MS, 3_600_000);
        assert_eq!(HISTORY_MAX, 720);
        assert_eq!(DEFAULT_MIN_WINDOW_MS, 60_000);
        assert_eq!(DEFAULT_REFRESH_PERIOD_MS, 5_000);
        assert_eq!(TICK_STEP_CANDIDATES_MS.len(), 11);
    }

    #[test]
    fn desired_tick_count_interpolates_from_dense_to_sparse() {
        assert_eq!(desired_tick_count(0), 10);
        assert_eq!(desired_tick_count(-100), 10);
        assert_eq!(desired_tick_count(DEFAULT_MIN_WINDOW_MS), 10);
        assert_eq!(desired_tick_count(HISTORY_WINDOW_MS), 4);
        assert_eq!(desired_tick_count(HISTORY_WINDOW_MS * 2), 4);
        assert_eq!(
            desired_tick_count((DEFAULT_MIN_WINDOW_MS + HISTORY_WINDOW_MS) / 2),
            7
        );
    }

    #[test]
    fn tick_step_never_drops_below_the_target() {
        assert_eq!(pick_tick_step(0), 1_000);
        assert_eq!(pick_tick_step(60_000), 10_000);
        assert_eq!(pick_tick_step(3_600_000), 900_000);
    }

    #[test]
    fn axis_clamps_span_and_emits_aligned_ticks() {
        let axis = compute_trend_time_axis(0, 30_000, &TrendAxisOptions::default());
        assert_eq!(axis.domain, (-30_000, 30_000));
        assert_eq!(
            axis.ticks,
            [-30_000, -20_000, -10_000, 0, 10_000, 20_000, 30_000]
        );

        let axis = compute_trend_time_axis(0, 10 * HISTORY_WINDOW_MS, &TrendAxisOptions::default());
        assert_eq!(axis.domain, (9 * HISTORY_WINDOW_MS, 10 * HISTORY_WINDOW_MS));
        assert!(axis
            .ticks
            .last()
            .map_or(false, |tick| *tick <= 10 * HISTORY_WINDOW_MS));
        assert!(axis.ticks.iter().all(|tick| tick % 900_000 == 0));
    }

    #[test]
    fn axis_honors_custom_window_bounds() {
        let opts = TrendAxisOptions {
            min_window_ms: Some(2_000),
            max_window_ms: Some(4_000),
        };
        let axis = compute_trend_time_axis(0, 100_000, &opts);
        assert_eq!(axis.domain, (96_000, 100_000));
        assert!(!axis.ticks.is_empty());
    }

    #[test]
    fn align_timestamp_rounds_to_the_period() {
        assert_eq!(align_timestamp(17_203, 5_000), 15_000);
        assert_eq!(align_timestamp(18_000, 5_000), 20_000);
        assert_eq!(align_timestamp(12_345, 0), 12_345);
        assert_eq!(align_timestamp(12_345, -1), 12_345);
    }
}
