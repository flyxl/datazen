//! The DataZen toast: a kit `Toast`, a region that stacks them, and a stateful host.
//!
//! ## Visual building blocks
//!
//! [`Toast`] and [`toast_region`]/[`ToastRegion`] wrap `rune-kit`'s `Toast` and `ToastRegion`
//! for one-shot/static rendering. The region is a bottom-right column (`ToastRegion` pins
//! `right=32, bottom=48`, `gap=12`); children are laid out in the order they are passed, so
//! **the first element is at the top of the stack and later elements appear below it**. When
//! rendering from a [`ToastHost`], pass cards in [`ToastHost::entries`] order (oldest first,
//! front/newest last) so the newest toast sits at the bottom of the region.
//!
//! ## Stateful lifecycle ([`ToastHost`])
//!
//! Timed auto-dismiss, coalescing, pause/expand and placement metadata live in `rune-kit`'s
//! [`ToastStack`] state machine. The app owns a [`ToastHost`] in its state, steps it with
//! events, schedules wakes from [`ToastEffect::WakeAt`] on the runtime/frame clock, and
//! renders from [`ToastHost::entries`]/[`ToastStack::place`] each frame.
//!
//! Wiring contract:
//! - **Holder:** app state owns `ToastHost<O>` (not a `Vec<El>`).
//! - **Show/dismiss:** call [`ToastHost::show`] / [`ToastHost::dismiss`] (they forward to
//!   [`ToastStack::step`]).
//! - **Timers:** when [`ToastHost::tick`] (or a `Tick` step) returns [`ToastEffect::Closed`],
//!   invoke the toast's close callback once for that id. When it returns
//!   [`ToastEffect::WakeAt(due)`], schedule a wake/tick at `due` on the runtime clock and
//!   feed `Tick { now }` back when due (no timer thread).
//! - **Pointer/visibility:** forward `PointerOver`/`PointerLeft`/`Pressed`/`Released` and page
//!   visibility to `step` so timers pause/expand as the kit stack specifies.
//! - **Render:** rebuild toast elements from [`ToastHost::entries`] every frame; visibility and
//!   placement hints come from [`ToastStack::place`].
//!
//! ```ignore
//! // state.toasts: ToastHost<MyToast>
//! // on show:  let fx = state.toasts.show(key, opts, Some(4.0), now); schedule(fx);
//! // on wake:  let fx = state.toasts.tick(now); for Closed(id) fire on_close once; schedule(fx);
//! // render:   let region = state.toasts.render(kit, |entry| build_toast(entry).build(kit));
//! ```
//!
//! [`lead`] and the action button API are thin pass-throughs to the kit toast; confirm them
//! against real call sites when those land (no speculative expansion).

// The views that call this land in later migration tasks; tests are the main callers today.
#![allow(dead_code)]

use rune::{El, Icon, Tx};
use rune_kit::toast_stack::{ToastEffect, ToastEvent, ToastStack};
use rune_kit::{Kit, Toast as KitToast, ToastRegion as KitToastRegion};

pub(crate) use rune_kit::toast_stack::{ToastEntry, ToastPage, ToastPlacement, ToastTimer};

/// One toast card (visual builder).
pub(crate) struct Toast {
    key: String,
    title: String,
    description: Option<String>,
    icon: Option<&'static Icon>,
    lead: Option<El>,
    close_handler: bool,
    closable: bool,
    actions: Vec<Box<dyn FnOnce(&Kit, KitToast) -> KitToast>>,
}

impl Toast {
    pub(crate) fn new(key: impl Into<String>, title: impl Into<String>) -> Toast {
        Toast {
            key: key.into(),
            title: title.into(),
            description: None,
            icon: None,
            lead: None,
            close_handler: false,
            closable: false,
            actions: Vec::new(),
        }
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> Toast {
        self.description = Some(description.into());
        self
    }

    /// A 16 point icon in the theme's base icon colour.
    pub(crate) fn icon(mut self, icon: &'static Icon) -> Toast {
        self.icon = Some(icon);
        self
    }

    /// Any 16 point leading element (coloured icons, a loader). Confirm at first real call site.
    pub(crate) fn lead(mut self, element: El) -> Toast {
        self.lead = Some(element);
        self
    }

    /// Shows the close button and runs `f` when it is pressed.
    pub(crate) fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Toast {
        self.close_handler = true;
        self.closable = true;
        self.actions.push(Box::new(move |_k, t| t.on_close(f)));
        self
    }

    /// Shows the close button without a handler.
    pub(crate) fn closable(mut self) -> Toast {
        self.closable = true;
        self
    }

    /// A primary or secondary small action button.
    pub(crate) fn action<S: 'static>(
        mut self,
        label: impl Into<String>,
        primary: bool,
        f: impl Fn(&mut S, &mut Tx) + 'static,
    ) -> Toast {
        let label = label.into();
        self.actions
            .push(Box::new(move |k, t| t.action(k, &label, primary, f)));
        self
    }

    pub(crate) fn build(self, k: &Kit) -> El {
        let mut kit = KitToast::new(&self.key, &self.title);
        if let Some(description) = self.description.as_deref() {
            kit = kit.description(description);
        }
        if let Some(icon) = self.icon {
            kit = kit.icon(k, icon);
        }
        if let Some(lead) = self.lead {
            kit = kit.lead(lead);
        }
        for bind in self.actions {
            kit = bind(k, kit);
        }
        if self.closable && !self.close_handler {
            kit = kit.closable();
        }
        kit.build(k)
    }
}

/// A bottom-right notification region. Mount it in the overlay layer (e.g. via `rune_kit::root`).
///
/// Children stack vertically in the order provided (first element at top).
pub(crate) struct ToastRegion {
    toasts: Vec<El>,
}

pub(crate) fn toast_region(toasts: Vec<El>) -> ToastRegion {
    ToastRegion { toasts }
}

impl ToastRegion {
    pub(crate) fn toast(mut self, toast: El) -> ToastRegion {
        self.toasts.push(toast);
        self
    }

    pub(crate) fn build(self, k: &Kit) -> El {
        self.toasts
            .into_iter()
            .fold(KitToastRegion::new(), |r, t| r.toast(t))
            .build(k)
    }
}

/// Stateful toast host: owns a [`ToastStack`], steps it, and renders from its entries.
///
/// The host does not spawn timers. Feed clock-driven `tick(now)` (or `step(Tick)`) and honor
/// [`ToastEffect::WakeAt`] by scheduling a wake at the given time; on [`ToastEffect::Closed`]
/// invoke the toast's close callback exactly once.
#[derive(Clone, Debug)]
pub(crate) struct ToastHost<O> {
    stack: ToastStack<O>,
}

impl<O> Default for ToastHost<O> {
    fn default() -> Self {
        ToastHost {
            stack: ToastStack::default(),
        }
    }
}

impl<O> ToastHost<O> {
    pub(crate) fn new() -> ToastHost<O> {
        ToastHost::default()
    }

    pub(crate) fn stack(&self) -> &ToastStack<O> {
        &self.stack
    }

    pub(crate) fn stack_mut(&mut self) -> &mut ToastStack<O> {
        &mut self.stack
    }

    /// Entries in stack order (oldest first, front/newest last).
    pub(crate) fn entries(&self) -> &[ToastEntry<O>] {
        self.stack.entries()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.stack.entries().is_empty()
    }

    pub(crate) fn step(&mut self, event: ToastEvent<O>) -> rune::machine::Effects<ToastEffect> {
        self.stack.step(event)
    }

    /// Show or update a toast. `secs` is the auto-dismiss duration (`None` = persistent).
    pub(crate) fn show(
        &mut self,
        key: impl Into<String>,
        options: O,
        secs: Option<f64>,
        now: f64,
    ) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::Shown {
            key: key.into(),
            options,
            secs,
            now,
        })
    }

    pub(crate) fn dismiss(&mut self, id: u64, now: f64) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::Dismissed { id: Some(id), now })
    }

    pub(crate) fn dismiss_all(&mut self, now: f64) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::Dismissed { id: None, now })
    }

    /// Advance timers: closes any toast whose due time has passed and returns effects
    /// (including [`ToastEffect::Closed`] for each expired toast and the next [`ToastEffect::WakeAt`]).
    pub(crate) fn tick(&mut self, now: f64) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::Tick { now })
    }

    pub(crate) fn pointer_over(&mut self, now: f64) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::PointerOver { now })
    }

    pub(crate) fn pointer_left(&mut self, now: f64) -> rune::machine::Effects<ToastEffect> {
        self.step(ToastEvent::PointerLeft { now })
    }

    /// Render current visible entries into a bottom-right toast region.
    ///
    /// `build` produces the card element for each entry (key should include the entry id for
    /// stable identity). Entries are rendered in stack order (oldest first, newest/front last)
    /// and filtered by [`ToastStack::place`] visibility so at most the front `VISIBLE` toasts
    /// are shown. Placement hints (`lift/scale/height/content`) are available via
    /// [`ToastStack::place`] for callers that need Sonner-style positioning.
    pub(crate) fn render(&self, k: &Kit, mut build: impl FnMut(&ToastEntry<O>) -> El) -> El {
        let cards = self
            .entries()
            .iter()
            .filter(|entry| {
                self.stack
                    .place(entry.id, |_| 0.0)
                    .is_some_and(|p| p.visible)
            })
            .map(|entry| build(entry))
            .collect::<Vec<_>>();
        toast_region(cards).build(k)
    }
}