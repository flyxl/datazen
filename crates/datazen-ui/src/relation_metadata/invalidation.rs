//! Schema/DDL invalidation bus, ported from React
//! `packages/frontend/src/lib/schemaCache.ts`'s `subscribeSchemaInvalidation`
//! and `notifyInvalidation`.
//!
//! React keeps the listener set in a process-global. Here it is an explicit,
//! single-threaded callback list the cache registers against, so tests and the
//! host can own their own buses without global mutable state.

/// A listener notified on every schema/DDL invalidation.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) type InvalidationListener = dyn Fn(&str, Option<&str>);

/// Explicit replacement for the React process-global invalidation set.
#[derive(Default)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct SchemaInvalidationBus {
    listeners: Vec<(usize, Box<InvalidationListener>)>,
    next_id: usize,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl SchemaInvalidationBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a listener; returns a token usable with `unsubscribe`.
    pub fn subscribe(&mut self, listener: Box<InvalidationListener>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.push((id, listener));
        id
    }

    pub fn unsubscribe(&mut self, id: usize) {
        self.listeners.retain(|(current, _)| *current != id);
    }

    /// Notify every listener, mirroring React's copy-then-call loop.
    pub fn notify(&self, db_session_id: &str, table_name: Option<&str>) {
        for (_, listener) in &self.listeners {
            listener(db_session_id, table_name);
        }
    }

    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}
