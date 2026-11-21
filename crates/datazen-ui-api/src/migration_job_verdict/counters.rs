use super::types::JobCounterValue;

const MAX_SAFE_INTEGER: u128 = 9_007_199_254_740_991;

pub fn read_job_counter(value: &JobCounterValue) -> Option<i64> {
    match value {
        JobCounterValue::Number(number) => {
            if !number.is_finite() || *number < 0.0 {
                return None;
            }
            Some(number.trunc() as i64)
        }
        JobCounterValue::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let parsed = trimmed.parse::<u128>().ok()?;
            if parsed > MAX_SAFE_INTEGER {
                return None;
            }
            Some(parsed as i64)
        }
        JobCounterValue::Null => None,
    }
}

pub fn sum_job_counters(values: &[JobCounterValue]) -> Option<i64> {
    let mut total = 0i64;
    for value in values {
        total = total.checked_add(read_job_counter(value)?)?;
    }
    Some(total)
}

pub fn is_migration_job_in_flight(state: &str) -> bool {
    state == "queued" || state == "running"
}
