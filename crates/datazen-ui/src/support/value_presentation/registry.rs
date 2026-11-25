//! Port of `packages/extension-points/src/valuePresentation/registry.ts`
//! (the valuePresentation slice of extension-points; lands beside its
//! first in-tree consumer, see the area module note).
//!
//! A set-registry keyed by contribution id: duplicate ids are rejected
//! (registration never overwrites), unregister is idempotent for unknown
//! ids, the release handle only removes its own registration, changes
//! notify subscribers (a panicking listener is isolated) and bump the
//! revision the host uses to invalidate presentation-plan caches.
//!
//! React keeps a module-level global singleton; the Rust host owns its
//! registry instance (constructed at startup, contributions registered by
//! `registerBundledContributions`, T182), so the port is the struct plus
//! its release handles — no global state.

use std::rc::Rc;

use crate::support::value_presentation::contracts::ValuePresentationContribution;

/// A release handle for one registration: removes only the registration it
/// came from (an id re-registered afterwards is a *different* registration
/// the old handle must not touch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationHandle {
    token: u64,
}

/// A subscription release handle with the same single-ownership semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubscriptionHandle {
    token: u64,
}

struct Registration {
    id: String,
    token: u64,
    contribution: Rc<dyn ValuePresentationContribution>,
}

struct Listener {
    token: u64,
    callback: Box<dyn Fn()>,
}

/// The value-presentation set registry (React
/// `ValuePresentationRegistryImpl`).
#[derive(Default)]
pub struct ValuePresentationRegistry {
    registrations: Vec<Registration>,
    listeners: Vec<Listener>,
    revision_counter: u64,
    next_token: u64,
}

impl ValuePresentationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a contribution; a duplicate id is an error (never an
    /// overwrite). On success returns the release handle.
    pub fn register(
        &mut self,
        contribution: Rc<dyn ValuePresentationContribution>,
    ) -> Result<RegistrationHandle, String> {
        let id = contribution.id().to_string();
        if self.registrations.iter().any(|registration| registration.id == id) {
            return Err(format!(
                "[ValuePresentationRegistry] duplicate contribution id '{id}': \
                 a value presentation with this id is already registered; \
                 unregister it first — registration never overwrites"
            ));
        }
        self.next_token += 1;
        let token = self.next_token;
        self.registrations.push(Registration { id, token, contribution });
        self.changed();
        Ok(RegistrationHandle { token })
    }

    /// Explicitly unregister by id; unknown ids are a no-op (no
    /// notification, no revision bump).
    pub fn unregister(&mut self, id: &str) {
        if let Some(position) =
            self.registrations.iter().position(|registration| registration.id == id)
        {
            self.registrations.remove(position);
            self.changed();
        }
    }

    /// Release one registration via its handle: removes the registration
    /// only if the handle's token still owns it (silent no-op otherwise).
    pub fn release(&mut self, handle: RegistrationHandle) {
        if let Some(position) = self
            .registrations
            .iter()
            .position(|registration| registration.token == handle.token)
        {
            self.registrations.remove(position);
            self.changed();
        }
    }

    pub fn get(&self, id: &str) -> Option<Rc<dyn ValuePresentationContribution>> {
        self.registrations
            .iter()
            .find(|registration| registration.id == id)
            .map(|registration| Rc::clone(&registration.contribution))
    }

    /// All contributions in registration order.
    pub fn list(&self) -> Vec<Rc<dyn ValuePresentationContribution>> {
        self.registrations
            .iter()
            .map(|registration| Rc::clone(&registration.contribution))
            .collect()
    }

    /// Subscribe to register/unregister changes; the handle unsubscribes.
    pub fn subscribe(&mut self, callback: Box<dyn Fn()>) -> SubscriptionHandle {
        self.next_token += 1;
        let token = self.next_token;
        self.listeners.push(Listener { token, callback });
        SubscriptionHandle { token }
    }

    pub fn unsubscribe(&mut self, handle: SubscriptionHandle) {
        self.listeners.retain(|listener| listener.token != handle.token);
    }

    /// The contribution revision: bumped on every successful
    /// register/unregister; stable when nothing changed.
    pub fn revision(&self) -> u64 {
        self.revision_counter
    }

    /// The single change entry point: bump the revision, then notify every
    /// listener (a snapshot of them — re-subscribing inside a listener does
    /// not change this round; a panicking listener is isolated).
    fn changed(&mut self) {
        self.revision_counter += 1;
        let listeners: Vec<Listener> = std::mem::take(&mut self.listeners);
        for Listener { callback, .. } in &listeners {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
            if result.is_err() {
                // React logs and continues; the Rust host has no console —
                // swallow and keep notifying the remaining listeners.
            }
        }
        self.listeners = listeners;
    }
}
