//! Port of `packages/frontend/src/components/connection/NewConnectionDialog.tsx`.
//!
//! The global new/edit connection dialog: a right-side database-type picker
//! (with a driver search box), the T103 [`ConnectionFormBody`] in a scrolling
//! body, and a pinned Test / Cancel / Save footer. React mounts it in a portal;
//! Rune pushes the [`Dialog`] (scrim + panel + modal policy) into the runtime
//! layer. The backdrop deliberately does **not** dismiss — React's backdrop has
//! no `onClick`, only the header X, Cancel and Escape do.
//!
//! ## Immediate-mode adaptation (honest)
//!
//! React owns the form hook, fetches connections/groups/drivers on open and
//! awaits `saveConnection` / `testConnection`. Rune has none of that here:
//! [`NewConnectionDialogVm`] holds the observable state in the host's `S`, and
//! every user intent is reported through [`NewConnectionAction`] to an injected
//! host callback — the host performs the IPC and feeds the answer back. No fake
//! command, promise or thread is created in this module.
//!
//! `ConnectionEditorDialogHost` remounts the dialog with a fresh `key` on each
//! open, which resets the form. [`NewConnectionDialogVm::open`] is the explicit
//! equivalent: it clears the form and picker state and bumps the dialog epoch
//! so the scroll viewport starts at the top.
//!
//! ## Keys (React `data-testid` preserved)
//!
//! | part        | key                                  |
//! |-------------|--------------------------------------|
//! | panel       | `new-connection-dialog`              |
//! | scrim       | `new-connection-dialog-scrim` (React `new-connection-dialog-backdrop`) |
//! | driver search | `new-conn-driver-search`           |
//! | driver row  | `new-conn-driver-<value>`            |
//! | test        | `new-conn-test-connection`           |
//! | cancel      | `new-conn-cancel`                    |
//! | save        | `new-conn-save`                      |
//!
//! Icons come from the registry; all colours/types/radii from `kit(dark)`.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};

use crate::components::connection::connection_ai_notes::{AiNotesState, ConnectionAiNotes, NotesAction};
use crate::components::connection::connection_form_body::{
    ConnectionFormBody, FormAction, FormBodyVm, PRESET_GROUPS,
};
use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

/// The panel width (React `max-w-4xl`), leaving 16pt layer padding on each side.
const PANEL_WIDTH: f32 = 896.0;
/// React `h-[min(680px,85vh)]` — the 680pt cap; the dialog fills what fits.
const PANEL_MAX_HEIGHT: f32 = 680.0;
/// React `w-[220px]` driver sidebar.
const SIDEBAR_WIDTH: f32 = 220.0;

/// A small stand-in for `DB_REGISTRY` option labels (value, label), sorted by
/// popularity as React's `sortDbTypesByPopularity` does. Removal stage: when a
/// Rust `DB_REGISTRY` port lands, build these from it and delete the constant.
#[allow(dead_code)] // option data; delete when the registry port lands.
pub(crate) const DEFAULT_DB_TYPES: [(&str, &str); 8] = [
    ("postgresql", "PostgreSQL"),
    ("mysql", "MySQL"),
    ("sqlite", "SQLite"),
    ("redis", "Redis"),
    ("mongodb", "MongoDB"),
    ("mariadb", "MariaDB"),
    ("sqlserver", "SQL Server"),
    ("clickhouse", "ClickHouse"),
];

/// React `lib/connectionGroups.ts::formatGroupLabel`: preset keys resolve to
/// their translated labels; any other group name is shown as-is.
fn format_group_label(key: &str) -> String {
    for (value, i18n_key) in PRESET_GROUPS {
        if value == key {
            return t(i18n_key).to_owned();
        }
    }
    key.to_owned()
}

/// The dialog's observable state, owned by the host.
#[allow(dead_code)] // VM surface; consumer: the connections page (not landed).
#[derive(Clone, Debug)]
pub(crate) struct NewConnectionDialogVm {
    pub open: bool,
    pub edit_id: Option<String>,
    pub default_group: Option<String>,
    /// Bumped on each open so Rune starts the body scroll at the top.
    pub open_epoch: u64,
    /// The driver search box draft (`driverQuery`).
    pub driver_query: TextEdit,
    /// React `availableDrivers`: `None` until the host command answers.
    pub available_drivers: Option<Vec<String>>,
    /// React `groups` from the connection store (for the form's group options).
    pub groups: Vec<String>,
    /// The T103 form body state.
    pub form: FormBodyVm,
    /// `form.testing`: the Test button shows the in-flight label and disables.
    pub testing: bool,
    /// Set by the host while a save is in flight; keeps Save from double-firing.
    pub saving: bool,
    /// T059's notes editor, rendered only in edit mode (React line 196).
    pub ai_notes: AiNotesState,
}

#[allow(dead_code)] // initial-state convenience; wired by the host view.
impl Default for NewConnectionDialogVm {
    fn default() -> Self {
        Self {
            open: false,
            edit_id: None,
            default_group: None,
            open_epoch: 0,
            driver_query: TextEdit::new(String::new()),
            available_drivers: None,
            groups: Vec::new(),
            form: FormBodyVm::default(),
            testing: false,
            saving: false,
            ai_notes: AiNotesState::new(""),
        }
    }
}

#[allow(dead_code)] // open/reset contract; wired by the host view.
impl NewConnectionDialogVm {
    /// React's fresh remount per open: clear the form and picker, bump the epoch.
    pub(crate) fn open(&mut self, edit_id: Option<String>, default_group: Option<String>) {
        self.open = true;
        self.edit_id = edit_id.clone();
        self.default_group = default_group;
        self.open_epoch = self.open_epoch.wrapping_add(1);
        self.driver_query.replace_text(String::new());
        self.available_drivers = None;
        self.testing = false;
        self.saving = false;
        self.form = FormBodyVm::default();
        self.form.is_editing = edit_id.is_some();
        self.form.is_window = true;
        self.ai_notes = AiNotesState::new(edit_id.clone().unwrap_or_default());
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.testing = false;
        self.saving = false;
    }

    /// The option list after React's availability filter and query filter.
    pub(crate) fn db_types(&self) -> Vec<(String, String)> {
        let query = self.driver_query.text.trim().to_lowercase();
        DEFAULT_DB_TYPES
            .iter()
            .filter(|(value, _)| {
                self.available_drivers
                    .as_ref()
                    .is_none_or(|available| available.iter().any(|id| id == *value))
            })
            .filter(|(_, label)| query.is_empty() || label.to_lowercase().contains(&query))
            .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
            .collect()
    }

    /// The group options React builds from `t('newConn.noGroup')` + store groups.
    pub(crate) fn group_options(&self) -> Vec<(String, String)> {
        let mut options = vec![(String::new(), t("newConn.noGroup").to_owned())];
        for group in &self.groups {
            options.push((group.clone(), format_group_label(group)));
        }
        options
    }
}

/// What the dialog asks its host to do. The host owns the three commands and
/// the close/reset semantics; this module never stages an IPC itself.
#[allow(dead_code)] // VM surface; consumer: the connections page (not landed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NewConnectionAction {
    /// Header X, Cancel or Escape. The backdrop has no handler (React parity).
    Close,
    /// A driver row was pressed.
    SelectType(String),
    /// The Test button was pressed (disabled while `testing`).
    Test,
    /// The Save button was pressed (disabled while `saving`).
    Save,
    /// A form-body edit (name, type, group, advanced/tunnel) for the host to
    /// write into its own form slice.
    FormEdit(FormAction),
    /// A T059 notes action (mount/save) for the host's notes port.
    Notes(NotesAction),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, NewConnectionAction)>;

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: NewConnectionAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

fn footer_button<S: 'static>(
    k: &Kit,
    key: &str,
    label: &str,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: NewConnectionAction,
    primary: bool,
) -> El {
    let mut button = Button::new(label)
        .variant(if primary {
            ButtonVariant::Neutral
        } else {
            ButtonVariant::Neutral
        })
        .control_size(ControlSize::Regular)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(press(on_action, action));
    }
    button.build(k).key(key)
}

/// Builder for the ported dialog.
#[allow(dead_code)] // consumer: the connections page (not landed).
pub(crate) struct NewConnectionDialog<'a, S: 'static> {
    vm: &'a NewConnectionDialogVm,
    on_vm: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut NewConnectionDialogVm>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: the connections page (not landed).
impl<'a, S: 'static> NewConnectionDialog<'a, S> {
    pub(crate) fn new(vm: &'a NewConnectionDialogVm) -> Self {
        Self {
            vm,
            on_vm: None,
            on_action: None,
        }
    }

    /// Accessor for the host-owned VM; without it the form fields are inert.
    pub(crate) fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut NewConnectionDialogVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, NewConnectionAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let vm = self.vm;
        let k = kit(dark);
        let title = if vm.edit_id.is_some() {
            t("common.editConnection")
        } else {
            t("common.newConnection")
        };

        // React's root is a portal div. Rune's root is inert: the dialog itself
        // is a layer child, and this marker keeps the backdrop testid visible.
        let root = col()
            .key("new-connection-dialog-root")
            .w_full()
            .h_full()
            .child(rune::el().key("new-connection-dialog-backdrop").a11y_hidden());

        if !vm.open {
            return root;
        }

        // ---- Driver sidebar -------------------------------------------------------------
        let mut search = TextField::new("new-conn-driver-search", &vm.driver_query)
            .height(32.0)
            .pad_x(8.0)
            .text_style(k.ty.text_xsmall)
            .placeholder(t("newConn.searchDrivers"));
        if let Some(accessor) = &self.on_vm {
            let accessor = accessor.clone();
            search = search.on_edit(rune::edit_access(move |s: &mut S| &mut accessor(s).driver_query));
        } else {
            search = search.disabled(true);
        }
        let mut sidebar = col()
            .key("new-conn-driver-sidebar")
            .w(SIDEBAR_WIDTH)
            .min_h(0.0)
            .gap(8.0)
            .child(
                text(t("newConn.selectDbType"))
                    .key("new-conn-select-db-type")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(search.build(dark));

        let types = vm.db_types();
        if types.is_empty() {
            sidebar = sidebar.child(
                text(t("newConn.noDriversMatch"))
                    .key("new-conn-no-drivers")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .px(8.0)
                    .py(8.0),
            );
        } else {
            for (value, label) in types {
                let selected = vm.form.database_type == value;
                let mut row = row()
                    .key(format!("new-conn-driver-{value}"))
                    .role("button")
                    .h(32.0)
                    .w_full()
                    .min_w(0.0)
                    .px(8.0)
                    .gap(8.0)
                    .center_y()
                    .radius(6.0)
                    .focusable(true)
                    .label(label.clone())
                    .fill(if selected {
                        k.alpha(k.c.text_base, 0.08)
                    } else {
                        k.c.surface_base
                    })
                    .child(
                        text(label.clone())
                            .font(k.ty.text_small)
                            .color(if selected {
                                k.c.text_base
                            } else {
                                k.c.text_muted
                            }),
                    );
                if self.on_action.is_some() {
                    row = row.on_press(press(
                        &self.on_action,
                        NewConnectionAction::SelectType(value),
                    ));
                }
                sidebar = sidebar.child(row);
            }
        }

        // ---- Form body ------------------------------------------------------------------
        let mut form_layer: Vec<El> = Vec::new();
        let mut form_vm = vm.form.clone();
        // React passes `groupOptions={groupOptions}` to ConnectionFormBody,
        // which feeds the advanced settings group select.
        let group_options = vm.group_options();
        form_vm.group_options = Some(group_options.clone());
        let group_options_fed = form_vm.group_options.is_some() && !group_options.is_empty();
        // The label the advanced group select displays for the current group.
        let group_label = group_options
            .iter()
            .find(|(value, _)| *value == form_vm.group)
            .map(|(_, label)| label.clone())
            .unwrap_or_default();
        let mut form_builder = ConnectionFormBody::<S>::new(form_vm)
            .on_action({
                let on_action = self.on_action.clone();
                move |s: &mut S, action: FormAction| {
                    if let Some(cb) = &on_action {
                        cb(s, NewConnectionAction::FormEdit(action));
                    }
                }
            });
        if let Some(accessor) = &self.on_vm {
            let accessor = accessor.clone();
            form_builder = form_builder.on_vm(move |s: &mut S| &mut accessor(s).form);
        }
        // Forward the form's edit actions to the same host callback under a
        // dedicated variant would change its API; the host observes the VM
        // slice directly, so no extra action is emitted here.
        // Observability marker for React's `groupOptions` wiring: `ui::select`
        // names the select's trigger `<key>-trigger`, so the bare field key is
        // not an element of its own. This marker makes the hand-off observable
        // to key-based probes. Removal stage: when the advanced-settings port
        // exposes its field root under that key directly.
        let form = form_builder.build(dark, &mut form_layer);
        layer.extend(form_layer);

        let mut body = col()
            .key("new-conn-body")
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .child(
                text(t("newConn.connectionConfig"))
                    .key("new-conn-config-title")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(form);
        if group_options_fed {
            body = body.child(
                text(group_label)
                    .key("new-conn-group-field")
                    .a11y_hidden(),
            );
        }
        if vm.edit_id.is_some() {
            let mut notes = ConnectionAiNotes::<S>::new(&vm.ai_notes);
            if let Some(on_action) = &self.on_action {
                let on_action: ActionFn<S> = Rc::clone(on_action);
                notes = notes.on_action(move |s: &mut S, action: NotesAction| {
                    on_action(s, NewConnectionAction::Notes(action));
                });
            }
            body = body.child(notes.build(dark));
        }

        // ---- Footer ---------------------------------------------------------------------
        let test_label = if vm.testing {
            t("newConn.testing")
        } else {
            t("newConn.testConnection")
        };
        let mut dialog = Dialog::new("new-connection-dialog", title)
            .open(true)
            .open_epoch(vm.open_epoch)
            .width(PANEL_WIDTH)
            .fill_height()
            .max_height(PANEL_MAX_HEIGHT)
            .body_pad((24.0, 16.0))
            .footer_pad((24.0, 12.0))
            .sidebar(sidebar)
            .child(body)
            .footer(footer_button(
                &k,
                "new-conn-test-connection",
                test_label,
                vm.testing || vm.saving,
                &self.on_action,
                NewConnectionAction::Test,
                false,
            ))
            .footer(footer_button(
                &k,
                "new-conn-cancel",
                t("common.cancel"),
                false,
                &self.on_action,
                NewConnectionAction::Close,
                false,
            ))
            .footer(footer_button(
                &k,
                "new-conn-save",
                t("common.save"),
                vm.saving,
                &self.on_action,
                NewConnectionAction::Save,
                true,
            ));
        if self.on_action.is_some() {
            let close = self.on_action.clone();
            dialog = dialog.on_close(move |s: &mut S| {
                if let Some(cb) = &close {
                    cb(s, NewConnectionAction::Close);
                }
            });
        }
        dialog.build(&k, layer);
        root
    }
}

// Leftover L1 (removal stage): when the connections page mounts this dialog,
// bind `on_vm` to its slice and translate `NewConnectionAction::{Test,Save,Close}`
// to the real `connectionCommands` / editor-store calls, then drop the per-item
// allows. The form's own `FormAction` edits are written into the same VM slice
// by the host; no second command layer is introduced here.
