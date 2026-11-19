//! Connection share/import dialog — port of
//! `packages/frontend/src/components/connection/ConnectionShareDialog.tsx`.
//!
//! Export paints two password fields with the exact React required/mismatch
//! keys, builds a UTC-dated `.datazenconnection` filename and stages one host
//! request; success closes and reports the count, failure maps through
//! [`ipc_connection_share_error`] with `common.exportFailed` fallback. Import
//! derives title/action/width from `mode` + `importSource` and paints the shared
//! import-fields view. All async work is injected: a press emits [`ShareAction`],
//! the host stages it and later delivers [`ShareEvent`] with an `epoch` (a late
//! answer for an old epoch is ignored). No fake IPC.
//!
//! ## Local stand-ins (removal stages)
//! - **T346 `useConnectionImport`** → [`ImportFlowState`] and
//!   [`ShareState::begin_import`] / [`ShareState::apply`]; remove when the hook
//!   port lands and drive it from the host.
//! - **T103 `ConnectionImportFields`** → [`build_import_fields`] paints the same
//!   React testids; remove the function and call the real port when it lands.
//! - **`lib/connectionShareError.ts`** → [`translate_connection_share_error`] /
//!   [`ipc_connection_share_error`]; remove when that `lib` port lands.
//!
//! ## Keys
//! `connection-share-dialog` (+ Dialog's `-title`/`-body`/`-footer`/`-close`),
//! `connection-share-password`, `connection-share-confirm-password`,
//! `connection-share-password-error`, `connection-share-cancel`,
//! `connection-share-primary`, plus shared import testids
//! `connection-import-fields`, `import-data-path`, `import-browse-folder`,
//! `import-browse-file`, `import-selected-file`, `import-password-input`,
//! `connection-import-error`.

use std::rc::Rc;

use chrono::{Datelike, Utc};
use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
const ROOT_KEY: &str = "connection-share-dialog";
/// React `max-w-sm` / `max-w-md` / `max-w-lg` in points (1rem = 16pt).
#[allow(dead_code)] // layout token; consumed by build and the geometry tests
pub(crate) const WIDTH_EXPORT: f32 = 384.0;
#[allow(dead_code)] // layout token; consumed by build and the geometry tests
pub(crate) const WIDTH_IMPORT_FILE: f32 = 448.0;
#[allow(dead_code)] // layout token; consumed by build and the geometry tests
pub(crate) const WIDTH_IMPORT_APP: f32 = 512.0;
/// React `ConnectionShareMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(dead_code)] // VM surface; wired by the view/outstanding host track
pub(crate) enum ShareMode {
    #[default]
    Export,
    Import,
}
/// React `ConnectionImportSource = 'file' | ConnectionImportApp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(dead_code)] // VM surface; wired by the view/outstanding host track
pub(crate) enum ImportSource {
    #[default]
    File,
    Dbx,
    Navicat,
    DataGrip,
    Dbeaver,
    TablePlus,
}

#[allow(dead_code)] // VM surface; consumed by title/label resolution and tests
impl ImportSource {
    /// React `isImportApp`: every source except `'file'`.
    pub(crate) fn is_app(self) -> bool {
        !matches!(self, Self::File)
    }

    /// React `CONNECTION_IMPORT_APP_LABEL` — brand names, not i18n keys.
    pub(crate) fn app_label(self) -> Option<&'static str> {
        match self {
            Self::Dbx => Some("DBX"),
            Self::Navicat => Some("Navicat"),
            Self::DataGrip => Some("DataGrip"),
            Self::Dbeaver => Some("DBeaver"),
            Self::TablePlus => Some("TablePlus"),
            Self::File => None,
        }
    }
}

/// Which React `browsePath('folder' | 'file')` button was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; delivered from the import-fields stand-in
pub(crate) enum BrowseKind {
    Folder,
    File,
}

/// React `ConnectionImportResult` (the fields the dialog forwards).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; delivered by ShareEvent::Imported
pub(crate) struct ImportResult {
    pub imported: u64,
    pub overwritten: u64,
    pub groups_added: u64,
    pub skipped: Vec<String>,
    pub source_format: Option<String>,
}

/// React `new Date().toISOString().slice(0, 10).replace(/-/g, '')`.
#[allow(dead_code)] // pure helper; consumed by begin_export and the tests
pub(crate) fn utc_date_ymd() -> String {
    let now = Utc::now();
    format!("{:04}{:02}{:02}", now.year(), now.month(), now.day())
}

/// React `datazen-connections-${date}.datazenconnection`.
#[allow(dead_code)] // pure helper; consumed by begin_export and the tests
pub(crate) fn export_filename(date_ymd: &str) -> String {
    format!("datazen-connections-{date_ymd}.datazenconnection")
}

/// Substitute React's `{name}` placeholders in a translated string.
fn fill(key: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = t(key).to_owned();
    for (name, value) in pairs { out = out.replace(&format!("{{{name}}}"), value); }
    out
}

// ---- local stand-in for lib/connectionShareError.ts (T346 companion) -------

const FORMAT_LABELS: [&str; 3] = ["DataZen", "TablePlus", "Encrypted connection"];

fn extract_format_label(message: &str) -> Option<&'static str> { FORMAT_LABELS.into_iter().find(|label| message.contains(label)) }

/// Mirror of React `translateConnectionShareError`, including its branch order:
/// exact table first, then the format-parameterised patterns, then the two
/// prefixes, then the trimmed raw string.
#[allow(dead_code)] // local stand-in; removed when lib/connectionShareError.ts lands
pub(crate) fn translate_connection_share_error(message: &str, fallback: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return fallback.to_owned();
    }

    let exact = match trimmed {
        "Password is required" => Some("connShare.error.passwordRequired"),
        "Password is required for encrypted connection import" => Some("connShare.error.encryptedImportPasswordRequired"),
        "Password is required for DBX encrypted import" => Some("connShare.error.dbxEncryptedPasswordRequired"),
        "Password is required for DataZen encrypted import" => Some("connShare.error.datazenEncryptedPasswordRequired"),
        "Password is required for DataZen import" => Some("connShare.error.datazenPasswordRequired"),
        "Import file is not valid UTF-8 text (for DataZen use .datazenconnection)" => Some("connShare.error.invalidUtf8"),
        "Unrecognized connection import format" => Some("connShare.error.unrecognizedFormat"),
        "No connection files to import" => Some("connShare.error.noFiles"),
        "Decryption failed: wrong password" => Some("connShare.error.decryptionWrongPassword"),
        "Invalid import file: missing 'connections' field" => Some("connShare.error.missingConnectionsField"),
        "missing connections" => Some("connShare.error.missingConnections"),
        "DBX decryption failed: wrong passphrase" => Some("connShare.error.dbxWrongPassphrase"),
        "DBX import: missing connections array" => Some("connShare.error.dbxMissingConnections"),
        _ => None,
    };
    if let Some(key) = exact {
        return t(key).to_owned();
    }

    if let Some(format) = extract_format_label(trimmed) {
        let f = [("format", format)];
        if trimmed == format!("Password is required for {format} import") {
            return fill("connShare.error.formatPasswordRequired", &f);
        }
        if trimmed == format!("Invalid {format} file: too short") {
            return fill("connShare.error.invalidFileTooShort", &f);
        }
        if trimmed == format!("Invalid {format} file: expected password-based encryption") {
            return fill("connShare.error.invalidFileEncryptionType", &f);
        }
        if trimmed.starts_with(&format!("Invalid {format} file: unsupported RNCryptor version")) {
            return fill("connShare.error.invalidFileRncryptorVersion", &f);
        }
        if trimmed.starts_with(&format!("{format} decryption failed: wrong password or corrupt file"))
            && trimmed.contains("Legacy DataZen")
        {
            return fill("connShare.error.decryptionFailedLegacyHint", &f);
        }
        if trimmed == format!("{format} decryption failed: wrong password or corrupt file") {
            return fill("connShare.error.decryptionFailed", &f);
        }
        if trimmed.starts_with(&format!("Invalid {format} JSON after decrypt:")) {
            return fill("connShare.error.invalidJsonAfterDecrypt", &f);
        }
        if trimmed.starts_with(&format!("{format} payload is not UTF-8:")) {
            return fill("connShare.error.payloadNotUtf8", &f);
        }
    }

    if trimmed.starts_with("Failed to read ") {
        return t("connShare.error.readFailed").to_owned();
    }
    if trimmed.starts_with("Unknown import source: ") {
        return t("connShare.error.unknownImportSource").to_owned();
    }
    trimmed.to_owned()
}

/// Mirror of React `ipcConnectionShareError`: the host hands over the raw IPC
/// message (already unwrapped from a string or an `Error`); blank → fallback.
#[allow(dead_code)] // local stand-in; removed when lib/connectionShareError.ts lands
pub(crate) fn ipc_connection_share_error(raw: &str, fallback: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return fallback.to_owned();
    }
    translate_connection_share_error(raw, fallback)
}

fn push_error(slot: &mut String, raw: &str, fallback: &str, out: &mut Vec<ShareOutput>) {
    let mapped = ipc_connection_share_error(raw, fallback);
    *slot = mapped.clone();
    out.push(ShareOutput::Error(mapped));
}

/// Local stand-in for `importFilePasswordPolicy` (T103's companion `lib` task).
#[allow(dead_code)] // local stand-in; removed when lib/importConnectionFile.ts lands
pub(crate) fn import_password_required(path: &str) -> bool {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some((_, ext)) = base.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "datazenconnection" | "datazenconnections" | "tableplusconnection"
    )
}

// ---- state machine ---------------------------------------------------------

/// React `useConnectionImport`'s values (T346 stand-in, removal stage above).
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // VM surface; owned by the host until T346 lands
pub(crate) struct ImportFlowState {
    pub password: TextEdit,
    pub data_path: TextEdit,
    pub path_found: bool,
    pub detecting: bool,
    pub submitting: bool,
    pub selected_file: Option<String>,
    pub local_error: String,
}

impl ImportFlowState {
    /// React `busy = submitting || detecting`.
    pub(crate) fn busy(&self) -> bool { self.submitting || self.detecting }
}

/// A validated export request handed to the host. `epoch` ties the late answer
/// to this submission.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; wired by the host/outstanding view track
pub(crate) struct ExportRequest {
    pub password: String,
    pub filename: String,
    pub epoch: u64,
}

/// All React `useState` values of the dialog plus import state and epoch.
#[derive(Clone, Debug)]
#[allow(dead_code)] // VM surface; owned by the host until the view track wires it
pub(crate) struct ShareState {
    pub open: bool,
    pub mode: ShareMode,
    pub source: ImportSource,
    pub password: TextEdit,
    pub confirm_password: TextEdit,
    pub submitting: bool,
    pub local_error: String,
    pub epoch: u64,
    pub import: ImportFlowState,
}

#[allow(dead_code)] // initial-state convenience; wired by the host/view track
impl ShareState {
    pub(crate) fn new(mode: ShareMode, source: ImportSource) -> Self {
        Self {
            open: false,
            mode,
            source,
            password: TextEdit::new(String::new()),
            confirm_password: TextEdit::new(String::new()),
            submitting: false,
            local_error: String::new(),
            epoch: 0,
            import: ImportFlowState::default(),
        }
    }

    /// React `handleExportSubmit` prologue + validation. On a guard failure the
    /// exact localised key text is stored and `None` returned (no IPC staged).
    pub(crate) fn begin_export(&mut self, date_ymd: &str) -> Option<ExportRequest> {
        self.local_error.clear();
        let password = self.password.text.clone();
        if password.trim().is_empty() {
            self.local_error = t("connShare.passwordRequired").to_owned();
            return None;
        }
        if password != self.confirm_password.text {
            self.local_error = t("connShare.passwordMismatch").to_owned();
            return None;
        }
        self.submitting = true;
        self.epoch = self.epoch.wrapping_add(1);
        Some(ExportRequest {
            password,
            filename: export_filename(date_ymd),
            epoch: self.epoch,
        })
    }

    /// The first press of the file flow only picks a file; the second imports.
    pub(crate) fn begin_import(&mut self) -> Option<ShareAction> {
        self.import.local_error.clear();
        if self.source.is_app() {
            if self.import.data_path.text.trim().is_empty() && !self.import.path_found {
                self.import.local_error = t("connShare.pathRequired").to_owned();
                return None;
            }
            self.import.submitting = true;
            self.epoch = self.epoch.wrapping_add(1);
            return Some(ShareAction::ImportFromApp {
                epoch: self.epoch,
                password: self.import.password.text.clone(),
                path: self.import.data_path.text.clone(),
                source: self.source,
            });
        }

        if let Some(file) = self.import.selected_file.clone() {
            if import_password_required(&file) && self.import.password.text.trim().is_empty() {
                self.import.local_error = t("connShare.encryptedImportPasswordRequired").to_owned();
                return None;
            }
            self.import.submitting = true;
            self.epoch = self.epoch.wrapping_add(1);
            Some(ShareAction::ImportAtPath {
                epoch: self.epoch,
                password: self.import.password.text.clone(),
                path: file,
            })
        } else {
            self.import.submitting = true;
            self.epoch = self.epoch.wrapping_add(1);
            Some(ShareAction::PickImportFile { epoch: self.epoch })
        }
    }

    /// Apply a host answer (stale epochs ignored); returns the callback outputs.
    pub(crate) fn apply(&mut self, event: ShareEvent, fallback: &str) -> Vec<ShareOutput> {
        if event.epoch() != self.epoch {
            return Vec::new();
        }
        let mut out = Vec::new();
        match event {
            ShareEvent::Exported { result, .. } => {
                self.submitting = false;
                match result {
                    Ok(count) => out.extend([ShareOutput::Close, ShareOutput::ExportSuccess(count)]),
                    Err(m) => push_error(&mut self.local_error, &m, fallback, &mut out),
                }
            }
            ShareEvent::FilePicked { result, .. } => {
                self.import.submitting = false;
                match result {
                    Ok(Some(path)) => self.import.selected_file = Some(path),
                    Ok(None) => {}
                    Err(m) => push_error(&mut self.import.local_error, &m, fallback, &mut out),
                }
            }
            ShareEvent::PathDetected { path, found, .. } => {
                self.import.detecting = false;
                self.import.data_path = TextEdit::new(path);
                self.import.path_found = found;
            }
            ShareEvent::PathPicked { result, .. } => {
                self.import.detecting = false;
                match result {
                    Ok(path) => { self.import.data_path = TextEdit::new(path); self.import.path_found = true; }
                    Err(m) => push_error(&mut self.import.local_error, &m, fallback, &mut out),
                }
            }
            ShareEvent::Imported { result, .. } => {
                self.import.submitting = false;
                match result {
                    Ok(value) => out.extend([ShareOutput::Close, ShareOutput::ImportSuccess(value)]),
                    Err(m) => push_error(&mut self.import.local_error, &m, fallback, &mut out),
                }
            }
        }
        out
    }

    /// React `primaryActionLabel`.
    pub(crate) fn primary_label(&self) -> String {
        if self.mode == ShareMode::Export {
            return t("connShare.exportAction").to_owned();
        }
        if !self.source.is_app() && self.import.selected_file.is_none() {
            t("connShare.chooseImportFile").to_owned()
        } else {
            t("connShare.importAction").to_owned()
        }
    }

    /// React `title`, including the `{app}` substitution for app imports.
    pub(crate) fn title(&self) -> String {
        if self.mode == ShareMode::Export {
            return t("common.exportConnections").to_owned();
        }
        match self.source.app_label() {
            Some(label) => fill("connShare.importFromAppTitle", &[("app", label)]),
            None => t("common.importConnections").to_owned(),
        }
    }

    /// React `dialogWidthClass` mapped to points.
    pub(crate) fn width(&self) -> f32 {
        if self.mode == ShareMode::Export {
            WIDTH_EXPORT
        } else if self.source.is_app() {
            WIDTH_IMPORT_APP
        } else {
            WIDTH_IMPORT_FILE
        }
    }

    /// React's cancel / primary `disabled` expressions.
    pub(crate) fn footer_disabled(&self) -> (bool, bool) {
        if self.mode == ShareMode::Export {
            (self.submitting, self.submitting)
        } else {
            (self.submitting || self.import.submitting, self.submitting || self.import.busy())
        }
    }
}

/// Presses the component asks its owner to perform. No IPC is executed here.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; handled by the host/outstanding view track
pub(crate) enum ShareAction {
    Close,
    /// Host calls `ShareState::begin_export(&date)` and stages the request.
    ExportSubmit { date: String },
    ImportSubmit,
    ImportFromApp { epoch: u64, password: String, path: String, source: ImportSource },
    ImportAtPath { epoch: u64, password: String, path: String },
    PickImportFile { epoch: u64 },
    BrowsePath { source: ImportSource, kind: BrowseKind },
}

/// Host answers delivered back to [`ShareState::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; delivered by the host/outstanding view track
pub(crate) enum ShareEvent {
    Exported { epoch: u64, result: Result<u64, String> },
    FilePicked { epoch: u64, result: Result<Option<String>, String> },
    PathDetected { epoch: u64, path: String, found: bool },
    PathPicked { epoch: u64, result: Result<String, String> },
    Imported { epoch: u64, result: Result<ImportResult, String> },
}

#[allow(dead_code)] // VM surface; consumed by apply's epoch guard
impl ShareEvent {
    pub(crate) fn epoch(&self) -> u64 {
        match self {
            Self::Exported { epoch, .. } | Self::FilePicked { epoch, .. }
            | Self::PathDetected { epoch, .. } | Self::PathPicked { epoch, .. }
            | Self::Imported { epoch, .. } => *epoch,
        }
    }
}

/// Side effects the host forwards to the component's callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // VM surface; handled by the host/outstanding view track
pub(crate) enum ShareOutput {
    Close,
    ExportSuccess(u64),
    ImportSuccess(ImportResult),
    Error(String),
}

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, ShareAction) + 'static>;

/// Builder for the ported dialog. `S` owns the [`ShareState`] and the text edits
/// the fields bind to; the widget never owns a file or an IPC call.
#[allow(dead_code)] // reusable component; mounted by the outstanding view track
pub(crate) struct ConnectionShareDialog<'a, S: 'static> {
    state: &'a ShareState,
    date: Option<String>,
    password_edit: EditFn<S>,
    confirm_edit: EditFn<S>,
    import_password_edit: EditFn<S>,
    import_path_edit: EditFn<S>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable component; mounted by the outstanding view track
impl<'a, S: 'static> ConnectionShareDialog<'a, S> {
    /// The four text edits are required: a rune text field only takes focus
    /// when it can reach the `TextEdit` it edits, so a missing accessor would
    /// make the field a dead control.
    pub(crate) fn new(
        state: &'a ShareState,
        password_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        confirm_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        import_password_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        import_path_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            date: None,
            password_edit: Rc::new(password_edit),
            confirm_edit: Rc::new(confirm_edit),
            import_password_edit: Rc::new(import_password_edit),
            import_path_edit: Rc::new(import_path_edit),
            on_action: None,
        }
    }

    /// Override the UTC date used in the export filename (tests pin this).
    pub(crate) fn date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, ShareAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k: &'static Kit = kit(dark);
        let state = self.state;
        let date = self
            .date
            .clone()
            .unwrap_or_else(|| utc_date_ymd());
        let (cancel_disabled, primary_disabled) = state.footer_disabled();

        let mut dialog = Dialog::new(ROOT_KEY, state.title())
            .open(state.open)
            .width(state.width())
            .fit()
            .body_max_height(360.0);
        if state.mode == ShareMode::Export {
            dialog = dialog.description(t("connShare.exportHint"));
        }

        let body = if state.mode == ShareMode::Export {
            build_export_body(k, state, &self)
        } else {
            build_import_fields(k, dark, state, &self)
        };
        dialog = dialog.child(body);

        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Neutral)
            .disabled(cancel_disabled);
        if let Some(on_action) = self.on_action.clone() {
            cancel = cancel.on_press(move |s: &mut S| on_action(s, ShareAction::Close));
        }
        dialog = dialog.footer(cancel.build(k).key(format!("{ROOT_KEY}-cancel")));

        let mut primary = Button::new(state.primary_label())
            .variant(ButtonVariant::Contrast)
            .disabled(primary_disabled);
        if let Some(on_action) = self.on_action.clone() {
            if state.mode == ShareMode::Export {
                primary = primary.on_press(move |s: &mut S| {
                    on_action(s, ShareAction::ExportSubmit { date: date.clone() })
                });
            } else {
                primary = primary.on_press(move |s: &mut S| on_action(s, ShareAction::ImportSubmit));
            }
        }
        dialog = dialog.footer(primary.build(k).key(format!("{ROOT_KEY}-primary")));

        if let Some(on_action) = self.on_action.clone() {
            dialog = dialog.on_close(move |s: &mut S| on_action(s, ShareAction::Close));
        }
        dialog.build(k, layer);
    }
}

/// A password field built on kit `TextInput` so `disabled(true)` genuinely
/// removes the edit accessor (a pasted keystroke cannot reach `state`).
fn secure_field<S: 'static>(
    k: &Kit,
    key: String,
    edit: &TextEdit,
    label: &str,
    disabled: bool,
    accessor: Option<EditFn<S>>,
) -> El {
    let mut input = rune_kit::TextInput::new(edit)
        .height(36.0)
        .secure()
        .label(t(label))
        .disabled(disabled);
    if !disabled {
        if let Some(accessor) = accessor {
            input = input.on_edit(move |s: &mut S| accessor(s));
        }
    }
    input.build(k).key(key)
}

/// Export body: password, confirm, local-error banner (React's three children).
fn build_export_body<S: 'static>(
    k: &Kit,
    state: &ShareState,
    owner: &ConnectionShareDialog<'_, S>,
) -> El {
    let disabled = state.submitting;
    let password = secure_field(
        k,
        format!("{ROOT_KEY}-password"),
        &state.password,
        "connShare.password",
        disabled,
        Some(owner.password_edit.clone()),
    );
    let confirm = secure_field(
        k,
        format!("{ROOT_KEY}-confirm-password"),
        &state.confirm_password,
        "connShare.confirmPassword",
        disabled,
        Some(owner.confirm_edit.clone()),
    );

    let mut body = col()
        .key(format!("{ROOT_KEY}-export-fields"))
        .w_full()
        .min_w(0.0)
        .gap(12.0)
        .child(field_label(k, "connShare.password", format!("{ROOT_KEY}-password-label")))
        .child(password)
        .child(field_label(k, "connShare.confirmPassword", format!("{ROOT_KEY}-confirm-password-label")))
        .child(confirm);
    if !state.local_error.is_empty() {
        body = body.child(
            text(state.local_error.clone())
                .key(format!("{ROOT_KEY}-password-error"))
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .radius(6.0)
                .px(12.0)
                .py(8.0)
                .fill(k.alpha(k.c.danger.fg, 0.1))
                .wrap_anywhere(),
        );
    }
    body
}

fn field_label(k: &Kit, key: &str, el_key: impl Into<String>) -> El {
    text(t(key))
        .key(el_key)
        .font(k.ty.text_xsmall)
        .color(k.c.text_muted)
}

/// Local stand-in for T103's `ConnectionImportFields` (removal stage in the
/// module doc). Paints the shared React testids from [`ImportFlowState`].
fn build_import_fields<S: 'static>(
    k: &Kit,
    dark: bool,
    state: &ShareState,
    owner: &ConnectionShareDialog<'_, S>,
) -> El {
    let imp = &state.import;
    let mut root = col()
        .key("connection-import-fields")
        .w_full()
        .min_w(0.0)
        .gap(12.0);

    if imp.selected_file.is_none() {
        root = root.child(
            text(t("connShare.importFormatsHint"))
                .key("import-formats-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        );
    }

    if state.source.is_app() {
        let path = TextField::new("import-data-path", &imp.data_path)
            .height(36.0)
            .label(t("connShare.dataPath"))
            .placeholder(t("connShare.dataPathPlaceholder"))
            .on_edit({
                let edit = owner.import_path_edit.clone();
                move |s: &mut S| edit(s)
            })
            .build(dark)
            .disabled(imp.busy());
        root = root.child(field_label(k, "connShare.dataPath", "import-data-path-label"));
        root = root.child(path);
        if imp.path_found {
            root = root.child(
                text(t("connShare.dataPathFoundHint"))
                    .key("import-data-path-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.success.fg)
                    .wrap(),
            );
        } else {
            root = root.child(
                text(t("connShare.dataPathMissingHint"))
                    .key("import-data-path-hint")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }
        let browse = |kind: BrowseKind| -> El {
            let mut button = Button::new(match kind {
                BrowseKind::Folder => t("connShare.browseFolder"),
                BrowseKind::File => t("connShare.browseFile"),
            })
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .disabled(imp.busy());
            if let Some(on_action) = owner.on_action.clone() {
                let source = state.source;
                button = button.on_press(move |s: &mut S| {
                    on_action(s, ShareAction::BrowsePath { source, kind })
                });
            }
            button
                .build(k)
                .key(match kind {
                    BrowseKind::Folder => "import-browse-folder",
                    BrowseKind::File => "import-browse-file",
                })
                .shrink()
        };
        root = root.child(
            row()
                .key("import-browse-row")
                .w_full()
                .gap(8.0)
                .flex_wrap()
                .child(browse(BrowseKind::Folder))
                .child(browse(BrowseKind::File)),
        );
    }

    if let Some(file) = imp.selected_file.as_ref() {
        root = root.child(
            text(format!("{}: {}", t("connShare.selectedImportFile"), file))
                .key("import-selected-file")
                .font(k.ty.text_xsmall)
                .color(k.c.text_base)
                .wrap_anywhere(),
        );
    }

    let pw = TextField::new("import-password-input", &imp.password)
        .height(36.0)
        .secure()
        .label(t("connShare.password"))
        .on_edit({
            let edit = owner.import_password_edit.clone();
            move |s: &mut S| edit(s)
        })
        .build(dark)
        .disabled(imp.submitting);
    root = root.child(field_label(k, "connShare.password", "import-password-label"));
    root = root.child(pw);

    let error = if !imp.local_error.is_empty() {
        Some(imp.local_error.clone())
    } else if !state.local_error.is_empty() {
        Some(state.local_error.clone())
    } else {
        None
    };
    if let Some(error) = error {
        root = root.child(
            text(error)
                .key("connection-import-error")
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .wrap_anywhere(),
        );
    }
    root
}
