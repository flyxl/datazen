//! The DataZen select: a kit `Select` in its field appearance, with the app's own `key`,
//! options and `open` state.
//!
//! Everything visual and behavioural lives in `rune-kit`'s `Select` (the trigger, the anchored
//! listbox, the keyboard model: Down/Up open it and move the highlight, Enter chooses, Escape
//! closes). This wrapper only builds the kit component and names the state the app owns, so a
//! view never re-implements choosing: `on_choose` receives the kit's choice, and whether the
//! listbox closes afterwards is the app's decision in that handler.

// The views that call this land in later migration tasks; `tests/components.rs` is the only
// caller today, so the lib target would otherwise warn about every item.
#![allow(dead_code)]

use rune::{El, Tx};
use rune_kit::{Kit, Select as KitSelect, SelectAppearance};

/// What the app's handlers configure on the kit select. `build` consumes the wrapper, so the
/// handlers are folded in last, once the static configuration is in place.
type Bind = Box<dyn FnOnce(KitSelect) -> KitSelect>;

/// A field that opens a listbox of options.
pub(crate) struct Select {
    key: String,
    options: Vec<String>,
    current: Option<String>,
    placeholder: Option<String>,
    label: Option<String>,
    width: Option<f32>,
    open: bool,
    disabled: bool,
    toggled: bool,
    binds: Vec<Bind>,
}

impl Select {
    /// `Select::new("engine", &["sqlite", "postgres"])`.
    pub(crate) fn new<S: AsRef<str>>(key: impl Into<String>, options: &[S]) -> Select {
        Select {
            key: key.into(),
            options: options.iter().map(|o| o.as_ref().to_owned()).collect(),
            current: None,
            placeholder: None,
            label: None,
            width: None,
            open: false,
            disabled: false,
            toggled: false,
            binds: Vec::new(),
        }
    }

    /// The chosen option, or `None` for the placeholder.
    pub(crate) fn current(mut self, current: Option<&str>) -> Select {
        self.current = current.map(str::to_owned);
        self
    }

    /// Shown while nothing is chosen.
    pub(crate) fn placeholder(mut self, placeholder: impl Into<String>) -> Select {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// The accessible name ("Model"). The chosen option is the a11y value.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Select {
        self.label = Some(label.into());
        self
    }

    /// A fixed width; without it the field fills its parent.
    pub(crate) fn width(mut self, width: f32) -> Select {
        self.width = Some(width);
        self
    }

    /// Whether the app's state has the listbox open.
    pub(crate) fn open(mut self, open: bool) -> Select {
        self.open = open;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Select {
        self.disabled = disabled;
        self
    }

    /// The trigger's press: open or close the listbox.
    pub(crate) fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Select {
        self.toggled = true;
        self.binds.push(Box::new(move |s| s.on_toggle(f)));
        self
    }

    /// Presses outside and Escape close the listbox.
    pub(crate) fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Select {
        self.binds.push(Box::new(move |s| s.on_close(f)));
        self
    }

    /// A choice, from the keyboard (Enter) or the pointer (a press on the row): both arrive
    /// here. `f(state, option, tx)`; closing the listbox is the app's call.
    pub(crate) fn on_choose<S: 'static>(
        mut self,
        f: impl Fn(&mut S, &str, &mut Tx) + Clone + 'static,
    ) -> Select {
        self.binds.push(Box::new(move |s| s.on_choose(f)));
        self
    }

    /// Builds the trigger, pushing the listbox into `layer` while `open`.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        // An enabled field with no toggle is a dead control: focusable, pressable, and inert.
        // A disabled one never opens, so it needs none.
        debug_assert!(
            self.disabled || self.toggled,
            "an enabled select without on_toggle cannot open"
        );
        let mut kit = KitSelect::new(&self.key, &self.options)
            .appearance(SelectAppearance::Field)
            .open(self.open)
            .disabled(self.disabled);
        if let Some(current) = self.current.as_deref() {
            kit = kit.current(Some(current));
        }
        if let Some(placeholder) = self.placeholder.as_deref() {
            kit = kit.placeholder(placeholder);
        }
        if let Some(label) = self.label.as_deref() {
            kit = kit.label(label);
        }
        let trigger = self
            .binds
            .into_iter()
            .fold(kit, |kit, bind| bind(kit))
            .build(k, layer);
        // The kit field is 280 points wide by default; DataZen fields fill their parent unless
        // a width is asked for (`El::w`/`w_full` are last-write-wins).
        match self.width {
            Some(width) => trigger.w(width),
            None => trigger.w_full(),
        }
    }
}
