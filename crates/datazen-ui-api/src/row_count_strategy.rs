use crate::types::{CountStrategy, RowCountMode};

pub struct CountStrategyInput {
    pub mode: RowCountMode,
    pub threshold: f64,
    pub known_row_count: Option<f64>,
}

pub fn resolve_count_strategy(input: &CountStrategyInput) -> Option<CountStrategy> {
    match input.mode {
        RowCountMode::None => Some(CountStrategy::Unsupported),
        RowCountMode::Exact => Some(CountStrategy::Exact),
        RowCountMode::Estimated => Some(CountStrategy::Estimated),
        RowCountMode::Auto => {
            let Some(known) = input.known_row_count else {
                return None;
            };
            if !known.is_finite() {
                return None;
            }
            if !input.threshold.is_finite() || input.threshold <= 0.0 {
                return Some(CountStrategy::Exact);
            }
            if known > input.threshold {
                Some(CountStrategy::Estimated)
            } else {
                Some(CountStrategy::Exact)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(mode: RowCountMode, threshold: f64, known: Option<f64>) -> CountStrategyInput {
        CountStrategyInput { mode, threshold, known_row_count: known }
    }

    #[test]
    fn explicit_modes_map_directly() {
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::None, 100.0, Some(999.0))),
            Some(CountStrategy::Unsupported)
        );
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::Exact, 100.0, None)),
            Some(CountStrategy::Exact)
        );
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::Estimated, 100.0, Some(1.0))),
            Some(CountStrategy::Estimated)
        );
    }

    #[test]
    fn auto_without_a_known_size_has_no_opinion() {
        assert_eq!(resolve_count_strategy(&input(RowCountMode::Auto, 100.0, None)), None);
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(resolve_count_strategy(&input(RowCountMode::Auto, 100.0, Some(invalid))), None);
        }
    }

    #[test]
    fn auto_uses_a_strict_greater_than_comparison() {
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::Auto, 100.0, Some(101.0))),
            Some(CountStrategy::Estimated)
        );
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::Auto, 100.0, Some(100.0))),
            Some(CountStrategy::Exact)
        );
        assert_eq!(
            resolve_count_strategy(&input(RowCountMode::Auto, 100.0, Some(0.0))),
            Some(CountStrategy::Exact)
        );
    }

    #[test]
    fn a_dirty_threshold_always_counts() {
        for threshold in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                resolve_count_strategy(&input(RowCountMode::Auto, threshold, Some(999_999.0))),
                Some(CountStrategy::Exact),
                "threshold {threshold}"
            );
        }
    }
}
