use std::collections::HashMap;

pub const VALUE_TTL_MS: i64 = 60_000;
pub const ERROR_TTL_MS: i64 = 10_000;

struct ValueEntry<T> {
    value: T,
    at: i64,
}

struct ErrorEntry {
    error: String,
    at: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadStart<T> {
    Cached(T),
    Failed(String),
    InFlight,
    Fresh(u64),
}

pub struct SchemaResourceCache<T> {
    values: HashMap<String, ValueEntry<T>>,
    errors: HashMap<String, ErrorEntry>,
    inflight: HashMap<String, u64>,
    next_id: u64,
}

impl<T> Default for SchemaResourceCache<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            errors: HashMap::new(),
            inflight: HashMap::new(),
            next_id: 0,
        }
    }
}

impl<T: Clone> SchemaResourceCache<T> {
    pub fn keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for key in self.values.keys().chain(self.errors.keys()).chain(self.inflight.keys()) {
            if !out.contains(key) {
                out.push(key.clone());
            }
        }
        out
    }

    pub fn invalidate(&mut self, matches: impl Fn(&str) -> bool) {
        let doomed: Vec<String> = self.keys().into_iter().filter(|key| matches(key)).collect();
        for key in doomed {
            self.values.remove(&key);
            self.errors.remove(&key);
            self.inflight.remove(&key);
        }
    }

    pub fn begin_load(&mut self, key: &str, now: i64) -> LoadStart<T> {
        if let Some(cached) = self.values.get(key) {
            if now - cached.at < VALUE_TTL_MS {
                return LoadStart::Cached(cached.value.clone());
            }
        }
        if let Some(failed) = self.errors.get(key) {
            if now - failed.at < ERROR_TTL_MS {
                return LoadStart::Failed(failed.error.clone());
            }
        }
        if self.inflight.contains_key(key) {
            return LoadStart::InFlight;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.inflight.insert(key.to_owned(), id);
        LoadStart::Fresh(id)
    }

    pub fn complete_load(&mut self, key: &str, id: u64, result: Result<T, String>, now: i64) {
        if self.inflight.get(key) != Some(&id) {
            return;
        }
        match result {
            Ok(value) => {
                self.values.insert(key.to_owned(), ValueEntry { value, at: now });
                self.errors.remove(key);
            }
            Err(error) => {
                self.errors.insert(key.to_owned(), ErrorEntry { error, at: now });
            }
        }
        self.inflight.remove(key);
    }

    pub fn inflight_id(&self, key: &str) -> Option<u64> {
        self.inflight.get(key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> SchemaResourceCache<String> {
        SchemaResourceCache::default()
    }

    #[test]
    fn a_fresh_request_then_success_is_cached_until_the_value_ttl() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("a", 0) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Ok("v".to_owned()), 1_000);
        assert_eq!(c.begin_load("a", 1_000 + VALUE_TTL_MS - 1), LoadStart::Cached("v".to_owned()));
        assert_eq!(c.begin_load("a", 1_000 + VALUE_TTL_MS), LoadStart::Fresh(2));
    }

    #[test]
    fn an_error_is_replayed_only_inside_the_error_ttl() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("a", 5_000) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Err("boom".to_owned()), 5_100);
        assert_eq!(c.begin_load("a", 5_100 + ERROR_TTL_MS - 1), LoadStart::Failed("boom".to_owned()));
        assert_eq!(c.begin_load("a", 5_100 + ERROR_TTL_MS), LoadStart::Fresh(2));
    }

    #[test]
    fn a_second_begin_while_inflight_joins_the_existing_request() {
        let mut c = cache();
        assert_eq!(c.begin_load("a", 0), LoadStart::Fresh(1));
        assert_eq!(c.begin_load("a", 1), LoadStart::InFlight);
        assert_eq!(c.inflight_id("a"), Some(1));
        c.complete_load("a", 1, Ok("v".to_owned()), 2);
        assert_eq!(c.inflight_id("a"), None);
    }

    #[test]
    fn invalidation_detaches_old_requests_so_they_cannot_overwrite() {
        let mut c = cache();
        assert_eq!(c.begin_load("a", 0), LoadStart::Fresh(1));
        c.invalidate(|key| key == "a");
        assert_eq!(c.begin_load("a", 1), LoadStart::Fresh(2));
        c.complete_load("a", 1, Ok("stale".to_owned()), 2);
        assert_eq!(c.inflight_id("a"), Some(2));
        c.complete_load("a", 2, Ok("fresh".to_owned()), 3);
        assert_eq!(c.begin_load("a", 3), LoadStart::Cached("fresh".to_owned()));
    }

    #[test]
    fn success_clears_a_prior_error() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("a", 0) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Err("boom".to_owned()), 1);
        let LoadStart::Fresh(id) = c.begin_load("a", ERROR_TTL_MS + 1) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Ok("value".to_owned()), ERROR_TTL_MS + 2);
        assert_eq!(c.begin_load("a", ERROR_TTL_MS + 2), LoadStart::Cached("value".to_owned()));
    }

    #[test]
    fn a_failure_leaves_an_expired_value_listed() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("a", 0) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Ok("old".to_owned()), 0);
        let LoadStart::Fresh(id) = c.begin_load("a", VALUE_TTL_MS) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Err("boom".to_owned()), VALUE_TTL_MS + 1);
        assert!(c.keys().contains(&"a".to_owned()));
        assert_eq!(c.keys().len(), 1);
        assert_eq!(c.begin_load("a", VALUE_TTL_MS + 1), LoadStart::Failed("boom".to_owned()));
    }

    #[test]
    fn keys_is_the_deduped_union_of_values_errors_and_inflight() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("a", 0) else { panic!("expected a fresh request") };
        c.complete_load("a", id, Ok("v".to_owned()), 1);
        let LoadStart::Fresh(id) = c.begin_load("b", 0) else { panic!("expected a fresh request") };
        c.complete_load("b", id, Err("boom".to_owned()), 1);
        assert_eq!(c.begin_load("c", 2), LoadStart::Fresh(3));
        assert!(c.keys().contains(&"a".to_owned()));
        assert!(c.keys().contains(&"b".to_owned()));
        assert!(c.keys().contains(&"c".to_owned()));
        assert_eq!(c.keys().len(), 3);
    }

    #[test]
    fn invalidate_only_touches_matching_keys() {
        let mut c = cache();
        let LoadStart::Fresh(id) = c.begin_load("keep", 0) else { panic!("expected a fresh request") };
        c.complete_load("keep", id, Ok("v".to_owned()), 0);
        let LoadStart::Fresh(id) = c.begin_load("drop", 0) else { panic!("expected a fresh request") };
        c.complete_load("drop", id, Ok("v".to_owned()), 0);
        c.invalidate(|key| key == "drop");
        assert_eq!(
            c.begin_load("keep", 0),
            LoadStart::Cached("v".to_owned())
        );
        assert_eq!(c.begin_load("drop", 0), LoadStart::Fresh(3));
    }

    #[test]
    fn a_stale_completion_is_ignored_without_an_inflight_entry() {
        let mut c = cache();
        c.complete_load("ghost", 99, Ok("v".to_owned()), 0);
        assert_eq!(c.begin_load("ghost", 0), LoadStart::Fresh(1));
    }
}
