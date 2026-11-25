//! `ImportDialog`, ported 1:1 from
//! `packages/frontend/src/windows/connection/ImportDialog.tsx` (DBR-01).
//!
//! React is a wizard with five surfaces in one dialog: file/format options,
//! target table + column mapping, a preview with diagnostics, conflict/commit
//! settings, and a batched execution report. The Rune port keeps that whole
//! structure as a host-owned [`ImportDialogVm`] and emits [`ImportAction`]s;
//! the host picks files, parses them, loads target columns and runs the import
//! through its own ports, then feeds the results back into the VM. No fake
//! IPC, no threads, no giant SQL string built here.
//!
//! ## Reuse / removal stages
//!
//! * `lib/importMapping.ts` is ported verbatim in [`mapping`] (pure) and stays.
//! * `lib/importData.ts` / `lib/importExecution.ts` and the IPC commands have
//!   no Rune port: the host parses and executes, feeding [`ImportParseResult`]
//!   / [`ImportRunResult`] into the VM. Remove that boundary when those ports
//!   land and the dialog can own its pipeline.
//! * React's `AlertTriangle` / `FileText` glyphs are not in the Rune icon
//!   registry. Warning banners keep their text, keys and palette tokens; the
//!   file button reuses the registry's table glyph. Swap them when the shared
//!   icon task adds those two glyphs.
//!
//! ## Keys
//!
//! T223's pinned slot keys stay intact: `content-view-import-dialog` with
//! `-title`, `-body`, `-close`, `-cancel`, `-confirm`, plus React's
//! `import-mapping-table`, `import-summary`, `import-conflict-warning`,
//! `import-duplicate-warning`, `import-preview`, `import-progress`,
//! `import-result`. Added keys are prefixed `import-` and documented at each
//! builder. All copy uses exact locale-pack keys; no new key is invented.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::ICON_LOADER2;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

mod mapping;
mod preview;
mod steps;

pub use mapping::{mapping_is_executable, plan_import_mapping};

/// React `PREVIEW_ROWS` / `SAMPLE_ROWS`.
pub const PREVIEW_ROWS: usize = 100;
pub const SAMPLE_ROWS: usize = 5;
/// React `max-w-3xl` (768pt), kept a little narrower for the 960pt window.
pub const DIALOG_WIDTH: f32 = 720.0;
pub const ROOT: &str = "content-view-import-dialog";

/// React `detectImportFormat`: the file extension drives the format cluster.
#[allow(dead_code)] // VM helper; consumed by the host feed and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportFormat {
    #[default]
    Csv,
    Json,
    Xlsx,
}

impl ImportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Xlsx => "xlsx",
        }
    }

    pub fn detect(name: &str) -> Option<Self> {
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let ext = base.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase())?;
        match ext.as_str() {
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "xlsx" => Some(Self::Xlsx),
            _ => None,
        }
    }
}

/// One parsed cell. React stores `unknown`; the renderer only distinguishes
/// null (italic `NULL`), JSON object/array (`JSON.stringify`) and scalar text.
#[allow(dead_code)] // VM mirror; consumed by the preview and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellValue {
    Null,
    Text(String),
    Json(String),
}

impl CellValue {
    pub fn display(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            Self::Text(value) | Self::Json(value) => value.clone(),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// React `ImportDiagnostic` (only the two kinds the wizard renders).
#[allow(dead_code)] // VM mirror; consumed by the warnings and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDiagnostic {
    pub kind: DiagnosticKind,
    pub column: String,
    pub line: usize,
}

#[allow(dead_code)] // VM mirror; consumed by the warnings and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    DuplicateColumn,
    RaggedRow,
}

/// React `ImportParseResult` plus XLSX `sheetNames`. Rows are stored in column
/// order so a cell is `rows[r][c]` → `columns[c]`; the host feeds them that way.
#[allow(dead_code)] // VM surface; supplied by the host parser.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportParseResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<CellValue>>,
    pub row_origins: Vec<usize>,
    pub diagnostics: Vec<ImportDiagnostic>,
    pub sheet_names: Vec<String>,
}

impl ImportParseResult {
    /// React `parsed.rows.slice(0, SAMPLE_ROWS).map(row[source]).find(non-null)`.
    pub fn sample(&self, column: usize) -> String {
        self.rows
            .iter()
            .take(SAMPLE_ROWS)
            .filter_map(|row| row.get(column))
            .find(|cell| !cell.is_null())
            .map(|cell| cell.display())
            .unwrap_or_else(|| "—".to_owned())
    }
}

/// React `TargetColumnsState`; `error` is empty when there is no error.
#[allow(dead_code)] // VM surface; supplied by the host schema feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetColumnsState {
    pub columns: Option<Vec<TargetColumn>>,
    pub loading: bool,
    pub error: String,
}

#[allow(dead_code)] // VM mirror; consumed by the mapping table and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetColumn {
    pub name: String,
    pub data_type: String,
}

/// React `ImportMappingMode`.
#[allow(dead_code)] // VM surface; consumed by the planner and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MappingMode {
    #[default]
    Auto,
    Name,
    Position,
}

#[allow(dead_code)] // VM mirror; consumed by the mapping table and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingEntry {
    pub source: String,
    pub target: Option<String>,
}

#[allow(dead_code)] // VM mirror; consumed by the conflict warning and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingConflict {
    pub target: String,
    pub sources: Vec<String>,
}

#[allow(dead_code)] // VM mirror; consumed by the summary and the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MappingPlan {
    pub entries: Vec<MappingEntry>,
    pub conflicts: Vec<MappingConflict>,
    pub unmapped: Vec<String>,
    pub untargeted: Vec<String>,
}

impl MappingPlan {
    pub fn mapped_count(&self) -> usize {
        self.entries.iter().filter(|e| e.target.is_some()).count()
    }

    pub fn skipped_count(&self) -> usize {
        self.entries.len() - self.mapped_count()
    }
}

/// A target-table option: React's `{ value, label }`, where PG families show
/// `schema.name` in the label.
#[allow(dead_code)] // VM mirror; consumed by the target select and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableOption {
    pub value: String,
    pub label: String,
}

#[allow(dead_code)] // VM mirror; consumed by CSV options and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Delimiter {
    #[default]
    Auto,
    Comma,
    Semicolon,
    Tab,
}

#[allow(dead_code)] // VM mirror; consumed by CSV options and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmptyUnquotedAs {
    #[default]
    Null,
    Empty,
}

#[allow(dead_code)] // VM mirror; consumed by execution settings and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnError {
    #[default]
    Stop,
    Skip,
}

#[allow(dead_code)] // VM mirror; consumed by execution settings and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitMode {
    #[default]
    Transaction,
    Autocommit,
}

/// React `WizardPhase`.
#[allow(dead_code)] // VM surface; consumed by the footer and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WizardPhase {
    #[default]
    Config,
    Running,
    Done,
}

#[allow(dead_code)] // VM mirror; consumed by the file row and the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadedFile {
    pub name: String,
    pub data_base64: String,
}

#[allow(dead_code)] // VM mirror; consumed by the progress banner and the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportProgressMsg {
    pub processed_rows: usize,
    pub committed_rows: usize,
    pub failed_rows: usize,
}

#[allow(dead_code)] // VM mirror; consumed by the result table and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowFailure {
    pub row_index: usize,
    pub source_line: Option<usize>,
    pub message: String,
}

#[allow(dead_code)] // VM mirror; consumed by the stopped result and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopInfo {
    pub row_index: usize,
    pub source_line: Option<usize>,
    pub message: String,
}

#[allow(dead_code)] // VM mirror; consumed by the result and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunStatus {
    #[default]
    Completed,
    Stopped,
    Cancelled,
}

#[allow(dead_code)] // VM mirror; consumed by the result section and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRunResult {
    pub status: RunStatus,
    pub committed_rows: usize,
    pub failed_rows: Vec<RowFailure>,
    pub stopped_by: Option<StopInfo>,
}

/// Which Select currently owns the popup. React keeps that state inside each
/// `Select`; Rune needs it from the caller, so the host stores one open id.
#[allow(dead_code)] // VM surface; consumed by the selects and the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OpenSelect {
    #[default]
    None,
    Encoding,
    Delimiter,
    Quote,
    Empty,
    Sheet,
    TargetTable,
    MatchMode,
    Override(String),
}

/// The full view snapshot the host fills each frame.
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportDialogVm {
    pub file: Option<LoadedFile>,
    pub encoding: String,
    pub delimiter: Delimiter,
    pub quote_char: String,
    pub first_row_is_header: bool,
    pub empty_unquoted_as: EmptyUnquotedAs,
    pub sheet_name: Option<String>,
    pub parsed: Option<ImportParseResult>,
    pub parse_error_message: String,
    pub local_error: String,
    pub targets: TargetColumnsState,
    pub target_table: String,
    /// Bound to the fallback text input when the host has no table options.
    pub target_table_edit: TextEdit,
    pub table_options: Vec<TableOption>,
    pub mapping_mode: MappingMode,
    pub overrides: Vec<(String, Option<String>)>,
    pub on_error: OnError,
    pub commit_mode: CommitMode,
    pub phase: WizardPhase,
    pub progress: Option<ImportProgressMsg>,
    pub result: Option<ImportRunResult>,
    pub refresh_failed: bool,
    pub open_select: OpenSelect,
}

impl Default for ImportDialogVm {
    fn default() -> Self {
        Self {
            file: None,
            encoding: "utf-8".to_owned(),
            delimiter: Delimiter::Auto,
            quote_char: "\"".to_owned(),
            first_row_is_header: true,
            empty_unquoted_as: EmptyUnquotedAs::Null,
            sheet_name: None,
            parsed: None,
            parse_error_message: String::new(),
            local_error: String::new(),
            targets: TargetColumnsState::default(),
            target_table: String::new(),
            target_table_edit: TextEdit::new(String::new()),
            table_options: Vec::new(),
            mapping_mode: MappingMode::Auto,
            overrides: Vec::new(),
            on_error: OnError::Stop,
            commit_mode: CommitMode::Transaction,
            phase: WizardPhase::Config,
            progress: None,
            result: None,
            refresh_failed: false,
            open_select: OpenSelect::None,
        }
    }
}

impl ImportDialogVm {
    /// React `detectImportFormat(file.name)`.
    pub fn format(&self) -> Option<ImportFormat> {
        self.file.as_ref().and_then(|file| ImportFormat::detect(&file.name))
    }

    /// React `duplicates` / `ragged` diagnostics.
    pub fn duplicates(&self) -> Vec<&ImportDiagnostic> {
        self.parsed
            .iter()
            .flat_map(|parsed| parsed.diagnostics.iter())
            .filter(|d| d.kind == DiagnosticKind::DuplicateColumn)
            .collect()
    }

    pub fn ragged(&self) -> Vec<&ImportDiagnostic> {
        self.parsed
            .iter()
            .flat_map(|parsed| parsed.diagnostics.iter())
            .filter(|d| d.kind == DiagnosticKind::RaggedRow)
            .collect()
    }

    /// A duplicate source column is a hard stop: mapping is keyed by name.
    pub fn blocked_by_duplicates(&self) -> bool {
        !self.duplicates().is_empty()
    }

    /// React `planImportMapping` memo. The host supplies parsed/target data.
    pub fn mapping_plan(&self) -> Option<MappingPlan> {
        let parsed = self.parsed.as_ref()?;
        if parsed.columns.is_empty() {
            return None;
        }
        let targets = self.targets.columns.as_ref()?;
        let target_names: Vec<String> = targets.iter().map(|column| column.name.clone()).collect();
        Some(plan_import_mapping(
            &parsed.columns,
            &target_names,
            self.mapping_mode,
            &self.overrides,
        ))
    }

    /// React `mappedPairs` (source, target) for every executable pair.
    #[allow(dead_code)] // host feed; consumed when the import pipeline is wired.
    pub fn mapped_pairs(&self) -> Vec<(String, String)> {
        self.mapping_plan()
            .map(|plan| {
                plan.entries
                    .into_iter()
                    .filter_map(|entry| entry.target.map(|target| (entry.source, target)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether this source has an override; `None` inside means "don't import".
    pub fn override_target(&self, source: &str) -> Option<Option<String>> {
        self.overrides
            .iter()
            .find(|(name, _)| name == source)
            .map(|(_, target)| target.clone())
    }

    /// React `canStart` — the single gate for the primary footer button.
    pub fn can_start(&self) -> bool {
        let plan = self.mapping_plan();
        self.phase == WizardPhase::Config
            && self.parsed.as_ref().is_some_and(|parsed| !parsed.rows.is_empty())
            && self.parse_error_message.is_empty()
            && !self.blocked_by_duplicates()
            && self.targets.columns.is_some()
            && self.targets.error.is_empty()
            && plan.as_ref().is_some_and(|plan| mapping_is_executable(plan))
    }
}

/// Discrete interactions the dialog asks the host to apply to `S`. The host
/// owns every command (file open, schema read, import run) — none is faked.
#[allow(dead_code)] // host port; remove when the host owns the full pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportAction {
    Close,
    PickFile,
    SetEncoding(String),
    SetDelimiter(Delimiter),
    SetQuoteChar(String),
    SetFirstRowIsHeader(bool),
    SetEmptyUnquotedAs(EmptyUnquotedAs),
    SetSheetName(String),
    SetTargetTable(String),
    SetMappingMode(MappingMode),
    SetOverride { source: String, target: Option<String> },
    SetOnError(OnError),
    SetCommitMode(CommitMode),
    Start,
    CancelImport,
    ResetForAnother,
    /// Rune-only: the host owns each Select's open state.
    SetOpenSelect(OpenSelect),
}

pub type ImportActionFn<S> = Rc<dyn Fn(&mut S, ImportAction) + 'static>;
/// Accessor for the host-owned VM (needed by the fallback target name input).
pub type ImportVmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ImportDialogVm + 'static>;

fn emit<S: 'static>(on_action: &Option<ImportActionFn<S>>, action: ImportAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build one of the wizard's selects. Rune's Select needs the open state and
/// an option label/value pair from the caller; the kit matches `current`
/// against labels, so the matching label is derived from the wire value.
#[allow(dead_code)] // rendering helper; delete with the host wiring.
pub fn make_select<S: 'static>(
    key: &str,
    labels: &[String],
    values: &[String],
    current: &str,
    label: &str,
    width: f32,
    open: bool,
    open_kind: OpenSelect,
    on_action: &Option<ImportActionFn<S>>,
    choose: impl Fn(&str) -> ImportAction + Clone + 'static,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let current_label = values
        .iter()
        .position(|value| value == current)
        .and_then(|index| labels.get(index))
        .cloned()
        .unwrap_or_else(|| current.to_owned());
    let mut select = crate::ui::select::Select::new(key, labels)
        .values(values)
        .current(Some(current_label.as_str()))
        .label(label)
        .width(width)
        .open(open);
    if let Some(callback) = on_action.clone() {
        let toggle = callback.clone();
        let kind = open_kind.clone();
        select = select.on_toggle(move |s: &mut S| {
            toggle(
                s,
                ImportAction::SetOpenSelect(if open { OpenSelect::None } else { kind.clone() }),
            )
        });
        let close = callback.clone();
        select = select.on_close(move |s: &mut S| {
            close(s, ImportAction::SetOpenSelect(OpenSelect::None))
        });
        let chosen = callback;
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
            chosen(s, choose(value));
            chosen(s, ImportAction::SetOpenSelect(OpenSelect::None));
        });
    }
    select.build(kit(dark), layer)
}

/// Render the import dialog into `layer`. The Dialog base supplies the
/// full-window scrim/layer (T223 pattern), header and scrolling body.
#[allow(dead_code)] // consumer: content_view_dialogs slot (host wiring not landed).
pub fn build<S: 'static>(
    vm: &ImportDialogVm,
    on_action: Option<ImportActionFn<S>>,
    vm_edit: Option<ImportVmFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    // React passes a no-op onClose while running: the modal cannot be dismissed
    // mid-flight. Rune needs a close handler to mount, so it simply emits no
    // action in that phase. Copy the phase so the handler is 'static.
    let phase = vm.phase;
    let dismiss = {
        let on_action = on_action.clone();
        move |s: &mut S| {
            if phase != WizardPhase::Running {
                if let Some(callback) = &on_action {
                    callback(s, ImportAction::Close);
                }
            }
        }
    };

    let row_count = vm.parsed.as_ref().map_or(0, |parsed| parsed.rows.len());
    let plan_for_footer = vm.mapping_plan();
    let needs_mapping = !vm.can_start()
        && row_count > 0
        && plan_for_footer
            .as_ref()
            .is_some_and(|plan| !mapping_is_executable(plan));

    let mut body = col()
        .key(format!("{ROOT}-wizard"))
        .w_full()
        .min_w(0.0)
        .gap(16.0)
        .child(steps::build_file_and_format(vm, &on_action, dark, layer));
    if needs_mapping {
        body = body.child(
            text(t("import.needsMapping"))
                .key(format!("{ROOT}-needs-mapping"))
                .a11y_hidden(),
        );
    }

    // React's two error channels: parse failure (with a file) and file-open
    // failure (without one). Both are copyable error rows with the same style.
    if !vm.parse_error_message.is_empty() {
        body = body.child(
            CopyableError::new(format!("{ROOT}-parse-error"), vm.parse_error_message.clone())
                .build::<S>(k),
        );
    }
    if vm.file.is_none() && !vm.local_error.is_empty() {
        body = body.child(
            CopyableError::new(format!("{ROOT}-pick-error"), vm.local_error.clone())
                .build::<S>(k),
        );
    }

    body = body
        .child(steps::build_target_and_mapping(vm, &on_action, &vm_edit, dark, layer))
        .child(preview::build_preview(vm, dark))
        .child(steps::build_execution(vm, &on_action, dark));
    if let Some(progress) = preview::build_progress(vm, dark) {
        body = body.child(progress);
    }
    if let Some(result) = preview::build_result::<S>(vm, dark) {
        body = body.child(result);
    }

    let mut dialog = Dialog::new(ROOT, t("common.importData"))
        .open(true)
        .description(t("import.description"))
        .fit()
        .width(DIALOG_WIDTH)
        .body_max_height(440.0)
        .child(row().key(format!("{ROOT}-body")).w_full().min_w(0.0).child(body));

    // Footer mirrors React's three phases while keeping T223's stable keys.
    match vm.phase {
        WizardPhase::Running => {
            dialog = dialog
                .footer(
                    Button::new(t("import.cancelImport"))
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::ExtraLarge)
                        .on_press(emit(&on_action, ImportAction::CancelImport))
                        .build(k)
                        .key(format!("{ROOT}-cancel")),
                )
                .footer(
                    Button::new(t("import.importing"))
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::ExtraLarge)
                        .icon(&ICON_LOADER2)
                        .disabled(true)
                        .build(k)
                        .key(format!("{ROOT}-confirm")),
                );
        }
        WizardPhase::Done => {
            dialog = dialog
                .footer(
                    Button::new(t("common.close"))
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::ExtraLarge)
                        .on_press(emit(&on_action, ImportAction::Close))
                        .build(k)
                        .key(format!("{ROOT}-cancel")),
                )
                .footer(
                    Button::new(t("import.importAnother"))
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::ExtraLarge)
                        .on_press(emit(&on_action, ImportAction::ResetForAnother))
                        .build(k)
                        .key(format!("{ROOT}-confirm")),
                );
        }
        WizardPhase::Config => {
            let label = if row_count > 0 {
                t("import.start").replace("{count}", &row_count.to_string())
            } else {
                t("import.import").to_owned()
            };
            let mut primary = Button::new(label)
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .disabled(!vm.can_start());
            if needs_mapping {
                primary = primary.help(t("import.needsMapping"));
            }
            dialog = dialog
                .footer(
                    Button::new(t("common.cancel"))
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::ExtraLarge)
                        .on_press(emit(&on_action, ImportAction::Close))
                        .build(k)
                        .key(format!("{ROOT}-cancel")),
                )
                .footer(primary.on_press(emit(&on_action, ImportAction::Start)).build(k).key(format!("{ROOT}-confirm")));
        }
    }
    dialog.on_close::<S>(dismiss).build(k, layer);
}
