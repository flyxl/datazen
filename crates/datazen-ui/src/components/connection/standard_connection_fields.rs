//! Port of `packages/frontend/src/components/connection/StandardConnectionFields.tsx`.
//!
//! The host / port / database / username / password block of a standard (or
//! index) connection form. React takes the whole `ConnectionFormState` and
//! calls its setters; Rune takes a small value mirror plus `TextEdit` accessors
//! so keystrokes route back into the host's state slice. The optional
//! `databaseField` prop is an `El` the caller supplies (React's
//! `IndexConnectionFields`); when absent this component renders its own
//! database field.
//!
//! Keys are the T103 stand-in's keys (`new-conn-host`, `-port`, `-database`,
//! `-username`, `-password` and the `-error` nodes) — no rename, so the
//! form-body tests keep their contract.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// The subset of `ConnectionFormState` this component reads.
#[allow(dead_code)] // contract mirror; remove when the shared form-state port lands.
#[derive(Clone, Copy)]
pub(crate) struct StandardFieldsValue<'a> {
    pub host: &'a TextEdit,
    pub port: &'a TextEdit,
    pub database: &'a TextEdit,
    pub username: &'a TextEdit,
    pub password: &'a TextEdit,
    pub host_error: &'a str,
    pub port_error: &'a str,
    pub database_error: &'a str,
    pub has_username: bool,
}

/// Builder for the ported field group.
#[allow(dead_code)] // consumer: ConnectionFormBody (this wave).
pub(crate) struct StandardConnectionFields<'a, S: 'static> {
    value: StandardFieldsValue<'a>,
    host: Option<EditFn<S>>,
    port: Option<EditFn<S>>,
    database: Option<EditFn<S>>,
    username: Option<EditFn<S>>,
    password: Option<EditFn<S>>,
    database_field: Option<El>,
    host_placeholder: String,
    database_placeholder: Option<String>,
}

#[allow(dead_code)] // consumer: ConnectionFormBody (this wave).
impl<'a, S: 'static> StandardConnectionFields<'a, S> {
    pub(crate) fn new(
        value: StandardFieldsValue<'a>,
        host: Option<EditFn<S>>,
        port: Option<EditFn<S>>,
        database: Option<EditFn<S>>,
        username: Option<EditFn<S>>,
        password: Option<EditFn<S>>,
    ) -> Self {
        Self {
            value,
            host,
            port,
            database,
            username,
            password,
            database_field: None,
            host_placeholder: "127.0.0.1".to_owned(),
            database_placeholder: None,
        }
    }

    /// React's `databaseField` prop.
    pub(crate) fn database_field(mut self, field: Option<El>) -> Self {
        self.database_field = field;
        self
    }

    pub(crate) fn host_placeholder(mut self, value: &str) -> Self {
        self.host_placeholder = value.to_owned();
        self
    }

    pub(crate) fn database_placeholder(mut self, value: Option<&str>) -> Self {
        self.database_placeholder = value.map(str::to_owned);
        self
    }

    fn field(
        &self,
        key: &str,
        edit: &TextEdit,
        accessor: Option<&EditFn<S>>,
        placeholder: Option<&str>,
        error: &str,
        secure: bool,
        dark: bool,
    ) -> El {
        let k = kit(dark);
        let invalid = !error.is_empty();
        let mut input = TextField::new(key, edit)
            .height(32.0)
            .pad_x(10.0)
            .radius(k.radius.sm)
            .fill(k.c.surface_base)
            .border(1.0, if invalid { k.c.danger.border } else { k.c.border_base });
        if let Some(ph) = placeholder {
            input = input.placeholder(ph);
        }
        if secure {
            input = input.secure();
        }
        if let Some(accessor) = accessor {
            let accessor = accessor.clone();
            input = input.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
        } else {
            input = input.disabled(true);
        }
        let mut block = col().key(format!("{key}-block")).grow(1.0).min_w(210.0).gap(4.0);
        block = block.child(
            Label::new(format!("{key}-label"), self.label_for(key))
                .required(matches!(key, "new-conn-host" | "new-conn-port"))
                .build(dark),
        );
        block = block.child(input.build(dark));
        if invalid {
            block = block.child(
                text(error)
                    .key(format!("{key}-error"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .min_w(0.0)
                    .wrap(),
            );
        }
        block
    }

    fn label_for(&self, key: &str) -> String {
        match key {
            "new-conn-host" => t("newConn.host").to_owned(),
            "new-conn-port" => t("newConn.port").to_owned(),
            "new-conn-database" => t("newConn.database").to_owned(),
            "new-conn-username" => t("newConn.username").to_owned(),
            "new-conn-password" => t("newConn.password").to_owned(),
            _ => String::new(),
        }
    }

    /// Build the fields in React order, ready to be appended to the form grid.
    pub(crate) fn build(self, dark: bool) -> Vec<El> {
        let mut out = Vec::new();
        out.push(self.field(
            "new-conn-host",
            self.value.host,
            self.host.as_ref(),
            Some(&self.host_placeholder),
            self.value.host_error,
            false,
            dark,
        ));
        out.push(self.field(
            "new-conn-port",
            self.value.port,
            self.port.as_ref(),
            None,
            self.value.port_error,
            false,
            dark,
        ));
        match &self.database_field {
            Some(field) => out.push(field.clone()),
            None => out.push(self.field(
                "new-conn-database",
                self.value.database,
                self.database.as_ref(),
                self.database_placeholder.as_deref(),
                self.value.database_error,
                false,
                dark,
            )),
        }
        if self.value.has_username {
            let ph = self.database_placeholder.as_deref().map(|_| "postgres");
            out.push(self.field(
                "new-conn-username",
                self.value.username,
                self.username.as_ref(),
                ph,
                "",
                false,
                dark,
            ));
        }
        out.push(self.field(
            "new-conn-password",
            self.value.password,
            self.password.as_ref(),
            None,
            "",
            true,
            dark,
        ));
        out
    }
}

// Leftover L1 (removal stage): when the shared `ConnectionFormState` contract
// lands, replace `StandardFieldsValue` with it and drop the per-item allows;
// the keys already match T103's stand-in so callers need no rename.
