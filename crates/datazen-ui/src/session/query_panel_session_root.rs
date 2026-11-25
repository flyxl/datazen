//! Port of `packages/frontend/src/lib/session/queryPanelSessionRoot.ts` — the
//! process-wide home that binds the query-panel session registry to the running
//! app.
//!
//! React keeps two module-level globals: a lazily created
//! `QueryPanelSessionRegistry` and a cached client-identity promise. This port
//! models them as one `QueryPanelSessionRoot` held in a thread-local slot. Rust
//! tests run on their own threads, so each gets the fresh module graph React
//! gets from `vi.resetModules()`; `reset_query_panel_session_root` is the
//! explicit teardown hook for a host that reuses a thread.
//!
//! Immediate-mode adaptation: no promises and no async. `getPlatformIdentity`
//! is a synchronous call whose result (including its failure) is cached, just
//! as React caches the settled promise. The backend binding is fail-loud at
//! first wire use (§7.4), and an empty identity still produces the exact
//! `Missing client identity` error.
//!
//! # Reuse / stand-in
//!
//! The DTOs (`ExecutionTarget`, `NamespaceTarget`, `OwnerRef`) come from T083
//! via `super::editor_session_controller`. This branch does **not** yet contain
//! T183's `crate::session::query_panel_session::QueryPanelSessionRegistry`, so
//! [`QueryPanelSessionRegistry`] below is a **local stand-in** with only the
//! construction identity this root needs. Removal stage: when T183 lands here,
//! delete the stand-in and return that crate-level registry from
//! `get_query_panel_session_registry` (no caller changes).
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the query-panel view binds the root".

use std::cell::RefCell;
use std::rc::Rc;

use super::editor_session_controller::{ExecutionTarget, NamespaceTarget, OwnerRef};

/// Errors from the root's fail-loud paths.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootError {
    /// §7.4: an unbound backend at a data-plane call site is a wiring gap.
    BackendUnbound,
    /// The host issued an empty client identity.
    MissingClientIdentity,
}

#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
impl RootError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::BackendUnbound => "Backend client is not bound",
            Self::MissingClientIdentity => "Missing client identity",
        }
    }
}

/// The bound backend facade. Only the identity read is used here; the shared
/// call counter lets a test pin React's "host identity fetched once" rule.
#[allow(dead_code)] // host binding; remove when the app assembly lands.
#[derive(Clone, Debug)]
pub struct BackendClientStub {
    pub client_instance_id: String,
    identity_calls: Rc<RefCell<usize>>,
}

#[allow(dead_code)] // host binding; remove when the app assembly lands.
impl BackendClientStub {
    pub fn new(client_instance_id: impl Into<String>) -> Self {
        Self {
            client_instance_id: client_instance_id.into(),
            identity_calls: Rc::new(RefCell::new(0)),
        }
    }

    /// `queryPanelSessionClient().getPlatformIdentity()` — empty ids fail loud.
    pub fn get_platform_identity(&self) -> Result<String, RootError> {
        *self.identity_calls.borrow_mut() += 1;
        if self.client_instance_id.is_empty() {
            return Err(RootError::MissingClientIdentity);
        }
        Ok(self.client_instance_id.clone())
    }

    pub fn identity_calls(&self) -> usize {
        *self.identity_calls.borrow()
    }
}

/// Local stand-in for T183's `QueryPanelSessionRegistry`. It keeps only the
/// shared-client identity the root pins; the full controller table lives in
/// T183 once that port is merged into this branch.
// Remove when T183 lands here; use its registry instead.
#[allow(dead_code)]
#[derive(Debug)]
pub struct QueryPanelSessionRegistry {
    identity_calls: Rc<RefCell<usize>>,
}

#[allow(dead_code)] // construction site; remove with the stand-in above.
impl QueryPanelSessionRegistry {
    fn new(client: &BackendClientStub) -> Self {
        Self {
            identity_calls: client.identity_calls.clone(),
        }
    }

    pub fn shares_client(&self, client: &BackendClientStub) -> bool {
        Rc::ptr_eq(&self.identity_calls, &client.identity_calls)
    }
}

/// The root's owned state: the bound client, the lazy registry and the cached
/// identity result.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
#[derive(Debug)]
pub struct QueryPanelSessionRoot {
    client: Option<BackendClientStub>,
    registry: Option<Rc<RefCell<QueryPanelSessionRegistry>>>,
    client_instance_id: Option<Result<String, RootError>>,
}

#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
impl QueryPanelSessionRoot {
    pub fn new() -> Self {
        Self {
            client: None,
            registry: None,
            client_instance_id: None,
        }
    }

    /// The app assembly's eager `requireBackendClient()` binding, before first
    /// paint. Only bookkeeping is construction-safe until this happens.
    pub fn bind_client(&mut self, client: BackendClientStub) {
        self.client = Some(client);
    }

    /// `queryPanelSessionClient()` — fail loud when unbound.
    pub fn query_panel_session_client(&self) -> Result<BackendClientStub, RootError> {
        self.client
            .clone()
            .ok_or(RootError::BackendUnbound)
    }

    /// `getQueryPanelSessionRegistry()` — one registry per root, created on
    /// first lookup and shared by every pane after that.
    pub fn get_query_panel_session_registry(
        &mut self,
    ) -> Result<Rc<RefCell<QueryPanelSessionRegistry>>, RootError> {
        if self.registry.is_none() {
            // The React registry is built with a lazy proxy; the identity read
            // happens only when a session opens. Binding is required here so a
            // never-bound app cannot silently hold a registry it cannot use.
            let client = self.query_panel_session_client()?;
            self.registry = Some(Rc::new(RefCell::new(QueryPanelSessionRegistry::new(&client))));
        }
        Ok(self
            .registry
            .clone()
            .ok_or(RootError::BackendUnbound)?)
    }

    /// `queryPanelClientInstanceId()` — the identity result is cached once,
    /// failure included, exactly like React's cached promise.
    pub fn query_panel_client_instance_id(&mut self) -> Result<String, RootError> {
        if self.client_instance_id.is_none() {
            let client = self.query_panel_session_client()?;
            self.client_instance_id = Some(client.get_platform_identity());
        }
        match self.client_instance_id.as_ref() {
            Some(result) => result.clone(),
            None => Err(RootError::BackendUnbound),
        }
    }

    /// `editorOwnerForPane(paneKey)` — the pane key is the editor's stable local
    /// owner id, so a duplicated tab (new pane key) gets a distinct owner.
    pub fn editor_owner_for_pane(&mut self, pane_key: &str) -> Result<OwnerRef, RootError> {
        let client_instance_id = self.query_panel_client_instance_id()?;
        // React's `toId` is an identity cast on a non-empty string, already
        // guaranteed for the client id by the check above; pane keys are passed
        // through verbatim.
        Ok(OwnerRef::Editor {
            client_instance_id,
            editor_session_id: pane_key.to_owned(),
        })
    }

    /// `queryPanelExecutionTarget` — a pure projection with a normalized
    /// namespace (`?? null` for both database and schema).
    pub fn query_panel_execution_target(
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> ExecutionTarget {
        ExecutionTarget {
            connection_id: connection_id.to_owned(),
            namespace: NamespaceTarget {
                database: database.map(str::to_owned),
                catalog: None,
                schema: schema.map(str::to_owned),
                path: Vec::new(),
            },
            object: None,
        }
    }

    /// Host teardown: drops the cached identity and registry. React only gets
    /// this via a fresh module graph; a long-lived Rune host needs it explicit.
    pub fn reset(&mut self) {
        self.client = None;
        self.registry = None;
        self.client_instance_id = None;
    }
}

thread_local! {
    /// The process-wide (per-thread) module graph.
    static SESSION_ROOT: RefCell<QueryPanelSessionRoot> = RefCell::new(QueryPanelSessionRoot::new());
}

/// `getQueryPanelSessionRegistry()` on the ambient root.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub fn get_query_panel_session_registry(
) -> Result<Rc<RefCell<QueryPanelSessionRegistry>>, RootError> {
    SESSION_ROOT.with(|root| root.borrow_mut().get_query_panel_session_registry())
}

/// `queryPanelClientInstanceId()` on the ambient root.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub fn query_panel_client_instance_id() -> Result<String, RootError> {
    SESSION_ROOT.with(|root| root.borrow_mut().query_panel_client_instance_id())
}

/// `editorOwnerForPane(paneKey)` on the ambient root.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub fn editor_owner_for_pane(pane_key: &str) -> Result<OwnerRef, RootError> {
    SESSION_ROOT.with(|root| root.borrow_mut().editor_owner_for_pane(pane_key))
}

/// Bind the ambient root's backend, mirroring the app assembly.
#[allow(dead_code)] // consumer: the app assembly; remove when it lands.
pub fn bind_session_client(client: BackendClientStub) {
    SESSION_ROOT.with(|root| root.borrow_mut().bind_client(client));
}

/// Tear down the ambient root (test/module-reload equivalent).
#[allow(dead_code)] // consumer: host teardown/tests; remove when a view owns the root.
pub fn reset_query_panel_session_root() {
    SESSION_ROOT.with(|root| root.borrow_mut().reset());
}

/// `queryPanelExecutionTarget` is pure and needs no root.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub fn query_panel_execution_target(
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> ExecutionTarget {
    QueryPanelSessionRoot::query_panel_execution_target(connection_id, database, schema)
}
