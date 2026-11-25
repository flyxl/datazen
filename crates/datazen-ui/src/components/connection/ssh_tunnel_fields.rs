//! Port of `packages/frontend/src/components/connection/SshTunnelFields.tsx`.
//!
//! The SSH tunnel editor: enable toggle, host/port/username, three auth methods
//! (password / private key / agent) with their dependent fields, and a nested
//! jump-host section with its own auth method. React keeps all values in the
//! form hook; Rune keeps them in [`SshFieldsState`] owned by the host and routes
//! edits through one `on_vm` accessor. Discrete presses (toggles, auth methods,
//! path pick) are reported as [`SshAction`] to the injected host callback — no
//! fake path-picker or IPC.
//!
//! `show_enable_toggle = false` hides the enable checkbox (the settings tunnel
//! editor owns the kind), matching React's prop. When SSH is unsupported the
//! component renders an inert hidden marker (Rune views cannot return null).
//!
//! ## Keys (React `data-testid` preserved)
//!
//! `new-conn-ssh-tunnel[-checkbox]`, `new-conn-ssh-auth-<method>`,
//! `new-conn-ssh-auth-agent-hint`, `new-conn-ssh-jump-toggle`,
//! `new-conn-ssh-jump-fields`, `new-conn-ssh-jump-host`,
//! `new-conn-ssh-jump-auth-<method>`; text fields use the `new-conn-ssh-*`
//! prefix. Auth-button glyphs are omitted (no registry key icon exists yet).

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_X};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::path_input::PathInput;
use crate::ui::toggle::Checkbox;

pub use super::tunnel_field_contracts::{
    SshAuthMethod, SshTunnelFieldsValue as SshFieldsState,
};

/// Discrete presses the editor asks the host to apply.
#[allow(dead_code)] // VM surface; consumer: the connection form host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshAction {
    SetEnabled(bool),
    SetAuth(SshAuthMethod),
    SetJumpEnabled(bool),
    SetJumpAuth(SshAuthMethod),
    /// `true` = jump host key path, `false` = primary key path.
    PickPath(bool),
}

type VmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut SshFieldsState + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, SshAction) + 'static>;

fn edit<S: 'static>(
    vm: &Option<VmFn<S>>,
    pick: for<'a> fn(&'a mut SshFieldsState) -> &'a mut TextEdit,
) -> Option<EditFn<S>> {
    vm.as_ref().map(|vm| {
        let vm = Rc::clone(vm);
        Rc::new(rune::edit_access(move |s: &mut S| pick(vm(s)))) as EditFn<S>
    })
}

fn press<S: 'static>(f: &Option<ActionFn<S>>, a: SshAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

fn field<S: 'static>(
    key: &str,
    edit: &TextEdit,
    accessor: Option<EditFn<S>>,
    placeholder: Option<&str>,
    secure: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut input = TextField::new(key, edit)
        .height(28.0)
        .pad_x(8.0)
        .text_style(k.ty.text_xsmall)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base);
    if let Some(ph) = placeholder {
        input = input.placeholder(ph);
    }
    if secure {
        input = input.secure();
    }
    match accessor {
        Some(accessor) => {
            input = input.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
        }
        None => input = input.disabled(true),
    }
    input.build(dark)
}

fn auth_row<S: 'static>(
    key_prefix: &str,
    current: SshAuthMethod,
    on_action: &Option<ActionFn<S>>,
    jump: bool,
    k: &Kit,
) -> El {
    let mut row_el = row()
        .key(format!("{key_prefix}-auth-row"))
        .w_full()
        .gap(6.0)
        .flex_wrap();
    for method in [SshAuthMethod::Password, SshAuthMethod::PrivateKey, SshAuthMethod::Agent] {
        let selected = current == method;
        let mut button = Button::new(t(method.label()))
            .variant(if selected {
                ButtonVariant::Neutral
            } else {
                ButtonVariant::Ghost
            })
            .control_size(ControlSize::Small);
        if on_action.is_some() {
            let action = if jump {
                SshAction::SetJumpAuth(method)
            } else {
                SshAction::SetAuth(method)
            };
            button = button.on_press(press(on_action, action));
        }
        row_el = row_el.child(button.build(k).key(format!("{key_prefix}-auth-{}", method.value())));
    }
    row_el
}

/// Builder for the ported field group.
#[allow(dead_code)] // consumer: ConnectionFormBody / tunnel settings (not wired).
pub struct SshTunnelFields<'a, S: 'static> {
    state: &'a SshFieldsState,
    supports_ssh: bool,
    show_enable_toggle: bool,
    on_vm: Option<VmFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: ConnectionFormBody / tunnel settings (not wired).
impl<'a, S: 'static> SshTunnelFields<'a, S> {
    pub fn new(state: &'a SshFieldsState, supports_ssh: bool) -> Self {
        Self {
            state,
            supports_ssh,
            show_enable_toggle: true,
            on_vm: None,
            on_action: None,
        }
    }

    pub fn show_enable_toggle(mut self, show: bool) -> Self {
        self.show_enable_toggle = show;
        self
    }

    pub fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut SshFieldsState + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub fn on_action(mut self, f: impl Fn(&mut S, SshAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        if !self.supports_ssh {
            return el().key("new-conn-ssh-fields-hidden").a11y_hidden();
        }
        let state = self.state;
        let visible = if self.show_enable_toggle {
            state.enabled
        } else {
            true
        };

        let mut root = col().key("new-conn-ssh-fields").w_full().min_w(0.0).gap(8.0);
        if self.show_enable_toggle {
            let toggle = Checkbox::new("new-conn-ssh-tunnel-checkbox", state.enabled)
                .label(t("newConn.sshTunnel"))
                .on_toggle(press(&self.on_action, SshAction::SetEnabled(!state.enabled)))
                .build(k);
            root = root.child(row().key("new-conn-ssh-tunnel").w_full().child(toggle));
        }
        if !visible {
            return root;
        }

        root = root.child(
            col()
                .key("new-conn-ssh-fields-panel")
                .w_full()
                .gap(8.0)
                .padding(12.0)
                .radius(8.0)
                .border(1.0, k.c.border_base)
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .flex_wrap()
                        .child(labelled("new-conn-ssh-host", t("newConn.sshHost"), true, field(
                            "new-conn-ssh-host",
                            &state.host,
                            edit(&self.on_vm, |vm| &mut vm.host),
                            Some("ssh.example.com"),
                            false,
                            dark,
                        ), dark))
                        .child(labelled("new-conn-ssh-port", t("newConn.sshPort"), true, field(
                            "new-conn-ssh-port",
                            &state.port,
                            edit(&self.on_vm, |vm| &mut vm.port),
                            Some("22"),
                            false,
                            dark,
                        ), dark)),
                )
                .child(labelled("new-conn-ssh-username", t("newConn.sshUsername"), true, field(
                    "new-conn-ssh-username",
                    &state.username,
                    edit(&self.on_vm, |vm| &mut vm.username),
                    Some("root"),
                    false,
                    dark,
                ), dark))
                .child(
                    col()
                        .key("new-conn-ssh-auth-method")
                        .w_full()
                        .gap(4.0)
                        .child(Label::new("new-conn-ssh-auth-method-label", t("newConn.authMethod")).required(true).build(dark))
                        .child(auth_row("new-conn-ssh", state.auth_method, &self.on_action, false, k)),
                ),
        );

        match state.auth_method {
            SshAuthMethod::Agent => {
                root = root.child(
                    text(t("newConn.authAgentHint"))
                        .key("new-conn-ssh-auth-agent-hint")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
            }
            SshAuthMethod::Password => {
                root = root.child(labelled("new-conn-ssh-password", t("newConn.sshPassword"), true, field(
                    "new-conn-ssh-password",
                    &state.password,
                    edit(&self.on_vm, |vm| &mut vm.password),
                    None,
                    true,
                    dark,
                ), dark));
            }
            SshAuthMethod::PrivateKey => {
                let mut key_path = PathInput::new("new-conn-ssh-key-path", &state.key_path)
                    .placeholder("~/.ssh/id_rsa")
                    .on_browse(|_s: &mut S| {});
                if let Some(on_action) = &self.on_action {
                    let on_action = on_action.clone();
                    key_path = key_path.on_browse(move |s: &mut S| {
                        on_action(s, SshAction::PickPath(false));
                    });
                }
                if let Some(accessor) = edit(&self.on_vm, |vm| &mut vm.key_path) {
                    key_path = key_path.on_edit(move |s: &mut S| accessor(s));
                }
                root = root
                    .child(labelled("new-conn-ssh-private-key", t("newConn.privateKey"), true, key_path.build(dark), dark))
                    .child(labelled("new-conn-ssh-passphrase", t("newConn.passphrase"), false, field(
                        "new-conn-ssh-passphrase",
                        &state.passphrase,
                        edit(&self.on_vm, |vm| &mut vm.passphrase),
                        Some(t("newConn.passphraseHint")),
                        true,
                        dark,
                    ), dark));
            }
        }

        let mut jump_toggle = row()
            .key("new-conn-ssh-jump-toggle")
            .role("checkbox")
            .h(24.0)
            .w(220.0)
            .gap(6.0)
            .center_y()
            .focusable(true)
            .checked(state.jump_enabled)
            .label(t("newConn.sshJump"))
            .child(if state.jump_enabled {
                icon(&ICON_CHECK, 12.0).color(k.c.text_accent)
            } else {
                icon(&ICON_X, 12.0).color(k.c.text_muted)
            })
            .child(text(t("newConn.sshJump")).font(k.ty.text_small).color(k.c.text_base));
        if self.on_action.is_some() {
            jump_toggle = jump_toggle.on_press(press(
                &self.on_action,
                SshAction::SetJumpEnabled(!state.jump_enabled),
            ));
        }
        root = root.child(jump_toggle);
        if state.jump_enabled {
            root = root.child(jump_fields(&self, k, dark));
        }
        root
    }
}

fn labelled(key: &str, label: &str, required: bool, field: El, dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(format!("{key}-block"))
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .child(Label::new(format!("{key}-label"), label).required(required).build(dark))
        .child(field)
}

fn jump_fields<S: 'static>(parent: &SshTunnelFields<'_, S>, k: &Kit, dark: bool) -> El {
    let state = parent.state;
    let mut jump = col()
        .key("new-conn-ssh-jump-fields")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .flex_wrap()
                .child(labelled("new-conn-ssh-jump-host", t("newConn.sshJumpHost"), true, field(
                    "new-conn-ssh-jump-host",
                    &state.jump_host,
                    edit(&parent.on_vm, |vm| &mut vm.jump_host),
                    Some("bastion.example.com"),
                    false,
                    dark,
                ), dark))
                .child(labelled("new-conn-ssh-jump-port", t("newConn.sshJumpPort"), true, field(
                    "new-conn-ssh-jump-port",
                    &state.jump_port,
                    edit(&parent.on_vm, |vm| &mut vm.jump_port),
                    Some("22"),
                    false,
                    dark,
                ), dark)),
        )
        .child(labelled("new-conn-ssh-jump-username", t("newConn.sshJumpUsername"), true, field(
            "new-conn-ssh-jump-username",
            &state.jump_username,
            edit(&parent.on_vm, |vm| &mut vm.jump_username),
            Some("ubuntu"),
            false,
            dark,
        ), dark))
        .child(auth_row("new-conn-ssh-jump", state.jump_auth_method, &parent.on_action, true, k));

    match state.jump_auth_method {
        SshAuthMethod::Password => {
            jump = jump.child(labelled("new-conn-ssh-jump-password", t("newConn.sshPassword"), true, field(
                "new-conn-ssh-jump-password",
                &state.jump_password,
                edit(&parent.on_vm, |vm| &mut vm.jump_password),
                None,
                true,
                dark,
            ), dark));
        }
        SshAuthMethod::PrivateKey => {
            let mut key_path = PathInput::new("new-conn-ssh-jump-key-path", &state.jump_key_path)
                .placeholder("~/.ssh/id_rsa")
                .on_browse(|_s: &mut S| {});
            if let Some(on_action) = &parent.on_action {
                let on_action = on_action.clone();
                key_path = key_path.on_browse(move |s: &mut S| {
                    on_action(s, SshAction::PickPath(true));
                });
            }
            if let Some(accessor) = edit(&parent.on_vm, |vm| &mut vm.jump_key_path) {
                key_path = key_path.on_edit(move |s: &mut S| accessor(s));
            }
            jump = jump.child(labelled("new-conn-ssh-jump-private-key", t("newConn.privateKey"), true, key_path.build(dark), dark))
                .child(labelled("new-conn-ssh-jump-passphrase", t("newConn.passphrase"), false, field(
                    "new-conn-ssh-jump-passphrase",
                    &state.jump_passphrase,
                    edit(&parent.on_vm, |vm| &mut vm.jump_passphrase),
                    None,
                    true,
                    dark,
                ), dark));
        }
        SshAuthMethod::Agent => {}
    }
    jump
}

// Leftover L1 (removal stage): when the tunnel settings editor / connection
// form host mounts this group, bind `on_vm` to its state slice and translate
// `SshAction::PickPath` to the host path picker; drop the per-item allows then.
