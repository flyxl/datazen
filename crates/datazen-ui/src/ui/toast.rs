//! The DataZen toast: a kit `Toast` and a region that stacks them.
//!
//! The visual piece and its entrance motion live in `rune-kit`'s `Toast` and `ToastRegion`;
//! the timed lifecycle (auto-dismiss, pause while expanded or held, coalescing by key,
//! peek/expand placement) lives in `rune-kit`'s [`ToastStack`] state machine. The app owns the
//! stack in its state and steps it on events; this module only builds the elements.
//!
//! ```ignore
//! // App state holds: `toasts: ToastStack<ToastOptions>`.
//! // On show:    toasts.step(ToastEvent::Shown { key, options, secs: Some(4.0), now });
//! // On tick:    toasts.step(ToastEvent::Tick { now });  // driven by the app's clock/wake
//! // On render:
//! let mut cards = Vec::new();
//! for entry in state.toasts.entries() {
//!     let mut card = toast::Toast::new(format!("toast-{}", entry.id), &entry.options.title)
//!         .on_close({ let id = entry.id; move |s: &mut AppState| { s.dismiss_toast(id); } });
//!     if let Some(d) = entry.options.description.as_deref() { card = card.description(d); }
//!     cards.push(card.build(kit));
//! }
//! layer.push(toast::toast_region(cards).build(kit));
//! ```
//!
//! No timer thread is introduced here; wake scheduling stays with the app/runtime
//! (`ToastEffect::WakeAt`).

// The views that call this land in later migration tasks; `tests/overlay.rs` is the only
// caller today, so the lib target would otherwise warn about every item.
#![allow(dead_code)]

use rune::{El, Icon, Tx};
use rune_kit::{Kit, Toast as KitToast, ToastRegion as KitToastRegion};

/// One toast card.
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

    /// Any 16 point leading element (coloured icons, a loader).
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