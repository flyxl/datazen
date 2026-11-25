//! The natural-language filter input, ported from React
//! `packages/frontend/src/components/ai/NlFilterInput.tsx` (T299).
//!
//! The table toolbar's smart-filter composer: the collapsed sparkle
//! toggle (or the unconfigured notice), the expanded input row (the
//! sparkles glyph, the NL input with the AI keyboard map, the
//! parse/swap spinner, the clear `×`), and the four status lines
//! (the parse error banner, the validation warning, the no-filters
//! note, the parsed-count status).
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useState(expanded)` / `useState(validationError)` | the VM's `expanded` / `validation_error` cells (the one-open-slot trade) |
//! | `abortRef` + the `tableName` effect (42-47) | [`table_changed`] — a pure query the host applies (reset the abort cell on a table change) |
//! | the `tableName` clear effect (36-40) | [`table_changed`]'s clear half — the host clears the nl-filter slice, the validation error, and the expanded flag |
//! | `handleParse` (49-83) | [`NlFilterInput::on_parse`] — the host drives the store's parse seams (`begin_parse_filter` → `complete/fail_parse_filter`), then applies [`parsed_filter_outcome`] (the column validation and the set-filters write the port computes pure) |
//! | `useAiKeyboard(() => void handleParse())` (85) | the T124 port's [`AiKeyboardInput::on_key`] — Enter submits, Shift+Enter a newline, the composition respected |
//! | `handleClear` (87-91) | [`NlFilterInput::on_clear`] — the host clears the store's nl slice and the panel's filters |
//! | `openSettingsWindow('ai')` (103) | [`NlFilterInput::on_open_ai_settings`] — the ExplainPanel's seam precedent |
//! | `useTableDataStore((s) => …columns)` (34) | the VM's `columns` (the host copies the panel's slice) |
//! | the parse journey's context check (56-58) | [`parsed_filter_outcome`]'s `context` inputs — the same-table/same-session guard |
//! | `ErrorBanner` (176) | the kit banner's plain variant (the filter editor's own ruling: `text-danger` is the plain variant's own colour) |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (internal) | `nl-filter-input` |
//! | the unconfigured notice (internal) | `nl-filter-not-configured` |
//! | the collapsed toggle (React test id) | `smart-filter-toggle` |
//! | the input row (internal) | `nl-filter-input-row` |
//! | the NL input (internal) | `nl-filter-input-field` |
//! | the parse button (internal) | `nl-filter-input-parse` |
//! | the parsing spinner + word (internal) | `nl-filter-input-parsing` / `-spinner` |
//! | the clear button (internal) | `nl-filter-input-clear` |
//! | the parse error banner (internal) | `nl-filter-input-error` |
//! | the validation warning (internal) | `nl-filter-input-validation` |
//! | the no-filters note (internal) | `nl-filter-input-empty` |
//! | the parsed-count status (internal) | `nl-filter-input-parsed` |
//!
//! ## Documented deviations from React
//!
//! * **The parse journey is a two-phase seam.** The host runs the
//!   async parse (the store's `begin/complete/fail_parse_filter`),
//!   then hands the filters (or the failure) to
//!   [`parsed_filter_outcome`] — the pure half (the unknown-column
//!   validation, the context guard, the set-filters write) runs as a
//!   pure computation the host applies.
//! * **`onMouseDown={e => e.preventDefault()}`** (the toolbar's
//!   focus-keep) needs no counterpart: Rune's press does not move the
//!   focus the way the web's does.
//! * **The validation warning is a plain warning line**, `role=alert`
//!   — the React comment's own point: forcing it through the error
//!   component would mislabel it.

use std::collections::HashSet;
use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};
use serde_json::Value;

use crate::ai_store::FilterCondition;
use crate::hooks::use_ai_keyboard::AiKeyboardInput;
use crate::i18n::t;
use crate::icons::{ICON_SPARKLES, ICON_X};
use crate::table_data_connection_state::ColumnSchema;
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// The panel's data: the store slices the input reads (the host
/// copies them), the panel's columns, and the two local cells.
#[derive(Clone, Debug, Default)]
pub struct NlFilterInputVm {
    /// `isConfigured`.
    pub is_configured: bool,
    /// `nlFilterInput` — the composer's text.
    pub input: String,
    /// `isParsingFilter`.
    pub is_parsing: bool,
    /// `parsedFilters` — the last parse's outcome.
    pub parsed_filters: Option<Vec<FilterCondition>>,
    /// `nlFilterError`.
    pub parse_error: Option<String>,
    /// The panel's columns (`useTableDataStore((s) => …columns)`).
    pub columns: Vec<ColumnSchema>,
    /// `expanded` — the collapsed toggle's cell.
    pub expanded: bool,
    /// `validationError` — the unknown-column warning's cell.
    pub validation_error: Option<String>,
}

/// React 36-47, the two `tableName` effects as one pure query: a table
/// change clears the nl-filter slice, the validation error, and the
/// expanded flag, and re-arms the abort cell (the stale-parse guard's
/// own reset). The host applies it whenever the table changes.
pub fn table_changed() -> NlFilterReset {
    NlFilterReset {
        clear_nl_filter: true,
        clear_validation_error: true,
        collapse: true,
        rearm_abort: true,
    }
}

/// The reset the host applies on a table change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NlFilterReset {
    pub clear_nl_filter: bool,
    pub clear_validation_error: bool,
    pub collapse: bool,
    pub rearm_abort: bool,
}

/// React 60-82, the pure half of `handleParse`: what a landed parse
/// does with its filters — the unknown-column validation (only once
/// the table has reported a non-empty column list: an empty list must
/// not turn a valid filter into a false warning), then the set-filters
/// write. The `context` inputs are the stale-parse guard (56-58): a
/// parse landing on a panel that moved on is dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum ParsedFilterOutcome {
    /// The parse landed on a panel that moved on — dropped.
    Stale,
    /// The filters name unknown columns: the warning (the trim, the
    /// dedupe, the case-insensitive membership).
    UnknownColumns(Vec<String>),
    /// The filters land: the write the host performs.
    Apply(Vec<FilterCondition>),
}

pub fn parsed_filter_outcome(
    filters: &[FilterCondition],
    columns: &[ColumnSchema],
    context_table: Option<&str>,
    context_db_session_id: Option<&str>,
    target_table: &str,
    target_db_session_id: &str,
) -> ParsedFilterOutcome {
    // React 56-58: the panel moved on — the parse is dropped.
    if context_table != Some(target_table) || context_db_session_id != Some(target_db_session_id) {
        return ParsedFilterOutcome::Stale;
    }
    if filters.is_empty() {
        // React 60: an empty list writes nothing (the `filters &&
        // filters.length > 0` gate).
        return ParsedFilterOutcome::Apply(Vec::new());
    }
    // Metadata may still be loading when the user parses a filter.
    // Only reject fields once the table has reported a non-empty
    // column list; an empty list must not turn a valid filter into a
    // false warning.
    if !columns.is_empty() {
        let known: HashSet<String> = columns
            .iter()
            .map(|column| column.name.to_lowercase())
            .collect();
        // React's `new Set(...)` over the trimmed columns: case-
        // SENSITIVE dedup (`EMAIL` and `email` are two entries), the
        // first-seen order.
        let mut seen: HashSet<String> = HashSet::new();
        let mut unknown: Vec<String> = Vec::new();
        for filter in filters {
            let column = filter.column.trim();
            if !column.is_empty()
                && !known.contains(&column.to_lowercase())
                && seen.insert(column.to_owned())
            {
                unknown.push(column.to_owned());
            }
        }
        if !unknown.is_empty() {
            return ParsedFilterOutcome::UnknownColumns(unknown);
        }
    }
    ParsedFilterOutcome::Apply(filters.to_vec())
}

/// The unknown-columns warning sentence (React 74-75):
/// `smartFilter.invalidColumns` with `{columns}` replaced.
pub fn unknown_columns_message(unknown: &[String]) -> String {
    t("smartFilter.invalidColumns").replace("{columns}", &unknown.join(", "))
}

/// The NL filter input. `S` is the host state that owns the VM, the
/// input's `TextEdit` + keyboard map, and every action.
pub struct NlFilterInput<'a, S: 'static> {
    vm: &'a NlFilterInputVm,
    /// The input's edit buffer (the host-held `TextEdit`).
    input_edit: Option<&'a rune::TextEdit>,
    /// The AI keyboard map's state (the T124 port).
    keyboard: Option<&'a AiKeyboardInput>,
    dark: bool,
    on_input_edit: Option<Rc<dyn Fn(&mut S) -> &mut rune::TextEdit>>,
    on_key: Option<Rc<dyn Fn(&mut S, rune::Key, bool)>>,
    on_composition_start: Option<Rc<dyn Fn(&mut S)>>,
    on_composition_end: Option<Rc<dyn Fn(&mut S)>>,
    on_expand: Option<Rc<dyn Fn(&mut S)>>,
    on_parse: Option<Rc<dyn Fn(&mut S)>>,
    on_clear: Option<Rc<dyn Fn(&mut S)>>,
    on_open_ai_settings: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> NlFilterInput<'a, S> {
    /// The input over its VM.
    pub fn new(vm: &'a NlFilterInputVm) -> Self {
        Self {
            vm,
            input_edit: None,
            keyboard: None,
            dark: true,
            on_input_edit: None,
            on_key: None,
            on_composition_start: None,
            on_composition_end: None,
            on_expand: None,
            on_parse: None,
            on_clear: None,
            on_open_ai_settings: None,
        }
    }

    /// The input's edit buffer (the controlled value's host cell).
    pub fn input_edit(mut self, edit: &'a rune::TextEdit) -> Self {
        self.input_edit = Some(edit);
        self
    }

    /// The AI keyboard map's state.
    pub fn keyboard(mut self, keyboard: &'a AiKeyboardInput) -> Self {
        self.keyboard = Some(keyboard);
        self
    }

    /// The theme.
    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// The input's edit accessor (the `TextField::on_edit` contract).
    pub fn on_input_edit(
        mut self,
        f: impl Fn(&mut S) -> &mut rune::TextEdit + 'static,
    ) -> Self {
        self.on_input_edit = Some(Rc::new(f));
        self
    }

    /// The AI keyboard map's key feed (`on_key` — the Enter the
    /// keyboard map swallows reports the submit to the host).
    pub fn on_key(
        mut self,
        f: impl Fn(&mut S, rune::Key, bool) + 'static,
    ) -> Self {
        self.on_key = Some(Rc::new(f));
        self
    }

    /// `onCompositionStart` / `onCompositionEnd` (the keyboard map's
    /// own pair).
    pub fn on_composition_start(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_composition_start = Some(Rc::new(f));
        self
    }
    pub fn on_composition_end(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_composition_end = Some(Rc::new(f));
        self
    }

    /// The collapsed toggle's press (`setExpanded(true)`).
    pub fn on_expand(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_expand = Some(Rc::new(f));
        self
    }

    /// `handleParse` — the parse press (and the input's Enter through
    /// the keyboard map): the host drives the store's parse seams and
    /// applies [`parsed_filter_outcome`].
    pub fn on_parse(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_parse = Some(Rc::new(f));
        self
    }

    /// `handleClear` + collapse (159-166): the host clears the store's
    /// nl slice and the panel's filters, then the input collapses.
    pub fn on_clear(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Rc::new(f));
        self
    }

    /// `openSettingsWindow('ai')` — the unconfigured notice's press.
    pub fn on_open_ai_settings(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_ai_settings = Some(Rc::new(f));
        self
    }

    /// The input. The caller roots it once.
    pub fn build(self, k: &Kit) -> El {
        let vm = self.vm;
        // React 96-111: the unconfigured notice — the sparkle and the
        // pack word, the settings press.
        if !vm.is_configured {
            return self.not_configured(k);
        }
        // React 113-127: the collapsed toggle.
        if !vm.expanded {
            return self.collapsed_toggle(k);
        }
        // React 129-196: the expanded composer.
        let mut root = col()
            .key("nl-filter-input")
            .min_w(0.0)
            .grow(1.0)
            .gap(4.0)
            .child(self.input_row(k));
        // The parse error banner (the plain variant — the filter
        // editor's own text-danger ruling).
        if let Some(banner) = Banner::new("nl-filter-input-error", vm.parse_error.clone().unwrap_or_default())
            .variant(BannerVariant::Plain)
            .build(k)
        {
            root = root.child(banner);
        }
        // The validation warning — a plain warning line, `role=alert`.
        if let Some(warning) = vm.validation_error.clone() {
            root = root.child(
                text(warning)
                    .key("nl-filter-input-validation")
                    .role("alert")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .wrap()
                    .min_w(0.0),
            );
        }
        // The no-filters note.
        if let Some(filters) = &vm.parsed_filters {
            if filters.is_empty() {
                root = root.child(
                    text(t("smartFilter.noFilters"))
                        .key("nl-filter-input-empty")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
            }
        }
        // The parsed-count status.
        if let Some(filters) = &vm.parsed_filters {
            if !filters.is_empty() && vm.validation_error.is_none() {
                let word =
                    t("smartFilter.parsed").replace("{count}", &filters.len().to_string());
                root = root.child(
                    text(word)
                        .key("nl-filter-input-parsed")
                        .role("status")
                        .font(k.ty.text_xsmall)
                        .color(k.c.success.fg)
                        .wrap()
                        .min_w(0.0),
                );
            }
        }
        root
    }

    /// React 96-111: the unconfigured notice.
    fn not_configured(&self, k: &Kit) -> El {
        let word = t("common.aiNotConfigured");
        let mut button = Button::new(word)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .icon(&ICON_SPARKLES)
            .help(t("common.aiNotConfigured"));
        if let Some(on_open) = &self.on_open_ai_settings {
            let action = Rc::clone(on_open);
            button = button.on_press(move |s: &mut S| action(s));
        }
        row()
            .key("nl-filter-not-configured")
            .shrink()
            .h(28.0)
            .items_center()
            .gap(4.0)
            .px(6.0)
            .child(button.build(k))
    }

    /// React 113-127: the collapsed sparkle toggle.
    fn collapsed_toggle(&self, k: &Kit) -> El {
        let mut toggle = Button::new(t("smartFilter.title"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .icon(&ICON_SPARKLES)
            .help(t("smartFilter.title"));
        if let Some(on_expand) = &self.on_expand {
            let action = Rc::clone(on_expand);
            toggle = toggle.on_press(move |s: &mut S| action(s));
        }
        toggle
            .build(k)
            .key("smart-filter-toggle")
    }

    /// React 130-171: the input row — the sparkles glyph, the NL input
    /// (disabled while parsing), the parse button or the parsing
    /// spinner, and the clear `×`.
    fn input_row(&self, k: &Kit) -> El {
        let vm = self.vm;
        let mut line = row()
            .key("nl-filter-input-row")
            .w_full()
            .h(28.0)
            .min_w(0.0)
            .items_center()
            .gap(8.0)
            .child(
                row()
                    .key("nl-filter-input-glyph")
                    .size(28.0, 28.0)
                    .center()
                    .child(rune::icon(&ICON_SPARKLES, 14.0).color(k.c.text_accent)),
            );

        // The NL input: the host's TextEdit, or the model text as a
        // plain read-only line without one (the unwired ruling).
        let input_el = match self.input_edit {
            Some(edit) => {
                let mut field = crate::ui::field::TextField::new("nl-filter-input-field", edit)
                    .placeholder(t("smartFilter.placeholder"))
                    .disabled(vm.is_parsing);
                if let Some(on_input_edit) = &self.on_input_edit {
                    let accessor = Rc::clone(on_input_edit);
                    field = field.on_edit(move |s: &mut S| accessor(s));
                }
                field.build(self.dark)
            }
            None => text(vm.input.clone())
                .key("nl-filter-input-field")
                .font(k.ty.text_small)
                .color(k.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        };
        line = line.child(col().key("nl-filter-input-slot").grow(1.0).min_w(0.0).child(input_el));

        // React 143-158: the parsing swap.
        if vm.is_parsing {
            line = line.child(
                row()
                    .key("nl-filter-input-parsing")
                    .shrink()
                    .items_center()
                    .gap(4.0)
                    .child(
                        Spinner::new("nl-filter-input-spinner")
                            .size(SpinnerSize::Sm)
                            .build(self.dark),
                    )
                    .child(
                        text(t("smartFilter.parsing"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
        } else {
            let mut parse = Button::new(t("smartFilter.parse"))
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::Small)
                .disabled(vm.input.trim().is_empty());
            if let Some(on_parse) = &self.on_parse {
                let action = Rc::clone(on_parse);
                parse = parse.on_press(move |s: &mut S| action(s));
            }
            line = line.child(parse.build(k).key("nl-filter-input-parse"));
        }

        // The clear `×` (React 159-170).
        let mut clear = Button::new(t("smartFilter.clear"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .icon(&ICON_X)
            .help(t("smartFilter.clear"));
        if let Some(on_clear) = &self.on_clear {
            let action = Rc::clone(on_clear);
            clear = clear.on_press(move |s: &mut S| action(s));
        }
        line.child(clear.build(k).key("nl-filter-input-clear"))
    }
}

/// Keeps the `Value` import honest: the input's text is a plain string
/// (no JSON value rides this component); the parse's wire values are
/// the store's own `FilterCondition` mirrors.
#[allow(dead_code)]
fn no_json_values_here(_: &Value) {}
