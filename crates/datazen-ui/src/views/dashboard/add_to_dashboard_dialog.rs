//! Port of `packages/frontend/src/windows/dashboard/AddToDashboardDialog.tsx`.
//!
//! The "add to dashboard" modal: pick an existing dashboard from the list the
//! host loads, or switch to creating a new one by name, then confirm.
//!
//! ## What maps where
//!
//! - `dashboardCommands.listDashboards()` is a host concern, not Rune's: the
//!   host calls [`AddToDashboardState::begin_open`] when the `open` prop
//!   turns true, fetches the list out-of-band and delivers it with
//!   [`AddToDashboardState::deliver_list`]. React's `cancelled` flag — set by
//!   the effect's cleanup when the dialog closes or re-opens — is the
//!   `request_epoch` token: a delivery from a superseded request (or after
//!   close) returns `false` and is dropped, exactly as a cancelled promise's
//!   `setState` never lands.
//! - `useLocaleDomains(['dashboard'])` (the web shell's async locale-pack
//!   gate, including its `title="…"` placeholder dialog) has no native
//!   counterpart: the zh-CN table is compiled in by `build.rs`, so the gate
//!   is elided and the dialog never renders half-translated keys.
//! - The five `useState` values (`dashboards`, `loading`, `selection`,
//!   `creatingNew`, `newName`) plus the open flag live in
//!   [`AddToDashboardState`], kept in the owning state `S`. `selection`
//!   stays a `String` where `""` is React's "nothing selected" (the confirm
//!   button's disabled guard reads it exactly that way).
//! - React's `Dashboard` wire type is the host's concern; the dialog needs
//!   only `id` + `name`, so [`DashboardOptionVm`] is the projection (the
//!   host converts, as with `PromptTemplateVm` in the AI settings port).
//! - The OK button's disabled expression — `loading || (creatingNew &&
//!   !newName.trim()) || (!creatingNew && !selection)` — is
//!   [`AddToDashboardState::can_confirm`], and `handleConfirm`'s two
//!   payloads are [`AddToDashboardTarget`] from [`Self::confirm`]: `('new',
//!   trimmed || t('dashboard.defaultName'))` keeps React's fall-back to the
//!   default name for a blank input.
//! - React's `text-accent hover:underline` link buttons (create-new /
//!   select-existing) render as pressable accent text rows: Rune has no
//!   `:hover` pseudo-class, so the underline is a documented delta (the
//!   accent colour and pressability carry the affordance).
//! - `autoFocus` on the name input: the kit text field takes the caret when
//!   it is the branch's only focusable field (there is no explicit focus
//!   call on `TextField`; the ported dialogs behave the same way).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the rest derive from the
//! component key (default `"add-to-dashboard"`):
//!
//! | part | key |
//! |------|-----|
//! | dialog shell | `<key>`, `<key>-title`, `<key>-close`, `<key>-body`, `<key>-footer`, `<key>-scrim`, `<key>-layer` (from `ui/dialog.rs`) |
//! | body root | `add-to-dashboard-dialog` |
//! | loading line | `<key>-loading` |
//! | target select | `dashboard-target-select` (its trigger key from `ui/select.rs`) |
//! | create-new link | `dashboard-target-new` |
//! | create-new title | `<key>-create-new-title` |
//! | name input | `dashboard-new-panel-name` |
//! | select-existing link | `<key>-select-existing` |
//! | cancel / confirm | `<key>-cancel`, `add-to-dashboard-confirm` |
//!
//! Known deltas (documented, not silent): the panel width is the React
//! panel's `max-w-xl` (432pt) rather than Tailwind's responsive `w-full`; the
//! link buttons lose their hover underline; `Input` maps to the kit
//! `TextField` (the ported-dialog convention); `Plus` is the registry
//! `ICON_PLUS` at 12pt (React `h-3 w-3`).

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_PLUS;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// React's `Dashboard` projection — the dialog reads only `id` and `name`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DashboardOptionVm {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// The confirm payload: React's `onConfirm(dashboardId | 'new', newName?)`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddToDashboardTarget {
    /// `onConfirm('new', name)` — the name defaults to
    /// `t('dashboard.defaultName')` when blank.
    New { name: String },
    /// `onConfirm(selection)` — an existing dashboard id.
    Existing { id: String },
}

/// Every `useState` of the React component plus the open flag, kept in the
/// owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default)]
pub(crate) struct AddToDashboardState {
    /// React's `open` prop; `false` paints nothing at all.
    pub(crate) open: bool,
    /// Bumped on each open; the [`Dialog`](crate::ui::dialog::Dialog) reset
    /// token for scroll and focus.
    pub(crate) open_epoch: u64,
    /// The load this state waits for; deliveries carrying an older token are
    /// dropped (React's effect-cleanup `cancelled` flag).
    pub(crate) request_epoch: u64,
    pub(crate) dashboards: Vec<DashboardOptionVm>,
    pub(crate) loading: bool,
    /// React's `selection` (`""` = nothing selected).
    pub(crate) selection: String,
    pub(crate) creating_new: bool,
    pub(crate) new_name: TextEdit,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl AddToDashboardState {
    /// The `open` effect's synchronous half: start loading, leave the
    /// create-new branch, prefill the default name. Stale `dashboards` and
    /// `selection` deliberately survive (React's state does too) until the
    /// delivery overwrites them. The host bumps `request_epoch` here (it
    /// mirrors `open_epoch`) and fetches.
    pub(crate) fn begin_open(&mut self) {
        self.open = true;
        self.open_epoch = self.open_epoch.saturating_add(1);
        self.request_epoch = self.open_epoch;
        self.loading = true;
        self.creating_new = false;
        self.new_name = TextEdit::new(t("dashboard.defaultName").to_owned());
    }

    /// The async half: the fetched list lands (React's `.then`) and the
    /// spinner stops (the `finally`). Returns `false` — with the state
    /// untouched — when the token is stale or the dialog closed, which is
    /// React's `cancelled` guard; an applied delivery returns `true`.
    pub(crate) fn deliver_list(&mut self, list: Vec<DashboardOptionVm>, token: u64) -> bool {
        if !self.open || token != self.request_epoch {
            return false;
        }
        self.selection = match list.first() {
            Some(first) => first.id.clone(),
            None => {
                // An empty list goes straight to the create-new branch.
                self.creating_new = true;
                String::new()
            }
        };
        self.dashboards = list;
        self.loading = false;
        true
    }

    /// The `open` prop turning false (header close, Escape, scrim, cancel).
    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    /// The select's `onChange`.
    pub(crate) fn select(&mut self, id: &str) {
        self.selection = id.to_owned();
    }

    /// The create-new link (`setCreatingNew(true)`); the name keeps whatever
    /// it holds (React does not reset it here either).
    pub(crate) fn begin_create_new(&mut self) {
        self.creating_new = true;
    }

    /// The select-existing link: back to the picker, selection back to the
    /// first dashboard (React `setSelection(dashboards[0]!.id)`).
    pub(crate) fn back_to_existing(&mut self) {
        if let Some(first) = self.dashboards.first() {
            self.selection = first.id.clone();
            self.creating_new = false;
        }
    }

    /// The OK button's `disabled` expression, verbatim:
    /// `loading || (creatingNew && !newName.trim()) || (!creatingNew && !selection)`.
    pub(crate) fn can_confirm(&self) -> bool {
        !(self.loading
            || (self.creating_new && self.new_name.text.trim().is_empty())
            || (!self.creating_new && self.selection.is_empty()))
    }

    /// `handleConfirm`'s payload: the trimmed name (defaulting to
    /// `t('dashboard.defaultName')` when blank) for the create-new branch,
    /// the selection for the picker branch.
    pub(crate) fn confirm(&self) -> AddToDashboardTarget {
        if self.creating_new {
            let name = self.new_name.text.trim();
            let name = if name.is_empty() { t("dashboard.defaultName") } else { name };
            AddToDashboardTarget::New { name: name.to_owned() }
        } else {
            AddToDashboardTarget::Existing { id: self.selection.clone() }
        }
    }
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
type CloseFn<S> = Rc<dyn Fn(&mut S) + 'static>;
#[allow(dead_code)] // reusable port; wired by the dashboard host track
type ChooseFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;
#[allow(dead_code)] // reusable port; wired by the dashboard host track
type ModeFn<S> = Rc<dyn Fn(&mut S) + 'static>;
#[allow(dead_code)] // reusable port; wired by the dashboard host track
type ConfirmFn<S> = Rc<dyn Fn(&mut S, AddToDashboardTarget) + 'static>;

/// Builder for the dialog, generic over the app state `S` that owns the
/// state (the ported-dialog pattern of `components/connection`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) struct AddToDashboardDialog<'a, S: 'static> {
    key: &'a str,
    state: &'a AddToDashboardState,
    on_close: Option<CloseFn<S>>,
    on_select: Option<ChooseFn<S>>,
    on_begin_new: Option<ModeFn<S>>,
    on_back_existing: Option<ModeFn<S>>,
    on_confirm: Option<ConfirmFn<S>>,
    name_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> AddToDashboardDialog<'a, S> {
    pub(crate) fn new(key: &'a str, state: &'a AddToDashboardState) -> Self {
        Self {
            key,
            state,
            on_close: None,
            on_select: None,
            on_begin_new: None,
            on_back_existing: None,
            on_confirm: None,
            name_edit: None,
        }
    }

    /// Header close / Escape / scrim / footer cancel.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// A dashboard picked from the list.
    pub(crate) fn on_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The create-new link.
    pub(crate) fn on_begin_new(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_begin_new = Some(Rc::new(f));
        self
    }

    /// The select-existing link.
    pub(crate) fn on_back_existing(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back_existing = Some(Rc::new(f));
        self
    }

    /// The OK action: the only place the confirm payload leaves the dialog.
    /// The payload is computed from the state at build time — the immediate
    /// mode equivalent of React reading the state inside `handleConfirm`.
    pub(crate) fn on_confirm(mut self, f: impl Fn(&mut S, AddToDashboardTarget) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(f));
        self
    }

    /// The name input's edit projection into `S` (`React onChange`), the
    /// `TextField::on_edit` convention of `ui/field.rs`.
    pub(crate) fn on_edit_name(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.name_edit = Some(Box::new(move |field| field.on_edit(f)));
        self
    }

    /// Paints the modal into `layer`; nothing at all while closed (React's
    /// `Dialog open={false}` renders null).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.state.open {
            return;
        }
        let k = kit(dark);
        let mut dialog = Dialog::new(self.key, t("dashboard.addToDashboard"))
            .open(true)
            .open_epoch(self.state.open_epoch)
            .width(432.0) // the React panel's max-w-xl
            .fit()
            .body_max_height(448.0) // 70vh at the 640pt minimum window
            .body_pad((15.0, 12.0)); // the React panel's px-5 py-4

        dialog = dialog.footer(self.cancel_button(&k));
        dialog = dialog.footer(self.confirm_button(&k));
        // `on_close` leaves `self` before the consuming `body` call.
        let close = self.on_close.clone();
        for child in self.body(&k, dark, layer) {
            dialog = dialog.child(child);
        }

        dialog = dialog.on_close(move |s: &mut S| {
            if let Some(close) = &close {
                close(s);
            }
        });
        dialog.build(k, layer);
    }

    // ---- footer ---------------------------------------------------------------

    fn cancel_button(&self, k: &Kit) -> El {
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large); // React h-8
        if let Some(close) = &self.on_close {
            let close = Rc::clone(close);
            cancel = cancel.on_press(move |s: &mut S| close(s));
        }
        cancel.build(k).key(format!("{}-cancel", self.key))
    }

    fn confirm_button(&self, k: &Kit) -> El {
        let disabled = !self.state.can_confirm();
        let mut confirm = Button::new(t("common.ok"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Large) // React h-8 px-3 text-xs
            .disabled(disabled);
        if !disabled {
            if let Some(confirm_cb) = &self.on_confirm {
                let confirm_cb = Rc::clone(confirm_cb);
                let payload = self.state.confirm();
                confirm = confirm.on_press(move |s: &mut S| confirm_cb(s, payload.clone()));
            }
        }
        confirm.build(k).key("add-to-dashboard-confirm")
    }

    // ---- body -----------------------------------------------------------------

    /// The three body branches, exactly one painted (React's `loading` /
    /// `!loading && !creatingNew` / `!loading && creatingNew` guards).
    fn body(self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> Vec<El> {
        if self.state.loading {
            return vec![text(t("common.loading"))
                .key(format!("{}-loading", self.key))
                .font(type_xs())
                .color(k.c.text_muted)];
        }
        if !self.state.creating_new {
            return vec![self.picker_branch(k, layer)];
        }
        vec![self.create_branch(k, dark)]
    }

    /// The existing-dashboard picker: hint, select, create-new link.
    fn picker_branch(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let labels: Vec<&str> = self.state.dashboards.iter().map(|d| d.name.as_str()).collect();
        let values: Vec<&str> = self.state.dashboards.iter().map(|d| d.id.as_str()).collect();
        let current = (!self.state.selection.is_empty()).then_some(self.state.selection.as_str());

        let mut select = Select::new("dashboard-target-select", &labels)
            .values(&values)
            .current(current)
            .placeholder(t("dashboard.selectPanel"))
            .width(288.0);
        if let Some(select_cb) = &self.on_select {
            let select_cb = Rc::clone(select_cb);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                select_cb(s, value.to_owned());
            });
        }

        let mut new_link = row().key("dashboard-target-new").gap(4.0).center_y();
        if let Some(begin_new) = &self.on_begin_new {
            let begin_new = Rc::clone(begin_new);
            new_link = new_link.on_press(move |s: &mut S| begin_new(s));
        }

        col()
            .key("add-to-dashboard-dialog")
            .w_full()
            .gap(8.0) // React space-y-2
            .child(text(t("dashboard.selectPanel")).font(type_xs()).color(k.c.text_muted))
            .child(select.build(k, layer))
            .child(
                new_link
                    .child(icon(&ICON_PLUS, 12.0).color(k.c.text_accent))
                    .child(text(t("dashboard.createNewPanel")).font(type_xs()).color(k.c.text_accent)),
            )
    }

    /// The create-new branch: title, name field, select-existing link.
    fn create_branch(self, k: &Kit, dark: bool) -> El {
        let mut field = TextField::new("dashboard-new-panel-name", &self.state.new_name)
            .placeholder(t("dashboard.name"))
            .w_full();
        if let Some(edit) = self.name_edit {
            field = edit(field);
        }

        let mut existing_link = text(t("dashboard.selectExisting"))
            .key(format!("{}-select-existing", self.key))
            .font(type_xs())
            .color(k.c.text_accent);
        if let Some(back) = &self.on_back_existing {
            let back = Rc::clone(back);
            existing_link = existing_link.on_press(move |s: &mut S| back(s));
        }

        let mut branch = col()
            .key("add-to-dashboard-dialog")
            .w_full()
            .gap(8.0) // React space-y-2
            .child(
                text(t("dashboard.createNewPanel"))
                    .key(format!("{}-create-new-title", self.key))
                    .font(TextStyle::new(12.0, 500.0)) // React text-xs font-medium
                    .color(k.c.text_base),
            )
            .child(field.build(dark));
        // The back-link only exists while at least one dashboard was listed
        // (React `{dashboards.length > 0 && …}`).
        if !self.state.dashboards.is_empty() {
            branch = branch.child(existing_link);
        }
        branch
    }
}

/// React's `text-xs` (12px).
fn type_xs() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}
