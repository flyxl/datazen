use std::collections::HashMap;

use serde_json::Value;

use super::time_axis::{
    align_timestamp, DEFAULT_REFRESH_PERIOD_MS, HISTORY_MAX, HISTORY_WINDOW_MS,
};
use super::types::{
    StatusRecord, TrendKind, TrendPoint, TrendPrevSample, TrendSeries, TrendUpdateInput,
    TrendUpdateResult, COMMAND_KEYS, TREND_SERIES,
};

const COUNTER_KEYS: [&str; 14] = [
    "questionsCounter",
    "xactCommitCounter",
    "xactRollbackCounter",
    "newSessionsCounter",
    "bytesInCounter",
    "bytesOutCounter",
    "blksReadCounter",
    "blksHitCounter",
    "tupFetchedCounter",
    "tupReturnedCounter",
    "tupInsertedCounter",
    "tupUpdatedCounter",
    "tupDeletedCounter",
    "tempBytesCounter",
];

pub fn number_from(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64().filter(|value| value.is_finite()),
        Some(Value::String(text)) if !text.trim().is_empty() => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}

fn append_point(series: Option<&TrendSeries>, now: i64, value: f64) -> TrendSeries {
    let mut next: TrendSeries = series
        .unwrap_or(&Vec::new())
        .iter()
        .copied()
        .filter(|point| point.t >= now - HISTORY_WINDOW_MS)
        .collect();
    next.push(TrendPoint { t: now, v: value });
    let excess = next.len().saturating_sub(HISTORY_MAX);
    next.into_iter().skip(excess).collect()
}

fn rate_for(
    counter_key: &str,
    record: &StatusRecord,
    prev: &TrendPrevSample,
    dt_sec: f64,
    uptime: Option<f64>,
) -> Option<f64> {
    let current = number_from(record.get(counter_key))?;
    if dt_sec > 0.0 {
        if let Some(previous) = prev.values.get(counter_key).copied().flatten() {
            let delta = current - previous;
            return Some(if delta < 0.0 { 0.0 } else { delta / dt_sec });
        }
    }
    if let Some(uptime) = uptime {
        if uptime > 0.0 {
            return Some(current / uptime);
        }
    }
    None
}

fn command_rate(
    current: f64,
    previous: Option<f64>,
    dt_sec: f64,
    uptime: Option<f64>,
) -> Option<f64> {
    if dt_sec > 0.0 {
        if let Some(previous) = previous {
            let delta = current - previous;
            return Some(if delta < 0.0 { 0.0 } else { delta / dt_sec });
        }
    }
    if let Some(uptime) = uptime {
        if uptime > 0.0 {
            return Some(current / uptime);
        }
    }
    None
}

pub fn update_trend_history(input: &TrendUpdateInput) -> TrendUpdateResult {
    let period_ms = input.period_ms.unwrap_or(DEFAULT_REFRESH_PERIOD_MS);
    let now = align_timestamp(input.now, period_ms);
    let dt_sec = if input.prev.ts > 0 {
        (now - input.prev.ts) as f64 / 1_000.0
    } else {
        0.0
    };
    let uptime = number_from(input.record.get("uptimeSeconds"));

    let mut history = HashMap::new();
    for (key, series) in &input.history {
        let kept: TrendSeries = series
            .iter()
            .copied()
            .filter(|point| point.t >= now - HISTORY_WINDOW_MS)
            .collect();
        let excess = kept.len().saturating_sub(HISTORY_MAX);
        let trimmed: TrendSeries = kept.into_iter().skip(excess).collect();
        if !trimmed.is_empty() {
            history.insert(key.clone(), trimmed);
        }
    }

    let mut prev_values = input.prev.values.clone();

    for (series_key, definition) in TREND_SERIES.iter() {
        if definition.kind != TrendKind::Rate {
            continue;
        }
        let counter = match definition.counter {
            Some(counter) => counter,
            None => continue,
        };
        if let Some(rate) = rate_for(counter, &input.record, &input.prev, dt_sec, uptime) {
            history.insert(
                (*series_key).to_owned(),
                append_point(history.get(*series_key), now, rate),
            );
        }
    }

    for (series_key, definition) in TREND_SERIES.iter() {
        if definition.kind != TrendKind::Gauge {
            continue;
        }
        let field = match definition.field {
            Some(field) => field,
            None => continue,
        };
        if let Some(value) = number_from(input.record.get(field)) {
            if value >= 0.0 {
                history.insert(
                    (*series_key).to_owned(),
                    append_point(history.get(*series_key), now, value),
                );
            }
        }
    }

    if let Some(Value::Object(statements)) = input.statements.as_ref() {
        for command in COMMAND_KEYS.iter() {
            let key = command.as_str();
            let current = match number_from(statements.get(key)) {
                Some(value) => value,
                None => continue,
            };
            let prev_key = format!("__stmt_{key}");
            let previous = input.prev.values.get(&prev_key).copied().flatten();
            if let Some(rate) = command_rate(current, previous, dt_sec, uptime) {
                let series_key = format!("cmd_{key}");
                history.insert(
                    series_key.clone(),
                    append_point(history.get(&series_key), now, rate),
                );
            }
            prev_values.insert(prev_key, Some(current));
        }
    }

    for key in COUNTER_KEYS.iter() {
        prev_values.insert((*key).to_owned(), number_from(input.record.get(*key)));
    }

    TrendUpdateResult {
        history,
        prev: TrendPrevSample {
            ts: now,
            values: prev_values,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn test_record() -> StatusRecord {
        let mut record = StatusRecord::new();
        record.insert("uptimeSeconds".to_owned(), json!(100));
        record.insert("questionsCounter".to_owned(), json!(1000));
        record.insert("xactCommitCounter".to_owned(), json!("200"));
        record.insert("sessionTotal".to_owned(), json!(5));
        record.insert("sessionActive".to_owned(), json!(-1));
        record
    }

    #[test]
    fn number_from_accepts_finite_numbers_and_numeric_strings() {
        assert_eq!(number_from(Some(&json!(12.5))), Some(12.5));
        assert_eq!(number_from(Some(&json!(" 42 "))), Some(42.0));
        assert_eq!(number_from(Some(&json!(true))), None);
        assert_eq!(number_from(Some(&Value::Null)), None);
        assert_eq!(number_from(Some(&json!(""))), None);
        assert_eq!(number_from(Some(&json!("oops"))), None);
        assert_eq!(number_from(None), None);
    }

    #[test]
    fn first_sample_seeds_rates_with_lifetime_average() {
        let input = TrendUpdateInput {
            record: test_record(),
            now: 17_203,
            period_ms: Some(5_000),
            prev: TrendPrevSample::default(),
            history: HashMap::new(),
            statements: None,
        };
        let result = update_trend_history(&input);
        let qps = result.history.get("qps").expect("qps series exists");
        assert_eq!(qps.len(), 1);
        assert_eq!(qps[0].t, 15_000);
        assert_eq!(qps[0].v, 10.0);
        let commits = result
            .history
            .get("commits")
            .expect("commits series exists");
        assert_eq!(commits[0].v, 2.0);
        assert_eq!(result.prev.ts, 15_000);
        assert_eq!(
            result.prev.values.get("questionsCounter"),
            Some(&Some(1000.0))
        );
        assert_eq!(
            result.prev.values.get("xactCommitCounter"),
            Some(&Some(200.0))
        );
    }

    #[test]
    fn subsequent_samples_diff_the_counter_and_clamp_negative_deltas() {
        let mut prev_values = HashMap::new();
        prev_values.insert("questionsCounter".to_owned(), Some(1_000.0));
        let input = TrendUpdateInput {
            record: test_record(),
            now: 25_000,
            prev: TrendPrevSample {
                ts: 20_000,
                values: prev_values,
            },
            history: HashMap::new(),
            statements: None,
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        assert_eq!(result.history["qps"][0].v, 0.0);

        let mut record = test_record();
        record.insert("questionsCounter".to_owned(), json!(1_050));
        let input = TrendUpdateInput {
            record,
            now: 25_000,
            prev: TrendPrevSample {
                ts: 20_000,
                values: {
                    let mut values = HashMap::new();
                    values.insert("questionsCounter".to_owned(), Some(1_000.0));
                    values
                },
            },
            history: HashMap::new(),
            statements: None,
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        assert_eq!(result.history["qps"][0].v, 10.0);
    }

    #[test]
    fn gauges_keep_non_negative_instant_values_only() {
        let input = TrendUpdateInput {
            record: test_record(),
            now: 10_000,
            prev: TrendPrevSample::default(),
            history: HashMap::new(),
            statements: None,
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        assert_eq!(result.history["sessionTotal"][0].v, 5.0);
        assert!(!result.history.contains_key("sessionActive"));
    }

    #[test]
    fn stale_history_points_are_dropped_and_capped() {
        let mut history = HashMap::new();
        history.insert(
            "qps".to_owned(),
            vec![
                TrendPoint { t: 0, v: 1.0 },
                TrendPoint { t: 200_000, v: 2.0 },
            ],
        );
        let input = TrendUpdateInput {
            record: test_record(),
            now: 3_700_000,
            prev: TrendPrevSample::default(),
            history,
            statements: None,
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        let qps = &result.history["qps"];
        assert_eq!(qps.len(), 2);
        assert_eq!(qps[0].t, 200_000);
        assert_eq!(qps[1].t, 3_700_000);
    }

    #[test]
    fn command_series_use_statement_counters_and_store_baselines() {
        let mut record = test_record();
        record.insert("uptimeSeconds".to_owned(), json!(10));
        let statements = json!({"select": 100, "insert": 20, "update": "garbage", "delete": 5});
        let input = TrendUpdateInput {
            record,
            now: 10_000,
            prev: TrendPrevSample::default(),
            history: HashMap::new(),
            statements: Some(statements),
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        assert_eq!(result.history["cmd_select"][0].v, 10.0);
        assert_eq!(result.history["cmd_delete"][0].v, 0.5);
        assert!(!result.history.contains_key("cmd_update"));
        assert_eq!(result.prev.values.get("__stmt_select"), Some(&Some(100.0)));

        let statements = json!({"select": 130, "insert": 20, "update": 0, "delete": 5});
        let mut prev_values = HashMap::new();
        prev_values.insert("__stmt_select".to_owned(), Some(100.0));
        let input = TrendUpdateInput {
            record: {
                let mut value = test_record();
                value.insert("uptimeSeconds".to_owned(), json!(10));
                value
            },
            now: 15_000,
            prev: TrendPrevSample {
                ts: 10_000,
                values: prev_values,
            },
            history: HashMap::new(),
            statements: Some(statements),
            period_ms: Some(5_000),
        };
        let result = update_trend_history(&input);
        assert_eq!(result.history["cmd_select"][0].v, 6.0);
        assert_eq!(result.history["cmd_insert"][0].v, 2.0);
    }
}
