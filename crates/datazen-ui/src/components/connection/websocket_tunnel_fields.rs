//! Port of `packages/frontend/src/components/connection/WebSocketTunnelFields.tsx`.
//!
//! React renders four controlled inputs (URL, mode, timeout, auth token) plus a
//! localized hint. The mode is a two-option select whose `onChange` normalizes
//! anything but `"raw_binary"` back to `"datazen_v1"`; [`WsMode::from_value`]
//! keeps that exact rule. There is deliberately no validation-error node: React
//! renders none. The form owns `validationErrors.wsUrl` (`newConn.required`),
//! and [`is_ws_url_valid`] mirrors that form rule as a pure helper so a caller
//! can pin it without inventing a test id.
//!
//! ## Local contract (removal stage)
//!
//! `WebSocketTunnelFieldsValue` from `tunnelFieldContracts.ts` has no Rust DTO
//! yet; [`WebSocketValue`] mirrors its values with `TextEdit`s. Remove it when
//! that contract is ported and drive the component from the shared value.
//!
//! ## Keys (React `data-testid` preserved)
//!
//! `new-conn-ws-fields`, `new-conn-ws-url`, `new-conn-ws-mode`,
//! `new-conn-ws-timeout`, `new-conn-ws-token`. The hint has no React test id;
//! `new-conn-ws-hint` follows the sibling `new-conn-http-proxy-hint` key so it
//! can be observed in the runtime tree.

use std::rc::Rc;

use rune::{col, text, El, TextEdit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::select::Select;

const ROOT: &str = "new-conn-ws-fields";

/// React `WebSocketTunnelFieldsValue['wsMode']`.
#[allow(dead_code)] // contract mirror; remove when tunnelFieldContracts lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WsMode {
    DatazenV1,
    RawBinary,
}

#[allow(dead_code)] // contract mirror; remove with the enum.
impl WsMode {
    /// The component's `onChange`: only the exact raw-binary value survives.
    pub(crate) fn from_value(value: &str) -> Self {
        if value == "raw_binary" {
            Self::RawBinary
        } else {
            Self::DatazenV1
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::DatazenV1 => "datazen_v1",
            Self::RawBinary => "raw_binary",
        }
    }

    /// The kit select matches `current` against option labels.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DatazenV1 => "newConn.wsModeDatazen",
            Self::RawBinary => "newConn.wsModeRaw",
        }
    }
}

impl Default for WsMode {
    fn default() -> Self {
        Self::DatazenV1
    }
}

/// Mirror of the form's wsUrl validation (`useConnectionForm.ts`):
/// `if (!wsUrl.trim()) errors.wsUrl = t('newConn.required')`.
#[allow(dead_code)] // form rule mirror; consumed by the connection form host.
pub(crate) fn is_ws_url_valid(url: &str) -> bool {
    !url.trim().is_empty()
}

/// Local mirror of `WebSocketTunnelFieldsValue`; removable with the contract.
#[allow(dead_code)] // contract mirror; remove when tunnelFieldContracts lands.
#[derive(Clone, Debug)]
pub(crate) struct WebSocketValue {
    pub url: TextEdit,
    pub mode: WsMode,
    /// Rune's Select needs its open state from the caller; React keeps it inside.
    pub mode_open: bool,
    pub timeout: TextEdit,
    pub auth_token: TextEdit,
}

impl Default for WebSocketValue {
    fn default() -> Self {
        Self {
            url: TextEdit::new(String::new()),
            mode: WsMode::DatazenV1,
            mode_open: false,
            timeout: TextEdit::new("30".to_owned()),
            auth_token: TextEdit::new(String::new()),
        }
    }
}

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type BoolFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut bool + 'static>;
type ModeFn<S> = Rc<dyn Fn(&mut S, WsMode) + 'static>;

/// Builder for the ported field group. `S` owns the text edits and handles the
/// mode change; no IPC or network call happens here.
#[allow(dead_code)] // consumer: the tunnel settings editor (not wired); remove when wired.
pub(crate) struct WebSocketTunnelFields<'a, S: 'static> {
    value: &'a WebSocketValue,
    url: EditFn<S>,
    timeout: EditFn<S>,
    auth_token: EditFn<S>,
    mode_open: Option<BoolFn<S>>,
    on_mode: Option<ModeFn<S>>,
}

#[allow(dead_code)] // consumer: the tunnel settings editor (not wired); remove when wired.
impl<'a, S: 'static> WebSocketTunnelFields<'a, S> {
    pub(crate) fn new(
        value: &'a WebSocketValue,
        url: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        timeout: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        auth_token: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            value,
            url: Rc::new(url),
            timeout: Rc::new(timeout),
            auth_token: Rc::new(auth_token),
            mode_open: None,
            on_mode: None,
        }
    }

    pub(crate) fn mode_open(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut bool + 'static,
    ) -> Self {
        self.mode_open = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_mode(mut self, f: impl Fn(&mut S, WsMode) + 'static) -> Self {
        self.on_mode = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let value = self.value;
        let mut root = col().key(ROOT).w_full().gap(12.0);

        root = root.child(field_block(
            "url",
            "newConn.wsUrl",
            "wss://relay.example.com/v1/tunnel",
            &value.url,
            &self.url,
            false,
            true,
            dark,
        ));

        let labels = [t(WsMode::DatazenV1.label()), t(WsMode::RawBinary.label())];
        let values = [WsMode::DatazenV1.value(), WsMode::RawBinary.value()];
        let mut mode = Select::new("new-conn-ws-mode", &labels)
            .values(&values)
            .current(Some(t(value.mode.label())))
            .label(t("newConn.wsMode"));
        if let Some(accessor) = self.mode_open.clone() {
            let open = accessor.clone();
            mode = mode.open(value.mode_open).on_toggle(move |s: &mut S| {
                *open(s) = true;
            });
            let open = accessor.clone();
            mode = mode.on_close(move |s: &mut S| *open(s) = false);
            if let Some(on_mode) = self.on_mode.clone() {
                let open = accessor;
                mode = mode.on_choose(move |s: &mut S, chosen: &str, _tx: &mut rune::Tx| {
                    on_mode(s, WsMode::from_value(chosen));
                    *open(s) = false;
                });
            }
        } else {
            // Without a host accessor the select simply never opens, never panics.
            mode = mode.on_toggle(|_s: &mut S| {});
        }
        root = root.child(
            col()
                .key("new-conn-ws-mode-block")
                .w_full()
                .gap(4.0)
                .child(
                    Label::new("new-conn-ws-mode-label", t("newConn.wsMode"))
                        .build(dark),
                )
                .child(mode.build(&k, layer)),
        );

        root = root.child(field_block(
            "timeout",
            "newConn.wsTimeout",
            "30",
            &value.timeout,
            &self.timeout,
            false,
            false,
            dark,
        ));
        root = root.child(field_block(
            "token",
            "newConn.wsAuthToken",
            "",
            &value.auth_token,
            &self.auth_token,
            true,
            false,
            dark,
        ));
        root.child(
            text(t("newConn.wsHint"))
                .key("new-conn-ws-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        )
    }
}

fn field_block<S: 'static>(
    suffix: &str,
    label_key: &str,
    placeholder: &str,
    value: &TextEdit,
    edit: &EditFn<S>,
    secure: bool,
    required: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let key = format!("new-conn-ws-{suffix}");
    let mut field = TextField::new(key.clone(), value)
        .height(32.0)
        .pad_x(10.0)
        .radius(k.radius.sm)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .placeholder(placeholder);
    if secure {
        field = field.secure();
    }
    let edit = edit.clone();
    field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
    col()
        .key(format!("{key}-block"))
        .w_full()
        .gap(4.0)
        .child(Label::new(format!("{key}-label"), t(label_key)).required(required).build(dark))
        .child(field.build(dark))
}

// Leftover L1 (removal stage): when the tunnel settings editor mounts this group,
// bind `mode_open` to its state slice, translate the mode press into its form
// setter, and drop the per-item allows once `tunnelFieldContracts` lands.
