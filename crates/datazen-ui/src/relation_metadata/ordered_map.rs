//! Minimal insertion-ordered map, mirroring the JS `Map` semantics the React
//! cache relies on (Rust's `HashMap` does not preserve iteration order).

/// Vector-backed map: iteration follows insertion order, and replacing a value
/// keeps its original slot.
#[derive(Clone, Debug)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct OrderedMap<K, V>(Vec<(K, V)>)
where
    K: PartialEq;

impl<K: PartialEq, V> OrderedMap<K, V> {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some((_, slot)) = self.0.iter_mut().find(|(k, _)| k == &key) {
            return Some(std::mem::replace(slot, value));
        }
        self.0.push((key, value));
        None
    }

    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let position = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(position).1)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
}
