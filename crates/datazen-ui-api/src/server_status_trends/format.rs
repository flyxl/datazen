use std::collections::HashMap;

use serde_json::Value;

use super::types::TrendSeries;

pub fn format_rate(value: f64) -> String {
    if value < 10.0 {
        format!("{value:.2}")
    } else if value < 1_000.0 {
        format!("{value:.1}")
    } else if value < 1_000_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{:.1}M", value / 1_000_000.0)
    }
}

pub fn format_bytes(value: f64) -> String {
    if value < 1_024.0 {
        format!("{value:.1} B")
    } else if value < 1_024.0 * 1_024.0 {
        format!("{:.1} KB", value / 1_024.0)
    } else if value < 1_024.0 * 1_024.0 * 1_024.0 {
        format!("{:.1} MB", value / 1_024.0 / 1_024.0)
    } else {
        format!("{:.1} GB", value / 1_024.0 / 1_024.0 / 1_024.0)
    }
}

pub fn format_byte_rate(value: f64) -> String {
    format!("{}/s", format_bytes(value))
}

pub fn latest_series_value(series: Option<&TrendSeries>) -> Option<f64> {
    series.and_then(|points| points.last()).map(|point| point.v)
}

fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => "[object Object]".to_owned(),
    }
}

pub fn card_rate_from_history(
    history: &HashMap<String, TrendSeries>,
    chart_key: &str,
    fallback: Option<&Value>,
) -> String {
    if let Some(value) = latest_series_value(history.get(chart_key)) {
        return format_rate(value);
    }
    match fallback {
        None => "—".to_owned(),
        Some(Value::Null) => "—".to_owned(),
        Some(Value::String(text)) if text.is_empty() => "—".to_owned(),
        Some(value) => js_string(value),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::server_status_trends::types::TrendPoint;

    #[test]
    fn format_rate_uses_the_four_bands() {
        assert_eq!(format_rate(0.0), "0.00");
        assert_eq!(format_rate(5.0), "5.00");
        assert_eq!(format_rate(12.0), "12.0");
        assert_eq!(format_rate(999.0), "999.0");
        assert_eq!(format_rate(1_500.0), "1.5K");
        assert_eq!(format_rate(2_500_000.0), "2.5M");
    }

    #[test]
    fn format_bytes_and_byte_rate_use_binary_units() {
        assert_eq!(format_bytes(0.0), "0.0 B");
        assert_eq!(format_bytes(1_024.0), "1.0 KB");
        assert_eq!(format_bytes(1_048_576.0), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824.0), "1.0 GB");
        assert_eq!(format_byte_rate(24.0), "24.0 B/s");
        assert_eq!(format_byte_rate(2_048.0), "2.0 KB/s");
    }

    #[test]
    fn latest_series_value_is_the_last_point() {
        assert_eq!(latest_series_value(None), None);
        assert_eq!(latest_series_value(Some(&Vec::new())), None);
        let series = vec![TrendPoint { t: 1, v: 3.0 }, TrendPoint { t: 2, v: 7.5 }];
        assert_eq!(latest_series_value(Some(&series)), Some(7.5));
    }

    #[test]
    fn card_prefers_the_latest_series_point() {
        let mut history = HashMap::new();
        history.insert(
            "qps".to_owned(),
            vec![TrendPoint { t: 1, v: 2.0 }, TrendPoint { t: 2, v: 12.0 }],
        );
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&json!("fallback"))),
            "12.0"
        );
    }

    #[test]
    fn card_falls_back_to_wire_values_or_the_dash() {
        let history = HashMap::new();
        assert_eq!(card_rate_from_history(&history, "qps", None), "—");
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&Value::Null)),
            "—"
        );
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&json!(""))),
            "—"
        );
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&json!(42))),
            "42"
        );
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&json!(true))),
            "true"
        );
        assert_eq!(
            card_rate_from_history(&history, "qps", Some(&json!({"a": 1}))),
            "[object Object]"
        );
    }
}
