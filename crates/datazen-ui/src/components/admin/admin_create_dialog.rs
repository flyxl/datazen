//! `AdminCreateDialog` — T013, the state half of the port.
//!
//! Ported from `packages/frontend/src/components/admin/AdminCreateDialog.tsx`
//! (132 lines): a `max-w-md` dialog that creates one named object
//! (`create_database` / `create_schema` via the two thin callers in
//! `packages/frontend/src/windows/connection/CreateDatabaseDialog.tsx` and
//! `CreateSchemaDialog.tsx`) by executing a driver command with
//! `{ name, ...optionalValues }`.
//!
//! | React source | Rune item |
//! |---|---|
//! | `AdminCreateDialogProps` (`AdminCreateDialog.tsx:14–29`) | [`AdminCreateDialogConfig`] |
//! | `useConnectionCommand(open ? dbSessionId : undefined, command)` (42) | [`AdminCreateDialogState::set_definition`] — the host feeds the definition in (see below) |
//! | `name` / `optionalValues` / `running` / `error` state (43–46) | [`AdminCreateDialogState`] fields |
//! | `resetForm` / `handleClose` (48–58) | [`AdminCreateDialogState::reset_form`] / [`AdminCreateDialogState::close`] |
//! | `handleOptionalChange` (60–62) | the per-field `TextEdit` in [`AdminCreateDialogState::optional_edit`] |
//! | `handleCreate` (64–92) | [`AdminCreateDialogState::create`] + [`AdminCreateDialogState::execute_succeeded`] / [`AdminCreateDialogState::execute_failed`] |
//! | `Dialog` + footer + name `Input` + `AdminSchemaFields` + `CopyableError` JSX (94–131) | `admin_create_dialog_view::AdminCreateDialog::build` |
//!
//! # Deliberate boundaries (VM placeholders, like the T012 host)
//!
//! * **No driver-command adapter call yet.** `datazen-ui-api` has no
//!   `getConnectionCommands` / `execute` adapter (the IPC shell is
//!   `driverCommands` in `packages/driver-sdk/src/ipc/driverCommands.ts`),
//!   so `create()` records the exact execute request in the
//!   [`AdminCreateDialogState::execute_requests`] outbox and holds
//!   `running` until the host answers through `execute_succeeded` /
//!   `execute_failed`. Draining the outbox belongs to the task that wires
//!   the driver commands; draining it early would destroy the history that
//!   task reads.
//! * **No definition fetch.** React fetches the command definition through
//!   `useConnectionCommand` (`hooks/useConnectionCommand.ts`); the host
//!   feeds the fetched fields through [`AdminCreateDialogState::set_definition`],
//!   which keeps the edits of fields that survive and drops the rest.
//! * **No `onClose` / `onCreated` callback edge.** React calls the props
//!   after a successful create; here the host owns this state and observes
//!   [`AdminCreateDialogState::is_open`] and the
//!   [`AdminCreateDialogState::created_names`] outbox, which the
//!   `CreateDatabaseDialog` / `CreateSchemaDialog` wiring task drains.
//! * **No `AdminSchemaFields` module yet.** That component is its own task
//!   (T065, with `PrivilegeSelector`); the optional-field rendering in the
//!   view is a minimal local rendering of the same `commandSchema.ts`
//!   rules (`fieldLabel` / `fieldPlaceholder` / the string-only guard —
//!   see [`AdminSchemaField`]), and T065 replaces it with the real port.
//!   The field model here is deliberately the smallest shape that port
//!   needs: name, JSON-schema `type`, `title`, `description`, `example` /
//!   `examples`.
//! * **The 1500 ms "copied" feedback is not reverted by this module.**
//!   Pressing the error's copy control sets `error_copied` (and the
//!   `CopyableError` base schedules a `wake_after(1500ms)` frame); the
//!   host tick that drives this dialog clears it via
//!   [`AdminCreateDialogState::set_error_copied`], exactly as the T012
//!   expiry tick does for its host. There is no such tick yet because the
//!   dialog has no `AppState` mount — the `CreateDatabaseDialog` wiring
//!   task owns it.
//! * **`create()` also guards on `running`.** React's `handleCreate` has
//!   no such guard (only the button is disabled), so pressing Enter in the
//!   name field while a create is in flight would fire a second execute.
//!   A second execute would create the same database/schema twice, so the
//!   guard is a deliberate deviation, recorded here.
//! * **React's `autoFocus` is a one-shot focus request, not an `El`
//!   flag.** The `autofocus` flag only counts on the focusable element
//!   itself (`modal_initial_focus` in
//!   `vendor/rune/crates/rune/src/runtime/input/modal_focus.rs`), and
//!   the kit `TextInput` builds its inner input where no caller can set
//!   it — an `autofocus` on the field's outer row is ignored and the
//!   modal falls back to its first Tab stop (the close button). So
//!   opening sets [`AdminCreateDialogState::take_focus_name_request`],
//!   and the view's `on_layout_tx` hook on the name field consumes it
//!   with `tx.focus("admin-create-name-input", true)` — the same
//!   take-then-focus shape `ai_chat_state` uses for its input. The flag
//!   is one-shot, so tabbing away is never undone by a later layout.
//!
//! The view half lives in `admin_create_dialog_view` (dialog / name field /
//! schema fields / error render, key convention, panel-width
//! approximation).

// The production consumer is the `CreateDatabaseDialog` /
// `CreateSchemaDialog` wiring task; until then the state is exercised by
// `tests/admin_create_dialog.rs` only.
#![allow(dead_code)]

use rune::TextEdit;

/// The dialog's props (`AdminCreateDialog.tsx:14–29`), owned by the host
/// and fixed for the dialog's lifetime. `title_key` / `name_label_key` are
/// `I18nKey`s resolved at render time through `t()` — the two known
/// callers pass `common.createDatabase` / `createDb.name` and
/// `common.createSchema` / `createSchema.name`, and `name_placeholder` is
/// the literal `my_database` / `my_schema`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdminCreateDialogConfig {
    pub db_session_id: String,
    pub command: String,
    pub title_key: String,
    pub name_label_key: String,
    pub name_placeholder: String,
    /// `fieldLabels` in definition order (a `Record` in React; a `Vec`
    /// keeps the render order deterministic).
    pub field_labels: Vec<(String, String)>,
    pub database: Option<String>,
}

impl AdminCreateDialogConfig {
    pub(crate) fn new(
        db_session_id: impl Into<String>,
        command: impl Into<String>,
        title_key: impl Into<String>,
        name_label_key: impl Into<String>,
        name_placeholder: impl Into<String>,
    ) -> Self {
        Self {
            db_session_id: db_session_id.into(),
            command: command.into(),
            title_key: title_key.into(),
            name_label_key: name_label_key.into(),
            name_placeholder: name_placeholder.into(),
            field_labels: Vec::new(),
            database: None,
        }
    }

    pub(crate) fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    pub(crate) fn field_label(
        mut self,
        field: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        self.field_labels.push((field.into(), label.into()));
        self
    }

    pub(crate) fn label_override(&self, field: &str) -> Option<&str> {
        self.field_labels
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, label)| label.as_str())
    }
}

/// One input-schema property (`lib/commandSchema.ts:3–8` `SchemaField`),
/// reduced to the fields the dialog renders. `schema` in React is the raw
/// JSON-schema object; only the keys the dialog reads are modelled here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AdminSchemaField {
    pub name: String,
    /// `schema.type`, if the driver sent one.
    pub schema_type: Option<String>,
    /// `schema.title`.
    pub title: Option<String>,
    /// `schema.description`.
    pub description: Option<String>,
    /// `schema.example`.
    pub example: Option<String>,
    /// `schema.examples`.
    pub examples: Vec<String>,
}

impl AdminSchemaField {
    /// A plain string field, the common case (`create_database`'s
    /// `encoding` / `owner`, `create_schema`'s `owner`).
    pub(crate) fn string_field(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema_type: Some("string".to_string()),
            ..Self::default()
        }
    }

    /// `renderField`'s guard (`AdminSchemaFields.tsx:22`): only string
    /// fields — and fields with no `type` at all — render an input.
    pub(crate) fn renders_input(&self) -> bool {
        self.schema_type.as_deref().map_or(true, |t| t == "string")
    }

    /// `fieldLabel` (`lib/commandSchema.ts:49–57`): `title`, else
    /// `description`, else the field name.
    pub(crate) fn label(&self) -> &str {
        self.title
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| self.description.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or(&self.name)
    }

    /// `fieldPlaceholder` (`lib/commandSchema.ts:39–47`): the first
    /// `examples` entry, else `example`.
    pub(crate) fn placeholder(&self) -> Option<&str> {
        self.examples
            .first()
            .map(String::as_str)
            .or(self.example.as_deref())
    }
}

/// One recorded `create()` — the exact fields React passes to
/// `driverCommands.execute` (`AdminCreateDialog.tsx:74–80`), held for the
/// host drain (see the module docs). `input` is a `Vec` so the order
/// React sends (`name` first, then the optional values in field order)
/// is preserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdminCreateExecuteRequest {
    pub db_session_id: String,
    pub command: String,
    pub input: Vec<(String, String)>,
    pub database: Option<String>,
}

/// The dialog's state: React's four `useState`s plus the props
/// (`AdminCreateDialog.tsx:42–46`).
#[derive(Clone, Debug)]
pub(crate) struct AdminCreateDialogState {
    config: AdminCreateDialogConfig,
    open: bool,
    /// Bumped on every open, so a reopened dialog scrolls to the top
    /// (the `Dialog::open_epoch` pattern).
    open_epoch: u64,
    name: TextEdit,
    /// One edit per visible schema field, in definition order.
    optional: Vec<(String, TextEdit)>,
    /// The fetched command definition's fields, in schema order.
    definition: Vec<AdminSchemaField>,
    running: bool,
    error: Option<String>,
    error_copied: bool,
    /// React's `autoFocus` on the name input, as a one-shot request:
    /// set by [`AdminCreateDialogState::open_dialog`], consumed by the
    /// view's layout hook (see the module docs).
    focus_name_pending: bool,
    execute_requests: Vec<AdminCreateExecuteRequest>,
    created_names: Vec<String>,
}

impl AdminCreateDialogState {
    pub(crate) fn new(config: AdminCreateDialogConfig) -> Self {
        Self {
            config,
            open: false,
            open_epoch: 0,
            name: TextEdit::new(String::new()),
            optional: Vec::new(),
            definition: Vec::new(),
            running: false,
            error: None,
            error_copied: false,
            focus_name_pending: false,
            execute_requests: Vec::new(),
            created_names: Vec::new(),
        }
    }

    pub(crate) fn config(&self) -> &AdminCreateDialogConfig {
        &self.config
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open_epoch(&self) -> u64 {
        self.open_epoch
    }

    /// The raw name text (untrimmed — trimming happens in `create()`,
    /// as React trims only when building the request).
    pub(crate) fn name_text(&self) -> &str {
        &self.name.text
    }

    pub(crate) fn name_edit(&mut self) -> &mut TextEdit {
        &mut self.name
    }

    pub(crate) fn name_edit_ref(&self) -> &TextEdit {
        &self.name
    }

    /// The `TextEdit` a schema field's input writes into, creating an
    /// empty one when the host never set the definition — the view's
    /// `on_edit` accessor always has somewhere to write.
    pub(crate) fn optional_edit(&mut self, field: &str) -> &mut TextEdit {
        if let Some(index) = self.optional.iter().position(|(name, _)| name == field) {
            return &mut self.optional[index].1;
        }
        self.optional
            .push((field.to_string(), TextEdit::new(String::new())));
        let index = self.optional.len() - 1;
        &mut self.optional[index].1
    }

    pub(crate) fn optional_edit_ref(&self, field: &str) -> Option<&TextEdit> {
        self.optional
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, edit)| edit)
    }

    pub(crate) fn optional_value(&self, field: &str) -> &str {
        self.optional_edit_ref(field).map_or("", |edit| &edit.text)
    }

    /// The fields the dialog renders: the definition minus `name`
    /// (`exclude={['name']}`), minus non-string fields
    /// (`AdminSchemaFields.tsx:22`).
    pub(crate) fn visible_fields(&self) -> Vec<&AdminSchemaField> {
        self.definition
            .iter()
            .filter(|field| field.name != "name" && field.renders_input())
            .collect()
    }

    /// The label a field renders with: the caller's `fieldLabels`
    /// override, else `fieldLabel` (`AdminSchemaFields.tsx:27`).
    pub(crate) fn field_label(&self, field: &AdminSchemaField) -> String {
        self.config
            .label_override(&field.name)
            .map(str::to_string)
            .unwrap_or_else(|| field.label().to_string())
    }

    pub(crate) fn running(&self) -> bool {
        self.running
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn error_copied(&self) -> bool {
        self.error_copied
    }

    pub(crate) fn set_error_copied(&mut self, copied: bool) {
        self.error_copied = copied;
    }

    pub(crate) fn execute_requests(&self) -> &[AdminCreateExecuteRequest] {
        &self.execute_requests
    }

    pub(crate) fn created_names(&self) -> &[String] {
        &self.created_names
    }

    /// The primary button's `disabled` (`AdminCreateDialog.tsx:114`):
    /// a blank name, or a create already in flight.
    pub(crate) fn can_create(&self) -> bool {
        !self.name.text.trim().is_empty() && !self.running
    }

    /// Opening does not reset the form — React's state survives while
    /// the component is mounted, and only `handleClose` / a successful
    /// create resets it. It does ask for the name focus again (React's
    /// `autoFocus` fires on every mount).
    pub(crate) fn open_dialog(&mut self) {
        self.open = true;
        self.open_epoch += 1;
        self.focus_name_pending = true;
    }

    /// The view's layout hook consumes the pending `autoFocus`: `true`
    /// once per open, `false` afterwards, so a later layout never steals
    /// the focus back.
    pub(crate) fn take_focus_name_request(&mut self) -> bool {
        std::mem::replace(&mut self.focus_name_pending, false)
    }

    /// React's `resetForm` (`AdminCreateDialog.tsx:48–52`): the name and
    /// the optional values are cleared and the error drops. `running`
    /// is untouched — a reset never cancels a flight.
    pub(crate) fn reset_form(&mut self) {
        self.name = TextEdit::new(String::new());
        for (_, edit) in &mut self.optional {
            *edit = TextEdit::new(String::new());
        }
        self.error = None;
    }

    /// React's `handleClose` (56–59): reset, then close.
    pub(crate) fn close(&mut self) {
        self.reset_form();
        self.open = false;
    }

    /// The definition fetch landing (`useConnectionCommand`, 42): the
    /// host passes the command's schema fields in schema order. Edits
    /// for fields that survive keep their text; dropped fields lose
    /// theirs; new fields start empty.
    pub(crate) fn set_definition(&mut self, fields: Vec<AdminSchemaField>) {
        let mut old = std::mem::take(&mut self.optional);
        let mut next: Vec<(String, TextEdit)> = Vec::new();
        for field in fields
            .iter()
            .filter(|f| f.name != "name" && f.renders_input())
        {
            let edit = old
                .iter()
                .position(|(name, _)| name == &field.name)
                .map(|index| old.remove(index).1)
                .unwrap_or_else(|| TextEdit::new(String::new()));
            next.push((field.name.clone(), edit));
        }
        self.optional = next;
        self.definition = fields;
    }

    /// Test / host helper: set the name without going through the input.
    pub(crate) fn set_name(&mut self, name: impl Into<String>) {
        self.name = TextEdit::new(name.into());
    }

    /// Test / host helper: set one optional value without going through
    /// the input.
    pub(crate) fn set_optional(&mut self, field: &str, value: impl Into<String>) {
        let edit = self.optional_edit(field);
        *edit = TextEdit::new(value.into());
    }

    /// React's `handleCreate` (64–92): builds `{ name, ...optional }`
    /// from the trimmed values — blank optional values are dropped —
    /// records the exact execute request and holds `running` until the
    /// host answers. A blank name, or a create already in flight, is a
    /// no-op (the in-flight guard is the documented deviation above).
    pub(crate) fn create(&mut self) {
        if !self.can_create() {
            return;
        }
        let mut input: Vec<(String, String)> =
            vec![("name".to_string(), self.name.text.trim().to_string())];
        for field in self.visible_fields() {
            let value = self.optional_value(&field.name);
            if !value.trim().is_empty() {
                input.push((field.name.clone(), value.trim().to_string()));
            }
        }
        self.running = true;
        self.error = None;
        self.execute_requests.push(AdminCreateExecuteRequest {
            db_session_id: self.config.db_session_id.clone(),
            command: self.config.command.clone(),
            input,
            database: self.config.database.clone(),
        });
    }

    /// The host's answer to a recorded execute request that succeeded:
    /// React's post-`await` path (84–87) — reset the form, record the
    /// created name for the `onCreated` edge, and close.
    pub(crate) fn execute_succeeded(&mut self) {
        let created = self.name.text.trim().to_string();
        self.reset_form();
        self.created_names.push(created);
        self.open = false;
        self.running = false;
    }

    /// The host's answer to a recorded execute request that failed:
    /// React's `catch` (88–89) — the error stays on the dialog and the
    /// form keeps its values, as `finally` (90–91) drops `running`.
    pub(crate) fn execute_failed(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.running = false;
    }
}
