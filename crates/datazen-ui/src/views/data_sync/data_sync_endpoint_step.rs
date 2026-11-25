//! T114: `windows/data-sync/DataSyncEndpointStep.tsx` ported to Rune.
//!
//! The first step of the Data Sync wizard: the scope-reconfirmation
//! notice (an unknown history run could not restore its endpoint scope)
//! and the saved-profile controls that restore a previously reviewed
//! endpoint scope, above the endpoints bar.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim; the parts
//! without one follow the step's own naming:
//!
//! | part | key |
//! |------|-----|
//! | step root | `data-sync-endpoint-step` (internal) |
//! | scope-review notice | `data-sync-recovery-scope-review` |
//! | scope-review sentence | `data-sync-recovery-scope-review-text` (internal) |
//! | reconfirm button | `data-sync-reconfirm-scope` |
//! | profile controls row | `data-sync-profile-controls` |
//! | name label | `data-sync-profile-name-label` (internal) |
//! | name input | `data-sync-profile-name` |
//! | load label | `data-sync-profile-load-label` (internal) |
//! | profile select | `data-sync-profile-select` |
//! | save button | `data-sync-profile-save` |
//! | load button | `data-sync-profile-load` |
//! | delete button | `data-sync-profile-delete` |
//! | endpoints bar mount (T322 fills it) | `data-sync-endpoints-bar` |
//!
//! ## Mounting
//!
//! [`DataSyncEndpointStep::build_inner`] returns the step element plus the
//! overlay layer (the profile select's listbox while it is open), so the
//! Data Sync window (T270/T268) can root the whole wizard once; `build` is
//! `build_inner` plus the kit root, exactly as the probes mount it. The
//! step is the wizard's first child, not a window root: the React root
//! `div.space-y-4` maps to a plain column.
//!
//! `EndpointsBar` (T322) is **not ported yet**, so the third block renders
//! an empty keyed mount slot for it — the same convention as
//! `DataSyncCompareStep`'s unported mounts. The endpoints props are not
//! modelled here at all: T322 owns the whole `EndpointsBarProps` model and
//! fills the slot; nothing else in this step reads them.
//!
//! ## Documented deviations from React
//!
//! * **The name input is the host's `TextEdit`.** React's `Input` is
//!   controlled (`value={profileName}` + `onChange`); Rune's
//!   [`TextField`] binds to a `TextEdit` the host's state owns, so
//!   `profileName` lives there: [`DataSyncEndpointStep::profile_name_edit`]
//!   passes the reference (the rendered value), and
//!   [`DataSyncEndpointStep::on_profile_name_edit`] binds the edits into
//!   it. React's per-keystroke `onProfileNameChange` rides the field's
//!   `on_change` ([`DataSyncEndpointStep::on_profile_name_change`]), where
//!   the host reads the same `TextEdit` it just wrote.
//! * **The select's open state is the host's.** React's `Select` keeps it
//!   internally; the kit select is immediate-mode, so the step takes the
//!   flag as data ([`DataSyncEndpointStep::select_open`]) and forwards the
//!   trigger press and the dismissal to the host's
//!   `on_select_toggle` / `on_select_close`.
//! * **The three profile buttons disable together.** React gates load and
//!   delete on `!selectedProfileId`; an empty selected id string is the
//!   kit select's "nothing chosen" state, exactly as the wrapper models it.
//! * **`EndpointsBarProps` is a mount slot, not props.** See Mounting.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// React `SyncProfile` (`backend-client/src/types/dataSync.ts:182-197`) as
/// this step reads it: the select lists `name` and answers `id`. The
/// persisted scope (connections, databases, schemas, table mappings,
/// options) passes through the host's save/load round-trip untouched, so
/// it is not modelled here.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Sync window track
pub struct SyncProfile {
    pub id: String,
    pub name: String,
}

/// The step's props (`DataSyncEndpointStepProps`) as data. `profileName`
/// is the host state's `TextEdit` (the controlled input), and the
/// endpoints props belong to T322's mount slot, so neither is a field
/// here.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired by the Data Sync window track
pub struct DataSyncEndpointStepVm {
    /// An unknown history run could not restore its endpoint scope: the
    /// reconfirmation notice mounts and the compare is blocked until the
    /// user confirms.
    pub scope_reconfirmation_required: bool,
    /// The saved profiles, in list order.
    pub profiles: Vec<SyncProfile>,
    /// The selected profile id; empty means nothing chosen (React's
    /// `''`-initialised `selectedProfileId`).
    pub selected_profile_id: String,
}

/// The first step of the Data Sync wizard. `S` is the host state that owns
/// the profile-name `TextEdit`, the select's open flag, and every action.
#[allow(dead_code)] // reusable component; wired by the Data Sync window track
pub struct DataSyncEndpointStep<'a, S: 'static> {
    vm: DataSyncEndpointStepVm,
    profile_name_edit: Option<&'a TextEdit>,
    select_open: bool,
    on_confirm_scope: Option<Rc<dyn Fn(&mut S)>>,
    on_profile_name_edit: Option<Rc<dyn Fn(&mut S) -> &mut TextEdit>>,
    on_profile_name_change: Option<Rc<dyn Fn(&mut S, &mut Tx)>>,
    on_select_profile: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_save_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_load_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_delete_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_select_toggle: Option<Rc<dyn Fn(&mut S)>>,
    on_select_close: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> DataSyncEndpointStep<'a, S> {
    /// The step over its props.
    pub fn new(vm: DataSyncEndpointStepVm) -> Self {
        Self {
            vm,
            profile_name_edit: None,
            select_open: false,
            on_confirm_scope: None,
            on_profile_name_edit: None,
            on_profile_name_change: None,
            on_select_profile: None,
            on_save_profile: None,
            on_load_profile: None,
            on_delete_profile: None,
            on_select_toggle: None,
            on_select_close: None,
        }
    }

    /// The host state's `TextEdit` holding the profile name (React's
    /// controlled `value`). Without it the field renders the placeholder
    /// over an empty edit, like a React step whose host never supplied a
    /// name.
    pub fn profile_name_edit(mut self, edit: &'a TextEdit) -> Self {
        self.profile_name_edit = Some(edit);
        self
    }

    /// Whether the host state holds the profile select's listbox open.
    pub fn select_open(mut self, open: bool) -> Self {
        self.select_open = open;
        self
    }

    /// React `onConfirmScope`: the scope-review notice's button.
    pub fn on_confirm_scope(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_scope = Some(Rc::new(f));
        self
    }

    /// The binding that writes the profile-name edits into the host's
    /// `TextEdit` (the `TextField::on_edit` accessor).
    pub fn on_profile_name_edit(
        mut self,
        f: impl Fn(&mut S) -> &mut TextEdit + 'static,
    ) -> Self {
        self.on_profile_name_edit = Some(Rc::new(f));
        self
    }

    /// React `onProfileNameChange(name)`: fired per change, with the host
    /// reading the same `TextEdit` the edit just wrote.
    pub fn on_profile_name_change(
        mut self,
        f: impl Fn(&mut S, &mut Tx) + 'static,
    ) -> Self {
        self.on_profile_name_change = Some(Rc::new(f));
        self
    }

    /// React `onSelectProfile(id)`: a choice from the select.
    pub fn on_select_profile(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_select_profile = Some(Rc::new(f));
        self
    }

    /// React `onSaveProfile`.
    pub fn on_save_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_profile = Some(Rc::new(f));
        self
    }

    /// React `onLoadProfile` (gated on a selected id).
    pub fn on_load_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_load_profile = Some(Rc::new(f));
        self
    }

    /// React `onDeleteProfile` (gated on a selected id).
    pub fn on_delete_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete_profile = Some(Rc::new(f));
        self
    }

    /// The select trigger's press: open or close the listbox.
    pub fn on_select_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_toggle = Some(Rc::new(f));
        self
    }

    /// Presses outside and Escape close the select's listbox.
    pub fn on_select_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_close = Some(Rc::new(f));
        self
    }

    /// The step on its own, exactly as the probes mount it.
    pub fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The step element plus the overlay layer (the profile select's
    /// listbox while it is open). The Data Sync window calls this once and
    /// roots the whole wizard once.
    pub fn build_inner(self, dark: bool) -> (El, Vec<El>) {
        let k = kit(dark);
        let mut layer = Vec::new();
        // React root: `div.space-y-4`.
        let mut root = col().key("data-sync-endpoint-step").w_full().gap(16.0);
        if self.vm.scope_reconfirmation_required {
            root = root.child(self.scope_review_notice(k, dark));
        }
        root = root.child(self.profile_controls(k, dark, &mut layer));
        // `EndpointsBar` (T322) fills this mount; React mounts it with the
        // whole `endpoints` prop set, which T322 owns.
        root = root.child(col().key("data-sync-endpoints-bar").w_full().min_w(0.0));
        (root, layer)
    }

    /// React's scope-review notice: `div.flex flex-wrap items-center
    /// justify-between gap-3 rounded-lg border border-warning/40
    /// bg-warning/10 p-3` with `role="status"`, the sentence, and the
    /// secondary confirm button.
    fn scope_review_notice(&self, k: &'static Kit, dark: bool) -> El {
        let mut notice = row()
            .key("data-sync-recovery-scope-review")
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(12.0)
            .radius(8.0)
            .border(1.0, k.alpha(k.c.warning.fg, 0.4))
            .fill(k.alpha(k.c.warning.fg, 0.1))
            .padding(12.0)
            .role("status")
            .child(
                text(t("migrationHistory.syncReconcileNeedsScope"))
                    .key("data-sync-recovery-scope-review-text")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        let mut button = Button::new(t("migrationHistory.syncReconcileConfirmScope"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small);
        if let Some(on_confirm) = &self.on_confirm_scope {
            let on_confirm = Rc::clone(on_confirm);
            button = button.on_press(move |state: &mut S| on_confirm(state));
        }
        notice = notice.child(button.build(k).key("data-sync-reconfirm-scope"));
        let _ = dark;
        notice
    }

    /// React's profile controls: `div.flex flex-wrap items-end gap-2
    /// rounded-lg border border-edge bg-surface-alt p-3` — the name label
    /// with the input, the load label with the select, and the three
    /// buttons.
    fn profile_controls(&self, k: &'static Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let mut controls = row()
            .key("data-sync-profile-controls")
            .flex_wrap()
            .items_end()
            .gap(8.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .padding(12.0)
            .child(self.name_field(k, dark))
            .child(self.profile_select_field(k, layer));
        controls = controls.child(self.save_button(k));
        controls = controls.child(self.load_button(k));
        controls = controls.child(self.delete_button(k));
        controls
    }

    /// The name label: React's `label.min-w-48 flex-1 text-xs text-fg-muted`
    /// with the `mb-1 block` caption and the controlled input (`h-8 w-full
    /// px-2 text-sm`). The field's `on_edit` binds the host's `TextEdit`
    /// — a host that wired none gets the field disabled (the wrapper
    /// debug-asserts an unbound field), which is the closest inert
    /// counterpart to a React input whose host never wired `onChange`.
    fn name_field(&self, k: &'static Kit, dark: bool) -> El {
        let mut input = TextField::new("data-sync-profile-name", self.name_edit())
            .height(32.0)
            .pad_x(8.0)
            .text_style(k.ty.text_small)
            .grow()
            .placeholder(t("sync.profile.namePlaceholder"));
        match &self.on_profile_name_edit {
            Some(on_edit) => {
                let on_edit = Rc::clone(on_edit);
                input = input.on_edit(move |state: &mut S| on_edit(state));
            }
            None => {
                input = input.disabled(true);
            }
        }
        if let Some(on_change) = &self.on_profile_name_change {
            let on_change = Rc::clone(on_change);
            input = input.on_change(move |state: &mut S, tx: &mut Tx| on_change(state, tx));
        }
        let field = input.build(dark);
        col()
            .key("data-sync-profile-name-field")
            .grow(1.0)
            .min_w(192.0)
            .gap(4.0)
            .child(
                text(t("sync.profile.name"))
                    .key("data-sync-profile-name-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(field)
    }

    /// The edit the name field renders; a process-wide empty one when the
    /// host passed no reference (the field renders disabled over it).
    fn name_edit(&self) -> &'a TextEdit {
        static EMPTY: std::sync::OnceLock<TextEdit> = std::sync::OnceLock::new();
        self.profile_name_edit.unwrap_or_else(|| {
            EMPTY.get_or_init(|| TextEdit::new(String::new()))
        })
    }

    /// The load label: React's `label.min-w-48 text-xs text-fg-muted` with
    /// the select (`w-full`).
    fn profile_select_field(&self, k: &'static Kit, layer: &mut Vec<El>) -> El {
        let labels: Vec<&str> = self.vm.profiles.iter().map(|p| p.name.as_str()).collect();
        let ids: Vec<&str> = self.vm.profiles.iter().map(|p| p.id.as_str()).collect();
        let current = self.vm.selected_profile_id.as_str();
        let mut select = Select::new("data-sync-profile-select", &labels)
            .values(&ids)
            .current(if current.is_empty() { None } else { Some(current) })
            .placeholder(t("sync.profile.select"))
            .label(t("sync.profile.load"))
            .open(self.select_open);
        // The wrapper debug-asserts an enabled select with no toggle: a
        // host that did not wire one still gets a field, opening it a
        // no-op.
        match &self.on_select_toggle {
            Some(toggle) => {
                let toggle = Rc::clone(toggle);
                select = select.on_toggle(move |state: &mut S| toggle(state));
            }
            None => select = select.on_toggle(|_: &mut S| {}),
        }
        if let Some(close) = &self.on_select_close {
            let close = Rc::clone(close);
            select = select.on_close(move |state: &mut S| close(state));
        }
        if let Some(choose) = &self.on_select_profile {
            let choose = Rc::clone(choose);
            select = select.on_choose(move |state: &mut S, profile_id: &str, _tx: &mut Tx| {
                choose(state, profile_id);
            });
        }
        col()
            .key("data-sync-profile-select-field")
            .min_w(192.0)
            .gap(4.0)
            .child(
                text(t("sync.profile.load"))
                    .key("data-sync-profile-load-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(select.build(k, layer))
    }

    /// React's save button: the default (primary) variant. The track
    /// convention maps React `primary` to kit `Contrast` (`ui/toolbar.rs`,
    /// `components/ai/ai_chat_panel_shell.rs`).
    fn save_button(&self, k: &'static Kit) -> El {
        let mut button = Button::new(t("sync.profile.save"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small);
        if let Some(on_save) = &self.on_save_profile {
            let on_save = Rc::clone(on_save);
            button = button.on_press(move |state: &mut S| on_save(state));
        }
        button.build(k).key("data-sync-profile-save")
    }

    /// React's load button: `secondary` (kit `Neutral`) and gated on a
    /// selected id.
    fn load_button(&self, k: &'static Kit) -> El {
        let mut button = Button::new(t("sync.profile.load"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .disabled(self.vm.selected_profile_id.is_empty());
        if !self.vm.selected_profile_id.is_empty() {
            if let Some(on_load) = &self.on_load_profile {
                let on_load = Rc::clone(on_load);
                button = button.on_press(move |state: &mut S| on_load(state));
            }
        }
        button.build(k).key("data-sync-profile-load")
    }

    /// React's delete button: `ghost`, gated on a selected id.
    fn delete_button(&self, k: &'static Kit) -> El {
        let mut button = Button::new(t("sync.profile.delete"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.vm.selected_profile_id.is_empty());
        if !self.vm.selected_profile_id.is_empty() {
            if let Some(on_delete) = &self.on_delete_profile {
                let on_delete = Rc::clone(on_delete);
                button = button.on_press(move |state: &mut S| on_delete(state));
            }
        }
        button.build(k).key("data-sync-profile-delete")
    }
}
