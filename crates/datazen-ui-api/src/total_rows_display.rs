use crate::types::CountStrategy;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TotalRowsView {
    Exact(f64),
    Estimated(f64),
    Unknown,
}

pub fn total_rows_view(kind: Option<CountStrategy>, total_rows: f64) -> TotalRowsView {
    match kind {
        Some(CountStrategy::Estimated) => TotalRowsView::Estimated(total_rows),
        Some(CountStrategy::Unsupported) => TotalRowsView::Unknown,
        _ => TotalRowsView::Exact(total_rows),
    }
}

fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

pub fn format_row_count(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let count = value.max(0.0).floor() as u64;
    if count < 1_000_000 {
        return group_thousands(count);
    }
    if count < 1_000_000_000 {
        return format!("{:.1}M", count as f64 / 1_000_000.0);
    }
    format!("{:.1}B", count as f64 / 1_000_000_000.0)
}

pub fn can_page_forward(view: &TotalRowsView, row_count: f64, page_size: f64) -> bool {
    match view {
        TotalRowsView::Unknown => page_size > 0.0 && row_count >= page_size,
        TotalRowsView::Estimated(_) => true,
        TotalRowsView::Exact(total) => *total > row_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_defaults_missing_kinds_to_exact() {
        assert_eq!(total_rows_view(None, 42.0), TotalRowsView::Exact(42.0));
        assert_eq!(
            total_rows_view(Some(CountStrategy::Exact), 7.0),
            TotalRowsView::Exact(7.0)
        );
        assert_eq!(
            total_rows_view(Some(CountStrategy::Estimated), 7.0),
            TotalRowsView::Estimated(7.0)
        );
        assert_eq!(
            total_rows_view(Some(CountStrategy::Unsupported), 7.0),
            TotalRowsView::Unknown
        );
    }

    #[test]
    fn compact_formatting_uses_commas_m_and_b() {
        assert_eq!(format_row_count(f64::NAN), "0");
        assert_eq!(format_row_count(f64::INFINITY), "0");
        assert_eq!(format_row_count(-5.0), "0");
        assert_eq!(format_row_count(12.9), "12");
        assert_eq!(format_row_count(999.0), "999");
        assert_eq!(format_row_count(1_234.0), "1,234");
        assert_eq!(format_row_count(999_999.0), "999,999");
        assert_eq!(format_row_count(1_000_000.0), "1.0M");
        assert_eq!(format_row_count(1_200_000.0), "1.2M");
        assert_eq!(format_row_count(1_000_000_000.0), "1.0B");
    }

    #[test]
    fn unknown_views_page_only_on_a_full_page() {
        assert!(!can_page_forward(&TotalRowsView::Unknown, 0.0, 50.0));
        assert!(!can_page_forward(&TotalRowsView::Unknown, 49.0, 50.0));
        assert!(can_page_forward(&TotalRowsView::Unknown, 50.0, 50.0));
        assert!(!can_page_forward(&TotalRowsView::Unknown, 50.0, 0.0));
    }

    #[test]
    fn estimates_never_close_the_grid_and_exact_totals_do() {
        assert!(can_page_forward(
            &TotalRowsView::Estimated(1.0),
            100.0,
            50.0
        ));
        assert!(can_page_forward(&TotalRowsView::Exact(100.0), 50.0, 50.0));
        assert!(!can_page_forward(&TotalRowsView::Exact(50.0), 50.0, 50.0));
        assert!(!can_page_forward(&TotalRowsView::Exact(10.0), 50.0, 50.0));
    }
}
