use std::collections::HashMap;
use std::time::Instant;

use crate::dashboard::types::{
    AlertMetricAgg, AlertMetricKind, AlertOperator, AlertRule, DashboardWidget, WidgetRun,
    WidgetRunStatus,
};

pub fn alert_op_str(op: AlertOperator) -> &'static str {
    match op {
        AlertOperator::Gt => ">",
        AlertOperator::Gte => ">=",
        AlertOperator::Lt => "<",
        AlertOperator::Lte => "<=",
        AlertOperator::Eq => "==",
        AlertOperator::Ne => "!=",
    }
}

/// Apply alert evaluation to a completed run (sets `alertValue` / `alertFired`).
pub fn evaluate_run_alert(run: &mut WidgetRun, widget: &DashboardWidget) {
    let Some(rule) = widget.alert.as_ref() else {
        return;
    };
    if run.status != WidgetRunStatus::Ok {
        return;
    }
    if let Some(value) = extract_metric(&run.columns, &run.rows, rule) {
        run.alert_value = Some(value);
        run.alert_fired = Some(eval_threshold(
            value,
            alert_op_str(rule.op),
            rule.threshold,
        ));
    }
}

/// Extract a numeric metric from query result rows per the alert rule.
pub fn extract_metric(
    columns: &[String],
    rows: &[Vec<serde_json::Value>],
    rule: &AlertRule,
) -> Option<f64> {
    let col_idx = columns.iter().position(|c| c == &rule.metric.column)?;
    let values: Vec<f64> = rows
        .iter()
        .filter_map(|row| row.get(col_idx).and_then(json_to_f64))
        .collect();
    if values.is_empty() {
        return None;
    }

    let agg = match rule.metric.kind {
        AlertMetricKind::Column => AlertMetricAgg::Last,
        AlertMetricKind::Aggregation => rule.metric.agg.unwrap_or(AlertMetricAgg::Last),
    };

    apply_agg(&values, agg)
}

fn json_to_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn apply_agg(values: &[f64], agg: AlertMetricAgg) -> Option<f64> {
    match agg {
        AlertMetricAgg::Last => values.last().copied(),
        AlertMetricAgg::Max => values.iter().copied().reduce(f64::max),
        AlertMetricAgg::Min => values.iter().copied().reduce(f64::min),
        AlertMetricAgg::Avg => {
            let sum: f64 = values.iter().sum();
            Some(sum / values.len() as f64)
        }
        AlertMetricAgg::Sum => Some(values.iter().sum()),
    }
}

/// Evaluate whether `value` satisfies the threshold comparison `op`.
pub fn eval_threshold(value: f64, op: &str, threshold: f64) -> bool {
    match op {
        ">" => value > threshold,
        ">=" => value >= threshold,
        "<" => value < threshold,
        "<=" => value <= threshold,
        "==" => value == threshold,
        "!=" => value != threshold,
        _ => false,
    }
}

/// Tracks last notification time per widget for cooldown enforcement.
pub struct CooldownBook {
    last_sent: HashMap<String, Instant>,
}

impl CooldownBook {
    pub fn new() -> Self {
        Self {
            last_sent: HashMap::new(),
        }
    }

    /// Returns true when a notification is allowed (no prior send or cooldown elapsed).
    pub fn should_notify(&mut self, widget_id: &str, cooldown_sec: u64, now: Instant) -> bool {
        match self.last_sent.get(widget_id) {
            None => true,
            Some(&sent_at) => now.duration_since(sent_at).as_secs() >= cooldown_sec,
        }
    }

    pub fn mark_sent(&mut self, widget_id: &str, now: Instant) {
        self.last_sent.insert(widget_id.to_string(), now);
    }
}

impl Default for CooldownBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{
        AlertChannel, AlertMetric, AlertMetricKind, AlertOperator, AlertRule,
    };

    fn sample_rule(kind: AlertMetricKind, agg: Option<AlertMetricAgg>) -> AlertRule {
        AlertRule {
            metric: AlertMetric {
                kind,
                column: "v".into(),
                agg,
            },
            op: AlertOperator::Gt,
            threshold: 0.0,
            cooldown_sec: 300,
            channels: vec![AlertChannel::Desktop],
        }
    }

    fn sample_rows(values: &[f64]) -> (Vec<String>, Vec<Vec<serde_json::Value>>) {
        let columns = vec!["v".into()];
        let rows = values
            .iter()
            .map(|v| vec![serde_json::json!(v)])
            .collect();
        (columns, rows)
    }

    #[test]
    fn extract_metric_last() {
        let (cols, rows) = sample_rows(&[10.0, 20.0, 30.0]);
        let rule = sample_rule(AlertMetricKind::Aggregation, Some(AlertMetricAgg::Last));
        assert_eq!(extract_metric(&cols, &rows, &rule), Some(30.0));
    }

    #[test]
    fn extract_metric_max() {
        let (cols, rows) = sample_rows(&[10.0, 50.0, 30.0]);
        let rule = sample_rule(AlertMetricKind::Aggregation, Some(AlertMetricAgg::Max));
        assert_eq!(extract_metric(&cols, &rows, &rule), Some(50.0));
    }

    #[test]
    fn extract_metric_avg() {
        let (cols, rows) = sample_rows(&[10.0, 20.0, 30.0]);
        let rule = sample_rule(AlertMetricKind::Aggregation, Some(AlertMetricAgg::Avg));
        assert_eq!(extract_metric(&cols, &rows, &rule), Some(20.0));
    }

    #[test]
    fn extract_metric_column_kind_uses_last() {
        let (cols, rows) = sample_rows(&[5.0, 15.0]);
        let rule = sample_rule(AlertMetricKind::Column, None);
        assert_eq!(extract_metric(&cols, &rows, &rule), Some(15.0));
    }

    #[test]
    fn extract_metric_missing_column_returns_none() {
        let (cols, rows) = sample_rows(&[1.0]);
        let mut rule = sample_rule(AlertMetricKind::Column, None);
        rule.metric.column = "missing".into();
        assert_eq!(extract_metric(&cols, &rows, &rule), None);
    }

    #[test]
    fn eval_threshold_gt() {
        assert!(eval_threshold(5.0, ">", 3.0));
        assert!(!eval_threshold(3.0, ">", 5.0));
    }

    #[test]
    fn eval_threshold_gte() {
        assert!(eval_threshold(5.0, ">=", 5.0));
        assert!(!eval_threshold(4.0, ">=", 5.0));
    }

    #[test]
    fn eval_threshold_lt() {
        assert!(eval_threshold(2.0, "<", 5.0));
        assert!(!eval_threshold(5.0, "<", 2.0));
    }

    #[test]
    fn eval_threshold_lte() {
        assert!(eval_threshold(5.0, "<=", 5.0));
        assert!(!eval_threshold(6.0, "<=", 5.0));
    }

    #[test]
    fn eval_threshold_eq() {
        assert!(eval_threshold(5.0, "==", 5.0));
        assert!(!eval_threshold(5.0, "==", 6.0));
    }

    #[test]
    fn eval_threshold_ne() {
        assert!(eval_threshold(5.0, "!=", 6.0));
        assert!(!eval_threshold(5.0, "!=", 5.0));
    }

    #[test]
    fn cooldown_blocks_second_notify() {
        let mut book = CooldownBook::new();
        let t0 = Instant::now();
        assert!(book.should_notify("w1", 60, t0));
        book.mark_sent("w1", t0);
        let t1 = t0 + std::time::Duration::from_secs(30);
        assert!(!book.should_notify("w1", 60, t1));
        let t2 = t0 + std::time::Duration::from_secs(60);
        assert!(book.should_notify("w1", 60, t2));
    }
}
