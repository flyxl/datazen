//! The DataZen switches: checkbox, switch and radio group over `rune-kit`.
//!
//! Each wrapper adds the DataZen `key` (the kit roots have none) and wraps the kit's handler
//! setters. Everything else — the 16 point control, the row that makes the label clickable,
//! the focus ring, the disabled state — is the kit's, so the port keeps one implementation of
//! how a switch looks and responds.
//!
//! The radio group is assembled from kit `RadioItem`s rather than kit `RadioGroup`: the kit
//! group names its rows `item0`, `item1`, … which collides as soon as a view shows two groups.
//! The container mirrors `RadioGroup::build` (label, description, `items` column, spacing) and
//! names each row `{group}-item-{index}` instead.

// The views that call this land in later migration tasks; `tests/components.rs` is the only
// caller today, so the lib target would otherwise warn about every item.
#![allow(dead_code)]

use rune::{col, text, El};
use rune_kit::{
    Checkbox as KitCheckbox, ControlSize, Kit, RadioItem as KitRadioItem, Switch as KitSwitch,
};

type CheckboxBind = Box<dyn FnOnce(KitCheckbox) -> KitCheckbox>;
type SwitchBind = Box<dyn FnOnce(KitSwitch) -> KitSwitch>;
type RadioBind = Box<dyn FnOnce(KitRadioItem) -> KitRadioItem>;

/// A row that toggles one boolean: a 16 point box and its label.
pub(crate) struct Checkbox {
    key: String,
    checked: bool,
    label: Option<String>,
    description: Option<String>,
    disabled: bool,
    binds: Vec<CheckboxBind>,
}

impl Checkbox {
    pub(crate) fn new(key: impl Into<String>, checked: bool) -> Checkbox {
        Checkbox {
            key: key.into(),
            checked,
            label: None,
            description: None,
            disabled: false,
            binds: Vec::new(),
        }
    }

    pub(crate) fn label(mut self, label: impl Into<String>) -> Checkbox {
        self.label = Some(label.into());
        self
    }

    /// A second line under the label.
    pub(crate) fn description(mut self, description: impl Into<String>) -> Checkbox {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Checkbox {
        self.disabled = disabled;
        self
    }

    /// A press anywhere on the row (the box or the label) toggles it.
    pub(crate) fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Checkbox {
        self.binds.push(Box::new(move |c| c.on_toggle(f)));
        self
    }

    pub(crate) fn build(self, k: &Kit) -> El {
        let mut kit = KitCheckbox::new(self.checked).disabled(self.disabled);
        if let Some(label) = self.label.as_deref() {
            kit = kit.label(label);
        }
        if let Some(description) = self.description.as_deref() {
            kit = kit.description(description);
        }
        let root = self
            .binds
            .into_iter()
            .fold(kit, |kit, bind| bind(kit))
            .build(k);
        root.key(self.key)
    }
}

/// A row that toggles one boolean: a sliding control and its label.
pub(crate) struct Switch {
    key: String,
    checked: bool,
    label: Option<String>,
    description: Option<String>,
    disabled: bool,
    control_size: ControlSize,
    web: bool,
    standard: bool,
    binds: Vec<SwitchBind>,
}

impl Switch {
    pub(crate) fn new(key: impl Into<String>, checked: bool) -> Switch {
        Switch {
            key: key.into(),
            checked,
            label: None,
            description: None,
            disabled: false,
            control_size: ControlSize::Mini,
            web: false,
            standard: false,
            binds: Vec::new(),
        }
    }

    pub(crate) fn label(mut self, label: impl Into<String>) -> Switch {
        self.label = Some(label.into());
        self
    }

    /// A second line under the label.
    pub(crate) fn description(mut self, description: impl Into<String>) -> Switch {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Switch {
        self.disabled = disabled;
        self
    }

    /// The control's size; compact by default (a 36 × 16 track).
    pub(crate) fn control_size(mut self, size: ControlSize) -> Switch {
        self.control_size = size;
        self
    }

    /// The web track (24 × 16 with a 12 point thumb) instead of the compact native one.
    pub(crate) fn web(mut self) -> Switch {
        self.web = true;
        self
    }

    /// The macOS standard control (28 × 16, its label in the standard ink).
    pub(crate) fn standard(mut self) -> Switch {
        self.standard = true;
        self
    }

    pub(crate) fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Switch {
        self.binds.push(Box::new(move |s| s.on_toggle(f)));
        self
    }

    pub(crate) fn build(self, k: &Kit) -> El {
        let mut kit = KitSwitch::new(self.checked)
            .disabled(self.disabled)
            .control_size(self.control_size);
        if self.web {
            kit = kit.web();
        }
        if self.standard {
            kit = kit.standard();
        }
        if let Some(label) = self.label.as_deref() {
            kit = kit.label(label);
        }
        if let Some(description) = self.description.as_deref() {
            kit = kit.description(description);
        }
        let root = self
            .binds
            .into_iter()
            .fold(kit, |kit, bind| bind(kit))
            .build(k);
        root.key(self.key)
    }
}

/// One row of a [`RadioGroup`]: a circular control, its label and an optional description.
pub(crate) struct RadioItem {
    label: String,
    checked: bool,
    description: Option<String>,
    disabled: bool,
    binds: Vec<RadioBind>,
}

impl RadioItem {
    pub(crate) fn new(label: impl Into<String>, checked: bool) -> RadioItem {
        RadioItem {
            label: label.into(),
            checked,
            description: None,
            disabled: false,
            binds: Vec::new(),
        }
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> RadioItem {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> RadioItem {
        self.disabled = disabled;
        self
    }

    /// A press anywhere on the row (the control or the label) selects it.
    pub(crate) fn on_select<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> RadioItem {
        self.binds.push(Box::new(move |i| i.on_select(f)));
        self
    }

    fn into_kit(self, group_disabled: bool) -> KitRadioItem {
        let mut kit =
            KitRadioItem::new(self.label, self.checked).disabled(self.disabled || group_disabled);
        if let Some(description) = self.description.as_deref() {
            kit = kit.description(description);
        }
        self.binds.into_iter().fold(kit, |kit, bind| bind(kit))
    }
}

/// A labelled set of [`RadioItem`]s; exactly one is checked.
pub(crate) struct RadioGroup {
    key: String,
    label: Option<String>,
    description: Option<String>,
    disabled: bool,
    items: Vec<RadioItem>,
}

impl RadioGroup {
    pub(crate) fn new(key: impl Into<String>) -> RadioGroup {
        RadioGroup {
            key: key.into(),
            label: None,
            description: None,
            disabled: false,
            items: Vec::new(),
        }
    }

    pub(crate) fn label(mut self, label: impl Into<String>) -> RadioGroup {
        self.label = Some(label.into());
        self
    }

    pub(crate) fn description(mut self, description: impl Into<String>) -> RadioGroup {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> RadioGroup {
        self.disabled = disabled;
        self
    }

    pub(crate) fn item(mut self, item: RadioItem) -> RadioGroup {
        self.items.push(item);
        self
    }

    pub(crate) fn build(self, k: &Kit) -> El {
        let c = &k.c;
        let mut root = col()
            .key(&self.key)
            .role("radiogroup")
            .focus_group()
            .gap(8.0);
        if let Some(label) = self.label.as_deref() {
            root = root.label(label).child(
                text(label)
                    .key("label")
                    .font(k.ty.small.lh(11.0))
                    .color(c.text_faint)
                    .a11y_hidden(),
            );
        }
        if let Some(description) = self.description.as_deref() {
            root = root.description(description).child(
                text(description)
                    .key("description")
                    .font(k.ty.small.lh(13.2))
                    .color(c.text_faint)
                    .a11y_hidden(),
            );
        }
        let n = self.items.len();
        let mut items = col().key("items").gap(12.0);
        for (i, item) in self.items.into_iter().enumerate() {
            items = items.child(
                item.into_kit(self.disabled)
                    .build(k)
                    .key(format!("{}-item-{i}", self.key))
                    .position_in_set(i + 1, n),
            );
        }
        root.child(items).disabled(self.disabled)
    }
}
