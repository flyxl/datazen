//! Page-death session release, ported from
//! `packages/frontend/src/lib/session/pageUnload.ts`.
//!
//! React keeps a process-global `Set` of closers plus an `installed` flag, and
//! `window` dispatches `pagehide` / `beforeunload` into `releaseOwnedSessions`.
//! Rune has neither a DOM nor a window event loop, so the registry is an
//! **explicit value the host owns**: it installs the two listeners at startup
//! and calls [`PageUnloadRegistry::release_owned_sessions`] at teardown. There
//! is no mutable process-global, no thread and no `unwrap`/`expect` here.
//!
//! # Faithful semantics
//!
//! * `releaseOwnedSessions` snapshots the closers, clears the registry *before*
//!   running them, then fires each one exactly once. A closer registered during
//!   a release is therefore not fired by that release.
//! * A throwing closer cannot abort the rest. Rust has no JS exception to
//!   swallow, so a closer reports failure as `Err(CloserError)`; the registry
//!   records it and continues with the next closer. A panic from a host closer
//!   is a host bug and is deliberately not caught with `catch_unwind`.
//! * `install` is idempotent — the host attaches `pagehide` and `beforeunload`
//!   only once, even though every registration asks to install.
//! * `register` returns the unregister handle as a value (React returns a
//!   closure). The host holds the token and passes it back to the registry it
//!   owns; each registration gets its own id, like distinct JS function refs.
//!
//! # Relation to T083
//!
//! This is the page-level *fallback* registry, not the controller release
//! mechanism. A host closer here typically calls
//! `EditorSessionController::fire_page_unload()` and executes the resulting
//! `SessionEffect::Close` (`ClosePurpose::PageUnload`). The controller keeps its
//! own one-shot `RegisterPageCloser` / `UnregisterPageCloser` effect pair; this
//! module only fans out to whatever closers the host registered and swallows
//! their failures.
//!
//! # Removal stage
//!
//! Consumer: the Rune app host teardown wiring, which has not landed. Remove
//! the per-item `#[allow(dead_code)]` annotations when the host owns a registry
//! and forwards its unload events; the value itself stays as the port.

/// A closer's failure, captured instead of thrown. Carries the closer id so a
/// host can log which fallback failed without a panic.
#[allow(dead_code)] // host error logging; remove when the host teardown wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloserError {
    pub closer_id: u64,
    pub message: String,
}

/// Opaque identity of one registration — React's returned unregister closure,
/// expressed as a value the owning host can hand back to the registry.
#[allow(dead_code)] // host unregister hook; remove when the host teardown wires it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageUnloadToken {
    pub id: u64,
}

/// The two `window` events React installs on first registration. Both dispatch
/// to [`PageUnloadRegistry::release_owned_sessions`].
#[allow(dead_code)] // host listener wiring; remove when the host teardown wires it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnloadEvent {
    PageHide,
    BeforeUnload,
}

/// A page-unload closer. `Err` stands in for a JS `throw`: the registry records
/// it and keeps firing the remaining closers.
pub type PageUnloadCloser = Box<dyn FnMut() -> Result<(), CloserError>>;

/// Observable result of one release, in registration order.
#[allow(dead_code)] // host diagnostics; remove when the host teardown wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseReport {
    /// Ids of the closers that were fired by this release.
    pub fired: Vec<u64>,
    /// Failures swallowed so the remaining closers still ran.
    pub errors: Vec<CloserError>,
}

/// Registry value owned by the host. No global state and no threads.
#[allow(dead_code)] // consumer: host teardown wiring (not landed); remove then.
#[derive(Default)]
pub struct PageUnloadRegistry {
    closers: Vec<(u64, PageUnloadCloser)>,
    next_id: u64,
    installed: bool,
    last_errors: Vec<CloserError>,
    release_count: u64,
}

impl PageUnloadRegistry {
    // Each item below names its consumer and removal stage individually: the
    // struct-level allowance does not propagate to associated items.
    #[allow(dead_code)] // consumer: host page-lifecycle wiring (not landed); remove when the host calls it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Install once. Returns `true` only on the first call, so the host attaches
    /// [`Self::listener_events`] exactly once (React's `installed` flag).
    #[allow(dead_code)] // consumer: host startup listener hook; remove when the host calls it.
    pub fn install(&mut self) -> bool {
        if self.installed {
            return false;
        }
        self.installed = true;
        true
    }

    #[allow(dead_code)] // consumer: host page-lifecycle wiring (not landed); remove when the host calls it.
    pub fn is_installed(&self) -> bool {
        self.installed
    }

    /// The fixed pair of events that both dispatch to release.
    #[allow(dead_code)] // consumer: host listener registration; remove when the host installs them.
    pub fn listener_events(&self) -> &'static [UnloadEvent] {
        const EVENTS: [UnloadEvent; 2] = [UnloadEvent::PageHide, UnloadEvent::BeforeUnload];
        &EVENTS
    }

    /// React `registerPageUnloadCloser`: install (idempotently), remember the
    /// closer, and return the handle that unregisters it.
    #[allow(dead_code)] // consumer: host closer registration; remove when the host calls it.
    pub fn register(&mut self, closer: PageUnloadCloser) -> PageUnloadToken {
        let _ = self.install();
        self.next_id += 1;
        let id = self.next_id;
        self.closers.push((id, closer));
        PageUnloadToken { id }
    }

    /// React's returned unregister closure. `false` means the closer was no
    /// longer registered (e.g. a release already cleared it).
    #[allow(dead_code)] // consumer: host controller disarm path; remove when the host calls it.
    pub fn unregister(&mut self, token: PageUnloadToken) -> bool {
        let before = self.closers.len();
        self.closers.retain(|(id, _)| *id != token.id);
        self.closers.len() != before
    }

    /// React `releaseOwnedSessions`: clear first, then fire each snapshotted
    /// closer once, swallowing failures so one broken closer cannot stop the
    /// rest. Never panics and never returns an error.
    #[allow(dead_code)] // consumer: app teardown hook (not landed); remove when the host calls it.
    pub fn release_owned_sessions(&mut self) -> ReleaseReport {
        let pending = std::mem::take(&mut self.closers);
        self.release_count += 1;
        self.last_errors.clear();
        let mut fired = Vec::with_capacity(pending.len());
        for (id, mut closer) in pending {
            fired.push(id);
            if let Err(error) = closer() {
                self.last_errors.push(error);
            }
        }
        ReleaseReport { fired, errors: self.last_errors.clone() }
    }

    #[allow(dead_code)] // consumer: host teardown diagnostics; remove when the host calls it.
    pub fn len(&self) -> usize {
        self.closers.len()
    }

    #[allow(dead_code)] // consumer: host teardown diagnostics; remove when the host calls it.
    pub fn is_empty(&self) -> bool {
        self.closers.is_empty()
    }

    #[allow(dead_code)] // consumer: host teardown observability; remove when the host calls it.
    pub fn release_count(&self) -> u64 {
        self.release_count
    }

    #[allow(dead_code)] // consumer: host error logging; remove when the host calls it.
    pub fn last_errors(&self) -> &[CloserError] {
        &self.last_errors
    }
}
