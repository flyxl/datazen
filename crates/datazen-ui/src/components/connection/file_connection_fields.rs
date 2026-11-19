//! Connection fields for the file form variant, ported 1:1 from
//! `packages/frontend/src/components/connection/FileConnectionFields.tsx`.
//!
//! React owns eleven `useState` values and three async `adb` commands. Rune has
//! no hooks and no IPC in a component, so the values live in an explicit
//! [`FileFieldsState`] owned by the host, presses become [`FileFieldsAction`]s,
//! and answers come back as [`FileFieldsEvent`]s through
//! [`FileFieldsState::apply`]. [`FileFieldsPort`] is the injected host port for
//! the native path picker and the three ADB calls — no fake IPC, no threads.
//!
//! ## Local mirrors (removal stages)
//!
//! `AdbPackage` / `AdbDatabaseFile` come from React `commands/adb.ts`; the
//! kernel has no Rust DTO for them. Remove when that command layer is ported.
//! `FileFieldsPort` is only needed until the connection form host wires it.
//!
//! ## Keys
//!
//! The file path keeps T103's `new-conn-db-file-path` (and `-error`) keys so
//! replacing its stand-in is invisible to existing tests. `PathInput` adds
//! `-input` / `-browse`. ADB controls use the same `new-conn-adb-*` prefix.
//! All strings come from the locale pack; no key is invented.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, Tx};

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::label::Label;
use crate::ui::path_input::PathInput;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// `AdbPackage` from React `commands/adb.ts`.
#[allow(dead_code)] // DTO mirror; remove when the adb command layer is ported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdbPackage {
    pub package_name: String,
}

/// `AdbDatabaseFile` from React `commands/adb.ts`.
#[allow(dead_code)] // DTO mirror; remove when the adb command layer is ported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdbDatabaseFile {
    pub path: String,
    pub name: String,
}

/// Which select listbox is open (Rune's select needs its open state from us).
#[allow(dead_code)] // VM surface; consumed by build and the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileSelect {
    Package,
    Database,
}

/// All `useState` values of the React component, owned by the host.
#[allow(dead_code)] // VM surface; owned by the connection form host.
#[derive(Clone, Debug, Default)]
pub(crate) struct FileFieldsState {
    pub adb_mode: bool,
    pub packages: Vec<AdbPackage>,
    pub databases: Vec<AdbDatabaseFile>,
    pub selected_package: String,
    pub selected_db_path: String,
    pub package_filter: TextEdit,
    pub loading_packages: bool,
    pub loading_databases: bool,
    pub pulling: bool,
    pub error: String,
    pub success: String,
    pub open_select: Option<FileSelect>,
}

/// Presses the component asks its owner to perform.
#[allow(dead_code)] // VM surface; handled by the host via handle_action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileFieldsAction {
    PickPath,
    ToggleAdb,
    ToggleSelect(Option<FileSelect>),
    SelectPackage(String),
    SelectDatabase(String),
    Pull,
}

/// Async answers the host feeds back into [`FileFieldsState::apply`].
#[allow(dead_code)] // VM surface; delivered by the host port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileFieldsEvent {
    PackagesLoaded(Result<Vec<AdbPackage>, String>),
    DatabasesLoaded(Result<Vec<AdbDatabaseFile>, String>),
    DatabasePulled(Result<Option<String>, String>),
    PathPicked(Option<String>),
}

/// What the surrounding form must do with an answer.
#[allow(dead_code)] // VM surface; applied by the host to `form.database`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileFieldsOutput {
    SetDatabase(String),
}

/// Injected host port: native path picker plus the three ADB commands.
#[allow(dead_code)] // host port; remove when the connection form host wires it.
pub(crate) trait FileFieldsPort {
    fn pick_path(&mut self);
    fn list_packages(&mut self);
    fn list_databases(&mut self, package: &str);
    fn pull_database(&mut self, package: &str, path: &str);
}

impl FileFieldsState {
    /// React's package filter: case-insensitive substring, soft (no hard drop
    /// of the current choice).
    pub(crate) fn filtered_packages(&self) -> Vec<&AdbPackage> {
        if self.package_filter.text.is_empty() {
            return self.packages.iter().collect();
        }
        let needle = self.package_filter.text.to_ascii_lowercase();
        self.packages
            .iter()
            .filter(|package| package.package_name.to_ascii_lowercase().contains(&needle))
            .collect()
    }

    /// React's pull-button *visibility*: `selectedPackage && selectedDbPath`.
    /// `pulling` must not unmount the button — it only disables it (D1).
    pub(crate) fn has_selection(&self) -> bool {
        !self.selected_package.is_empty() && !self.selected_db_path.is_empty()
    }

    /// Whether a pull press should be staged (in-flight pulls are ignored).
    pub(crate) fn can_pull(&self) -> bool {
        self.has_selection() && !self.pulling
    }

    /// Translate a press into state changes plus a host-port call.
    #[allow(dead_code)] // consumer: connection form host wiring (not landed); remove when wired.
    pub(crate) fn handle_action(&mut self, action: FileFieldsAction, port: &mut dyn FileFieldsPort) {
        match action {
            FileFieldsAction::PickPath => port.pick_path(),
            FileFieldsAction::ToggleAdb => {
                let next = !self.adb_mode;
                self.adb_mode = next;
                if next && self.packages.is_empty() {
                    self.loading_packages = true;
                    self.error.clear();
                    port.list_packages();
                }
            }
            FileFieldsAction::ToggleSelect(which) => self.open_select = which,
            FileFieldsAction::SelectPackage(package) => {
                self.selected_package = package.clone();
                self.selected_db_path.clear();
                self.databases.clear();
                self.open_select = None;
                if package.is_empty() {
                    return;
                }
                self.loading_databases = true;
                self.error.clear();
                port.list_databases(&package);
            }
            FileFieldsAction::SelectDatabase(path) => {
                self.selected_db_path = path;
                self.open_select = None;
            }
            FileFieldsAction::Pull => {
                if !self.can_pull() {
                    return;
                }
                self.pulling = true;
                self.error.clear();
                self.success.clear();
                port.pull_database(&self.selected_package, &self.selected_db_path);
            }
        }
    }

    /// Apply one host answer and return the change the form must make.
    #[allow(dead_code)] // consumer: adb command layer / form host (not landed); remove when wired.
    pub(crate) fn apply(&mut self, event: FileFieldsEvent) -> Vec<FileFieldsOutput> {
        match event {
            FileFieldsEvent::PackagesLoaded(result) => {
                self.loading_packages = false;
                match result {
                    Ok(packages) => {
                        self.packages = packages;
                        if self.packages.is_empty() {
                            self.error = t("newConn.adbNoPackages").to_owned();
                        }
                    }
                    Err(message) => self.error = message,
                }
                Vec::new()
            }
            FileFieldsEvent::DatabasesLoaded(result) => {
                self.loading_databases = false;
                match result {
                    Ok(databases) => {
                        self.databases = databases;
                        if self.databases.is_empty() {
                            self.error = t("newConn.adbNoDatabases").to_owned();
                        }
                    }
                    Err(message) => self.error = message,
                }
                Vec::new()
            }
            FileFieldsEvent::DatabasePulled(result) => {
                self.pulling = false;
                match result {
                    Ok(Some(saved)) => {
                        self.success = t("newConn.adbPullSuccess").to_owned();
                        vec![FileFieldsOutput::SetDatabase(saved)]
                    }
                    Ok(None) => Vec::new(),
                    Err(message) => {
                        self.error = format!("{}: {}", t("newConn.adbPullFailed"), message);
                        Vec::new()
                    }
                }
            }
            FileFieldsEvent::PathPicked(path) => path
                .map(|path| vec![FileFieldsOutput::SetDatabase(path)])
                .unwrap_or_default(),
        }
    }
}

type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;
type ActionFn<S> = Rc<dyn Fn(&mut S, FileFieldsAction) + 'static>;

/// Builder for the field group. `S` owns the two text edits and the host port.
#[allow(dead_code)] // consumer: ConnectionFormBody (wired by T193).
pub(crate) struct FileConnectionFields<'a, S: 'static> {
    state: &'a FileFieldsState,
    database: &'a TextEdit,
    database_error: &'a str,
    filter_edit: Option<EditFn<S>>,
    database_edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // consumer: ConnectionFormBody (wired by T193).
impl<'a, S: 'static> FileConnectionFields<'a, S> {
    pub(crate) fn new(
        state: &'a FileFieldsState,
        database: &'a TextEdit,
        database_error: &'a str,
    ) -> Self {
        Self {
            state,
            database,
            database_error,
            filter_edit: None,
            database_edit: None,
            on_action: None,
        }
    }

    pub(crate) fn filter_edit(
        mut self,
        edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.filter_edit = Some(Rc::new(edit));
        self
    }

    pub(crate) fn database_edit(
        mut self,
        edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.database_edit = Some(Rc::new(edit));
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, FileFieldsAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let state = self.state;
        let database = self.database;
        let database_error = self.database_error;
        let filter_edit = self.filter_edit.clone();
        let database_edit = self.database_edit.clone();
        let on_action = self.on_action.clone();
        let act = |action: FileFieldsAction| {
            let callback = on_action.clone();
            move |s: &mut S| {
                if let Some(cb) = &callback {
                    cb(s, action.clone());
                }
            }
        };

        let mut path = PathInput::new("new-conn-db-file-path", database)
            .placeholder("/path/to/db.sqlite")
            .error(!database_error.is_empty())
            .label(t("newConn.dbFilePath"))
            .on_browse(act(FileFieldsAction::PickPath));
        if let Some(edit) = database_edit {
            path = path.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
        }
        let mut path_block = col()
            .key("new-conn-db-file-path-block")
            .w_full()
            .gap(4.0)
            .child(
                Label::new("new-conn-db-file-path-label", t("newConn.dbFilePath"))
                    .required(true)
                    .build(dark),
            )
            .child(path.build(dark));
        if !database_error.is_empty() {
            path_block = path_block.child(
                text(database_error)
                    .key("new-conn-db-file-path-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        }

        let mut root = col()
            .key("new-conn-file-fields")
            .w_full()
            .gap(12.0)
            .child(path_block)
            .child(
                Checkbox::new("new-conn-adb-mode", state.adb_mode)
                    .label(t("newConn.adbMode"))
                    .on_toggle(act(FileFieldsAction::ToggleAdb))
                    .build(&k),
            );

        if !state.adb_mode {
            return root;
        }
        root = root.child(
            col()
                .key("new-conn-adb-panel")
                .w_full()
                .gap(12.0)
                .child(build_package_section(state, filter_edit.as_ref(), &on_action, dark, layer))
                .child(build_database_section(state, &on_action, dark, layer)),
        );
        if state.has_selection() {
            let label = if state.pulling {
                t("newConn.adbPulling")
            } else {
                t("newConn.adbPull")
            };
            root = root.child(
                rune_kit::Button::new(label)
                    .variant(rune_kit::ButtonVariant::Contrast)
                    .disabled(state.pulling)
                    .build(&k)
                    .key("new-conn-adb-pull")
                    .on_press(act(FileFieldsAction::Pull)),
            );
        }
        if !state.error.is_empty() {
            root = root.child(
                text(state.error.clone())
                    .key("new-conn-adb-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        }
        if !state.success.is_empty() {
            root = root.child(
                text(state.success.clone())
                    .key("new-conn-adb-success")
                    .font(k.ty.text_xsmall)
                    .color(k.c.success.fg)
                    .wrap(),
            );
        }
        root
    }
}

fn build_package_section<S: 'static>(
    state: &FileFieldsState,
    filter_edit: Option<&EditFn<S>>,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let filter_value = TextEdit::new(state.package_filter.text.clone());
    let mut filter = TextField::new("new-conn-adb-package-filter", &filter_value)
        .height(32.0)
        .pad_x(10.0)
        .radius(k.radius.sm)
        .placeholder(t("newConn.adbSearchPackage"));
    if let Some(edit) = filter_edit {
        let edit = edit.clone();
        filter = filter.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
    }
    let mut section = col()
        .key("new-conn-adb-package-section")
        .w_full()
        .gap(4.0)
        .child(Label::new("new-conn-adb-package-label", t("newConn.adbSelectPackage")).build(dark))
        .child(filter.build(dark));
    if state.loading_packages {
        return section.child(
            text(t("newConn.adbLoadingPackages"))
                .key("new-conn-adb-loading-packages")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    let packages = state.filtered_packages();
    let labels: Vec<String> = packages.iter().map(|p| p.package_name.clone()).collect();
    let values: Vec<String> = packages.iter().map(|p| p.package_name.clone()).collect();
    let callback = on_action.clone();
    let mut select = Select::new("new-conn-adb-package", &labels)
        .values(&values)
        .current(Some(state.selected_package.as_str()))
        .open(state.open_select == Some(FileSelect::Package))
        .on_toggle(move |s: &mut S| {
            if let Some(cb) = &callback {
                cb(s, FileFieldsAction::ToggleSelect(Some(FileSelect::Package)));
            }
        });
    let callback = on_action.clone();
    select = select.on_close(move |s: &mut S| {
        if let Some(cb) = &callback {
            cb(s, FileFieldsAction::ToggleSelect(None));
        }
    });
    let callback = on_action.clone();
    select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
        if let Some(cb) = &callback {
            cb(s, FileFieldsAction::SelectPackage(value.to_owned()));
            cb(s, FileFieldsAction::ToggleSelect(None));
        }
    });
    section.child(select.build(&k, layer))
}

fn build_database_section<S: 'static>(
    state: &FileFieldsState,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    if state.selected_package.is_empty() {
        return row().key("new-conn-adb-database-section").h(0.0);
    }
    let mut section = col()
        .key("new-conn-adb-database-section")
        .w_full()
        .gap(4.0)
        .child(Label::new("new-conn-adb-database-label", t("newConn.adbSelectDatabase")).build(dark));
    if state.loading_databases {
        return section.child(
            text(t("newConn.adbLoadingDatabases"))
                .key("new-conn-adb-loading-databases")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    let labels: Vec<String> = state
        .databases
        .iter()
        .map(|db| format!("{} ({})", db.name, db.path))
        .collect();
    let values: Vec<String> = state.databases.iter().map(|db| db.path.clone()).collect();
    let callback = on_action.clone();
    let mut select = Select::new("new-conn-adb-database", &labels)
        .values(&values)
        .current(Some(state.selected_db_path.as_str()))
        .open(state.open_select == Some(FileSelect::Database))
        .on_toggle(move |s: &mut S| {
            if let Some(cb) = &callback {
                cb(s, FileFieldsAction::ToggleSelect(Some(FileSelect::Database)));
            }
        });
    let callback = on_action.clone();
    select = select.on_close(move |s: &mut S| {
        if let Some(cb) = &callback {
            cb(s, FileFieldsAction::ToggleSelect(None));
        }
    });
    let callback = on_action.clone();
    select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
        if let Some(cb) = &callback {
            cb(s, FileFieldsAction::SelectDatabase(value.to_owned()));
            cb(s, FileFieldsAction::ToggleSelect(None));
        }
    });
    section.child(select.build(&k, layer))
}
