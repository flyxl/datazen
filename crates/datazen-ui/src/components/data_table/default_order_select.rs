//! The table's default-order override field, ported 1:1 from
//! `packages/frontend/src/components/DataTable/DefaultOrderSelect.tsx`.
//!
//! ```text
//! <div class="flex items-center gap-1">
//!   <span class="hidden sm:inline">排序</span>
//!   <Select class="h-7 w-[148px]" ... />
//! </div>
//! ```
//!
//! The tree is unchanged: one label and one select, 4 points apart. The label's
//! React class `hidden sm:inline` never hides it at DataZen's minimum window
//! (960), so it is always built. The control is `ui/select.rs` (kit `Select`,
//! field appearance, 148 points wide) and the four choices are the React
//! `DefaultOrderOverride` union; `on_change` receives that enum, never the
//! label that the kit reports.
//!
//! ## Key convention
//!
//! | part              | key                    |
//! |-------------------|------------------------|
//! | root              | `<key>`                |
//! | label             | `<key>-label`          |
//! | field (prefix)    | `<key>-field`          |
//! | trigger           | `<key>-field-trigger`  |
//! | listbox content   | `<key>-field-content`  |
//! | dismissal scrim   | `<key>-field-scrim`    |
//!
//! `<key>-field` is the key this module hands to `ui/select.rs`; it is only the
//! logical group name and is **not** a findable node. The kit derives the real
//! keys from it: `<key>-field-trigger` always exists, `<key>-field-content` and
//! `<key>-field-scrim` exist while the listbox is open (`on_close` mounts the
//! scrim). List rows are `option0`…`option3` (three rows while the primary key
//! is composite).
//!
//! The selected row and the `primaryKeyDesc` row carry the React values
//! (`inherit`, `primaryKeyAsc`, `primaryKeyDesc`, `none`), so a stored value
//! round-trips without a label lookup.
//!
//! ## Gaps against React (no rebuild of the control)
//!
//! - React renders the `primaryKeyDesc` row disabled for a composite primary
//!   key (`aria-disabled` + `opacity-40`, and its arrow movement skips it);
//!   `rune-kit`'s `Select` has no per-option disabled state. The port therefore
//!   **does not offer that row at all** while the primary key is composite: the
//!   listbox holds three choices, and the reason is the field's accessible
//!   description. React greys the row out; we leave it out.
//! - React's `title={composite ? unavailable : undefined}` on the trigger is a
//!   native browser tooltip; the kit select has no `title` and DataZen's
//!   `ui/tooltip` is state-driven (the caller owns hover intent), so the same
//!   text is only the accessible description here.
//! - React's `aria-label` falls back to the select's placeholder; this port
//!   names the control with its visible label (`defaultSort.panelLabel`), which
//!   is stable across the open/closed state.
//!
//! Wiring into the table toolbar is a later task; until then this module and
//! its test file are the only callers.

use rune::{row, text, El, Tx};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::select::Select;

/// The table's default-order override: the React
/// `'inherit' | 'primaryKeyAsc' | 'primaryKeyDesc' | 'none'` union.
#[allow(dead_code)] // ported base; remove when the table toolbar call site lands
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DefaultOrderOverride {
    /// Follow `settings.defaultTableSort`.
    #[default]
    Inherit,
    PrimaryKeyAsc,
    PrimaryKeyDesc,
    None,
}

#[allow(dead_code)] // ported base; remove when the table toolbar call site lands
impl DefaultOrderOverride {
    /// Every choice in listbox order, as React declares it.
    const ALL: [DefaultOrderOverride; 4] = [
        DefaultOrderOverride::Inherit,
        DefaultOrderOverride::PrimaryKeyAsc,
        DefaultOrderOverride::PrimaryKeyDesc,
        DefaultOrderOverride::None,
    ];

    /// The value React stores (`tableData/types.ts`).
    pub(crate) fn value(self) -> &'static str {
        match self {
            DefaultOrderOverride::Inherit => "inherit",
            DefaultOrderOverride::PrimaryKeyAsc => "primaryKeyAsc",
            DefaultOrderOverride::PrimaryKeyDesc => "primaryKeyDesc",
            DefaultOrderOverride::None => "none",
        }
    }

    /// The label key shown for this choice (`packages/frontend/src/locales/zh-CN/query.ts`).
    fn label_key(self) -> &'static str {
        match self {
            DefaultOrderOverride::Inherit => "defaultSort.inherit",
            DefaultOrderOverride::PrimaryKeyAsc => "defaultSort.primaryKeyAsc",
            DefaultOrderOverride::PrimaryKeyDesc => "defaultSort.primaryKeyDesc",
            DefaultOrderOverride::None => "defaultSort.none",
        }
    }

    /// The stored value back to its choice; `None` for a value this union does not know.
    pub(crate) fn from_value(value: &str) -> Option<DefaultOrderOverride> {
        DefaultOrderOverride::ALL
            .into_iter()
            .find(|choice| choice.value() == value)
    }
}

/// What a builder step configures on the select before it is built.
type Bind = Box<dyn FnOnce(Select) -> Select>;

/// A choice handler, given whether the primary key is composite at build time.
type Change = Box<dyn FnOnce(Select, bool) -> Select>;

/// The default-order field and its visible label.
#[allow(dead_code)] // ported base; remove when the table toolbar call site lands
pub(crate) struct DefaultOrderSelect {
    key: String,
    value: DefaultOrderOverride,
    primary_key_count: u32,
    disabled: bool,
    open: bool,
    toggled: bool,
    binds: Vec<Bind>,
    change: Option<Change>,
}

#[allow(dead_code)] // ported base; remove when the table toolbar call site lands
impl DefaultOrderSelect {
    /// `DefaultOrderSelect::new("table-order", DefaultOrderOverride::Inherit)`.
    pub(crate) fn new(key: impl Into<String>, value: DefaultOrderOverride) -> DefaultOrderSelect {
        DefaultOrderSelect {
            key: key.into(),
            value,
            primary_key_count: 0,
            disabled: false,
            open: false,
            toggled: false,
            binds: Vec::new(),
            change: None,
        }
    }

    /// The table's primary-key column count. More than one disables
    /// `primaryKeyDesc`, exactly as React's `composite` does.
    pub(crate) fn primary_key_count(mut self, count: u32) -> Self {
        self.primary_key_count = count;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Whether the app's state has the listbox open.
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The trigger's press: open or close the listbox.
    pub(crate) fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggled = true;
        self.binds.push(Box::new(move |select: Select| select.on_toggle(f)));
        self
    }

    /// Presses outside and Escape close the listbox.
    pub(crate) fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.binds
            .push(Box::new(move |select: Select| select.on_close(f)));
        self
    }

    /// A choice, from the keyboard (Enter) or the pointer (a press on the row).
    /// `f(state, override)`; closing the listbox is the app's call.
    pub(crate) fn on_change<S: 'static>(
        mut self,
        f: impl Fn(&mut S, DefaultOrderOverride) + Clone + 'static,
    ) -> Self {
        self.change = Some(Box::new(move |select: Select, composite: bool| {
            select.on_choose(move |state: &mut S, value: &str, _tx: &mut Tx| {
                let Some(picked) = DefaultOrderOverride::from_value(value) else {
                    return;
                };
                // Defensive: the row is absent while the primary key is composite, so this
                // value cannot arrive; keep the guard so a stale stored value cannot win.
                if composite && picked == DefaultOrderOverride::PrimaryKeyDesc {
                    return;
                }
                f(state, picked);
            })
        }));
        self
    }

    /// Builds the label and the select, pushing the listbox into `layer` while
    /// `open` (the same two-part build as `ui/select.rs`).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let key = self.key;
        let composite = self.primary_key_count > 1;
        // React greys the `primaryKeyDesc` row out for a composite key; the kit has no
        // per-option disabled row, so the choice is left out instead of offered and ignored.
        let choices: Vec<DefaultOrderOverride> = if composite {
            DefaultOrderOverride::ALL
                .into_iter()
                .filter(|choice| *choice != DefaultOrderOverride::PrimaryKeyDesc)
                .collect()
        } else {
            DefaultOrderOverride::ALL.to_vec()
        };
        let labels: Vec<&str> = choices
            .iter()
            .map(|choice| t(choice.label_key()))
            .collect();
        let values: Vec<&str> = choices.iter().map(|choice| choice.value()).collect();
        let mut select = Select::new(format!("{key}-field"), &labels)
            .values(&values)
            .current(Some(self.value.value()))
            .label(t("defaultSort.panelLabel"))
            .width(148.0)
            .open(self.open)
            .disabled(self.disabled);
        debug_assert!(
            self.disabled || self.toggled,
            "an enabled default-order select without on_toggle cannot open"
        );
        for bind in self.binds {
            select = bind(select);
        }
        if let Some(change) = self.change {
            select = change(select, composite);
        }
        let mut field = select.build(k, layer);
        if composite {
            field = field.description(t("defaultSort.primaryKeyDescUnavailable"));
        }
        row()
            .key(key.clone())
            .items_center()
            .gap(4.0)
            .child(
                text(t("defaultSort.panelLabel"))
                    .key(format!("{key}-label"))
                    .font(k.ty.ui)
                    .color(k.c.text_base),
            )
            .child(field)
    }
}
