//! Create-user dialog, ported from
//! `packages/frontend/src/windows/connection/CreateUserDialog.tsx`.
//!
//! React is a two-step flow: create the user (username + optional password),
//! then optionally grant privileges on a database. Both steps call the driver
//! through injected commands; this port emits [`CreateUserAction`]s and the
//! host executes them — no fake IPC, no store dependency.
//!
//! ## Local stand-ins (removal stage)
//!
//! `PrivilegeSelector` / `usePrivilegeOptions` read the driver command
//! definition; this view layer has no such port, so [`CreateUserState`] carries
//! a host-supplied privilege list (empty with no definition, exactly React's
//! fallback). Remove when `commandSchema` lands and derive the list there.
//! `show_grant_option` / `show_grant_step` use the same host-supplied facts.
//!
//! ## Keys
//!
//! Preserves T223's slot keys: `content-view-create-user-dialog` with `-body`,
//! `-context`, `-cancel`, `-confirm`. Added: `-username`, `-password`,
//! `-success`, `-database`, `-privileges`, `-privilege-<n>`, `-select-all`,
//! `-grant-option`, `-skip`, `-error`.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

const ROOT: &str = "content-view-create-user-dialog";

/// React's `step` state.
#[allow(dead_code)] // VM surface; consumed by the host result feed (not landed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CreateUserStep {
    #[default]
    Create,
    Grant,
}

/// Host calls the two steps emit. The host owns `dbSessionId` and the command
/// names; the actions carry only the form values.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreateUserAction {
    Close,
    Create { username: String, password: String },
    Grant { username: String, database: String, privileges: Vec<String>, grant_option: bool },
    Created { username: String },
    Skip { username: String },
}

/// All `useState` values plus the host-supplied fact snapshot.
#[allow(dead_code)] // VM surface; owned by the host until the driver definition lands.
#[derive(Clone, Debug, Default)]
pub(crate) struct CreateUserState {
    pub username: TextEdit,
    pub password: TextEdit,
    pub selected_privileges: Vec<String>,
    pub grant_option: bool,
    pub target_database: String,
    /// Rune's Select needs its open state from the caller; React keeps it inside.
    pub database_select_open: bool,
    pub databases: Vec<String>,
    pub privileges: Vec<String>,
    pub show_grant_option: bool,
    pub show_grant_step: bool,
    pub running: bool,
    pub error: String,
    pub step: CreateUserStep,
    pub created_username: String,
}

impl CreateUserState {
    /// React `resetForm`.
    #[allow(dead_code)] // consumer: the dialog host result feed (not landed); remove when wired.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            privileges: self.privileges.clone(),
            databases: self.databases.clone(),
            show_grant_option: self.show_grant_option,
            show_grant_step: self.show_grant_step,
            ..Self::default()
        };
    }

    /// React `togglePrivilege`: insertion-ordered set semantics.
    pub(crate) fn toggle_privilege(&mut self, privilege: &str) {
        if let Some(index) = self.selected_privileges.iter().position(|p| p == privilege) {
            self.selected_privileges.remove(index);
        } else {
            self.selected_privileges.push(privilege.to_owned());
        }
    }

    /// React `selectAllPrivileges`: selecting everything toggles to empty.
    pub(crate) fn select_all_privileges(&mut self) {
        if self.selected_privileges.len() == self.privileges.len() {
            self.selected_privileges.clear();
        } else {
            self.selected_privileges = self.privileges.clone();
        }
    }

    /// React `handleCreate` guard + command payload.
    pub(crate) fn begin_create(&mut self) -> Option<CreateUserAction> {
        let username = self.username.text.trim().to_owned();
        if username.is_empty() {
            return None;
        }
        self.running = true;
        self.error.clear();
        Some(CreateUserAction::Create { username, password: self.password.text.clone() })
    }

    /// Resolve the create answer: success advances to the grant step.
    #[allow(dead_code)] // consumer: the host create-user result feed (not landed); remove when wired.
    pub(crate) fn apply_create(
        &mut self,
        result: Result<(), String>,
        username: &str,
    ) -> Vec<CreateUserAction> {
        self.running = false;
        match result {
            Ok(()) => {
                self.created_username = username.trim().to_owned();
                self.step = CreateUserStep::Grant;
                Vec::new()
            }
            Err(message) => {
                self.error = message;
                Vec::new()
            }
        }
    }

    /// React `handleGrant` guard + command payload.
    pub(crate) fn begin_grant(&mut self) -> Option<CreateUserAction> {
        if self.selected_privileges.is_empty() {
            return None;
        }
        self.running = true;
        self.error.clear();
        Some(CreateUserAction::Grant {
            username: self.created_username.clone(),
            database: self.target_database.clone(),
            privileges: self.selected_privileges.clone(),
            grant_option: self.grant_option,
        })
    }

    /// Resolve the grant answer: success resets and reports creation.
    #[allow(dead_code)] // consumer: the host create-user result feed (not landed); remove when wired.
    pub(crate) fn apply_grant(&mut self, result: Result<(), String>) -> Vec<CreateUserAction> {
        self.running = false;
        match result {
            Ok(()) => {
                let username = self.created_username.clone();
                self.reset();
                vec![CreateUserAction::Created { username: username.clone() }, CreateUserAction::Close]
            }
            Err(message) => {
                self.error = message;
                Vec::new()
            }
        }
    }

    /// React `handleSkipGrant`.
    pub(crate) fn skip_grant(&mut self) -> Vec<CreateUserAction> {
        let username = self.created_username.clone();
        self.reset();
        vec![CreateUserAction::Skip { username }, CreateUserAction::Close]
    }

    /// React `t('createUser.success').replace('{name}', createdUsername)`.
    pub(crate) fn success_text(&self) -> String {
        t("createUser.success").replace("{name}", &self.created_username)
    }
}

type ActionFn<S> = Rc<dyn Fn(&mut S, CreateUserAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: CreateUserAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the dialog into `layer`. The open gate is applied by the slot host.
#[allow(dead_code)] // consumer: content_view_dialogs slot (wired by T286).
pub(crate) fn build<S: 'static>(
    state: &CreateUserState,
    context: Option<&str>,
    state_edit: Option<Rc<dyn Fn(&mut S) -> &mut CreateUserState>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
    submit_override: Option<Rc<dyn Fn(&mut S)>>,
) {
    let k = kit(dark);
    let is_create = state.step == CreateUserStep::Create;
    let title = if is_create { t("common.createUser") } else { t("common.grantPrivileges") };

    let submit: Rc<dyn Fn(&mut S)> = match submit_override {
        Some(submit) => submit,
        None => {
            let state_edit = state_edit.clone();
            let on_action = on_action.clone();
            Rc::new(move |s: &mut S| {
                if let Some(state_edit) = &state_edit {
                    let action = {
                        let form = state_edit(s);
                        if form.step == CreateUserStep::Create {
                            form.begin_create()
                        } else {
                            form.begin_grant()
                        }
                    };
                    if let Some(action) = action {
                        if let Some(callback) = &on_action {
                            callback(s, action);
                        }
                    }
                }
            })
        }
    };

    let mut root = col().key(ROOT).w_full().min_w(0.0).gap(12.0);
    if is_create {
        root = build_create_body(root, state, state_edit.clone(), &submit, dark);
    } else {
        root = build_grant_body(root, state, state_edit.clone(), &submit, dark, layer);
    }
    if !state.error.is_empty() {
        root = root.child(CopyableError::new(format!("{ROOT}-error"), state.error.clone()).build::<S>(k));
    }

    let mut dialog = Dialog::new(ROOT, title)
        .open(true)
        .fit()
        .width(480.0)
        .body_max_height(320.0)
        .child(row().key(format!("{ROOT}-body")).w_full().min_w(0.0).child(root));
    if let Some(context) = context {
        dialog = dialog.child(text(context).key(format!("{ROOT}-context")).wrap());
    }

    let close = emit(&on_action, CreateUserAction::Close);
    if is_create {
        dialog = dialog.footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .on_press(close.clone())
                .build(k)
                .key(format!("{ROOT}-cancel")),
        );
        dialog = dialog.footer(
            Button::new(t("common.createUser"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .disabled(state.username.text.trim().is_empty() || state.running)
                .on_press(move |s: &mut S| submit(s))
                .build(k)
                .key(format!("{ROOT}-confirm")),
        );
    } else {
        let skip = {
            let state_edit = state_edit.clone();
            let on_action = on_action.clone();
            move |s: &mut S| {
                if let Some(state_edit) = &state_edit {
                    for action in state_edit(s).skip_grant() {
                        if let Some(callback) = &on_action {
                            callback(s, action);
                        }
                    }
                }
            }
        };
        dialog = dialog.footer(
            Button::new(t("common.skip"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .on_press(skip)
                .build(k)
                .key(format!("{ROOT}-skip")),
        );
        dialog = dialog.footer(
            Button::new(t("createUser.createAndGrant"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .disabled(state.selected_privileges.is_empty() || state.running)
                .on_press(move |s: &mut S| submit(s))
                .build(k)
                .key(format!("{ROOT}-confirm")),
        );
    }
    dialog.on_close::<S>(close).build(k, layer);
}

fn build_create_body<S: 'static>(
    mut root: El,
    state: &CreateUserState,
    state_edit: Option<Rc<dyn Fn(&mut S) -> &mut CreateUserState>>,
    submit: &Rc<dyn Fn(&mut S)>,
    dark: bool,
) -> El {
    let mut username = TextField::new(format!("{ROOT}-username"), &state.username)
        .height(36.0)
        .placeholder("new_user")
        .label(t("createUser.username"));
    if let Some(state_edit) = state_edit.clone() {
        let user_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> = {
            let state_edit = state_edit.clone();
            Rc::new(move |s: &mut S| &mut state_edit(s).username)
        };
        username = username.on_edit(rune::edit_access(move |s: &mut S| user_edit(s)));
        let enter = submit.clone();
        username = username.on_submit(move |s: &mut S, _tx: &mut Tx| enter(s));
    } else {
        username = username.disabled(true);
    }
    root = root.child(
        Label::new(format!("{ROOT}-username-label"), t("createUser.username")).build(dark),
    ).child(username.build(dark));

    let mut password = TextField::new(format!("{ROOT}-password"), &state.password)
        .height(36.0)
        .placeholder("••••••••")
        .label(t("createUser.password"))
        .secure();
    if let Some(state_edit) = state_edit {
        let pass_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> = {
            let state_edit = state_edit.clone();
            Rc::new(move |s: &mut S| &mut state_edit(s).password)
        };
        password = password.on_edit(rune::edit_access(move |s: &mut S| pass_edit(s)));
        let enter = submit.clone();
        password = password.on_submit(move |s: &mut S, _tx: &mut Tx| enter(s));
    } else {
        password = password.disabled(true);
    }
    root.child(
        Label::new(format!("{ROOT}-password-label"), t("createUser.password")).build(dark),
    ).child(password.build(dark))
}

fn build_grant_body<S: 'static>(
    mut root: El,
    state: &CreateUserState,
    state_edit: Option<Rc<dyn Fn(&mut S) -> &mut CreateUserState>>,
    _submit: &Rc<dyn Fn(&mut S)>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    root = root.child(
        text(state.success_text())
            .key(format!("{ROOT}-success"))
            .wrap(),
    );
    if !state.show_grant_step {
        return root;
    }

    let labels: Vec<String> = std::iter::once(t("createUser.allDatabases").to_owned())
        .chain(state.databases.iter().cloned())
        .collect();
    let values: Vec<String> = std::iter::once(String::new())
        .chain(state.databases.iter().cloned())
        .collect();
    // The kit matches `current` against option labels; an empty target means
    // React's "All databases" default, not an empty string.
    let current = if state.target_database.is_empty() {
        t("createUser.allDatabases")
    } else {
        state.target_database.as_str()
    };
    let mut db_sel = Select::new(format!("{ROOT}-database"), &labels)
        .values(&values)
        .current(Some(current))
        .label(t("createUser.database"))
        .open(state.database_select_open)
        // The base requires a toggle even when no host state is wired; without
        // an accessor the list simply never opens rather than panicking.
        .on_toggle(move |_s: &mut S| {});
    if let Some(on_db) = state_edit.clone() {
        let open = on_db.clone();
        db_sel = db_sel.on_toggle(move |s: &mut S| open(s).database_select_open = true);
        let close = on_db.clone();
        db_sel = db_sel.on_close(move |s: &mut S| close(s).database_select_open = false);
        db_sel = db_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            let form = on_db(s);
            form.target_database = value.to_owned();
            form.database_select_open = false;
        });
    }
    root = root.child(Label::new(format!("{ROOT}-database-label"), t("createUser.database")).build(dark))
        .child(db_sel.build(&kit(dark), layer));

    if !state.privileges.is_empty() {
        let mut privileges_row = row()
            .key(format!("{ROOT}-privileges"))
            .w_full()
            .child(text(t("createUser.privileges")).wrap());
        if let Some(state_edit) = state_edit.clone() {
            let select_all = {
                let state_edit = state_edit.clone();
                move |s: &mut S| state_edit(s).select_all_privileges()
            };
            privileges_row = privileges_row.child(
                text(t("createUser.selectAll"))
                    .key(format!("{ROOT}-select-all"))
                    .role("button")
                    .focusable(true)
                    .on_press(move |s: &mut S| select_all(s)),
            );
        }
        root = root.child(privileges_row);
        for (index, privilege) in state.privileges.iter().enumerate() {
            let checked = state.selected_privileges.contains(privilege);
            let mut checkbox = Checkbox::new(format!("{ROOT}-privilege-{index}"), checked)
                .label(privilege.clone());
            if let Some(state_edit) = state_edit.clone() {
                let toggle = {
                    let state_edit = state_edit.clone();
                    let value = privilege.clone();
                    move |s: &mut S| state_edit(s).toggle_privilege(&value)
                };
                checkbox = checkbox.on_toggle(move |s: &mut S| toggle(s));
            }
            root = root.child(checkbox.build(&kit(dark)));
        }
    }
    if state.show_grant_option {
        let mut checkbox = Checkbox::new(format!("{ROOT}-grant-option"), state.grant_option)
            .label(t("createUser.grantOption"));
        if let Some(grant_edit) = state_edit.clone() {
            checkbox = checkbox.on_toggle(move |s: &mut S| {
                let next = !grant_edit(s).grant_option;
                grant_edit(s).grant_option = next;
            });
        }
        root = root.child(checkbox.build(&kit(dark)));
    }
    root
}

/// Snapshot entry point for the T223 slot: the host owns no form state there.
#[allow(dead_code)] // consumer: content_view_dialogs slot; remove when the host wires form state.
pub(crate) fn build_snapshot<S: 'static>(
    state: &CreateUserState,
    context: Option<&str>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let submit = {
        let on_action = on_action.clone();
        let action = if state.step == CreateUserStep::Create {
            Some(CreateUserAction::Create {
                username: state.username.text.trim().to_owned(),
                password: state.password.text.clone(),
            })
        } else {
            None
        };
        move |s: &mut S| {
            if let Some(action) = &action {
                if let Some(callback) = &on_action {
                    callback(s, action.clone());
                }
            }
        }
    };
    build::<S>(
        state,
        context,
        None,
        on_action.clone(),
        dark,
        layer,
        Some(Rc::new(submit)),
    );
}
