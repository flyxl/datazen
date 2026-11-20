//! Port of `packages/frontend/src/components/connection/useConnectionImport.ts`
//! as an immediate-mode slice + effect machine.
//!
//! React's hook owns eight pieces of state and calls five async commands. Rune
//! has no promises, so the state lives in [`ImportState`] and the commands
//! become [`ImportEffect`]s the host executes, feeding answers back through
//! [`ImportState::step`]. No timer, thread or fake IPC is invented.
//!
//! The two-step file flow is preserved literally: the first `Submit` only picks
//! a file (and returns), the second one checks the password policy and imports.
//! A stale detection result is ignored by comparing the generation stamped on
//! the effect and its answer (React's `cancelled` closure flag).
//!
//! # Reuse / stand-ins
//!
//! `ImportSource`, `ImportResult`, `BrowseKind`, `PasswordPolicy`,
//! `ipc_connection_share_error` and `ImportVm` come from the existing
//! connection-share/import ports (T103/T137/T239) — nothing is duplicated.
//! `ImportStrings` carries the three translated strings React gets from
//! `t()`; removal stage: call `t()` at the view and pass resolved strings.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the import view wires this slice".

use super::connection_import_fields::{ImportVm, PasswordPolicy};
use super::connection_share_dialog::{
    ipc_connection_share_error, BrowseKind, ImportResult, ImportSource,
};

/// Translated strings the hook uses (React calls `t()` directly).
#[allow(dead_code)] // stand-in; remove when the view resolves these with t().
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportStrings {
    pub(crate) path_required: String,
    pub(crate) password_required: String,
    pub(crate) import_failed: String,
    pub(crate) choose_file: String,
    pub(crate) import_action: String,
}

/// The eight React state variables plus a detection generation.
#[allow(dead_code)] // slice; remove with the import view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportState {
    source: ImportSource,
    enabled: bool,
    password: String,
    data_path: String,
    path_found: bool,
    detecting: bool,
    submitting: bool,
    local_error: String,
    selected_file: Option<String>,
    generation: u64,
}

#[allow(dead_code)] // slice; remove with the import view.
impl ImportState {
    pub(crate) fn new(source: ImportSource, enabled: bool) -> Self {
        Self {
            source,
            enabled,
            password: String::new(),
            data_path: String::new(),
            path_found: false,
            detecting: false,
            submitting: false,
            local_error: String::new(),
            selected_file: None,
            generation: 0,
        }
    }

    // --- derived values -----------------------------------------------------

    pub(crate) fn source(&self) -> ImportSource {
        self.source
    }

    pub(crate) fn app_import(&self) -> bool {
        self.source.is_app()
    }

    pub(crate) fn password(&self) -> &str {
        &self.password
    }

    pub(crate) fn data_path(&self) -> &str {
        &self.data_path
    }

    pub(crate) fn path_found(&self) -> bool {
        self.path_found
    }

    pub(crate) fn detecting(&self) -> bool {
        self.detecting
    }

    pub(crate) fn submitting(&self) -> bool {
        self.submitting
    }

    pub(crate) fn busy(&self) -> bool {
        self.submitting || self.detecting
    }

    pub(crate) fn local_error(&self) -> &str {
        &self.local_error
    }

    pub(crate) fn selected_file(&self) -> Option<&str> {
        self.selected_file.as_deref()
    }

    /// React `selectedFile ? importFilePasswordPolicy(selectedFile) : null`.
    pub(crate) fn import_password_policy(&self) -> Option<PasswordPolicy> {
        self.selected_file
            .as_deref()
            .map(PasswordPolicy::from_path)
    }

    /// React `primaryActionLabel`: choose a file until one is selected in the
    /// file flow, then the import action.
    pub(crate) fn primary_action_label<'a>(&self, strings: &'a ImportStrings) -> &'a str {
        if !self.app_import() && self.selected_file.is_none() {
            &strings.choose_file
        } else {
            &strings.import_action
        }
    }

    /// Project into the existing widget VM (no duplicate view model).
    pub(crate) fn to_vm(&self, inline: bool) -> ImportVm {
        ImportVm {
            password: rune::TextEdit::new(self.password.clone()),
            data_path: rune::TextEdit::new(self.data_path.clone()),
            path_found: self.path_found,
            app_import: self.app_import(),
            busy: self.busy(),
            submitting: self.submitting,
            local_error: self.local_error.clone(),
            selected_file: self.selected_file.clone(),
            password_required: self.import_password_policy() == Some(PasswordPolicy::Required),
            inline,
        }
    }

    // --- user edits ---------------------------------------------------------

    pub(crate) fn set_password(&mut self, password: impl Into<String>) {
        self.password = password.into();
    }

    pub(crate) fn set_data_path(&mut self, path: impl Into<String>) {
        self.data_path = path.into();
    }

    pub(crate) fn set_path_found(&mut self, found: bool) {
        self.path_found = found;
    }

    pub(crate) fn set_local_error(&mut self, message: impl Into<String>) {
        self.local_error = message.into();
    }

    /// `replaceError` sets the local error and notifies the caller.
    fn replace_error(&mut self, message: String, events: &mut Vec<ImportEvent>) {
        self.local_error = message.clone();
        events.push(ImportEvent::Error(message));
    }

    fn map_error(&self, raw: &str, strings: &ImportStrings) -> String {
        ipc_connection_share_error(raw, &strings.import_failed)
    }

    // --- transitions --------------------------------------------------------

    /// Drive the machine with one host answer or user intent.
    pub(crate) fn step(
        &mut self,
        input: ImportInput,
        strings: &ImportStrings,
    ) -> ImportStep {
        let mut effects = Vec::new();
        let mut events = Vec::new();
        match input {
            ImportInput::Enabled(enabled) => {
                self.generation += 1;
                if !enabled {
                    // React's disabled branch clears every piece of state.
                    self.password.clear();
                    self.data_path.clear();
                    self.path_found = false;
                    self.local_error.clear();
                    self.submitting = false;
                    self.detecting = false;
                    self.selected_file = None;
                } else {
                    self.local_error.clear();
                    self.selected_file = None;
                    if self.app_import() {
                        self.detecting = true;
                        effects.push(ImportEffect::DetectPath {
                            source: self.source,
                            generation: self.generation,
                        });
                    } else {
                        self.data_path.clear();
                        self.path_found = false;
                        self.detecting = false;
                    }
                }
            }
            ImportInput::Detected { path, found, generation } => {
                if generation != self.generation {
                    return ImportStep::empty();
                }
                self.data_path = path;
                self.path_found = found;
                self.detecting = false;
            }
            ImportInput::DetectFailed { generation } => {
                if generation != self.generation {
                    return ImportStep::empty();
                }
                self.data_path.clear();
                self.path_found = false;
                self.detecting = false;
            }
            ImportInput::BrowsePath(kind) => {
                if self.app_import() {
                    effects.push(ImportEffect::BrowsePath {
                        kind,
                        source: self.source,
                        generation: self.generation,
                    });
                }
            }
            ImportInput::BrowsePicked { path, generation } => {
                if generation != self.generation {
                    return ImportStep::empty();
                }
                self.data_path = path;
                self.path_found = true;
                self.local_error.clear();
            }
            ImportInput::BrowseFailed { raw, generation } => {
                if generation != self.generation {
                    return ImportStep::empty();
                }
                let message = self.map_error(&raw, strings);
                self.replace_error(message, &mut events);
            }
            ImportInput::PickImportFile => {
                self.local_error.clear();
                self.submitting = true;
                effects.push(ImportEffect::PickImportFile);
            }
            ImportInput::FilePicked(file) => {
                self.submitting = false;
                if let Some(file) = file {
                    self.selected_file = Some(file);
                }
            }
            ImportInput::FilePickFailed(raw) => {
                self.submitting = false;
                let message = self.map_error(&raw, strings);
                self.replace_error(message, &mut events);
            }
            ImportInput::Submit => {
                self.local_error.clear();
                if self.app_import()
                    && self.data_path.trim().is_empty()
                    && !self.path_found
                {
                    self.local_error = strings.path_required.clone();
                    events.push(ImportEvent::Error(strings.path_required.clone()));
                    return ImportStep { effects, events, outcome: None };
                }
                self.submitting = true;
                if self.app_import() {
                    effects.push(ImportEffect::ImportFromApp {
                        source: self.source,
                        password: self.password.clone(),
                        data_path: self.data_path.clone(),
                    });
                } else {
                    match self.selected_file.clone() {
                        None => effects.push(ImportEffect::PickImportFile),
                        Some(file) => {
                            if self.import_password_policy() == Some(PasswordPolicy::Required)
                                && self.password.trim().is_empty()
                            {
                                self.submitting = false;
                                self.local_error = strings.password_required.clone();
                                events.push(ImportEvent::Error(strings.password_required.clone()));
                            } else {
                                effects.push(ImportEffect::ImportAtPath {
                                    password: self.password.clone(),
                                    file,
                                });
                            }
                        }
                    }
                }
            }
            ImportInput::ImportOk(result) => {
                // React only clears `submitting` in `finally`; the password and
                // selected file survive until `enabled` goes false.
                self.submitting = false;
                events.push(ImportEvent::Success(result.clone()));
            }
            ImportInput::ImportFailed(raw) => {
                self.submitting = false;
                let message = self.map_error(&raw, strings);
                let mut step_events = Vec::new();
                self.replace_error(message, &mut step_events);
                events.extend(step_events);
            }
        }
        ImportStep { effects, events, outcome: None }
    }
}

/// Work the host performs (the injected `connectionCommands` calls).
#[allow(dead_code)] // emitted work; remove with the import view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImportEffect {
    DetectPath {
        source: ImportSource,
        generation: u64,
    },
    BrowsePath {
        kind: BrowseKind,
        source: ImportSource,
        generation: u64,
    },
    PickImportFile,
    ImportFromApp {
        source: ImportSource,
        password: String,
        data_path: String,
    },
    ImportAtPath {
        password: String,
        file: String,
    },
}

/// Callbacks React wires via `onImportSuccess` / `onError`.
#[allow(dead_code)] // events; remove with the import view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImportEvent {
    Success(ImportResult),
    Error(String),
}

#[allow(dead_code)] // step result; remove with the import view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportStep {
    pub(crate) effects: Vec<ImportEffect>,
    pub(crate) events: Vec<ImportEvent>,
    pub(crate) outcome: Option<()>,
}

impl ImportStep {
    fn empty() -> Self {
        Self {
            effects: Vec::new(),
            events: Vec::new(),
            outcome: None,
        }
    }
}

#[allow(dead_code)] // input; remove with the import view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImportInput {
    Enabled(bool),
    Detected {
        path: String,
        found: bool,
        generation: u64,
    },
    DetectFailed {
        generation: u64,
    },
    BrowsePath(BrowseKind),
    BrowsePicked {
        path: String,
        generation: u64,
    },
    BrowseFailed {
        raw: String,
        generation: u64,
    },
    PickImportFile,
    FilePicked(Option<String>),
    FilePickFailed(String),
    Submit,
    ImportOk(ImportResult),
    ImportFailed(String),
}
