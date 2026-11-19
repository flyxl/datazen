//! Port of `packages/frontend/src/components/connection/ConnectionFormBody.tsx`.
//!
//! The body of the new/edit connection form: a required name field, an optional
//! database-type + group pair, the connection fields of the chosen form variant
//! (standard / file / index), the clear-stored-credentials checkbox (edit only),
//! the reused [`ConnectionAdvancedSettings`] accordion, and the test-result area
//! (a green success banner or a copyable danger error).
//!
//! ## Structural boundary (later tasks)
//!
//! React's direct dependencies are separate migration tasks and are **not**
//! imported here. They are represented by local, honest stand-ins so this file
//! compiles standalone and keeps the React shape:
//!
//! - `ConnectionFormState` (`useConnectionForm`) → [`FormBodyVm`], a plain VM the
//!   owner keeps in its `S`. Text inputs are bound through one
//!   [`ConnectionFormBody::on_vm`] accessor; discrete interactions go through
//!   [`ConnectionFormBody::on_action`].
//! - `getDriverConnectionForm` / driver-validator forms → always `None`, so the
//!   built-in variants render on the `else` branch, exactly like React.
//! - `DB_REGISTRY` → [`FormBodyVm::db_type_options`] (caller supplied, with a
//!   small default list). Removal stage: when a Rust `DB_REGISTRY` port lands,
//!   replace the default with the registry's `(value, label)` pairs.
//! - `PRESET_GROUP_OPTIONS` → [`PRESET_GROUPS`] below. Removal stage: when
//!   `lib/connectionGroups.ts` is ported, import its values instead.
//! - `StandardConnectionFields` / `FileConnectionFields` /
//!   `IndexConnectionFields` → the small field groups in this file. Removal
//!   stage: when each sibling component is ported, call it from here.
//! - `Input` → [`TextField`]; `Select` → [`Select`]; `Label` → [`Label`];
//!   `CopyableError` → [`CopyableError`]. `autoFocus` (window variant) has no
//!   Rune equivalent and is documented as such.
//!
//! React `data-testid` values are kept verbatim (`new-conn-name`). Derived keys
//! use the same `new-conn` prefix as `connection_advanced_settings.rs`.
//! All colour/type/radius values come from `kit(dark)`; no colour literal or new
//! glyph is authored here.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, Tx};

use crate::components::connection::connection_advanced_settings::{
    AdvancedAction, AdvancedSettingsVm, ConnectionAdvancedSettings,
};
use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// React `form.formVariant` union (the three built-in variants; a driver form
/// is a separate extension point and is always `None` in this port).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; consumed by build and the tests
pub(crate) enum FormVariant {
    File,
    Index,
    Standard,
}

#[allow(dead_code)] // reusable VM surface
impl FormVariant {
    pub(crate) fn from_value(value: &str) -> Self {
        match value {
            "file" => FormVariant::File,
            "index" => FormVariant::Index,
            _ => FormVariant::Standard,
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            FormVariant::File => "file",
            FormVariant::Index => "index",
            FormVariant::Standard => "standard",
        }
    }
}

/// Minimal stand-in for `PRESET_GROUP_OPTIONS` (`lib/connectionGroups.ts`).
/// Removal stage: import the ported `connectionGroups` constants instead.
#[allow(dead_code)] // reusable VM data; consumed by this module's build
pub(crate) const PRESET_GROUPS: [(&str, &str); 3] = [
    ("preset:production", "newConn.groupProd"),
    ("preset:development", "newConn.groupDev"),
    ("preset:testing", "newConn.groupTest"),
];

/// Minimal stand-in for `DB_REGISTRY` (`lib/databaseTypes.ts`) option labels.
/// Removal stage: derive from the ported `DB_REGISTRY` instead.
#[allow(dead_code)] // reusable VM data; consumed by FormBodyVm::default
pub(crate) const DEFAULT_DB_TYPES: [(&str, &str); 5] = [
    ("postgresql", "PostgreSQL"),
    ("mysql", "MySQL"),
    ("sqlite", "SQLite"),
    ("redis", "Redis"),
    ("mongodb", "MongoDB"),
];

/// Which of this component's own selects owns the app's open state. The
/// advanced settings selects keep their own `SelectKind` in their VM, so these
/// stay local and never collide with that namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum FormSelect {
    DbType,
    Group,
}

/// The view model: a cloneable projection of React's `ConnectionFormState`
/// plus the props of `ConnectionFormBodyProps`. The owner keeps the real values
/// in its `S`; this snapshot drives one render.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable VM surface; wired by later callers/tests
pub(crate) struct FormBodyVm {
    pub name: TextEdit,
    pub form_variant: FormVariant,
    /// React `variant === 'window'`.
    pub is_window: bool,
    pub show_db_type_select: bool,
    pub db_type_options: Vec<(String, String)>,
    pub database_type: String,
    /// React's `groupOptions` prop (feeds the advanced settings group select).
    pub group_options: Option<Vec<(String, String)>>,
    pub group: String,
    /// Standard/file connection fields.
    pub host: TextEdit,
    pub port: TextEdit,
    pub database: TextEdit,
    pub username: TextEdit,
    pub password: TextEdit,
    pub has_username: bool,
    /// `form.validationErrors` for the fields this component renders.
    pub host_error: String,
    pub port_error: String,
    pub database_error: String,
    /// Edit-only clear-stored-credentials checkbox.
    pub is_editing: bool,
    pub supports_clear_credentials: bool,
    pub clear_stored_credentials: bool,
    /// Test result area (`testResultRef` children).
    pub test_ok: String,
    pub test_err: String,
    /// Own select open state (see [`FormSelect`]).
    pub open_select: Option<FormSelect>,
    /// The advanced/tunnel accordion this body composes.
    pub advanced: AdvancedSettingsVm,
}

#[allow(dead_code)] // initial-state convenience; wired by later callers/tests
impl Default for FormBodyVm {
    fn default() -> Self {
        FormBodyVm {
            name: TextEdit::new(String::new()),
            form_variant: FormVariant::Standard,
            is_window: false,
            show_db_type_select: false,
            db_type_options: DEFAULT_DB_TYPES
                .iter()
                .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
                .collect(),
            database_type: "postgresql".to_owned(),
            group_options: None,
            group: "preset:development".to_owned(),
            host: TextEdit::new("127.0.0.1".to_owned()),
            port: TextEdit::new("5432".to_owned()),
            database: TextEdit::new("postgres".to_owned()),
            username: TextEdit::new("postgres".to_owned()),
            password: TextEdit::new(String::new()),
            has_username: true,
            host_error: String::new(),
            port_error: String::new(),
            database_error: String::new(),
            is_editing: false,
            supports_clear_credentials: false,
            clear_stored_credentials: false,
            test_ok: String::new(),
            test_err: String::new(),
            open_select: None,
            advanced: AdvancedSettingsVm::default(),
        }
    }
}

/// Every discrete interaction the widget asks the host to apply to `S`. Text
/// input is routed directly through the [`ConnectionFormBody::on_vm`] accessor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum FormAction {
    DatabaseType(String),
    Group(String),
    ClearStoredCredentials(bool),
    /// `None` closes whichever of this component's selects is open.
    OpenSelect(Option<FormSelect>),
    /// Forwarded to the composed advanced/tunnel accordion.
    Advanced(AdvancedAction),
}

type VmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut FormBodyVm + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, FormAction) + 'static>;

/// Derive a per-field `TextEdit` accessor from the whole-VM accessor.
fn field_edit<S: 'static>(
    vm: Option<&VmFn<S>>,
    pick: for<'a> fn(&'a mut FormBodyVm) -> &'a mut TextEdit,
) -> Option<EditFn<S>> {
    vm.map(|vm| {
        let vm = vm.clone();
        // Pin the derived accessor to `El::on_edit`'s higher-ranked signature
        // before erasing it to a trait object (Rc<dyn Fn> alone does not carry
        // the HRTB through a later `.on_edit` call).
        Rc::new(rune::edit_access(move |s: &mut S| pick(vm(s)))) as EditFn<S>
    })
}

/// Wrap an action as a zero-argument press handler.
fn press<S: 'static>(f: &Option<ActionFn<S>>, a: FormAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

/// A single-line field matching React's `<Input>` (with a red border when its
/// validation error is present). With no `on_edit` accessor the field is
/// disabled, so an unwired caller gets an honest inert field rather than a
/// focusable dead control.
fn input_field<S: 'static>(
    key: &str,
    value: &TextEdit,
    edit: Option<&EditFn<S>>,
    placeholder: Option<&str>,
    error: bool,
    secure: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut field = TextField::new(key, value)
        .height(32.0)
        .pad_x(10.0)
        .radius(k.radius.sm)
        .fill(k.c.surface_base)
        .border(1.0, if error { k.c.danger.border } else { k.c.border_base });
    if let Some(ph) = placeholder {
        field = field.placeholder(ph);
    }
    if secure {
        field = field.secure();
    }
    match edit {
        Some(edit) => {
            let edit = edit.clone();
            field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
        }
        None => field = field.disabled(true),
    }
    field.build(dark)
}

/// A visible label + field + optional validation error, mirroring each React
/// `<div>` in the form grid.
fn field_block(
    key: &str,
    label: &str,
    required: bool,
    field: El,
    error: Option<(&str, &str)>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut block = col()
        .key(format!("{key}-block"))
        .grow(1.0)
        .min_w(210.0)
        .gap(4.0)
        .child(Label::new(format!("{key}-label"), label).required(required).build(dark))
        .child(field);
    if let Some((error_key, message)) = error {
        block = block.child(
            text(message)
                .key(error_key)
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .min_w(0.0)
                .wrap(),
        );
    }
    block
}

/// Builder for the ported component. The caller owns `S`; this struct holds a
/// VM snapshot plus the two ports back into `S`.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct ConnectionFormBody<S: 'static> {
    vm: FormBodyVm,
    on_vm: Option<VmFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> ConnectionFormBody<S> {
    pub(crate) fn new(vm: FormBodyVm) -> Self {
        ConnectionFormBody {
            vm,
            on_vm: None,
            on_action: None,
        }
    }

    /// Accessor for the mutable VM in `S`. Without it every text field is
    /// inert (see [`input_field`]).
    pub(crate) fn on_vm(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut FormBodyVm + 'static,
    ) -> Self {
        self.on_vm = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, FormAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Builds the whole body, pushing any open select listboxes into `layer`.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let ConnectionFormBody { vm, on_vm, on_action } = self;
        let k = kit(dark);
        let name_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.name);
        let host_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.host);
        let port_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.port);
        let database_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.database);
        let username_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.username);
        let password_edit = field_edit(on_vm.as_ref(), |vm| &mut vm.password);

        // React line 35: `grid grid-cols-1 gap-3 md:grid-cols-2`. Rune has no
        // CSS grid; a wrapping row with two-column-friendly min widths keeps
        // the same structure and stays inside the minimum window.
        let name_field = input_field(
            "new-conn-name",
            &vm.name,
            name_edit.as_ref(),
            Some(t("newConn.namePlaceholder")),
            false,
            false,
            dark,
        );
        let mut grid = col().key("connection-form-body-grid").w_full().gap(12.0);

        // React line 36: the name block is `md:col-span-2`.
        grid = grid.child(
            col().key("new-conn-name-block").w_full().gap(4.0)
                .child(
                    Label::new("new-conn-name-label", t("newConn.connName"))
                        .required(true)
                        .build(dark),
                )
                .child(name_field),
        );

        // React lines 47-75: database type + group selects when requested.
        if vm.show_db_type_select {
            let db_labels: Vec<&str> = vm.db_type_options.iter().map(|o| o.1.as_str()).collect();
            let db_values: Vec<&str> = vm.db_type_options.iter().map(|o| o.0.as_str()).collect();
            let mut db_sel = Select::new("new-conn-db-type-field", &db_labels)
                .values(&db_values)
                .current(Some(vm.database_type.as_str()))
                .label(t("newConn.dbType"))
                .open(vm.open_select == Some(FormSelect::DbType))
                .on_toggle(press(&on_action, FormAction::OpenSelect(Some(FormSelect::DbType))))
                .on_close(press(&on_action, FormAction::OpenSelect(None)));
            let f = on_action.clone();
            db_sel = db_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                if let Some(cb) = &f {
                    cb(s, FormAction::DatabaseType(value.to_owned()));
                    cb(s, FormAction::OpenSelect(None));
                }
            });
            let db_block = field_block(
                "new-conn-db-type",
                t("newConn.dbType"),
                true,
                db_sel.build(k, layer),
                None,
                dark,
            );

            // React builds `noGroup` + the three preset groups.
            let mut group_options: Vec<(String, String)> = vec![(
                String::new(),
                t("newConn.noGroup").to_owned(),
            )];
            group_options.extend(
                PRESET_GROUPS
                    .iter()
                    .map(|(v, key)| ((*v).to_owned(), t(key).to_owned())),
            );
            let group_labels: Vec<&str> = group_options.iter().map(|o| o.1.as_str()).collect();
            let group_values: Vec<&str> = group_options.iter().map(|o| o.0.as_str()).collect();
            let mut group_sel = Select::new("new-conn-group-field", &group_labels)
                .values(&group_values)
                .current(Some(vm.group.as_str()))
                .label(t("newConn.group"))
                .open(vm.open_select == Some(FormSelect::Group))
                .on_toggle(press(&on_action, FormAction::OpenSelect(Some(FormSelect::Group))))
                .on_close(press(&on_action, FormAction::OpenSelect(None)));
            let f = on_action.clone();
            group_sel = group_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                if let Some(cb) = &f {
                    cb(s, FormAction::Group(value.to_owned()));
                    cb(s, FormAction::OpenSelect(None));
                }
            });
            let group_block = field_block(
                "new-conn-group",
                t("newConn.group"),
                false,
                group_sel.build(k, layer),
                None,
                dark,
            );
            grid = grid.child(
                row()
                    .key("new-conn-type-group-row")
                    .w_full()
                    .gap(12.0)
                    .flex_wrap()
                    .child(db_block)
                    .child(group_block),
            );
        }

        // React lines 77-94: the chosen form-variant fields.
        let mut fields = row()
            .key("new-conn-fields-row")
            .w_full()
            .gap(12.0)
            .flex_wrap();
        match vm.form_variant {
            FormVariant::File => {
                // React `FileConnectionFields`: the always-visible database
                // file path (ADB import is a separate later task).
                let path = input_field(
                    "new-conn-db-file-path",
                    &vm.database,
                    database_edit.as_ref(),
                    Some("/path/to/db.sqlite"),
                    !vm.database_error.is_empty(),
                    false,
                    dark,
                );
                fields = fields.child(field_block(
                    "new-conn-db-file-path",
                    t("newConn.dbFilePath"),
                    true,
                    path,
                    (!vm.database_error.is_empty()).then_some((
                        "new-conn-db-file-path-error",
                        vm.database_error.as_str(),
                    )),
                    dark,
                ));
            }
            FormVariant::Index => {
                // React `IndexConnectionFields` is passed as `databaseField`
                // into `StandardConnectionFields` (host/port/user/password).
                for block in standard_fields(
                    &vm,
                    host_edit.as_ref(),
                    port_edit.as_ref(),
                    None,
                    username_edit.as_ref(),
                    password_edit.as_ref(),
                    Some(index_field(database_edit.as_ref(), &vm.database, dark)),
                    dark,
                ) {
                    fields = fields.child(block);
                }
            }
            FormVariant::Standard => {
                for block in standard_fields(
                    &vm,
                    host_edit.as_ref(),
                    port_edit.as_ref(),
                    database_edit.as_ref(),
                    username_edit.as_ref(),
                    password_edit.as_ref(),
                    None,
                    dark,
                ) {
                    fields = fields.child(block);
                }
            }
        }
        grid = grid.child(fields);

        let mut root = col()
            .key("connection-form-body")
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .child(grid);

        // React lines 97-110: edit-only clear-stored-credentials checkbox.
        if vm.is_editing && vm.supports_clear_credentials {
            root = root.child(
                Checkbox::new(
                    "new-conn-clear-stored-credentials",
                    vm.clear_stored_credentials,
                )
                .label(t("newConn.clearStoredCredentials"))
                .description(t("newConn.clearStoredCredentialsHint"))
                .on_toggle(press(
                    &on_action,
                    FormAction::ClearStoredCredentials(!vm.clear_stored_credentials),
                ))
                .build(k),
            );
        }

        // React line 112: the advanced/tunnel accordion (reused verbatim).
        let mut adv_vm = vm.advanced.clone();
        if let Some(groups) = &vm.group_options {
            adv_vm.group_options = Some(groups.clone());
        }
        let mut advanced_layer: Vec<El> = Vec::new();
        let advanced_action = on_action.clone();
        let advanced = ConnectionAdvancedSettings::<S>::new(adv_vm)
            .on_action(move |s: &mut S, a| {
                if let Some(cb) = &advanced_action {
                    cb(s, FormAction::Advanced(a));
                }
            })
            .build(k, &mut advanced_layer);
        root = root.child(advanced);
        layer.extend(advanced_layer);

        // React lines 114-127: the `testResultRef` area.
        let mut result = col()
            .key("new-conn-test-result")
            .w_full()
            .min_w(0.0)
            .gap(8.0);
        if !vm.test_ok.is_empty() {
            let message = format!("{}{}", t("newConn.testSuccess"), vm.test_ok);
            result = result.child(
                col()
                    .key("new-conn-test-ok")
                    .role("status")
                    .w_full()
                    .min_w(0.0)
                    .radius(k.radius.md)
                    .fill(k.alpha(k.c.success.fg, 0.10))
                    .border(1.0, k.alpha(k.c.success.fg, 0.20))
                    .padding(12.0)
                    .child(
                        text(message)
                            .key("new-conn-test-ok-text")
                            .font(k.ty.text_xsmall)
                            .color(k.c.success.fg)
                            .min_w(0.0)
                            .wrap(),
                    ),
            );
        }
        if !vm.test_err.is_empty() {
            result = result.child(
                CopyableError::new("new-conn-test-error", vm.test_err.clone())
                    .copy_button(true)
                    .build::<S>(k),
            );
        }
        root.child(result)
    }
}

/// `IndexConnectionFields` stand-in: a numeric database index, clamped to
/// `0..=15` like React. The clamp lives in the VM's edit path in production;
/// here it is only a placeholder node value, so it is left to the caller.
#[allow(dead_code)] // helper; consumed by this module's build
fn index_field<S: 'static>(edit: Option<&EditFn<S>>, value: &TextEdit, dark: bool) -> El {
    input_field(
        "new-conn-database-index",
        value,
        edit,
        Some("0"),
        false,
        false,
        dark,
    )
}

/// `StandardConnectionFields` stand-in: host / port / database / username /
/// password, with validation errors on host, port and database.
#[allow(dead_code)] // helper; consumed by this module's build
fn standard_fields<S: 'static>(
    vm: &FormBodyVm,
    host_edit: Option<&EditFn<S>>,
    port_edit: Option<&EditFn<S>>,
    database_edit: Option<&EditFn<S>>,
    username_edit: Option<&EditFn<S>>,
    password_edit: Option<&EditFn<S>>,
    database_field: Option<El>,
    dark: bool,
) -> Vec<El> {
    // React window variant uses `prod-db.example.com`, dialog uses `127.0.0.1`.
    let host_placeholder = if vm.is_window {
        "prod-db.example.com"
    } else {
        "127.0.0.1"
    };
    let database_placeholder = if vm.is_window { Some("myapp_production") } else { None };

    let mut out = Vec::new();
    out.push(field_block(
        "new-conn-host",
        t("newConn.host"),
        true,
        input_field(
            "new-conn-host",
            &vm.host,
            host_edit,
            Some(host_placeholder),
            !vm.host_error.is_empty(),
            false,
            dark,
        ),
        (!vm.host_error.is_empty())
            .then_some(("new-conn-host-error", vm.host_error.as_str())),
        dark,
    ));
    out.push(field_block(
        "new-conn-port",
        t("newConn.port"),
        true,
        input_field(
            "new-conn-port",
            &vm.port,
            port_edit,
            None,
            !vm.port_error.is_empty(),
            false,
            dark,
        ),
        (!vm.port_error.is_empty())
            .then_some(("new-conn-port-error", vm.port_error.as_str())),
        dark,
    ));

    match database_field {
        Some(field) => out.push(field),
        None => {
            out.push(field_block(
                "new-conn-database",
                t("newConn.database"),
                false,
                input_field(
                    "new-conn-database",
                    &vm.database,
                    database_edit,
                    database_placeholder,
                    !vm.database_error.is_empty(),
                    false,
                    dark,
                ),
                (!vm.database_error.is_empty())
                    .then_some(("new-conn-database-error", vm.database_error.as_str())),
                dark,
            ));
        }
    }

    if vm.has_username {
        out.push(field_block(
            "new-conn-username",
            t("newConn.username"),
            false,
            input_field(
                "new-conn-username",
                &vm.username,
                username_edit,
                database_placeholder.map(|_| "postgres"),
                false,
                false,
                dark,
            ),
            None,
            dark,
        ));
    }
    out.push(field_block(
        "new-conn-password",
        t("newConn.password"),
        false,
        input_field(
            "new-conn-password",
            &vm.password,
            password_edit,
            None,
            false,
            true,
            dark,
        ),
        None,
        dark,
    ));
    out
}

// Leftover L1 (removal stage: when `views/connections.rs` is rewritten to mount
// this port): provide `FormBodyVm` from the connection dialog's `AppState`
// slice, bind `on_vm` to that slice, and translate `FormAction` into the host's
// form mutations. Until then the component is not mounted in production and the
// stand-ins above are documented rather than pretending to be the real deps.
