//! Thin host for the connection share dialog, ported from
//! `packages/frontend/src/components/connection/ConnectionShareDialogHost.tsx`.
//!
//! React mounts one global host in `MainPage` and drives it from
//! `useConnectionShareStore` (`open` / `mode` / `importSource`). This port keeps
//! those three values in an explicit host-owned [`ConnectionShareDialogHost`]
//! value, renders the T137 [`ConnectionShareDialog`], and translates its
//! outputs into host actions: close the dialog, show a success/error
//! `ResultMessageDialog`, and refresh the connection list after an import.
//!
//! # Immediate-mode adaptation
//!
//! There is no zustand store and no promise here. The host owns the state and
//! the caller stages IPC through the T137 action/event seam — this module does
//! no fake IPC. [`ShareHostPort`] is the injected host port for the one real
//! host action (re-fetch connections and groups after a successful import);
//! the host that mounts this component implements it.
//!
//! ## Local stand-ins (removal stages)
//!
//! - `ResultMessageDialog` has no Rune port yet; [`MessageDialogState`] and the
//!   two footer buttons here mirror its React keys. Remove them when the
//!   `packages/ui` port lands and compose it instead.
//! - `lib/connectionShareError.ts::formatConnectionImportSuccess` is mirrored
//!   by [`format_import_success`]; remove when that `lib` port lands.
//! - [`ShareHostPort`] is only needed until `MainPage` wires a real host port.
//!
//! ## Keys
//!
//! Share dialog keys come from T137 (`connection-share-dialog`, `-cancel`,
//! `-primary`). The message dialog is keyed `result-message-dialog` with
//! `result-message-text`, `result-message-copy` and `result-message-ok`,
//! matching the React `data-testid`s.

use std::rc::Rc;
use std::time::Duration;

use rune::{text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::components::connection::connection_share_dialog::{
    ConnectionShareDialog, ImportResult, ImportSource, ShareAction, ShareEvent, ShareMode,
    ShareOutput, ShareState,
};
use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_COPY};
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

const COPIED_FEEDBACK_MS: u64 = 1500;
const MESSAGE_ROOT: &str = "result-message-dialog";

/// `ResultMessageDialog`'s `kind`.
#[allow(dead_code)] // local stand-in; remove when the ResultMessageDialog port lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Error,
    Success,
}

impl Default for MessageKind {
    fn default() -> Self {
        Self::Error
    }
}

/// The three React `useState` values of `ResultMessageDialog`.
#[allow(dead_code)] // local stand-in; remove when the ResultMessageDialog port lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageDialogState {
    pub open: bool,
    pub kind: MessageKind,
    pub text: String,
}

/// React's `fetchConnections()` + `fetchGroups()` after a successful import.
/// The host that mounts this component injects the implementation.
#[allow(dead_code)] // host port; remove when MainPage wires the real refresh.
pub trait ShareHostPort {
    fn refresh_connections(&mut self);
}

/// Substitute React's `{name}` placeholders in a translated key.
fn fill(key: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = t(key).to_owned();
    for (name, value) in pairs {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Mirror of `formatConnectionImportSuccess`.
#[allow(dead_code)] // local stand-in; remove when lib/connectionShareError.ts lands.
pub fn format_import_success(result: &ImportResult) -> String {
    let imported = result.imported.to_string();
    let overwritten = result.overwritten.to_string();
    let groups_added = result.groups_added.to_string();
    if !result.skipped.is_empty() {
        let skipped = result.skipped.len().to_string();
        fill(
            "connShare.importSuccessWithSkipped",
            &[
                ("imported", &imported),
                ("overwritten", &overwritten),
                ("groupsAdded", &groups_added),
                ("skipped", &skipped),
            ],
        )
    } else {
        fill(
            "connShare.importSuccess",
            &[
                ("imported", &imported),
                ("overwritten", &overwritten),
                ("groupsAdded", &groups_added),
            ],
        )
    }
}

/// Mirror of `t('connShare.exportSuccess', { count })`.
#[allow(dead_code)] // local stand-in; remove when lib/connectionShareError.ts lands.
pub fn format_export_success(count: u64) -> String {
    let count = count.to_string();
    fill("connShare.exportSuccess", &[("count", &count)])
}

/// The state this host owns: the share store's three values (carried by T137's
/// [`ShareState`]) plus the result-message dialog's three values.
#[allow(dead_code)] // consumer: MainPage host wiring (not landed); remove when it lands.
#[derive(Clone, Debug)]
pub struct ConnectionShareDialogHost {
    pub share: ShareState,
    pub message: MessageDialogState,
}

impl Default for ConnectionShareDialogHost {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // consumer: MainPage host wiring (not landed); remove when it lands.
impl ConnectionShareDialogHost {
    pub fn new() -> Self {
        Self {
            share: ShareState::new(ShareMode::Export, ImportSource::File),
            message: MessageDialogState::default(),
        }
    }

    /// React `openConnectionShareDialog(mode, importSource)`: a fresh mount, so
    /// the dialog state starts clean with the requested mode and source.
    pub fn open(&mut self, mode: ShareMode, source: ImportSource) {
        self.share = ShareState::new(mode, source);
        self.share.open = true;
    }

    /// React `closeConnectionShareDialog`: only `open` flips; the dialog state
    /// itself stays until the next fresh open.
    pub fn close(&mut self) {
        self.share.open = false;
    }

    /// `showMessageDialog(text, kind)` from React.
    pub fn show_message(&mut self, text: impl Into<String>, kind: MessageKind) {
        self.message.text = text.into();
        self.message.kind = kind;
        self.message.open = true;
    }

    pub fn close_message(&mut self) {
        self.message.open = false;
    }

    /// The one callback translation the host owns directly.
    pub fn handle_action(&mut self, action: ShareAction) {
        if action == ShareAction::Close {
            self.close();
        }
        // Other actions are staged as IPC by the app host; no fake IPC here.
    }

    /// Translate T137's callback outputs exactly as React's three handlers do:
    /// close first, then a success/error message; import success also refreshes.
    pub fn handle_outputs(&mut self, outputs: Vec<ShareOutput>, port: &mut dyn ShareHostPort) {
        for output in outputs {
            match output {
                ShareOutput::Close => self.close(),
                ShareOutput::ExportSuccess(count) => {
                    self.show_message(format_export_success(count), MessageKind::Success);
                }
                ShareOutput::ImportSuccess(result) => {
                    self.show_message(format_import_success(&result), MessageKind::Success);
                    port.refresh_connections();
                }
                ShareOutput::Error(message) => {
                    // T137 emits only the error output here; React's host onError
                    // closes the share dialog itself before showing the message.
                    self.close();
                    self.show_message(message, MessageKind::Error);
                }
            }
        }
    }

    /// Deliver a host answer through T137's epoch guard, then translate its
    /// outputs. A stale epoch produces no output and no host action.
    pub fn apply_event(
        &mut self,
        event: ShareEvent,
        fallback: &str,
        port: &mut dyn ShareHostPort,
    ) {
        let outputs = self.share.apply(event, fallback);
        self.handle_outputs(outputs, port);
    }
}

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut rune::TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, ShareAction) + 'static>;
type VoidFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type CopyFn<S> = Rc<dyn Fn(&mut S, &mut Tx) + 'static>;

/// View builder: the host value plus the state accessors and callbacks the
/// mounted dialogs need (Rune has no component-local state).
#[allow(dead_code)] // consumer: MainPage host wiring (not landed); remove when it lands.
pub struct HostView<'a, S: 'static> {
    host: &'a ConnectionShareDialogHost,
    password: EditFn<S>,
    confirm: EditFn<S>,
    import_password: EditFn<S>,
    import_path: EditFn<S>,
    copied: bool,
    on_action: Option<ActionFn<S>>,
    on_message_close: Option<VoidFn<S>>,
    on_copy: Option<CopyFn<S>>,
}

#[allow(dead_code)] // consumer: MainPage host wiring (not landed); remove when it lands.
impl<'a, S: 'static> HostView<'a, S> {
    pub fn new(
        host: &'a ConnectionShareDialogHost,
        password: impl for<'b> Fn(&'b mut S) -> &'b mut rune::TextEdit + 'static,
        confirm: impl for<'b> Fn(&'b mut S) -> &'b mut rune::TextEdit + 'static,
        import_password: impl for<'b> Fn(&'b mut S) -> &'b mut rune::TextEdit + 'static,
        import_path: impl for<'b> Fn(&'b mut S) -> &'b mut rune::TextEdit + 'static,
    ) -> Self {
        Self {
            host,
            password: Rc::new(password),
            confirm: Rc::new(confirm),
            import_password: Rc::new(import_password),
            import_path: Rc::new(import_path),
            copied: false,
            on_action: None,
            on_message_close: None,
            on_copy: None,
        }
    }

    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    pub fn on_action(mut self, f: impl Fn(&mut S, ShareAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub fn on_message_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_message_close = Some(Rc::new(f));
        self
    }

    pub fn on_copy(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k = kit(dark);
        let host = self.host;
        let mut share_dialog = ConnectionShareDialog::<S>::new(
            &host.share,
            move |s: &mut S| (&*self.password)(s),
            move |s: &mut S| (&*self.confirm)(s),
            move |s: &mut S| (&*self.import_password)(s),
            move |s: &mut S| (&*self.import_path)(s),
        );
        if let Some(on_action) = self.on_action.clone() {
            share_dialog = share_dialog.on_action(move |s: &mut S, action| on_action(s, action));
        }
        share_dialog.build(dark, layer);

        if !host.message.open {
            return;
        }
        let title = if host.message.kind == MessageKind::Error {
            t("common.error")
        } else {
            t("common.success")
        };
        let mut message_dialog = Dialog::new(MESSAGE_ROOT, title)
            .open(true)
            .width(384.0)
            .fit()
            .body_max_height(280.0);
        message_dialog = message_dialog.child(
            text(host.message.text.clone())
                .key("result-message-text")
                .font(k.ty.small)
                .color(k.c.text_base)
                .selectable(true)
                .wrap_anywhere(),
        );

        // React shows the copy control only for the error kind.
        if host.message.kind == MessageKind::Error {
            let label = if self.copied { t("common.copied") } else { t("common.copy") };
            let glyph = if self.copied { &ICON_CHECK } else { &ICON_COPY };
            let message = host.message.text.clone();
            let on_copy = self.on_copy.clone();
            let copy = Button::new(label)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(glyph)
                .build(k)
                .key("result-message-copy")
                .on_press_tx(move |s: &mut S, tx: &mut Tx| {
                    tx.copy(message.clone());
                    tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
                    if let Some(handler) = &on_copy {
                        handler(s, tx);
                    }
                });
            message_dialog = message_dialog.footer(copy);
        }

        let mut ok = Button::new(t("common.ok"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular);
        if let Some(on_close) = self.on_message_close.clone() {
            ok = ok.on_press(move |s: &mut S| on_close(s));
        }
        message_dialog = message_dialog.footer(ok.build(k).key("result-message-ok"));
        if let Some(on_close) = self.on_message_close.clone() {
            message_dialog = message_dialog.on_close(move |s: &mut S| on_close(s));
        }
        message_dialog.build(k, layer);
    }
}
