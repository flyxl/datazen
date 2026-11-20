//! HTTP proxy tunnel fields, ported 1:1 from
//! `packages/frontend/src/components/connection/HttpProxyTunnelFields.tsx`.
//!
//! React renders six controlled inputs (host, port, scheme, timeout, username,
//! password) plus a localized hint. There is no local validation in that source:
//! the form owns validation errors, so no error key is invented here.
//!
//! ## Contract
//!
//! [`HttpProxyValue`] is `HttpProxyTunnelFieldsValue` re-exported from
//! `tunnel_field_contracts`, the single definition site. Existing callers keep
//! the old local name; remove the re-export when they import the contract
//! directly.
//!
//! ## Keys
//!
//! Root `new-conn-http-proxy-fields`; fields `new-conn-http-proxy-{host,port,
//! scheme,timeout,username,password}` (the React `data-testid`s) and a
//! `new-conn-http-proxy-hint` text node.

use std::rc::Rc;

use rune::{col, text, El, TextEdit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::select::Select;

const ROOT: &str = "new-conn-http-proxy-fields";

pub(crate) use super::tunnel_field_contracts::{
    HttpProxyScheme, HttpProxyTunnelFieldsValue as HttpProxyValue,
};

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type SchemeFn<S> = Rc<dyn Fn(&mut S, HttpProxyScheme) + 'static>;
type BoolFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut bool + 'static>;

/// Builder for the field group. `S` owns the five text edits and handles the
/// scheme change; no IPC or network call happens here.
#[allow(dead_code)] // consumer: the tunnel settings editor (not wired); remove when wired.
pub(crate) struct HttpProxyTunnelFields<'a, S: 'static> {
    value: &'a HttpProxyValue,
    host: EditFn<S>,
    port: EditFn<S>,
    username: EditFn<S>,
    password: EditFn<S>,
    timeout: EditFn<S>,
    scheme_open: Option<BoolFn<S>>,
    on_scheme: Option<SchemeFn<S>>,
}

#[allow(dead_code)] // consumer: the tunnel settings editor (not wired); remove when wired.
impl<'a, S: 'static> HttpProxyTunnelFields<'a, S> {
    pub(crate) fn new(
        value: &'a HttpProxyValue,
        host: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        port: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        username: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        password: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        timeout: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            value,
            host: Rc::new(host),
            port: Rc::new(port),
            username: Rc::new(username),
            password: Rc::new(password),
            timeout: Rc::new(timeout),
            scheme_open: None,
            on_scheme: None,
        }
    }

    pub(crate) fn scheme_open(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut bool + 'static,
    ) -> Self {
        self.scheme_open = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_scheme(
        mut self,
        f: impl Fn(&mut S, HttpProxyScheme) + 'static,
    ) -> Self {
        self.on_scheme = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let value = self.value;
        let mut root = col().key(ROOT).w_full().gap(12.0);

        root = root.child(field_block(
            "host",
            "newConn.httpProxyHost",
            "proxy.corp.example",
            value.host.clone(),
            self.host.clone(),
            false,
            true,
            dark,
        ));
        root = root.child(field_block(
            "port",
            "newConn.httpProxyPort",
            "8080",
            value.port.clone(),
            self.port.clone(),
            false,
            true,
            dark,
        ));

        let labels = ["HTTP", "HTTPS"];
        let values = ["http", "https"];
        let mut scheme = Select::new("new-conn-http-proxy-scheme", &labels)
            .values(&values)
            .current(Some(value.scheme.label()))
            .label(t("newConn.httpProxyScheme"));
        if let Some(accessor) = self.scheme_open.clone() {
            let open = accessor.clone();
            scheme = scheme.open(value.scheme_open).on_toggle(move |s: &mut S| {
                *open(s) = true;
            });
            let open = accessor.clone();
            scheme = scheme.on_close(move |s: &mut S| *open(s) = false);
            if let Some(on_scheme) = self.on_scheme.clone() {
                let open = accessor.clone();
                scheme = scheme.on_choose(move |s: &mut S, chosen: &str, _tx: &mut rune::Tx| {
                    on_scheme(s, HttpProxyScheme::from_value(chosen));
                    *open(s) = false;
                });
            }
        } else {
            // The base requires a toggle for an enabled select; without a host
            // accessor it simply never opens rather than panicking in debug.
            scheme = scheme.on_toggle(|_s: &mut S| {});
        }
        root = root.child(
            col()
                .key("new-conn-http-proxy-scheme-block")
                .w_full()
                .gap(4.0)
                .child(
                    Label::new("new-conn-http-proxy-scheme-label", t("newConn.httpProxyScheme"))
                        .required(true)
                        .build(dark),
                )
                .child(scheme.build(&k, layer)),
        );

        root = root.child(field_block(
            "timeout",
            "newConn.httpProxyTimeout",
            "30",
            value.timeout.clone(),
            self.timeout.clone(),
            false,
            false,
            dark,
        ));
        root = root.child(field_block(
            "username",
            "newConn.httpProxyUsername",
            "",
            value.username.clone(),
            self.username.clone(),
            false,
            false,
            dark,
        ));
        root = root.child(field_block(
            "password",
            "newConn.httpProxyPassword",
            "",
            value.password.clone(),
            self.password.clone(),
            true,
            false,
            dark,
        ));
        root.child(
            text(t("newConn.httpProxyHint"))
                .key("new-conn-http-proxy-hint")
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
    value: TextEdit,
    edit: EditFn<S>,
    secure: bool,
    required: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let key = format!("new-conn-http-proxy-{suffix}");
    let mut field = TextField::new(key.clone(), &value)
        .height(32.0)
        .pad_x(10.0)
        .radius(k.radius.sm)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .placeholder(placeholder);
    if secure {
        field = field.secure();
    }
    field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
    col()
        .key(format!("{key}-block"))
        .w_full()
        .gap(4.0)
        .child(Label::new(format!("{key}-label"), t(label_key)).required(required).build(dark))
        .child(field.build(dark))
}
