//! Cross-component bridge for "run this SQL in the query panel", ported
//! from React `packages/frontend/src/stores/queryRunRequestStore.ts`
//! (T385).
//!
//! The AI chat drawer lives outside the mounted `QueryPanel`, while
//! execution safety (Safe Mode, read-only blocking, production/high-risk
//! confirmation) lives inside the panel's execution gate. This registry
//! lets the chat hand a statement to the owning panel without
//! re-implementing that gate.
//!
//! Requests that arrive before the panel has registered are kept in
//! order as pending and flushed once when the panel mounts: "create a
//! query tab and run once" is a single user action, and two quick clicks
//! cannot lose the first statement.
//!
//! ## Rust shape
//!
//! React's module-level registry becomes the host-held
//! [`QueryRunRequestStore`]: one instance lives with the app state, the
//! owning panel registers its runner when it mounts (handing the
//! registration back to [`QueryRunRequestStore::unregister`] when it
//! goes), and the chat calls
//! [`QueryRunRequestStore::request_query_run`] with the host state so
//! the runner reaches the panel's own execution gate.

use std::collections::HashMap;
use std::rc::Rc;

/// One panel's runner (`QueryRunner`): hand the statement to the owning
/// panel's execution gate.
pub type QueryRunner<S> = Rc<dyn Fn(&mut S, &str)>;

/// The registration a panel holds while it is mounted.
#[derive(Clone, Debug)]
pub struct QueryRunnerRegistration {
    panel_id: String,
}

/// The registry (`runners` + `pending`).
#[derive(Default)]
pub struct QueryRunRequestStore<S: 'static> {
    runners: HashMap<String, QueryRunner<S>>,
    pending: HashMap<String, Vec<String>>,
}

impl<S: 'static> QueryRunRequestStore<S> {
    /// `registerQueryRunner`: register the active panel's runner and
    /// flush a request queued while mounting, in order.
    pub fn register_query_runner(
        &mut self,
        state: &mut S,
        panel_id: &str,
        runner: QueryRunner<S>,
    ) -> QueryRunnerRegistration {
        let registration = QueryRunnerRegistration {
            panel_id: panel_id.to_owned(),
        };
        let queued = self.pending.remove(panel_id);
        self.runners.insert(panel_id.to_owned(), runner.clone());
        // The flush runs after the runner is registered, matching React
        // (the set happens before the queue is read).
        if let Some(queued) = queued {
            for sql in queued {
                runner(state, &sql);
            }
        }
        registration
    }

    /// `requestQueryRun`: ask the panel to run `sql` — `true` when a
    /// mounted panel accepted it immediately, `false` when it was
    /// queued for the panel that is about to mount.
    pub fn request_query_run(
        &mut self,
        state: &mut S,
        panel_id: &str,
        sql: &str,
    ) -> bool {
        if let Some(runner) = self.runners.get(panel_id).cloned() {
            runner(state, sql);
            return true;
        }
        self.pending
            .entry(panel_id.to_owned())
            .or_default()
            .push(sql.to_owned());
        false
    }

    /// The unregister closure: drop the panel's runner and its pending
    /// queue with it.
    pub fn unregister(&mut self, registration: &QueryRunnerRegistration) {
        self.runners.remove(&registration.panel_id);
        self.pending.remove(&registration.panel_id);
    }
}
