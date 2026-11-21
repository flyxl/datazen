pub const HISTORY_UNKNOWN_DB_KEY: &str = "__unknown_db__";

pub trait HistoryGroupEntry {
    fn database(&self) -> Option<&str>;
    fn executed_at(&self) -> &str;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryHistoryGroup<T> {
    pub key: String,
    pub label: String,
    pub entries: Vec<T>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryScope {
    Database { database: Option<String> },
    All,
}

fn parse_timestamp(value: &str) -> Option<f64> {
    if !value.is_ascii() || value.len() < 10 {
        return None;
    }
    let digits = |start: usize, end: usize| -> Option<i64> {
        let text = &value[start..end];
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse::<i64>().ok()
    };
    let year = digits(0, 4)?;
    if value.as_bytes()[4] != b'-' {
        return None;
    }
    let month = digits(5, 7)?;
    if value.as_bytes()[7] != b'-' {
        return None;
    }
    let day = digits(8, 10)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut rest = &value[10..];
    let mut hour = 0i64;
    let mut minute = 0i64;
    let mut second = 0i64;
    let mut millis = 0i64;
    if rest.starts_with('T') || rest.starts_with(' ') {
        rest = &rest[1..];
        if rest.len() < 5 {
            return None;
        }
        hour = digits_at(rest, 0, 2)?;
        if rest.as_bytes()[2] != b':' {
            return None;
        }
        minute = digits_at(rest, 3, 5)?;
        rest = &rest[5..];
        if rest.starts_with(':') {
            if rest.len() < 3 {
                return None;
            }
            second = digits_at(rest, 1, 3)?;
            rest = &rest[3..];
            if rest.starts_with('.') {
                let count = rest[1..].chars().take_while(|ch| ch.is_ascii_digit()).count();
                if count == 0 {
                    return None;
                }
                for ch in rest[1..1 + count.min(3)].chars() {
                    millis = millis * 10 + i64::from(ch as u8 - b'0');
                }
                for _ in 0..3_usize.saturating_sub(count) {
                    millis *= 10;
                }
                rest = &rest[1 + count..];
            }
        }
        if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=59).contains(&second) {
            return None;
        }
        if rest.starts_with('Z') {
            rest = &rest[1..];
        } else if rest.len() >= 6 && (rest.starts_with('+') || rest.starts_with('-')) {
            if rest.as_bytes()[3] != b':' {
                return None;
            }
            let offset_hour = digits_at(rest, 1, 3)?;
            let offset_minute = digits_at(rest, 4, 6)?;
            rest = &rest[6..];
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
        }
        if !rest.is_empty() {
            return None;
        }
    } else if !rest.is_empty() {
        return None;
    }
    let shifted_year = year - i64::from(month <= 2);
    let era = if shifted_year >= 0 { shifted_year } else { shifted_year - 399 } / 400;
    let year_of_era = shifted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146097 + day_of_era - 719468;
    Some((days * 86_400 + hour * 3600 + minute * 60 + second) as f64 * 1000.0 + millis as f64)
}

fn digits_at(text: &str, start: usize, end: usize) -> Option<i64> {
    let part = &text[start..end];
    if !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    part.parse::<i64>().ok()
}

pub fn group_query_history<T: HistoryGroupEntry + Clone>(
    entries: &[T],
    unknown_label: &str,
) -> Vec<QueryHistoryGroup<T>> {
    let mut buckets: Vec<(String, Vec<T>)> = Vec::new();
    for entry in entries {
        let database = entry.database().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        });
        let key = database.unwrap_or_else(|| HISTORY_UNKNOWN_DB_KEY.to_owned());
        if let Some(bucket) = buckets.iter_mut().find(|(bucket_key, _)| bucket_key == &key) {
            bucket.1.push(entry.clone());
        } else {
            buckets.push((key, vec![entry.clone()]));
        }
    }
    let mut groups: Vec<QueryHistoryGroup<T>> = buckets
        .into_iter()
        .map(|(key, group_entries)| QueryHistoryGroup {
            label: if key == HISTORY_UNKNOWN_DB_KEY {
                unknown_label.to_owned()
            } else {
                key.clone()
            },
            key,
            entries: group_entries,
        })
        .collect();
    groups.sort_by(|left, right| {
        let left_time = left.entries.first().map_or("", |entry| entry.executed_at());
        let right_time = right.entries.first().map_or("", |entry| entry.executed_at());
        match (parse_timestamp(left_time), parse_timestamp(right_time)) {
            (Some(left_ms), Some(right_ms)) => right_ms.total_cmp(&left_ms),
            _ => right_time.cmp(left_time),
        }
    });
    groups
}

pub fn find_group_for_database<'a, T>(
    groups: &'a [QueryHistoryGroup<T>],
    current_database: Option<&str>,
) -> Option<&'a QueryHistoryGroup<T>> {
    let database = current_database?.trim();
    if database.is_empty() {
        return None;
    }
    groups.iter().find(|group| group.key == database)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Entry {
        database: Option<String>,
        executed_at: String,
    }

    impl HistoryGroupEntry for Entry {
        fn database(&self) -> Option<&str> {
            self.database.as_deref()
        }

        fn executed_at(&self) -> &str {
            &self.executed_at
        }
    }

    fn entry(database: Option<&str>, executed_at: &str) -> Entry {
        Entry {
            database: database.map(str::to_owned),
            executed_at: executed_at.to_owned(),
        }
    }

    #[test]
    fn sentinel_key_is_pinned() {
        assert_eq!(HISTORY_UNKNOWN_DB_KEY, "__unknown_db__");
    }

    #[test]
    fn groups_preserve_entry_order_and_group_by_trimmed_database() {
        let entries = vec![
            entry(Some(" main "), "2026-01-03T00:00:00Z"),
            entry(Some("other"), "2026-01-02T00:00:00Z"),
            entry(Some("main"), "2026-01-01T00:00:00Z"),
        ];
        let groups = group_query_history(&entries, "Unrecorded");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "main");
        assert_eq!(groups[0].label, "main");
        assert_eq!(groups[0].entries.len(), 2);
        assert_eq!(groups[0].entries[0].executed_at, "2026-01-03T00:00:00Z");
        assert_eq!(groups[1].key, "other");
    }

    #[test]
    fn unrecorded_rows_land_in_a_trailing_labeled_group() {
        let entries = vec![
            entry(None, "2026-01-01T00:00:00Z"),
            entry(Some("   "), "2026-01-02T00:00:00Z"),
            entry(Some("main"), "2026-01-03T00:00:00Z"),
        ];
        let groups = group_query_history(&entries, "Unrecorded");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "main");
        assert_eq!(groups[1].key, HISTORY_UNKNOWN_DB_KEY);
        assert_eq!(groups[1].label, "Unrecorded");
        assert_eq!(groups[1].entries.len(), 2);
    }

    #[test]
    fn group_order_uses_numeric_timestamps_not_lexicographic() {
        let entries = vec![
            entry(Some("a"), "2026-01-01T00:00:00Z"),
            entry(Some("b"), "2026-01-01T00:00:00.5Z"),
        ];
        let groups = group_query_history(&entries, "U");
        assert_eq!(groups[0].key, "b");
    }

    #[test]
    fn invalid_timestamps_fall_back_to_reverse_lexicographic() {
        let entries = vec![
            entry(Some("a"), "bad"),
            entry(Some("b"), "worse"),
        ];
        let groups = group_query_history(&entries, "U");
        assert_eq!(groups[0].key, "b", "bt.localeCompare(at) puts larger text first");
    }

    #[test]
    fn find_group_trims_and_returns_none_without_a_match() {
        let groups = group_query_history(&[entry(Some("main"), "2026-01-01")], "U");
        assert_eq!(find_group_for_database(&groups, Some(" main ")).map(|group| group.key.as_str()), Some("main"));
        assert!(find_group_for_database(&groups, None).is_none());
        assert!(find_group_for_database(&groups, Some("   ")).is_none());
        assert!(find_group_for_database(&groups, Some("other")).is_none());
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::HISTORY_UNKNOWN_DB_KEY, "__unknown_db__");
        let _ = crate::group_query_history::<Entry>;
        let _ = crate::find_group_for_database::<Entry>;
        let _: Option<crate::QueryHistoryGroup<Entry>> = None;
        let _: Option<crate::HistoryScope> = None;
    }
}
