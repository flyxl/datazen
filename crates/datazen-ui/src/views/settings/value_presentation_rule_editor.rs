//! Port of
//! `packages/frontend/src/windows/settings/ValuePresentationRuleEditor.tsx`
//! (T469) — the single global rule's edit form (issue-32 T08, PRD
//! §3.1, design §5). It lives inside the ValuePresentation settings
//! section's collapsible rule list, expanded for a new rule or an
//! edit; it only edits a local draft, and the Save button hands the
//! validated rule back to the panel (the panel writes the store on
//! its own save).
//!
//! ## What maps where
//!
//! - The form → rule single source of truth is
//!   `validateValuePresentationRule`: both the save and the sample
//!   preview first build the form into unknown JSON
//!   ([`form_to_rule_input`]) and validate it; an illegal expression
//!   or configuration cannot save, and the pathed reasons list
//!   in place (an inline error, not a global toast).
//! - The sample preview runs the same road as the runtime: the
//!   validated rule's options go through the contribution's
//!   prepare / present ([`compute_rule_sample_preview`]). An
//!   all-empty sample still allows configuring (the no-sample
//!   hint) — it never invents real data.
//! - The selects are the shared kit Select (the listbox trigger;
//!   the React `data-testid` sits on the trigger, the options are
//!   reachable through the wrapper's own contract); each choose is
//!   guarded by the is-X type check ([`PatternMode::from_wire`] and
//!   friends) exactly as React's `isPatternMode` etc.
//! - The regex case switch: in regex mode the "ignore case"
//!   checkbox is the flags `'i'`; no other flag is offered (the
//!   restricted subset, see `valuePresentationRules`).
//! - `update(patch)` (every edit clears the error) is
//!   [`ValuePresentationRuleEditorState::edit_landed`] for text
//!   edits (the kit's change event) and the `set_*` setters for
//!   the choice / checkbox edits.
//! - `handleSaveRule` is
//!   [`ValuePresentationRuleEditorState::begin_save`]: the problems
//!   sentence, the duplicate-name check against `otherNames`, then
//!   the `onSave` slot.
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`value-presentation-rule-
//! editor`, `-name`, `-enabled`, `-connection`, `-database`,
//! `-namespace`, the three `-table/column/type` `-mode`/`-value`/
//! `-ignore-case` triples, `-interpretation`, `-format`,
//! `-display-timezone`, `-display-iana-name`, `-source-timezone`,
//! `-source-iana-name`, the seven `-sample-*` inputs,
//! `-sample-preview`, `-sample-match`, `-error`, `-save`,
//! `-cancel`).
//!
//! Known deltas: the mode select's `fitContent`/`shrink-0` is a
//! fixed 110pt width; `title={label}` (the hover tooltip) has no
//! Rune equivalent.

// No production consumer yet (the ValuePresentation settings
// section T474 mounts it once a host wires it); this crate's tests
// are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};
use serde_json::{json, Value};

use crate::i18n::t;
use crate::support::value_presentation::contracts::{
    ColumnSource, PresentationColumnContext, PresentationEnvironment, PresentationPrepareInput,
    PresentationResult, ValuePresentationContribution, VersionedPresentationConfig,
};
use crate::support::value_presentation::presentation_plan::{
    resolve_presentation_semantic_kind, PresentationPlanColumn,
};
use crate::support::value_presentation::timestamp_config::{
    FormatSerde, TimestampInterpretation, UnixUnitSerde,
};
use crate::support::value_presentation::timestamp_contribution::timestamp_options_to_config_value;
use crate::support::value_presentation::timestamp_format::TimeZoneSpec;
use crate::support::value_presentation::value_presentation_rules::{
    match_value_presentation_rule, validate_value_presentation_rule, ValuePresentationNamePattern,
    ValuePresentationRule, ValuePresentationRuleContext, ValuePresentationRuleValidation,
};
use crate::ui::field::TextField;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;
use crate::views::settings::settings_ui::setting_row;

/// `text-sm` — the body copy.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
/// `text-sm font-medium` — the legends.
const TEXT_SM_MEDIUM: TextStyle = TextStyle::new(14.0, 500.0);
/// `text-sm font-semibold` — the editor's title.
const TEXT_SM_SEMIBOLD: TextStyle = TextStyle::new(14.0, 600.0);
/// `text-xs` — the ignore-case label.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);

/// `PatternMode` — an exact whole-string match or a restricted
/// regex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternMode {
    Exact,
    Regex,
}

impl PatternMode {
    pub fn as_wire(self) -> &'static str {
        match self {
            PatternMode::Exact => "exact",
            PatternMode::Regex => "regex",
        }
    }

    /// `isPatternMode(value)`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "exact" => Some(PatternMode::Exact),
            "regex" => Some(PatternMode::Regex),
            _ => None,
        }
    }
}

/// `InterpretationChoice` — the form's four-way interpretation
/// vocabulary (the rule's own kind folds unix-seconds and
/// unix-milliseconds into one `unix` kind with a unit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpretationChoice {
    Native,
    UnixSeconds,
    UnixMilliseconds,
    Text,
}

impl InterpretationChoice {
    pub fn as_wire(self) -> &'static str {
        match self {
            InterpretationChoice::Native => "native",
            InterpretationChoice::UnixSeconds => "unix-seconds",
            InterpretationChoice::UnixMilliseconds => "unix-milliseconds",
            InterpretationChoice::Text => "text",
        }
    }

    /// `isInterpretationChoice(value)`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "native" => Some(InterpretationChoice::Native),
            "unix-seconds" => Some(InterpretationChoice::UnixSeconds),
            "unix-milliseconds" => Some(InterpretationChoice::UnixMilliseconds),
            "text" => Some(InterpretationChoice::Text),
            _ => None,
        }
    }
}

/// The display time zone vocabulary (`'local' | 'utc' | 'iana'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeZoneKindChoice {
    Local,
    Utc,
    Iana,
}

impl TimeZoneKindChoice {
    pub fn as_wire(self) -> &'static str {
        match self {
            TimeZoneKindChoice::Local => "local",
            TimeZoneKindChoice::Utc => "utc",
            TimeZoneKindChoice::Iana => "iana",
        }
    }

    /// `isTimeZoneKindChoice(value)`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "local" => Some(TimeZoneKindChoice::Local),
            "utc" => Some(TimeZoneKindChoice::Utc),
            "iana" => Some(TimeZoneKindChoice::Iana),
            _ => None,
        }
    }
}

/// `SourceTimeZoneChoice` — the source zone adds the "no override"
/// arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTimeZoneChoice {
    None,
    Local,
    Utc,
    Iana,
}

impl SourceTimeZoneChoice {
    pub fn as_wire(self) -> &'static str {
        match self {
            SourceTimeZoneChoice::None => "none",
            SourceTimeZoneChoice::Local => "local",
            SourceTimeZoneChoice::Utc => "utc",
            SourceTimeZoneChoice::Iana => "iana",
        }
    }

    /// `isSourceTimeZoneChoice(value)`.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "none" => Some(SourceTimeZoneChoice::None),
            "local" => Some(SourceTimeZoneChoice::Local),
            "utc" => Some(SourceTimeZoneChoice::Utc),
            "iana" => Some(SourceTimeZoneChoice::Iana),
            _ => None,
        }
    }
}

/// `FormatSerde`'s wire vocabulary (`'iso' | 'iso-tz' | 'locale'`).
pub fn format_as_wire(format: FormatSerde) -> &'static str {
    match format {
        FormatSerde::Iso => "iso",
        FormatSerde::IsoTz => "iso-tz",
        FormatSerde::Locale => "locale",
    }
}

/// `isTimestampOutputFormat(value)`.
pub fn format_from_wire(value: &str) -> Option<FormatSerde> {
    match value {
        "iso" => Some(FormatSerde::Iso),
        "iso-tz" => Some(FormatSerde::IsoTz),
        "locale" => Some(FormatSerde::Locale),
        _ => None,
    }
}

/// `RuleFormState` — the local draft (React's plain-string fields
/// are the `TextEdit`s the kit edits in place).
#[derive(Clone, PartialEq)]
pub struct RuleFormState {
    pub name: TextEdit,
    pub enabled: bool,
    pub connection_id: TextEdit,
    pub database: TextEdit,
    pub namespace: TextEdit,
    pub table_mode: PatternMode,
    pub table_value: TextEdit,
    pub table_ignore_case: bool,
    pub column_mode: PatternMode,
    pub column_value: TextEdit,
    pub column_ignore_case: bool,
    pub type_mode: PatternMode,
    pub type_value: TextEdit,
    pub type_ignore_case: bool,
    pub interpretation: InterpretationChoice,
    pub format: FormatSerde,
    pub display_time_zone_kind: TimeZoneKindChoice,
    pub display_iana_name: TextEdit,
    pub source_time_zone_kind: SourceTimeZoneChoice,
    pub source_iana_name: TextEdit,
    pub sample_value: TextEdit,
    pub sample_column: TextEdit,
    pub sample_type: TextEdit,
    pub sample_connection: TextEdit,
    pub sample_database: TextEdit,
    pub sample_namespace: TextEdit,
    pub sample_table: TextEdit,
}

impl RuleFormState {
    /// `DEFAULT_FORM` with the column pattern defaulting to regex
    /// (the others default to exact).
    pub fn default_form() -> Self {
        Self {
            name: TextEdit::default(),
            enabled: true,
            connection_id: TextEdit::default(),
            database: TextEdit::default(),
            namespace: TextEdit::default(),
            table_mode: PatternMode::Exact,
            table_value: TextEdit::default(),
            table_ignore_case: false,
            column_mode: PatternMode::Regex,
            column_value: TextEdit::default(),
            column_ignore_case: false,
            type_mode: PatternMode::Exact,
            type_value: TextEdit::default(),
            type_ignore_case: false,
            interpretation: InterpretationChoice::UnixMilliseconds,
            format: FormatSerde::IsoTz,
            display_time_zone_kind: TimeZoneKindChoice::Local,
            display_iana_name: TextEdit::default(),
            source_time_zone_kind: SourceTimeZoneChoice::None,
            source_iana_name: TextEdit::default(),
            sample_value: TextEdit::default(),
            sample_column: TextEdit::default(),
            sample_type: TextEdit::default(),
            sample_connection: TextEdit::default(),
            sample_database: TextEdit::default(),
            sample_namespace: TextEdit::default(),
            sample_table: TextEdit::default(),
        }
    }
}

impl Default for RuleFormState {
    fn default() -> Self {
        Self::default_form()
    }
}

/// A pattern field's three-way projection (mode, value, ignore
/// case) from the rule's stored pattern.
fn pattern_fields(
    pattern: Option<&ValuePresentationNamePattern>,
    default_mode: PatternMode,
) -> (PatternMode, String, bool) {
    match pattern {
        None => (default_mode, String::new(), false),
        Some(ValuePresentationNamePattern::Exact { value }) => {
            (PatternMode::Exact, value.clone(), false)
        }
        Some(ValuePresentationNamePattern::Regex { source, flags }) => (
            PatternMode::Regex,
            source.clone(),
            flags.as_deref() == Some("i"),
        ),
    }
}

/// `formFromRule(rule)`.
pub fn form_from_rule(rule: &ValuePresentationRule) -> RuleFormState {
    let r#match = &rule.r#match;
    let options = &rule.options;
    let (table_mode, table_value, table_ignore_case) =
        pattern_fields(r#match.table.as_ref(), PatternMode::Exact);
    let (column_mode, column_value, column_ignore_case) =
        pattern_fields(r#match.column.as_ref(), PatternMode::Regex);
    let (type_mode, type_value, type_ignore_case) =
        pattern_fields(r#match.database_type_name.as_ref(), PatternMode::Exact);
    let interpretation = match &options.interpretation {
        TimestampInterpretation::Native => InterpretationChoice::Native,
        TimestampInterpretation::Text => InterpretationChoice::Text,
        TimestampInterpretation::Unix { unit } => match unit {
            UnixUnitSerde::Seconds => InterpretationChoice::UnixSeconds,
            UnixUnitSerde::Milliseconds => InterpretationChoice::UnixMilliseconds,
        },
    };
    RuleFormState {
        name: TextEdit::new(rule.name.clone()),
        enabled: rule.enabled,
        connection_id: TextEdit::new(r#match.connection_id.clone().unwrap_or_default()),
        database: TextEdit::new(r#match.database.clone().unwrap_or_default()),
        namespace: TextEdit::new(r#match.namespace.clone().unwrap_or_default()),
        table_mode,
        table_value: TextEdit::new(table_value),
        table_ignore_case,
        column_mode,
        column_value: TextEdit::new(column_value),
        column_ignore_case,
        type_mode,
        type_value: TextEdit::new(type_value),
        type_ignore_case,
        interpretation,
        format: options.format,
        display_time_zone_kind: match &options.display_time_zone {
            TimeZoneSpec::Local => TimeZoneKindChoice::Local,
            TimeZoneSpec::Utc => TimeZoneKindChoice::Utc,
            TimeZoneSpec::Iana { .. } => TimeZoneKindChoice::Iana,
        },
        display_iana_name: TextEdit::new(match &options.display_time_zone {
            TimeZoneSpec::Iana { name } => name.clone(),
            _ => String::new(),
        }),
        source_time_zone_kind: match &options.source_time_zone {
            None => SourceTimeZoneChoice::None,
            Some(TimeZoneSpec::Local) => SourceTimeZoneChoice::Local,
            Some(TimeZoneSpec::Utc) => SourceTimeZoneChoice::Utc,
            Some(TimeZoneSpec::Iana { .. }) => SourceTimeZoneChoice::Iana,
        },
        source_iana_name: TextEdit::new(match &options.source_time_zone {
            Some(TimeZoneSpec::Iana { name }) => name.clone(),
            _ => String::new(),
        }),
        ..RuleFormState::default_form()
    }
}

/// `patternInput(mode, value, ignoreCase)` — `None` for a blank
/// value; the trimmed exact value or the trimmed regex source with
/// `'i'` only when ignore-case is on.
pub fn pattern_input(
    mode: PatternMode,
    value: &str,
    ignore_case: bool,
) -> Option<Value> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    match mode {
        PatternMode::Exact => Some(json!({ "kind": "exact", "value": trimmed })),
        PatternMode::Regex if ignore_case => {
            Some(json!({ "kind": "regex", "source": trimmed, "flags": "i" }))
        }
        PatternMode::Regex => Some(json!({ "kind": "regex", "source": trimmed })),
    }
}

/// `timeZoneSpec(kind, ianaName)` — the trimmed IANA name.
pub fn time_zone_spec(kind: TimeZoneKindChoice, iana_name: &str) -> TimeZoneSpec {
    match kind {
        TimeZoneKindChoice::Iana => TimeZoneSpec::Iana { name: iana_name.trim().to_owned() },
        TimeZoneKindChoice::Local => TimeZoneSpec::Local,
        TimeZoneKindChoice::Utc => TimeZoneSpec::Utc,
    }
}

/// A blank-trimmed `Option`.
fn trim_opt(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// `formToRuleInput(form)` — the form assembled into the unknown
/// JSON the single validator reads.
pub fn form_to_rule_input(form: &RuleFormState) -> Value {
    let mut rule_match = json!({});
    if let Some(connection) = trim_opt(&form.connection_id.text) {
        rule_match["connectionId"] = json!(connection);
    }
    if let Some(database) = trim_opt(&form.database.text) {
        rule_match["database"] = json!(database);
    }
    if let Some(namespace) = trim_opt(&form.namespace.text) {
        rule_match["namespace"] = json!(namespace);
    }
    if let Some(table) = pattern_input(form.table_mode, &form.table_value.text, form.table_ignore_case) {
        rule_match["table"] = table;
    }
    if let Some(column) =
        pattern_input(form.column_mode, &form.column_value.text, form.column_ignore_case)
    {
        rule_match["column"] = column;
    }
    if let Some(type_name) =
        pattern_input(form.type_mode, &form.type_value.text, form.type_ignore_case)
    {
        rule_match["databaseTypeName"] = type_name;
    }

    let interpretation = match form.interpretation {
        InterpretationChoice::UnixSeconds => {
            json!({ "kind": "unix", "unit": "seconds" })
        }
        InterpretationChoice::UnixMilliseconds => {
            json!({ "kind": "unix", "unit": "milliseconds" })
        }
        InterpretationChoice::Native => json!({ "kind": "native" }),
        InterpretationChoice::Text => json!({ "kind": "text" }),
    };
    let mut options = json!({
        "interpretation": interpretation,
        "format": format_as_wire(form.format),
        "displayTimeZone": time_zone_spec(
            form.display_time_zone_kind,
            &form.display_iana_name.text,
        ),
    });
    if form.source_time_zone_kind != SourceTimeZoneChoice::None {
        options["sourceTimeZone"] = serde_json::to_value(time_zone_spec(
            match form.source_time_zone_kind {
                SourceTimeZoneChoice::Iana => TimeZoneKindChoice::Iana,
                SourceTimeZoneChoice::Local => TimeZoneKindChoice::Local,
                SourceTimeZoneChoice::Utc => TimeZoneKindChoice::Utc,
                SourceTimeZoneChoice::None => TimeZoneKindChoice::Local,
            },
            &form.source_iana_name.text,
        ))
        .unwrap_or(Value::Null);
    }
    json!({
        "name": form.name.text,
        "enabled": form.enabled,
        "match": rule_match,
        "options": options,
    })
}

/// `RuleSamplePreview`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleSamplePreview {
    /// The sample inputs are blank — configuring stays allowed.
    NoSample,
    /// No contribution is registered for the rule's renderer.
    Unavailable,
    /// The rule does not match the sample's column context.
    NotMatched,
    /// A hit: the presented text (and the interpretation it took).
    Matched { text: String, interpretation: Option<String> },
    /// A hit that left the value unchanged (with the reason).
    MatchedUnchanged { reason: Option<String> },
}

/// `computeRuleSamplePreview`'s inputs.
pub struct RuleSampleInput<'a> {
    pub rule: &'a ValuePresentationRule,
    pub sample_value: &'a str,
    pub sample_column: &'a str,
    pub sample_type_name: Option<&'a str>,
    pub sample_connection_id: Option<&'a str>,
    pub sample_database: Option<&'a str>,
    pub sample_namespace: Option<&'a str>,
    pub sample_table: Option<&'a str>,
    pub contribution: Option<Rc<dyn ValuePresentationContribution>>,
    pub environment: PresentationEnvironment<'a>,
}

/// `computeRuleSamplePreview` — the sample preview runs the same
/// road as the runtime (the contribution's prepare / present).
pub fn compute_rule_sample_preview(input: &RuleSampleInput<'_>) -> RuleSamplePreview {
    let value = input.sample_value.trim();
    let column = input.sample_column.trim();
    if value.is_empty() || column.is_empty() {
        return RuleSamplePreview::NoSample;
    }
    let context = ValuePresentationRuleContext {
        connection_id: input.sample_connection_id,
        database: input.sample_database,
        namespace: input.sample_namespace,
        table: input.sample_table,
        column,
        database_type_name: input.sample_type_name,
    };
    if !match_value_presentation_rule(input.rule, &context) {
        return RuleSamplePreview::NotMatched;
    }
    let Some(contribution) = &input.contribution else {
        return RuleSamplePreview::Unavailable;
    };
    // The source context only exists when both the table and the
    // connection are present (React's own condition).
    let source = match (input.sample_table, input.sample_connection_id) {
        (Some(table), Some(connection_id)) => Some(ColumnSource {
            connection_id,
            database: input.sample_database,
            namespace: input.sample_namespace,
            table,
            column,
        }),
        _ => None,
    };
    let semantic_kind = resolve_presentation_semantic_kind(&PresentationPlanColumn {
        name: column,
        data_type: input.sample_type_name,
        cell_type: None,
        hints: None,
    });
    let config = VersionedPresentationConfig {
        config_version: contribution.config_version(),
        config: timestamp_options_to_config_value(&input.rule.options),
    };
    let prepared = contribution.prepare(&PresentationPrepareInput {
        config: &config,
        column_override: None,
        column: PresentationColumnContext {
            column_name: column,
            database_type_name: input.sample_type_name,
            semantic_kind,
            source,
        },
        environment: input.environment,
    });
    // The sample value rides as the raw string: unix parsing
    // accepts integer strings, native/text are strings already —
    // the same interpretation path as the runtime's present.
    let result = prepared.present(&Value::String(value.to_owned()));
    match result {
        PresentationResult::Text { text, interpretation } => {
            RuleSamplePreview::Matched { text, interpretation }
        }
        PresentationResult::Unchanged { reason } => RuleSamplePreview::MatchedUnchanged { reason },
    }
}

/// Which select is open (the kit contract: an enabled select opens
/// only through its on_toggle; the scrim closes through
/// on_close; a choose closes too).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorSelect {
    TableMode,
    ColumnMode,
    TypeMode,
    Interpretation,
    Format,
    DisplayTimeZone,
    SourceTimeZone,
}

/// The editor's state.
#[derive(Clone, PartialEq)]
pub struct ValuePresentationRuleEditorState {
    /// `null` = a new rule; `Some` = editing it (the title and the
    /// form's initial value).
    pub initial_rule: Option<ValuePresentationRule>,
    pub form: RuleFormState,
    /// The inline error (the save's refusal sentence).
    pub error: Option<String>,
    /// The open select (None = all closed).
    pub open_select: Option<EditorSelect>,
}

impl Default for ValuePresentationRuleEditorState {
    fn default() -> Self {
        Self {
            initial_rule: None,
            form: RuleFormState::default_form(),
            error: None,
            open_select: None,
        }
    }
}

impl std::fmt::Debug for ValuePresentationRuleEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValuePresentationRuleEditorState")
            .field("initial_rule", &self.initial_rule)
            .field("error", &self.error)
            .field("open_select", &self.open_select)
            .field("form.name", &self.form.name.text)
            .field("form.enabled", &self.form.enabled)
            .finish_non_exhaustive()
    }
}

impl ValuePresentationRuleEditorState {
    /// `useState(() => initialRule === null ? DEFAULT_FORM :
    /// formFromRule(initialRule))`.
    pub fn new(initial_rule: Option<&ValuePresentationRule>) -> Self {
        Self {
            initial_rule: initial_rule.cloned(),
            form: match initial_rule {
                None => RuleFormState::default_form(),
                Some(rule) => form_from_rule(rule),
            },
            ..Self::default()
        }
    }

    /// `update(patch)`'s error half — every landed edit clears it.
    pub fn edit_landed(&mut self) {
        self.error = None;
    }

    /// The title: new vs edit.
    pub fn is_new(&self) -> bool {
        self.initial_rule.is_none()
    }

    /// `showSourceTimeZone` — the source zone only exists for the
    /// string-interpretation kinds.
    pub fn show_source_time_zone(&self) -> bool {
        matches!(
            self.form.interpretation,
            InterpretationChoice::Native | InterpretationChoice::Text
        )
    }

    /// The `useMemo`'s validation — the form assembled and run
    /// through the single validator.
    pub fn validated(&self) -> ValuePresentationRuleValidation {
        validate_value_presentation_rule(&form_to_rule_input(&self.form))
    }

    /// The `useMemo`'s sample preview over the validated rule.
    pub fn sample_preview(
        &self,
        contribution: Option<Rc<dyn ValuePresentationContribution>>,
        environment: PresentationEnvironment<'_>,
    ) -> RuleSamplePreview {
        let ValuePresentationRuleValidation::Ok(rule) = self.validated() else {
            return RuleSamplePreview::NoSample;
        };
        compute_rule_sample_preview(&RuleSampleInput {
            rule: &rule,
            sample_value: &self.form.sample_value.text,
            sample_column: &self.form.sample_column.text,
            sample_type_name: trim_opt(&self.form.sample_type.text),
            sample_connection_id: trim_opt(&self.form.sample_connection.text),
            sample_database: trim_opt(&self.form.sample_database.text),
            sample_namespace: trim_opt(&self.form.sample_namespace.text),
            sample_table: trim_opt(&self.form.sample_table.text),
            contribution,
            environment,
        })
    }

    /// `handleSaveRule`: the problems sentence (an invalid rule),
    /// the duplicate-name check, then the validated rule for the
    /// `onSave` slot.
    pub fn begin_save(
        &mut self,
        other_names: &[String],
    ) -> Option<ValuePresentationRule> {
        match self.validated() {
            ValuePresentationRuleValidation::Err(problems) => {
                let joined = problems
                    .iter()
                    .map(|problem| format!("{}: {}", problem.path, problem.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                self.error = Some(format!(
                    "{}: {}",
                    t("settings.valuePresentation.rules.editor.invalidRule"),
                    joined
                ));
                None
            }
            ValuePresentationRuleValidation::Ok(rule) => {
                if other_names.iter().any(|name| name == &rule.name) {
                    self.error =
                        Some(t("settings.valuePresentation.rules.editor.duplicateName").to_owned());
                    None
                } else {
                    Some(*rule)
                }
            }
        }
    }

    // The choice / checkbox edits (`update(patch)`).

    /// The enabled checkbox.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.form.enabled = enabled;
        self.edit_landed();
    }

    pub fn set_table_mode(&mut self, mode: PatternMode) {
        self.form.table_mode = mode;
        self.edit_landed();
    }

    pub fn set_table_ignore_case(&mut self, ignore_case: bool) {
        self.form.table_ignore_case = ignore_case;
        self.edit_landed();
    }

    pub fn set_column_mode(&mut self, mode: PatternMode) {
        self.form.column_mode = mode;
        self.edit_landed();
    }

    pub fn set_column_ignore_case(&mut self, ignore_case: bool) {
        self.form.column_ignore_case = ignore_case;
        self.edit_landed();
    }

    pub fn set_type_mode(&mut self, mode: PatternMode) {
        self.form.type_mode = mode;
        self.edit_landed();
    }

    pub fn set_type_ignore_case(&mut self, ignore_case: bool) {
        self.form.type_ignore_case = ignore_case;
        self.edit_landed();
    }

    pub fn set_interpretation(&mut self, interpretation: InterpretationChoice) {
        self.form.interpretation = interpretation;
        self.edit_landed();
    }

    pub fn set_format(&mut self, format: FormatSerde) {
        self.form.format = format;
        self.edit_landed();
    }

    pub fn set_display_time_zone_kind(&mut self, kind: TimeZoneKindChoice) {
        self.form.display_time_zone_kind = kind;
        self.edit_landed();
    }

    pub fn set_source_time_zone_kind(&mut self, kind: SourceTimeZoneChoice) {
        self.form.source_time_zone_kind = kind;
        self.edit_landed();
    }

    /// The kit select contract's toggle.
    pub fn toggle_select(&mut self, which: EditorSelect) {
        self.open_select = if self.open_select == Some(which) {
            None
        } else {
            Some(which)
        };
    }

    /// The kit select contract's close (scrim + choose).
    pub fn close_select(&mut self) {
        self.open_select = None;
    }
}

/// The host lens adapter for one of the form's `TextEdit`s (a
/// top-level fn shape: the HRTB needs the named form).
fn form_field_of<S: 'static>(
    editor_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState>,
    field: fn(&mut RuleFormState) -> &mut TextEdit,
) -> Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> {
    let editor_edit = Rc::clone(editor_edit);
    Rc::new(move |s: &mut S| field(&mut editor_edit(s).form))
}

/// One text input over the form (the kit's change event clears the
/// error — React's `update`).
fn text_input<S: 'static>(
    dark: bool,
    editor_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState>,
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    field: fn(&mut RuleFormState) -> &mut TextEdit,
) -> El {
    TextField::new(key, edit)
        .height(36.0)
        .placeholder(placeholder)
        .on_edit::<S>({
            let lens = form_field_of(editor_edit, field);
            move |s: &mut S| lens(s)
        })
        .on_change::<S>({
            let editor_edit = Rc::clone(editor_edit);
            move |s: &mut S, _tx: &mut rune::Tx| editor_edit(s).edit_landed()
        })
        .build(dark)
}

/// One typed select over its vocabulary (the labels, the values,
/// the current label, the open state, the guard and the write) —
/// the T434 `typed_select` shape.
fn choice_select<S: 'static>(
    editor_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState>,
    open: bool,
    which: EditorSelect,
    key: &str,
    entries: &[(String, String)], // (wire value, label)
    current_wire: &str,
    width: f32,
    apply: Rc<dyn Fn(&mut ValuePresentationRuleEditorState, &str)>,
) -> Select {
    let labels: Vec<String> = entries.iter().map(|(_, label)| label.clone()).collect();
    let values: Vec<String> = entries.iter().map(|(value, _)| value.clone()).collect();
    let current_label = entries
        .iter()
        .find(|(value, _)| value == current_wire)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| current_wire.to_owned());
    let mut select = Select::new(key, &labels)
        .values(&values)
        .current(Some(current_label.as_str()))
        .width(width)
        .open(open);
    // The kit contract: an enabled select opens only through its
    // on_toggle; the scrim closes through on_close.
    {
        let editor_edit = Rc::clone(editor_edit);
        select = select.on_toggle(move |s: &mut S| editor_edit(s).toggle_select(which));
    }
    {
        let editor_edit = Rc::clone(editor_edit);
        select = select.on_close(move |s: &mut S| editor_edit(s).close_select());
    }
    {
        let editor_edit = Rc::clone(editor_edit);
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
            let editor = editor_edit(s);
            apply(editor, value);
            // The listbox closes on choose (the wrapper's own
            // contract).
            editor.close_select();
        });
    }
    select
}

/// `ValuePresentationRuleEditor({ initialRule, otherNames,
/// contribution, environment, onSave, onCancel })`: the form over
/// the state, the state lens and the host's action slots.
pub struct ValuePresentationRuleEditor<'a, S: 'static> {
    state: &'a ValuePresentationRuleEditorState,
    dark: bool,
    editor_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState>,
    /// The contribution the sample preview prepares through
    /// (React's `contribution` prop).
    contribution: Option<Rc<dyn ValuePresentationContribution>>,
    /// The environment snapshot (React's `environment` prop).
    environment: PresentationEnvironment<'a>,
    /// The other rules' names (React's `otherNames` prop — the
    /// duplicate check).
    other_names: Vec<String>,
    on_save: Option<Rc<dyn Fn(&mut S, ValuePresentationRule)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> ValuePresentationRuleEditor<'a, S> {
    pub fn new(
        state: &'a ValuePresentationRuleEditorState,
        dark: bool,
        editor_edit: impl for<'b> Fn(&'b mut S) -> &'b mut ValuePresentationRuleEditorState + 'static,
        contribution: Option<Rc<dyn ValuePresentationContribution>>,
        environment: PresentationEnvironment<'a>,
    ) -> Self {
        Self {
            state,
            dark,
            editor_edit: Rc::new(editor_edit),
            contribution,
            environment,
            other_names: Vec::new(),
            on_save: None,
            on_cancel: None,
        }
    }

    /// `otherNames` — the duplicate-name check's vocabulary.
    pub fn with_other_names(mut self, names: Vec<String>) -> Self {
        self.other_names = names;
        self
    }

    /// `onSave(rule)` — the validated rule back to the panel.
    pub fn on_save(mut self, f: impl Fn(&mut S, ValuePresentationRule) + 'static) -> Self {
        self.on_save = Some(Rc::new(f));
        self
    }

    /// `onCancel`.
    pub fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// The form card.
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let editor_edit = &self.editor_edit;
        let dark = self.dark;
        let form = &state.form;

        // `mt-2 space-y-3 rounded-md border border-edge bg-surface
        // p-3`.
        let mut card = col()
            .key("value-presentation-rule-editor")
            .w_full()
            .gap(12.0) // space-y-3
            .padding(12.0)
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);

        // The title.
        card = card.child(
            text(
                if state.is_new() {
                    t("settings.valuePresentation.rules.editor.newTitle").to_owned()
                } else {
                    t("settings.valuePresentation.rules.editor.editTitle").to_owned()
                },
            )
            .font(TEXT_SM_SEMIBOLD)
            .color(k.c.text_base),
        );

        // The name.
        card = card.child(setting_row(
            k,
            layer,
            "value-presentation-rule-name-row".to_owned(),
            t("settings.valuePresentation.rules.editor.name"),
            None,
            text_input::<S>(
                dark,
                editor_edit,
                "value-presentation-rule-name",
                &form.name,
                "",
                |f| &mut f.name,
            ),
        ));

        // The enabled checkbox.
        {
            let mut enabled = Checkbox::new("value-presentation-rule-enabled", form.enabled)
                .label(t("settings.valuePresentation.rules.editor.enabled"));
            {
                let editor_edit = Rc::clone(editor_edit);
                enabled = enabled.on_toggle(move |s: &mut S| {
                    let editor = editor_edit(s);
                    editor.set_enabled(!editor.form.enabled);
                });
            }
            card = card.child(enabled.build(k));
        }

        // The match fieldset.
        let mut match_set = col().key("value-presentation-rule-match-set").w_full().gap(8.0);
        match_set = match_set.child(
            text(t("settings.valuePresentation.rules.editor.matchTitle").to_owned())
                .font(TEXT_SM_MEDIUM)
                .color(k.c.text_base),
        );
        let match_fields: [(&str, &str, &TextEdit, fn(&mut RuleFormState) -> &mut TextEdit); 3] = [
            (
                "value-presentation-rule-connection",
                t("settings.valuePresentation.rules.editor.connection"),
                &form.connection_id,
                |f| &mut f.connection_id,
            ),
            (
                "value-presentation-rule-database",
                t("settings.valuePresentation.rules.editor.database"),
                &form.database,
                |f| &mut f.database,
            ),
            (
                "value-presentation-rule-namespace",
                t("settings.valuePresentation.rules.editor.namespace"),
                &form.namespace,
                |f| &mut f.namespace,
            ),
        ];
        for (key, label, edit, field) in match_fields {
            match_set = match_set.child(setting_row(
                k,
                layer,
                format!("{key}-row"),
                label,
                None,
                text_input::<S>(dark, editor_edit, key, edit, "", field),
            ));
        }
        match_set = match_set.child(self.pattern_row(
            k,
            layer,
            "value-presentation-rule-table",
            t("settings.valuePresentation.rules.editor.table"),
            EditorSelect::TableMode,
            form.table_mode,
            &form.table_value,
            form.table_ignore_case,
            |f| &mut f.table_value,
            Rc::new(|editor, value| {
                if let Some(mode) = PatternMode::from_wire(value) {
                    editor.set_table_mode(mode);
                }
            }),
            Rc::new(|editor: &mut ValuePresentationRuleEditorState| {
                editor.set_table_ignore_case(!editor.form.table_ignore_case);
            }),
        ));
        match_set = match_set.child(self.pattern_row(
            k,
            layer,
            "value-presentation-rule-column",
            t("settings.valuePresentation.rules.editor.column"),
            EditorSelect::ColumnMode,
            form.column_mode,
            &form.column_value,
            form.column_ignore_case,
            |f| &mut f.column_value,
            Rc::new(|editor, value| {
                if let Some(mode) = PatternMode::from_wire(value) {
                    editor.set_column_mode(mode);
                }
            }),
            Rc::new(|editor: &mut ValuePresentationRuleEditorState| {
                editor.set_column_ignore_case(!editor.form.column_ignore_case);
            }),
        ));
        match_set = match_set.child(self.pattern_row(
            k,
            layer,
            "value-presentation-rule-type",
            t("settings.valuePresentation.rules.editor.typeName"),
            EditorSelect::TypeMode,
            form.type_mode,
            &form.type_value,
            form.type_ignore_case,
            |f| &mut f.type_value,
            Rc::new(|editor, value| {
                if let Some(mode) = PatternMode::from_wire(value) {
                    editor.set_type_mode(mode);
                }
            }),
            Rc::new(|editor: &mut ValuePresentationRuleEditorState| {
                editor.set_type_ignore_case(!editor.form.type_ignore_case);
            }),
        ));
        card = card.child(match_set);

        // The options fieldset.
        let mut options_set = col().key("value-presentation-rule-options-set").w_full().gap(8.0);
        options_set = options_set.child(
            text(t("settings.valuePresentation.rules.editor.optionsTitle").to_owned())
                .font(TEXT_SM_MEDIUM)
                .color(k.c.text_base),
        );

        // The interpretation select (the preview's source labels).
        let interpretation_entries: Vec<(String, String)> = [
            InterpretationChoice::Native,
            InterpretationChoice::UnixSeconds,
            InterpretationChoice::UnixMilliseconds,
            InterpretationChoice::Text,
        ]
        .iter()
        .map(|choice| {
            (
                choice.as_wire().to_owned(),
                t(match choice {
                    InterpretationChoice::Native => {
                        "settings.valuePresentation.preview.source.native"
                    }
                    InterpretationChoice::UnixSeconds => {
                        "settings.valuePresentation.preview.source.unix-seconds"
                    }
                    InterpretationChoice::UnixMilliseconds => {
                        "settings.valuePresentation.preview.source.unix-milliseconds"
                    }
                    InterpretationChoice::Text => "settings.valuePresentation.preview.source.text",
                })
                .to_owned(),
            )
        })
        .collect();
        let interpretation_el = choice_select::<S>(
            editor_edit,
            state.open_select == Some(EditorSelect::Interpretation),
            EditorSelect::Interpretation,
            "value-presentation-rule-interpretation",
            &interpretation_entries,
            form.interpretation.as_wire(),
            180.0,
            Rc::new(|editor, value| {
                if let Some(choice) = InterpretationChoice::from_wire(value) {
                    editor.set_interpretation(choice);
                }
            }),
        )
        .build(k, layer);
        options_set = options_set.child(setting_row(
            k,
            layer,
            "value-presentation-rule-interpretation-row".to_owned(),
            t("settings.valuePresentation.rules.editor.interpretation"),
            None,
            interpretation_el,
        ));

        // The format select.
        let format_entries: Vec<(String, String)> = [FormatSerde::Iso, FormatSerde::IsoTz, FormatSerde::Locale]
            .iter()
            .map(|format| {
                (
                    format_as_wire(*format).to_owned(),
                    t(match format {
                        FormatSerde::Iso => "settings.valuePresentation.format.iso",
                        FormatSerde::IsoTz => "settings.valuePresentation.format.iso-tz",
                        FormatSerde::Locale => "settings.valuePresentation.format.locale",
                    })
                    .to_owned(),
                )
            })
            .collect();
        let format_el = choice_select::<S>(
            editor_edit,
            state.open_select == Some(EditorSelect::Format),
            EditorSelect::Format,
            "value-presentation-rule-format",
            &format_entries,
            format_as_wire(form.format),
            140.0,
            Rc::new(|editor, value| {
                if let Some(format) = format_from_wire(value) {
                    editor.set_format(format);
                }
            }),
        )
        .build(k, layer);
        options_set = options_set.child(setting_row(
            k,
            layer,
            "value-presentation-rule-format-row".to_owned(),
            t("settings.valuePresentation.format"),
            None,
            format_el,
        ));

        // The display time zone select.
        let tz_entries: Vec<(String, String)> = [
            TimeZoneKindChoice::Local,
            TimeZoneKindChoice::Utc,
            TimeZoneKindChoice::Iana,
        ]
        .iter()
        .map(|kind| {
            (
                kind.as_wire().to_owned(),
                t(match kind {
                    TimeZoneKindChoice::Local => "settings.valuePresentation.timeZone.local",
                    TimeZoneKindChoice::Utc => "settings.valuePresentation.timeZone.utc",
                    TimeZoneKindChoice::Iana => "settings.valuePresentation.timeZone.iana",
                })
                .to_owned(),
            )
        })
        .collect();
        let display_tz_el = choice_select::<S>(
            editor_edit,
            state.open_select == Some(EditorSelect::DisplayTimeZone),
            EditorSelect::DisplayTimeZone,
            "value-presentation-rule-display-timezone",
            &tz_entries,
            form.display_time_zone_kind.as_wire(),
            140.0,
            Rc::new(|editor, value| {
                if let Some(kind) = TimeZoneKindChoice::from_wire(value) {
                    editor.set_display_time_zone_kind(kind);
                }
            }),
        )
        .build(k, layer);
        options_set = options_set.child(setting_row(
            k,
            layer,
            "value-presentation-rule-display-timezone-row".to_owned(),
            t("settings.valuePresentation.displayTimeZone"),
            None,
            display_tz_el,
        ));

        // The display IANA name (the iana arm only).
        if form.display_time_zone_kind == TimeZoneKindChoice::Iana {
            options_set = options_set.child(setting_row(
                k,
                layer,
                "value-presentation-rule-display-iana-name-row".to_owned(),
                t("settings.valuePresentation.ianaName"),
                None,
                text_input::<S>(
                    dark,
                    editor_edit,
                    "value-presentation-rule-display-iana-name",
                    &form.display_iana_name,
                    "Asia/Shanghai",
                    |f| &mut f.display_iana_name,
                ),
            ));
        }

        // The source time zone (the string interpretations only).
        if state.show_source_time_zone() {
            let source_entries: Vec<(String, String)> = [SourceTimeZoneChoice::None]
                .iter()
                .chain([SourceTimeZoneChoice::Local, SourceTimeZoneChoice::Utc, SourceTimeZoneChoice::Iana].iter())
                .map(|kind| {
                    (
                        kind.as_wire().to_owned(),
                        t(match kind {
                            SourceTimeZoneChoice::None => {
                                "settings.valuePresentation.rules.editor.sourceTimeZone.none"
                            }
                            SourceTimeZoneChoice::Local => {
                                "settings.valuePresentation.timeZone.local"
                            }
                            SourceTimeZoneChoice::Utc => "settings.valuePresentation.timeZone.utc",
                            SourceTimeZoneChoice::Iana => {
                                "settings.valuePresentation.timeZone.iana"
                            }
                        })
                        .to_owned(),
                    )
                })
                .collect();
            let source_tz_el = choice_select::<S>(
                editor_edit,
                state.open_select == Some(EditorSelect::SourceTimeZone),
                EditorSelect::SourceTimeZone,
                "value-presentation-rule-source-timezone",
                &source_entries,
                form.source_time_zone_kind.as_wire(),
                140.0,
                Rc::new(|editor, value| {
                    if let Some(kind) = SourceTimeZoneChoice::from_wire(value) {
                        editor.set_source_time_zone_kind(kind);
                    }
                }),
            )
            .build(k, layer);
            options_set = options_set.child(setting_row(
                k,
                layer,
                "value-presentation-rule-source-timezone-row".to_owned(),
                t("settings.valuePresentation.rules.editor.sourceTimeZone"),
                None,
                source_tz_el,
            ));
            if form.source_time_zone_kind == SourceTimeZoneChoice::Iana {
                options_set = options_set.child(setting_row(
                    k,
                    layer,
                    "value-presentation-rule-source-iana-name-row".to_owned(),
                    t("settings.valuePresentation.ianaName"),
                    None,
                    text_input::<S>(
                        dark,
                        editor_edit,
                        "value-presentation-rule-source-iana-name",
                        &form.source_iana_name,
                        "Asia/Shanghai",
                        |f| &mut f.source_iana_name,
                    ),
                ));
            }
        }
        card = card.child(options_set);

        // The sample fieldset.
        let mut sample_set = col().key("value-presentation-rule-sample-set").w_full().gap(8.0);
        sample_set = sample_set.child(
            text(t("settings.valuePresentation.rules.editor.sampleTitle").to_owned())
                .font(TEXT_SM_MEDIUM)
                .color(k.c.text_base),
        );
        let sample_fields: [(&str, &str, &TextEdit, fn(&mut RuleFormState) -> &mut TextEdit); 7] = [
            (
                "value-presentation-rule-sample-value",
                t("settings.valuePresentation.rules.editor.sampleValue"),
                &form.sample_value,
                |f| &mut f.sample_value,
            ),
            (
                "value-presentation-rule-sample-column",
                t("settings.valuePresentation.rules.editor.column"),
                &form.sample_column,
                |f| &mut f.sample_column,
            ),
            (
                "value-presentation-rule-sample-type",
                t("settings.valuePresentation.rules.editor.typeName"),
                &form.sample_type,
                |f| &mut f.sample_type,
            ),
            (
                "value-presentation-rule-sample-table",
                t("settings.valuePresentation.rules.editor.table"),
                &form.sample_table,
                |f| &mut f.sample_table,
            ),
            (
                "value-presentation-rule-sample-connection",
                t("settings.valuePresentation.rules.editor.connection"),
                &form.sample_connection,
                |f| &mut f.sample_connection,
            ),
            (
                "value-presentation-rule-sample-database",
                t("settings.valuePresentation.rules.editor.database"),
                &form.sample_database,
                |f| &mut f.sample_database,
            ),
            (
                "value-presentation-rule-sample-namespace",
                t("settings.valuePresentation.rules.editor.namespace"),
                &form.sample_namespace,
                |f| &mut f.sample_namespace,
            ),
        ];
        for (key, label, edit, field) in sample_fields {
            sample_set = sample_set.child(setting_row(
                k,
                layer,
                format!("{key}-row"),
                label,
                None,
                text_input::<S>(dark, editor_edit, key, edit, "", field),
            ));
        }

        // The preview box.
        let preview = state.sample_preview(
            self.contribution.clone(),
            self.environment,
        );
        let mut preview_box = col()
            .key("value-presentation-rule-sample-preview")
            .w_full()
            .padding(12.0)
            .radius(6.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak); // bg-surface-alt
        match &preview {
            RuleSamplePreview::NoSample => {
                preview_box = preview_box.child(
                    text(t("settings.valuePresentation.rules.editor.sampleHint").to_owned())
                        .font(TEXT_SM)
                        .color(k.c.text_muted),
                );
            }
            RuleSamplePreview::Unavailable => {
                preview_box = preview_box.child(
                    text(t("settings.valuePresentation.preview.unavailable").to_owned())
                        .font(TEXT_SM)
                        .color(k.c.text_muted),
                );
            }
            RuleSamplePreview::NotMatched => {
                preview_box = preview_box.child(
                    text(t("settings.valuePresentation.rules.editor.matchMiss").to_owned())
                        .key("value-presentation-rule-sample-match")
                        .font(TEXT_SM)
                        .color(k.c.text_muted),
                );
            }
            RuleSamplePreview::Matched { text: hit, .. } => {
                preview_box = preview_box.child(
                    row()
                        .key("value-presentation-rule-sample-match")
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(
                            text(t("settings.valuePresentation.rules.editor.matchHit").to_owned())
                                .font(TEXT_SM)
                                .color(k.c.text_base),
                        )
                        .child(
                            text(hit.clone())
                                .font(k.ty.mono)
                                .color(k.c.text_base),
                        ),
                );
            }
            RuleSamplePreview::MatchedUnchanged { reason } => {
                let mut line = t("settings.valuePresentation.preview.unchanged").to_owned();
                if let Some(reason) = reason {
                    line.push_str(": ");
                    line.push_str(reason);
                }
                preview_box = preview_box.child(
                    row()
                        .key("value-presentation-rule-sample-match")
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(
                            text(t("settings.valuePresentation.rules.editor.matchHit").to_owned())
                                .font(TEXT_SM)
                                .color(k.c.text_base),
                        )
                        .child(text(line).font(TEXT_SM).color(k.c.text_muted)),
                );
            }
        }
        sample_set = sample_set.child(preview_box);
        card = card.child(sample_set);

        // The error.
        if let Some(error) = &state.error {
            card = card.child(
                text(error.clone())
                    .key("value-presentation-rule-error")
                    .font(TEXT_SM)
                    .color(k.c.danger.fg),
            );
        }

        // The footer.
        let mut save = Button::new(t("settings.valuePresentation.rules.editor.saveRule"))
            .control_size(ControlSize::Small); // React primary
        {
            let on_save = self.on_save.clone();
            let editor_edit = Rc::clone(editor_edit);
            let other_names = self.other_names.clone();
            save = save.on_press(move |s: &mut S| {
                let rule = {
                    let editor = editor_edit(s);
                    editor.begin_save(&other_names)
                };
                if let (Some(rule), Some(on_save)) = (rule, &on_save) {
                    on_save(s, rule);
                }
            });
        }
        let mut cancel = Button::new(t("settings.valuePresentation.rules.editor.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(on_cancel) = &self.on_cancel {
            let on_cancel = Rc::clone(on_cancel);
            cancel = cancel.on_press(move |s: &mut S| on_cancel(s));
        }
        let footer = row()
            .key("value-presentation-rule-footer")
            .w_full()
            .gap(8.0) // gap-2
            .center_y()
            .child(save.build(k).key("value-presentation-rule-save"))
            .child(cancel.build(k).key("value-presentation-rule-cancel"));
        card = card.child(footer);

        card
    }

    /// `PatternField`: the mode select + the value input + the
    /// regex-only ignore-case checkbox.
    #[allow(clippy::too_many_arguments)]
    fn pattern_row(
        &self,
        k: &Kit,
        layer: &mut Vec<El>,
        key: &str,
        label: &str,
        which: EditorSelect,
        mode: PatternMode,
        value_edit: &TextEdit,
        ignore_case: bool,
        value_field: fn(&mut RuleFormState) -> &mut TextEdit,
        apply_mode: Rc<dyn Fn(&mut ValuePresentationRuleEditorState, &str)>,
        toggle_ignore_case: Rc<dyn Fn(&mut ValuePresentationRuleEditorState)>,
    ) -> El {
        let editor_edit = &self.editor_edit;
        let mode_entries: Vec<(String, String)> = [PatternMode::Exact, PatternMode::Regex]
            .iter()
            .map(|mode| {
                (
                    mode.as_wire().to_owned(),
                    t(match mode {
                        PatternMode::Exact => {
                            "settings.valuePresentation.rules.editor.exact"
                        }
                        PatternMode::Regex => {
                            "settings.valuePresentation.rules.editor.regex"
                        }
                    })
                    .to_owned(),
                )
            })
            .collect();
        let mut content = row()
            .key(format!("{key}-content"))
            .w_full()
            .gap(8.0) // gap-2
            .center_y()
            .child(
                choice_select::<S>(
                    editor_edit,
                    self.state.open_select == Some(which),
                    which,
                    &format!("{key}-mode"),
                    &mode_entries,
                    mode.as_wire(),
                    110.0,
                    apply_mode,
                )
                .build(k, layer),
            )
            .child(
                text_input::<S>(
                    self.dark,
                    editor_edit,
                    &format!("{key}-value"),
                    value_edit,
                    "",
                    value_field,
                )
                .grow(1.0),
            );
        // The ignore-case checkbox (the regex mode only).
        if mode == PatternMode::Regex {
            let mut checkbox = Checkbox::new(format!("{key}-ignore-case"), ignore_case)
                .label(t("settings.valuePresentation.rules.editor.ignoreCase"));
            {
                let editor_edit = Rc::clone(editor_edit);
                checkbox = checkbox.on_toggle(move |s: &mut S| {
                    let editor = editor_edit(s);
                    toggle_ignore_case(editor);
                });
            }
            content = content.child(checkbox.build(k));
        }
        setting_row(k, layer, format!("{key}-row"), label, None, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::value_presentation::timestamp_contribution::timestamp_contribution;

    fn sample_rule() -> ValuePresentationRule {
        let input = json!({
            "name": "epoch-ms",
            "enabled": true,
            "match": {
                "column": { "kind": "regex", "source": ".*_at$" }
            },
            "options": {
                "interpretation": { "kind": "unix", "unit": "milliseconds" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "local" }
            }
        });
        let ValuePresentationRuleValidation::Ok(rule) =
            validate_value_presentation_rule(&input)
        else {
            panic!("the fixture must validate");
        };
        *rule
    }

    #[test]
    fn the_default_form_matches_react() {
        let form = RuleFormState::default_form();
        assert_eq!(form.name.text, "");
        assert!(form.enabled);
        assert_eq!(form.table_mode, PatternMode::Exact);
        assert_eq!(form.column_mode, PatternMode::Regex, "the column defaults to regex");
        assert_eq!(form.type_mode, PatternMode::Exact);
        assert!(!form.table_ignore_case && !form.column_ignore_case && !form.type_ignore_case);
        assert_eq!(form.interpretation, InterpretationChoice::UnixMilliseconds);
        assert_eq!(form.format, FormatSerde::IsoTz);
        assert_eq!(form.display_time_zone_kind, TimeZoneKindChoice::Local);
        assert_eq!(form.source_time_zone_kind, SourceTimeZoneChoice::None);
        assert_eq!(form.display_iana_name.text, "");
        // The wire vocabulary round-trips.
        assert_eq!(PatternMode::from_wire("exact"), Some(PatternMode::Exact));
        assert_eq!(PatternMode::from_wire("regex"), Some(PatternMode::Regex));
        assert_eq!(PatternMode::from_wire("fuzzy"), None);
        assert_eq!(
            InterpretationChoice::from_wire("unix-seconds"),
            Some(InterpretationChoice::UnixSeconds)
        );
        assert_eq!(InterpretationChoice::from_wire("unix"), None);
        assert_eq!(format_from_wire("iso-tz"), Some(FormatSerde::IsoTz));
        assert_eq!(format_from_wire("rfc"), None);
        assert_eq!(
            SourceTimeZoneChoice::from_wire("none"),
            Some(SourceTimeZoneChoice::None)
        );
        assert_eq!(TimeZoneKindChoice::from_wire("iana"), Some(TimeZoneKindChoice::Iana));
    }

    #[test]
    fn pattern_input_trims_and_flags() {
        assert_eq!(pattern_input(PatternMode::Exact, "  orders  ", false), Some(json!({"kind": "exact", "value": "orders"})));
        assert_eq!(pattern_input(PatternMode::Regex, "  .*_at$  ", false), Some(json!({"kind": "regex", "source": ".*_at$"})));
        assert_eq!(
            pattern_input(PatternMode::Regex, ".*_AT$", true),
            Some(json!({"kind": "regex", "source": ".*_AT$", "flags": "i"}))
        );
        // Only 'i' exists — never the other flags.
        assert_eq!(pattern_input(PatternMode::Exact, "   ", false), None);
        assert_eq!(pattern_input(PatternMode::Regex, "", true), None);
        // The time zone spec trims the IANA name.
        assert_eq!(
            time_zone_spec(TimeZoneKindChoice::Iana, "  Asia/Shanghai "),
            TimeZoneSpec::Iana { name: "Asia/Shanghai".to_owned() }
        );
        assert_eq!(time_zone_spec(TimeZoneKindChoice::Utc, "ignored"), TimeZoneSpec::Utc);
    }

    #[test]
    fn the_form_assembles_the_rule_input() {
        let mut form = RuleFormState::default_form();
        form.name.text = "epoch-ms".to_owned();
        form.column_value.text = ".*_at$".to_owned();
        form.connection_id.text = "  pg-main ".to_owned();
        form.table_value.text = "   ".to_owned(); // blank → absent
        let input = form_to_rule_input(&form);
        // The blank trims vanish; the connection trims.
        assert_eq!(input["match"]["connectionId"], json!("pg-main"));
        assert!(input["match"].get("table").is_none());
        assert_eq!(input["match"]["column"], json!({"kind": "regex", "source": ".*_at$"}));
        assert_eq!(input["options"]["interpretation"], json!({"kind": "unix", "unit": "milliseconds"}));
        assert_eq!(input["options"]["format"], json!("iso-tz"));
        assert_eq!(input["options"]["displayTimeZone"], json!({"kind": "local"}));
        assert!(input["options"].get("sourceTimeZone").is_none());
        // The source zone appears only when chosen.
        form.interpretation = InterpretationChoice::Native;
        form.source_time_zone_kind = SourceTimeZoneChoice::Iana;
        form.source_iana_name.text = "UTC".to_owned();
        let input = form_to_rule_input(&form);
        assert_eq!(
            input["options"]["sourceTimeZone"],
            json!({"kind": "iana", "name": "UTC"})
        );
        assert_eq!(input["options"]["interpretation"], json!({"kind": "native"}));
    }

    #[test]
    fn the_form_round_trips_a_rule() {
        let rule = sample_rule();
        let form = form_from_rule(&rule);
        assert_eq!(form.name.text, "epoch-ms");
        assert!(form.enabled);
        assert_eq!(form.column_mode, PatternMode::Regex);
        assert_eq!(form.column_value.text, ".*_at$");
        assert!(!form.column_ignore_case);
        // An absent pattern keeps the field's default mode + blank.
        assert_eq!(form.table_mode, PatternMode::Exact);
        assert_eq!(form.table_value.text, "");
        // An absent pattern keeps the field's default mode + blank
        // (the column's own default is regex).
        let table_only = json!({
            "name": "table-only",
            "enabled": true,
            "match": { "table": { "kind": "exact", "value": "orders" } },
            "options": {
                "interpretation": { "kind": "unix", "unit": "seconds" },
                "format": "iso",
                "displayTimeZone": { "kind": "utc" }
            }
        });
        let ValuePresentationRuleValidation::Ok(table_only) =
            validate_value_presentation_rule(&table_only)
        else {
            panic!("the fixture must validate");
        };
        let form = form_from_rule(&table_only);
        assert_eq!(form.table_mode, PatternMode::Exact);
        assert_eq!(form.table_value.text, "orders");
        assert_eq!(form.column_mode, PatternMode::Regex, "the absent column keeps the regex default");
        assert_eq!(form.column_value.text, "");
        // A unix-seconds rule round-trips its unit (the split
        // survives the form).
        let input = form_to_rule_input(&form);
        let ValuePresentationRuleValidation::Ok(round) = validate_value_presentation_rule(&input)
        else {
            panic!("the seconds round trip must validate");
        };
        assert_eq!(round.options, table_only.options);
        // A flags 'i' regex latches the ignore-case checkbox.
        let flagged = json!({
            "name": "flagged",
            "enabled": false,
            "match": { "column": { "kind": "regex", "source": "X", "flags": "i" } },
            "options": {
                "interpretation": { "kind": "native" },
                "format": "locale",
                "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                "sourceTimeZone": { "kind": "utc" }
            }
        });
        let ValuePresentationRuleValidation::Ok(flagged) =
            validate_value_presentation_rule(&flagged)
        else {
            panic!("the fixture must validate");
        };
        let form = form_from_rule(&flagged);
        assert!(!form.enabled);
        assert!(form.column_ignore_case);
        assert_eq!(form.interpretation, InterpretationChoice::Native);
        assert_eq!(form.format, FormatSerde::Locale);
        assert_eq!(form.display_time_zone_kind, TimeZoneKindChoice::Iana);
        assert_eq!(form.display_iana_name.text, "Asia/Shanghai");
        assert_eq!(form.source_time_zone_kind, SourceTimeZoneChoice::Utc);
    }

    #[test]
    fn every_edit_clears_the_error() {
        let mut state = ValuePresentationRuleEditorState::new(Some(&sample_rule()));
        state.error = Some("stale".to_owned());
        state.edit_landed();
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_enabled(false);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_column_mode(PatternMode::Exact);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_interpretation(InterpretationChoice::Native);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_format(FormatSerde::Locale);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_display_time_zone_kind(TimeZoneKindChoice::Iana);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_source_time_zone_kind(SourceTimeZoneChoice::Utc);
        assert_eq!(state.error, None);
        state.error = Some("stale".to_owned());
        state.set_table_ignore_case(true);
        assert_eq!(state.error, None);
        // The source-time-zone face follows the interpretation.
        assert!(state.show_source_time_zone());
        state.set_interpretation(InterpretationChoice::UnixMilliseconds);
        assert!(!state.show_source_time_zone());
    }

    #[test]
    fn an_invalid_rule_refuses_with_the_pathed_sentence() {
        let mut state = ValuePresentationRuleEditorState::new(None);
        // No name and no match conditions: two problems.
        assert!(state.begin_save(&[]).is_none());
        let error = state.error.expect("the refusal sentence");
        assert!(error.starts_with(t("settings.valuePresentation.rules.editor.invalidRule")));
        assert!(error.contains("name"));
        // A duplicate name refuses alone.
        let mut state = ValuePresentationRuleEditorState::new(None);
        state.form.name.text = "epoch-ms".to_owned();
        state.form.column_value.text = ".*_at$".to_owned();
        let others = vec!["epoch-ms".to_owned()];
        assert!(state.begin_save(&others).is_none());
        assert_eq!(
            state.error.as_deref(),
            Some(t("settings.valuePresentation.rules.editor.duplicateName"))
        );
        // A fresh valid name saves the validated rule.
        state.form.name.text = "epoch-ms-2".to_owned();
        let saved = state.begin_save(&others).expect("the validated rule");
        assert_eq!(saved.name, "epoch-ms-2");
        assert!(saved.r#match.column.is_some());
    }

    #[test]
    fn the_sample_preview_walks_its_five_faces() {
        let environment = PresentationEnvironment { locale: "en-US", local_time_zone: "Asia/Shanghai" };
        // A blank sample is no-sample (configuring stays allowed).
        let mut state = ValuePresentationRuleEditorState::new(Some(&sample_rule()));
        assert_eq!(state.sample_preview(None, environment), RuleSamplePreview::NoSample);
        // An invalid form never previews either.
        state.form.name.text = "has space".to_owned();
        state.form.sample_value.text = "1700000000000".to_owned();
        state.form.sample_column.text = "created_at".to_owned();
        assert_eq!(state.sample_preview(None, environment), RuleSamplePreview::NoSample);
        state.form.name.text = "epoch-ms".to_owned();
        // A valid form without a contribution is unavailable.
        assert_eq!(state.sample_preview(None, environment), RuleSamplePreview::Unavailable);
        // A value without a column is still no-sample (both are
        // required before anything runs).
        state.form.sample_column.text = String::new();
        state.form.sample_value.text = "1700000000000".to_owned();
        assert_eq!(state.sample_preview(None, environment), RuleSamplePreview::NoSample);
        state.form.sample_column.text = "created_at".to_owned();
        assert_eq!(state.sample_preview(None, environment), RuleSamplePreview::Unavailable);
        // The bundled contribution: a matching, applicable column
        // presents (the int8 sample type resolves to Number, and
        // unix applies to numbers).
        state.form.sample_type.text = "int8".to_owned();
        let contribution = timestamp_contribution();
        match state.sample_preview(Some(contribution.clone()), environment) {
            RuleSamplePreview::Matched { text, .. } => {
                assert!(!text.is_empty(), "the unix-millis sample formats");
            }
            other => panic!("a matching sample presents: {other:?}"),
        }
        // A typeless sample column is semantic-other: the honest
        // not-applicable unchanged (React's own gate).
        state.form.sample_type.text = String::new();
        match state.sample_preview(Some(contribution.clone()), environment) {
            RuleSamplePreview::MatchedUnchanged { reason } => {
                assert_eq!(reason.as_deref(), Some("not-applicable"));
            }
            other => panic!("a typeless sample stays unchanged: {other:?}"),
        }
        // A non-matching column is not-matched.
        state.form.sample_type.text = "int8".to_owned();
        state.form.sample_column.text = "total_price".to_owned();
        assert_eq!(
            state.sample_preview(Some(contribution), environment),
            RuleSamplePreview::NotMatched
        );
        // A non-integer unix sample falls back unchanged.
        state.form.sample_column.text = "created_at".to_owned();
        state.form.sample_value.text = "not-a-number".to_owned();
        match state.sample_preview(Some(timestamp_contribution()), environment) {
            RuleSamplePreview::MatchedUnchanged { .. } => {}
            other => panic!("an unparsable unix sample stays unchanged: {other:?}"),
        }
    }

    #[test]
    fn the_source_context_needs_table_and_connection_both() {
        let environment = PresentationEnvironment { locale: "en-US", local_time_zone: "Asia/Shanghai" };
        let rule = sample_rule();
        let contribution = timestamp_contribution();
        // Both present: the source rides.
        let preview = compute_rule_sample_preview(&RuleSampleInput {
            rule: &rule,
            sample_value: "1700000000000",
            sample_column: "created_at",
            sample_type_name: Some("int8"),
            sample_connection_id: Some("pg-main"),
            sample_database: Some("shop"),
            sample_namespace: None,
            sample_table: Some("orders"),
            contribution: Some(contribution.clone()),
            environment,
        });
        assert!(matches!(preview, RuleSamplePreview::Matched { .. }));
        // The table without the connection: no source (React's own
        // condition), still presented.
        let preview = compute_rule_sample_preview(&RuleSampleInput {
            rule: &rule,
            sample_value: "1700000000000",
            sample_column: "created_at",
            sample_type_name: Some("int8"),
            sample_connection_id: None,
            sample_database: None,
            sample_namespace: None,
            sample_table: Some("orders"),
            contribution: Some(contribution),
            environment,
        });
        assert!(matches!(preview, RuleSamplePreview::Matched { .. }));
    }

    #[test]
    fn the_select_machine_toggles_and_closes() {
        let mut state = ValuePresentationRuleEditorState::new(None);
        assert_eq!(state.open_select, None);
        state.toggle_select(EditorSelect::Interpretation);
        assert_eq!(state.open_select, Some(EditorSelect::Interpretation));
        // The same select toggles closed; a different one switches.
        state.toggle_select(EditorSelect::Interpretation);
        assert_eq!(state.open_select, None);
        state.toggle_select(EditorSelect::Format);
        state.toggle_select(EditorSelect::DisplayTimeZone);
        assert_eq!(state.open_select, Some(EditorSelect::DisplayTimeZone));
        // The scrim's close clears whatever is open.
        state.close_select();
        assert_eq!(state.open_select, None);
    }

    #[test]
    fn a_new_rule_and_an_edit_ride_the_same_form() {
        let fresh = ValuePresentationRuleEditorState::new(None);
        assert!(fresh.is_new());
        assert_eq!(fresh.form.name.text, "");
        let editing = ValuePresentationRuleEditorState::new(Some(&sample_rule()));
        assert!(!editing.is_new());
        assert_eq!(editing.form.name.text, "epoch-ms");
        assert_eq!(editing.initial_rule.as_ref().map(|r| r.name.as_str()), Some("epoch-ms"));
        // The rule the form assembles is structurally the rule that
        // loaded it (the canonical round trip).
        let input = form_to_rule_input(&editing.form);
        let ValuePresentationRuleValidation::Ok(round) = validate_value_presentation_rule(&input)
        else {
            panic!("the round trip must validate");
        };
        assert_eq!(round.name, sample_rule().name);
        assert_eq!(round.options, sample_rule().options);
    }
}
