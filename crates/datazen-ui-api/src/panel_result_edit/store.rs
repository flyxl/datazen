use std::collections::HashMap;

use super::types::{ResolveTicket, ResultOriginRequest, ServerEditability};

#[derive(Clone, Debug, Default)]
pub struct PanelResultEditStore {
    verdicts: HashMap<String, Option<ServerEditability>>,
    resolving: HashMap<String, bool>,
    errors: HashMap<String, Option<String>>,
    generations: HashMap<String, u64>,
}

impl PanelResultEditStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_resolve(&mut self, key: &str, request: ResultOriginRequest) -> Option<ResolveTicket> {
        if self.resolving.get(key).copied().unwrap_or(false) {
            return None;
        }
        let generation = self.generations.get(key).copied().unwrap_or(0) + 1;
        self.generations.insert(key.to_owned(), generation);
        self.resolving.insert(key.to_owned(), true);
        self.errors.insert(key.to_owned(), None);
        Some(ResolveTicket { key: key.to_owned(), generation, request })
    }

    pub fn complete_resolve(&mut self, ticket: ResolveTicket, result: Result<ServerEditability, String>) {
        let ResolveTicket { key, generation, .. } = ticket;
        if self.generations.get(&key).copied().unwrap_or(0) != generation {
            return;
        }
        match result {
            Ok(verdict) => {
                self.verdicts.insert(key.clone(), Some(verdict));
            }
            Err(error) => {
                self.verdicts.insert(key.clone(), None);
                self.errors.insert(key.clone(), Some(error));
            }
        }
        if self.generations.get(&key).copied().unwrap_or(0) == generation {
            self.resolving.insert(key, false);
        }
    }

    pub fn clear_for_result(&mut self, key: &str) {
        self.verdicts.remove(key);
        self.errors.remove(key);
        self.resolving.remove(key);
        let generation = self.generations.get(key).copied().unwrap_or(0) + 1;
        self.generations.insert(key.to_owned(), generation);
    }

    pub fn clear_all(&mut self) {
        *self = Self::default();
    }

    pub fn verdict(&self, key: &str) -> Option<&ServerEditability> {
        self.verdicts.get(key).and_then(|verdict| verdict.as_ref())
    }

    pub fn resolving(&self, key: &str) -> bool {
        self.resolving.get(key).copied().unwrap_or(false)
    }

    pub fn error(&self, key: &str) -> Option<&str> {
        self.errors.get(key).and_then(|error| error.as_deref())
    }

    pub fn generation(&self, key: &str) -> u64 {
        self.generations.get(key).copied().unwrap_or(0)
    }
}
