//! Form-neutral driver event outlet contract and its `inventory` registration.
//!
//! Some drivers produce events for a human-facing surface — the Redis
//! driver's Pub/Sub messages are the canonical example. Such a driver must
//! not decide *how* those events reach a user: the desktop build forwards
//! them to the webview as Tauri events, a headless server would forward
//! them over its own transport, and neither is the driver's business.
//!
//! So the contract is split in two, exactly like the driver factory:
//!
//! * [`DriverEventOutlet`] is the **driver's** declaration: "I have an event
//!   outlet; hand me a sink". It is registered at link time with
//!   [`register_event_outlet!`], so a driver that is compiled into the
//!   binary declares itself without the host knowing which drivers exist.
//! * [`DriverEventSink`] is the **host's** side: one implementation per form.
//!   The host installs it at startup, at runtime — the link-time constant is
//!   only "an outlet exists", never "this sink".
//!
//! Nothing in this module names a host framework. The desktop sink lives in
//! the host crate (`src-tauri/src/platform/event_sink.rs`); a server sink
//! would live in the server crate. A driver sees only the two traits and
//! [`DriverEvent`].

use std::sync::Arc;

/// One event a driver emits through an outlet.
///
/// The name is a stable, form-neutral identifier chosen by the driver
/// (e.g. `"redis-pubsub-message"`); the payload is the already-serialized
/// event body. A sink forwards both verbatim, so the consumer sees exactly
/// what the driver produced — the sink never reshapes a payload.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DriverEvent {
    /// Stable event name, chosen by the driver.
    pub name: &'static str,
    /// Serialized event payload.
    pub payload: serde_json::Value,
}

/// The destination a driver hands its events to.
///
/// Implemented by the **host**, once per form. The contract is deliberately
/// tiny: one method, one event, no return value. A sink that cannot deliver
/// (no window yet, transport down) reports that through its own logging —
/// the driver has no way to act on it and must not be asked to.
pub trait DriverEventSink: Send + Sync + 'static {
    /// Deliver one event.
    ///
    /// Implementations must not block: this is called from the driver's own
    /// task (a subscription loop), not from a request path.
    fn emit(&self, event: &DriverEvent);
}

/// A driver's declaration that it has an event outlet.
///
/// Registered at link time — see [`register_event_outlet!`]. Which sink is
/// installed is a runtime decision made by the host during startup.
pub trait DriverEventOutlet: Send + Sync + 'static {
    /// Stable identifier of this outlet (e.g. `"redis-pubsub"`).
    fn outlet_id(&self) -> &'static str;

    /// Install the host's sink into this outlet.
    ///
    /// Called once per process, at startup. Implementations are expected to
    /// keep the sink and use it from then on; a driver that receives events
    /// before this call has no sink yet and must apply the same policy it
    /// applied before the sink existed (see the Redis driver, which drops
    /// messages that arrive before installation).
    fn install(&self, sink: Arc<dyn DriverEventSink>);
}

// Collect all event outlets registered across the binary (including plugins).
inventory::collect!(&'static dyn DriverEventOutlet);

/// Register a driver event outlet at link time. Usage:
///
/// ```ignore
/// struct MyOutlet;
/// impl DriverEventOutlet for MyOutlet {
///     fn outlet_id(&self) -> &'static str { "my-outlet" }
///     fn install(&self, sink: Arc<dyn DriverEventSink>) { /* … */ }
/// }
/// datazen_driver_api::register_event_outlet!(&MyOutlet);
/// ```
#[macro_export]
macro_rules! register_event_outlet {
    ($outlet:expr) => {
        $crate::inventory::submit!($outlet as &'static dyn $crate::DriverEventOutlet);
    };
}

/// Iterate over all registered driver event outlets.
///
/// The host calls this at startup to install its own sink into every outlet
/// that was linked into the binary.
pub fn iter_event_outlets() -> inventory::iter<&'static dyn DriverEventOutlet> {
    inventory::iter::<&'static dyn DriverEventOutlet>
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A sink that records what it was handed.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<DriverEvent>>,
    }

    impl DriverEventSink for RecordingSink {
        fn emit(&self, event: &DriverEvent) {
            self.events.lock().expect("recording sink is not poisoned").push(event.clone());
        }
    }

    struct TestOutlet;

    impl DriverEventOutlet for TestOutlet {
        fn outlet_id(&self) -> &'static str {
            "test-outlet"
        }

        fn install(&self, sink: Arc<dyn DriverEventSink>) {
            sink.emit(&DriverEvent {
                name: "test-event",
                payload: serde_json::json!({ "answer": 42 }),
            });
        }
    }

    crate::register_event_outlet!(&TestOutlet);

    #[test]
    fn a_registered_outlet_is_discoverable_and_installable() {
        let outlet = iter_event_outlets()
            .into_iter()
            .find(|outlet| outlet.outlet_id() == "test-outlet")
            .expect("the test outlet must be registered at link time");

        let sink = Arc::new(RecordingSink::default());
        outlet.install(sink.clone());

        let events = sink.events.lock().expect("recording sink is not poisoned");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "test-event");
        assert_eq!(events[0].payload["answer"], serde_json::json!(42));
    }
}
