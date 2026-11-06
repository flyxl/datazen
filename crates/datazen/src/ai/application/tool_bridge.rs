//! Opt-in plugin and workflow tool bridges (T22).
//!
//! The model-facing names are fixed (`plugin_<id>__<tool>`,
//! `workflow_<id>`). This module owns the provider-neutral rules: name
//! parsing, argument-schema validation, the five-minute approval vocabulary,
//! conservative workflow read-only classification and result shaping. Hosts
//! own manifest/workflow storage, the approval presenter, execution and audit.

use crate::ai::domain::types::ToolDefinition;
use crate::workflow::domain::model::{ErrorHandlingConfig, WorkflowDefinition, WorkflowStep};
use datazen_driver_api::SqlLiteralDialect;
use serde_json::{Map, Value};
use std::time::Duration;

pub const PLUGIN_TOOL_PREFIX: &str = "plugin_";
pub const WORKFLOW_TOOL_PREFIX: &str = "workflow_";
/// Non-read-only bridge calls pause for this long before the host denies them.
pub const BRIDGE_APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Hard cap applied before a bridge result reaches the model. The tool loop
/// applies its own gate cap as well; this keeps a workflow's full result from
/// ever being buffered without a bound.
pub const BRIDGE_RESULT_LIMIT: usize = 64 * 1024;
const MAX_ARGS_BYTES: usize = 64 * 1024;
const MAX_ARG_DEPTH: usize = 16;

/// A parsed bridge name. The payload is not trusted until the host resolves it
/// against the current opt-in settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeToolKind {
    Plugin {
        plugin_id: String,
        tool_name: String,
    },
    Workflow {
        workflow_id: String,
    },
}

impl BridgeToolKind {
    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin { .. })
    }

    pub fn is_workflow(&self) -> bool {
        matches!(self, Self::Workflow { .. })
    }
}

/// The only decisions a bridge approval may produce. `Deny` carries a short,
/// credential-free reason safe to show back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeToolDecision {
    Allow,
    Deny { reason: String },
    Timeout,
    Cancel,
}

impl BridgeToolDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancel)
    }

    pub fn audit_decision(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny { .. } => "deny",
            Self::Timeout => "timeout",
            Self::Cancel => "cancel",
        }
    }

    pub fn refusal_reason(&self) -> String {
        match self {
            Self::Allow => String::new(),
            Self::Deny { reason } => reason.clone(),
            Self::Timeout => "approval timed out (5 minutes)".into(),
            Self::Cancel => "cancelled by the user".into(),
        }
    }
}

pub fn plugin_tool_name(plugin_id: &str, tool_name: &str) -> String {
    format!("{PLUGIN_TOOL_PREFIX}{plugin_id}__{tool_name}")
}

pub fn workflow_tool_name(workflow_id: &str) -> String {
    format!("{WORKFLOW_TOOL_PREFIX}{workflow_id}")
}

/// Parse either bridge prefix. Plugin ids may contain dots/hyphens but never
/// `__`; the separator therefore resolves unambiguously.
pub fn parse_bridge_tool(name: &str) -> Option<BridgeToolKind> {
    if let Some(rest) = name.strip_prefix(PLUGIN_TOOL_PREFIX) {
        let (plugin_id, tool_name) = rest.split_once("__")?;
        if plugin_id.is_empty()
            || tool_name.is_empty()
            || plugin_id.contains("__")
            || tool_name.contains("__")
        {
            return None;
        }
        return Some(BridgeToolKind::Plugin {
            plugin_id: plugin_id.to_string(),
            tool_name: tool_name.to_string(),
        });
    }
    if let Some(workflow_id) = name.strip_prefix(WORKFLOW_TOOL_PREFIX) {
        if workflow_id.is_empty() || workflow_id.contains('/') {
            return None;
        }
        return Some(BridgeToolKind::Workflow {
            workflow_id: workflow_id.to_string(),
        });
    }
    None
}

pub fn plugin_tool_definition(
    plugin_id: &str,
    tool_name: &str,
    description: &str,
    input_schema: &Value,
) -> ToolDefinition {
    ToolDefinition {
        name: plugin_tool_name(plugin_id, tool_name),
        description: description.to_string(),
        parameters: input_schema.clone(),
    }
}

pub fn workflow_tool_definition(
    workflow_id: &str,
    name: &str,
    description: &str,
    variables: &[crate::workflow::domain::model::WorkflowVariable],
) -> ToolDefinition {
    let mut text = format!("Run the saved workflow \"{name}\".");
    if !description.trim().is_empty() {
        // The host-supplied workflow description is data, never an instruction
        // that can widen this tool's authority.
        text.push(' ');
        text.push_str(description.trim());
    }
    ToolDefinition {
        name: workflow_tool_name(workflow_id),
        description: text,
        parameters: workflow_input_schema(variables),
    }
}

/// Build the model-facing JSON schema for a workflow's variables.
pub fn workflow_input_schema(
    variables: &[crate::workflow::domain::model::WorkflowVariable],
) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for variable in variables {
        let mut schema = Map::new();
        match variable.var_type.to_ascii_lowercase().as_str() {
            "string" | "text" => {
                schema.insert("type".into(), Value::String("string".into()));
            }
            "number" | "float" | "double" => {
                schema.insert("type".into(), Value::String("number".into()));
            }
            "integer" | "int" => {
                schema.insert("type".into(), Value::String("integer".into()));
            }
            "boolean" | "bool" => {
                schema.insert("type".into(), Value::String("boolean".into()));
            }
            "array" => {
                schema.insert("type".into(), Value::String("array".into()));
            }
            "object" | "json" => {
                schema.insert("type".into(), Value::String("object".into()));
            }
            _ => {}
        }
        if !variable.description.trim().is_empty() {
            schema.insert(
                "description".into(),
                Value::String(variable.description.clone()),
            );
        }
        if let Some(default) = &variable.default {
            schema.insert("default".into(), default.clone());
        }
        properties.insert(variable.name.clone(), Value::Object(schema));
        if variable.required.unwrap_or(false) {
            required.push(Value::String(variable.name.clone()));
        }
    }
    let mut root = Map::new();
    root.insert("type".into(), Value::String("object".into()));
    root.insert("properties".into(), Value::Object(properties));
    root.insert("required".into(), Value::Array(required));
    root.insert("additionalProperties".into(), Value::Bool(false));
    Value::Object(root)
}

/// Parse and validate model-supplied arguments against a tool's declared
/// schema. The arguments are rejected, not coerced, on mismatch.
pub fn parse_and_validate_arguments(arguments: &str, schema: &Value) -> Result<Value, String> {
    if arguments.len() > MAX_ARGS_BYTES {
        return Err("arguments are too large".into());
    }
    let value: Value =
        serde_json::from_str(arguments).map_err(|_| "arguments must be valid JSON".to_string())?;
    validate_value(schema, &value, "arguments", 0)?;
    Ok(value)
}

fn validate_value(schema: &Value, value: &Value, path: &str, depth: usize) -> Result<(), String> {
    if depth > MAX_ARG_DEPTH {
        return Err(format!("{path}: nested too deeply"));
    }
    let Some(object) = schema.as_object() else {
        return Ok(());
    };

    if let Some(constant) = object.get("const") {
        if value != constant {
            return Err(format!("{path}: must equal the declared constant"));
        }
    }
    if let Some(options) = object.get("enum").and_then(Value::as_array) {
        if !options.iter().any(|option| option == value) {
            return Err(format!("{path}: value is not in the allowed set"));
        }
    }
    if let Some(declared) = object.get("type") {
        let matches = match declared {
            Value::String(kind) => type_matches(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| type_matches(kind, value)),
            _ => true,
        };
        if !matches {
            return Err(format!("{path}: has the wrong type"));
        }
    }

    if let Value::Object(map) = value {
        if let Some(required) = object.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(field) {
                    return Err(format!("{path}.{field}: is required"));
                }
            }
        }
        let properties = object.get("properties").and_then(Value::as_object);
        if let Some(properties) = properties {
            for (key, child) in map {
                if let Some(child_schema) = properties.get(key) {
                    validate_value(child_schema, child, &format!("{path}.{key}"), depth + 1)?;
                } else if object.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Err(format!("{path}.{key}: is not an allowed argument"));
                }
            }
        } else if object.get("additionalProperties") == Some(&Value::Bool(false)) && !map.is_empty()
        {
            return Err(format!("{path}: does not accept arguments"));
        }
    }

    if let Value::Array(items) = value {
        if let Some(items_schema) = object.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_value(items_schema, item, &format!("{path}[{index}]"), depth + 1)?;
            }
        }
    }

    if let Value::String(text) = value {
        if let Some(min) = object.get("minLength").and_then(Value::as_u64) {
            if (text.chars().count() as u64) < min {
                return Err(format!("{path}: is shorter than the minimum length"));
            }
        }
        if let Some(max) = object.get("maxLength").and_then(Value::as_u64) {
            if (text.chars().count() as u64) > max {
                return Err(format!("{path}: is longer than the maximum length"));
            }
        }
    }

    if let Some(number) = value.as_f64() {
        if let Some(min) = object.get("minimum").and_then(Value::as_f64) {
            if number < min {
                return Err(format!("{path}: is below the minimum"));
            }
        }
        if let Some(max) = object.get("maximum").and_then(Value::as_f64) {
            if number > max {
                return Err(format!("{path}: is above the maximum"));
            }
        }
    }

    Ok(())
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// A workflow is read-only only when every reachable step is a plain query
/// that the shared SQL guard accepts in read-only mode. Commands, resource
/// blocks, or unparsable SQL all fail closed to "requires approval".
pub fn workflow_is_read_only(workflow: &WorkflowDefinition) -> bool {
    if !workflow.blocks.is_empty() {
        return false;
    }
    if !error_config_is_read_only(workflow.error_handling.as_ref()) {
        return false;
    }
    steps_are_read_only(&workflow.steps)
}

fn steps_are_read_only(steps: &[WorkflowStep]) -> bool {
    steps.iter().all(step_is_read_only)
}

fn step_is_read_only(step: &WorkflowStep) -> bool {
    let read_only = match step {
        WorkflowStep::Query { sql, on_error, .. } => {
            crate::execution::domain::sql_guard::check_sql(sql, true, false).is_ok()
                && error_config_is_read_only(on_error.as_ref())
        }
        WorkflowStep::Command { .. } => false,
        WorkflowStep::Ai { on_error, .. } => error_config_is_read_only(on_error.as_ref()),
        WorkflowStep::Condition {
            then_steps,
            else_steps,
            ..
        } => {
            steps_are_read_only(then_steps)
                && else_steps
                    .as_deref()
                    .map(steps_are_read_only)
                    .unwrap_or(true)
        }
        WorkflowStep::ForEach { steps, .. } => steps_are_read_only(steps),
        WorkflowStep::Merge { on_error, .. } | WorkflowStep::Transform { on_error, .. } => {
            error_config_is_read_only(on_error.as_ref())
        }
    };
    read_only
}

fn error_config_is_read_only(config: Option<&ErrorHandlingConfig>) -> bool {
    config
        .and_then(|config| config.fallback_steps.as_deref())
        .map(steps_are_read_only)
        .unwrap_or(true)
}

/// Truncate a bridge result to the fixed bridge cap. UTF-8 boundaries are
/// preserved so the result can always be sent back to a provider.
pub fn truncate_bridge_result(result: &str) -> String {
    if result.len() <= BRIDGE_RESULT_LIMIT {
        return result.to_string();
    }
    let mut end = BRIDGE_RESULT_LIMIT;
    while end > 0 && !result.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\u{2026}(truncated: {} bytes omitted)",
        &result[..end],
        result.len() - end
    )
}

/// Best-effort row count for audit metadata. Payloads never leave this helper;
/// only the count is returned.
pub fn count_result_rows(value: &Value) -> Option<i64> {
    match value {
        Value::Array(rows) => Some(rows.len() as i64),
        Value::Object(map) => {
            for key in ["rows", "data", "results", "items"] {
                if let Some(Value::Array(rows)) = map.get(key) {
                    return Some(rows.len() as i64);
                }
            }
            None
        }
        _ => None,
    }
}

/// How a SQL template treats backslashes inside single-quoted strings.
///
/// The host chooses this from the target connection's driver. The shared
/// renderer defaults to the SQL standard, where a backslash is an ordinary
/// character; MySQL-family engines use backslash escapes instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlTemplateDialect {
    #[default]
    Standard,
    BackslashEscapes,
}

impl From<SqlLiteralDialect> for SqlTemplateDialect {
    fn from(dialect: SqlLiteralDialect) -> Self {
        match dialect {
            SqlLiteralDialect::MySql | SqlLiteralDialect::ClickHouse => Self::BackslashEscapes,
            SqlLiteralDialect::DuckDb
            | SqlLiteralDialect::Postgres
            | SqlLiteralDialect::SqlServer
            | SqlLiteralDialect::Sqlite => Self::Standard,
        }
    }
}

/// Render one plugin input template with validated model arguments.
///
/// SQL-shaped fields (a `sql`/`*_sql` key, or a bare string template) use the
/// SQL-aware renderer. Other fields retain the legacy typed placeholder
/// behavior so a template such as `{"limit":"{{limit}}"}` still yields a
/// number rather than a string.
pub fn render_input_template(template: &Value, arguments: &Value) -> Result<Value, String> {
    render_input_template_with_dialect(template, arguments, SqlTemplateDialect::Standard)
}

/// SQL-aware variant used by hosts that know the target driver.
pub fn render_input_template_with_dialect(
    template: &Value,
    arguments: &Value,
    dialect: SqlTemplateDialect,
) -> Result<Value, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "arguments must be an object".to_string())?;
    let root_is_sql = template.is_string();
    render_template_value(template, arguments, 0, dialect, root_is_sql)
}

fn render_template_value(
    value: &Value,
    arguments: &Map<String, Value>,
    depth: usize,
    dialect: SqlTemplateDialect,
    is_sql: bool,
) -> Result<Value, String> {
    if depth > MAX_ARG_DEPTH {
        return Err("input template is nested too deeply".into());
    }
    match value {
        Value::String(text) => {
            if is_sql {
                return Ok(Value::String(render_sql_template(
                    text, arguments, dialect,
                )?));
            }
            if let Some(name) = exact_placeholder(text) {
                return arguments
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("missing template argument '{name}'"));
            }
            Ok(Value::String(render_plain_template(text, arguments)?))
        }
        Value::Array(items) => {
            if is_sql {
                return Err("a SQL template must be a string".into());
            }
            Ok(Value::Array(
                items
                    .iter()
                    .map(|item| render_template_value(item, arguments, depth + 1, dialect, false))
                    .collect::<Result<_, _>>()?,
            ))
        }
        Value::Object(map) => {
            if is_sql {
                return Err("a SQL template must be a string".into());
            }
            let mut out = Map::new();
            for (key, item) in map {
                let child_is_sql = is_sql_template_key(key);
                out.insert(
                    key.clone(),
                    render_template_value(item, arguments, depth + 1, dialect, child_is_sql)?,
                );
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

fn is_sql_template_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower == "sql" || lower.ends_with("_sql")
}

fn render_plain_template(text: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    let mut rendered = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let before = &rest[..start];
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err("unterminated template placeholder".into());
        };
        let name = &after_start[..end];
        let Some(argument) = arguments.get(name) else {
            return Err(format!("missing template argument '{name}'"));
        };
        rendered.push_str(before);
        rendered.push_str(&display_argument(argument));
        rest = &after_start[end + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SqlLexState {
    Normal,
    Single { escape_backslash: bool },
    Double,
    Backtick,
    Bracket,
    LineComment,
    BlockComment { depth: usize },
    DollarQuote { delimiter: String },
}

/// Render a SQL template without ever concatenating an arbitrary value into
/// an identifier or expression.
///
/// The scanner is deliberately small but strict:
/// - placeholders inside single-quoted literals become escaped string content;
/// - placeholders outside a string must be a number/bool/NULL and token
///   delimited, otherwise the call is rejected;
/// - placeholders in double-quoted identifiers, dollar quotes and comments are
///   rejected (fail closed) rather than guessed;
/// - unterminated quotes/comments/dollar quotes and unterminated placeholders
///   are errors.
pub fn render_sql_template(
    template: &str,
    arguments: &Map<String, Value>,
    dialect: SqlTemplateDialect,
) -> Result<String, String> {
    let mut output = String::with_capacity(template.len());
    let mut state = SqlLexState::Normal;
    let mut index = 0;

    while index < template.len() {
        let rest = &template[index..];
        if let Some(rest) = rest.strip_prefix("{{") {
            let Some(end) = rest.find("}}") else {
                return Err("unterminated template placeholder".into());
            };
            let name = &rest[..end];
            if name.is_empty() || name.contains("{{") || name.contains("}}") {
                return Err("template placeholder name is invalid".into());
            }
            let argument = arguments
                .get(name)
                .ok_or_else(|| format!("missing template argument '{name}'"))?;
            match &state {
                SqlLexState::Normal => {
                    let literal = safe_sql_literal(argument)?;
                    if !template_boundary_before(template, index)
                        || !template_boundary_after(template, index + 2 + end + 2)
                    {
                        return Err(format!(
                            "template argument '{name}' cannot be concatenated into a SQL token"
                        ));
                    }
                    output.push_str(&literal);
                }
                SqlLexState::Single { escape_backslash } => {
                    output.push_str(&sql_string_content(argument, *escape_backslash)?);
                }
                SqlLexState::Double => {
                    return Err(format!(
                        "template argument '{name}' inside a double-quoted identifier is not allowed"
                    ));
                }
                SqlLexState::DollarQuote { .. } => {
                    return Err(format!(
                        "template argument '{name}' inside a dollar-quoted string is not allowed"
                    ));
                }
                SqlLexState::LineComment | SqlLexState::BlockComment { .. } => {
                    return Err(format!(
                        "template argument '{name}' inside a SQL comment is not allowed"
                    ));
                }
                SqlLexState::Backtick | SqlLexState::Bracket => {
                    return Err(format!(
                        "template argument '{name}' inside a quoted identifier is not allowed"
                    ));
                }
            }
            index += 2 + end + 2;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        let next = match &state {
            SqlLexState::Normal => {
                if rest.starts_with("--") {
                    output.push_str("--");
                    index += 2;
                    SqlLexState::LineComment
                } else if rest.starts_with("/*") {
                    output.push_str("/*");
                    index += 2;
                    SqlLexState::BlockComment { depth: 1 }
                } else if ch == '#' && dialect == SqlTemplateDialect::BackslashEscapes {
                    output.push('#');
                    index += 1;
                    SqlLexState::LineComment
                } else if let Some((delimiter, len)) = dollar_quote_open(rest) {
                    output.push_str(delimiter);
                    index += len;
                    SqlLexState::DollarQuote {
                        delimiter: delimiter.to_string(),
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    match ch {
                        '\'' => SqlLexState::Single {
                            escape_backslash: dialect == SqlTemplateDialect::BackslashEscapes
                                || has_postgres_escape_prefix(template, index - ch.len_utf8()),
                        },
                        '"' => SqlLexState::Double,
                        '`' => SqlLexState::Backtick,
                        '[' => SqlLexState::Bracket,
                        _ => SqlLexState::Normal,
                    }
                }
            }
            SqlLexState::Single { escape_backslash } => {
                if ch == '\'' {
                    if rest[1..].starts_with('\'') {
                        output.push_str("''");
                        index += 2;
                        SqlLexState::Single {
                            escape_backslash: *escape_backslash,
                        }
                    } else {
                        output.push('\'');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && *escape_backslash {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    // MySQL can run with NO_BACKSLASH_ESCAPES, where `\'`
                    // closes the string instead of escaping the quote. The
                    // same template would then have a different string
                    // context, so reject that ambiguous spelling rather than
                    // render a value under the wrong assumption.
                    if dialect == SqlTemplateDialect::BackslashEscapes && escaped == '\'' {
                        return Err(
                            "a backslash-escaped quote is ambiguous in a backslash-escape SQL dialect"
                                .into(),
                        );
                    }
                    // In a backslash-escape string `\{{` would consume the
                    // opening brace of a placeholder as an escaped character.
                    // Reject instead of silently leaving a half-rendered
                    // template in the SQL text.
                    if escaped == '{' && rest[1..].starts_with("{{") {
                        return Err("template placeholder cannot follow a backslash escape".into());
                    }
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Single {
                        escape_backslash: true,
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Single {
                        escape_backslash: *escape_backslash,
                    }
                }
            }
            SqlLexState::Double => {
                if ch == '"' {
                    if rest[1..].starts_with('"') {
                        output.push_str("\"\"");
                        index += 2;
                        SqlLexState::Double
                    } else {
                        output.push('"');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && dialect == SqlTemplateDialect::BackslashEscapes {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Double
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Double
                }
            }
            SqlLexState::Backtick => {
                if ch == '`' {
                    if rest[1..].starts_with('`') {
                        output.push_str("``");
                        index += 2;
                        SqlLexState::Backtick
                    } else {
                        output.push('`');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else if ch == '\\' && dialect == SqlTemplateDialect::BackslashEscapes {
                    let mut chars = rest.chars();
                    chars.next();
                    let Some(escaped) = chars.next() else {
                        return Err("unterminated escape at end of SQL template".into());
                    };
                    output.push('\\');
                    output.push(escaped);
                    index += 1 + escaped.len_utf8();
                    SqlLexState::Backtick
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Backtick
                }
            }
            SqlLexState::Bracket => {
                if ch == ']' {
                    if rest[1..].starts_with(']') {
                        output.push_str("]]");
                        index += 2;
                        SqlLexState::Bracket
                    } else {
                        output.push(']');
                        index += 1;
                        SqlLexState::Normal
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::Bracket
                }
            }
            SqlLexState::LineComment => {
                output.push(ch);
                index += ch.len_utf8();
                if ch == '\n' {
                    SqlLexState::Normal
                } else {
                    SqlLexState::LineComment
                }
            }
            SqlLexState::BlockComment { depth } => {
                if rest.starts_with("/*") {
                    output.push_str("/*");
                    index += 2;
                    SqlLexState::BlockComment { depth: depth + 1 }
                } else if rest.starts_with("*/") {
                    output.push_str("*/");
                    index += 2;
                    if *depth <= 1 {
                        SqlLexState::Normal
                    } else {
                        SqlLexState::BlockComment { depth: depth - 1 }
                    }
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::BlockComment { depth: *depth }
                }
            }
            SqlLexState::DollarQuote { delimiter } => {
                if rest.starts_with(delimiter.as_str()) {
                    output.push_str(delimiter);
                    index += delimiter.len();
                    SqlLexState::Normal
                } else {
                    output.push(ch);
                    index += ch.len_utf8();
                    SqlLexState::DollarQuote {
                        delimiter: delimiter.clone(),
                    }
                }
            }
        };
        state = next;
    }

    match state {
        SqlLexState::Normal | SqlLexState::LineComment => Ok(output),
        SqlLexState::Single { .. } => Err("unterminated single-quoted SQL string".into()),
        SqlLexState::Double => Err("unterminated double-quoted SQL identifier".into()),
        SqlLexState::Backtick => Err("unterminated backtick-quoted SQL identifier".into()),
        SqlLexState::Bracket => Err("unterminated bracketed SQL identifier".into()),
        SqlLexState::BlockComment { .. } => Err("unterminated SQL block comment".into()),
        SqlLexState::DollarQuote { .. } => Err("unterminated SQL dollar quote".into()),
    }
}

fn safe_sql_literal(value: &Value) -> Result<String, String> {
    match value {
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(flag) => Ok(if *flag { "TRUE" } else { "FALSE" }.to_string()),
        Value::Null => Ok("NULL".to_string()),
        Value::String(_) => Err(
            "string template arguments are only allowed inside a single-quoted SQL literal".into(),
        ),
        Value::Array(_) | Value::Object(_) => {
            Err("array/object template arguments are not valid SQL literals".into())
        }
    }
}

fn sql_string_content(value: &Value, escape_backslash: bool) -> Result<String, String> {
    let rendered = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => {
            if *flag {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Null => "NULL".to_string(),
        Value::Array(_) | Value::Object(_) => {
            return Err("array/object template arguments cannot be placed in a SQL string".into());
        }
    };
    Ok(escape_sql_string_content(&rendered, escape_backslash))
}

fn escape_sql_string_content(value: &str, escape_backslash: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\'' => escaped.push_str("''"),
            '\\' if escape_backslash => escaped.push_str("\\\\"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn template_boundary_before(template: &str, index: usize) -> bool {
    let previous = template[..index].chars().next_back();
    !previous.is_some_and(is_sql_identifier_char) && previous != Some('.')
}

fn template_boundary_after(template: &str, index: usize) -> bool {
    let next = template[index..].chars().next();
    !next.is_some_and(is_sql_identifier_char) && next != Some('.')
}

fn is_sql_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '@'
}

fn has_postgres_escape_prefix(template: &str, quote_index: usize) -> bool {
    let before = &template[..quote_index];
    let mut chars = before.chars().rev();
    let Some(prefix) = chars.next() else {
        return false;
    };
    if !matches!(prefix, 'e' | 'E') {
        return false;
    }
    !chars.next().is_some_and(is_sql_identifier_char)
}

/// Recognise `$$` and `$tag$` dollar-quote openers at the start of `rest`.
fn dollar_quote_open(rest: &str) -> Option<(&str, usize)> {
    if !rest.starts_with('$') {
        return None;
    }
    if rest.starts_with("$$") {
        return Some((&rest[..2], 2));
    }
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || !(bytes[1].is_ascii_alphabetic() || bytes[1] == b'_') {
        return None;
    }
    let mut end = 2;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if bytes.get(end) == Some(&b'$') {
        Some((&rest[..=end], end + 1))
    } else {
        None
    }
}

fn exact_placeholder(text: &str) -> Option<&str> {
    text.strip_prefix("{{")?
        .strip_suffix("}}")
        .filter(|name| !name.is_empty() && !name.contains("{{") && !name.contains("}}"))
}

fn display_argument(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    #[test]
    fn names_round_trip_and_reject_ambiguous_plugins() {
        let parsed = parse_bridge_tool("plugin_acme.demo__list_items").unwrap();
        assert_eq!(
            parsed,
            BridgeToolKind::Plugin {
                plugin_id: "acme.demo".into(),
                tool_name: "list_items".into()
            }
        );
        assert_eq!(
            plugin_tool_name("acme.demo", "list_items"),
            "plugin_acme.demo__list_items"
        );
        assert_eq!(
            parse_bridge_tool("workflow_wf-1"),
            Some(BridgeToolKind::Workflow {
                workflow_id: "wf-1".into()
            })
        );
        assert!(parse_bridge_tool("plugin_acme__").is_none());
        assert!(parse_bridge_tool("plugin_a__b__c").is_none());
        assert!(parse_bridge_tool("workflow_").is_none());
        assert!(parse_bridge_tool("mcp/x/y").is_none());
    }

    #[test]
    fn schema_validation_rejects_missing_wrong_and_unknown_fields() {
        assert!(parse_and_validate_arguments(r#"{"name":"x"}"#, &object_schema()).is_ok());
        assert!(
            parse_and_validate_arguments(r#"{"name":"x","count":2}"#, &object_schema()).is_ok()
        );
        assert!(parse_and_validate_arguments(r#"{}"#, &object_schema()).is_err());
        assert!(
            parse_and_validate_arguments(r#"{"name":"x","count":"2"}"#, &object_schema()).is_err()
        );
        assert!(
            parse_and_validate_arguments(r#"{"name":"x","extra":1}"#, &object_schema()).is_err()
        );
        assert!(parse_and_validate_arguments(r#"{"name":""}"#, &object_schema()).is_err());
        assert!(parse_and_validate_arguments("not json", &object_schema()).is_err());
    }

    #[test]
    fn template_rendering_preserves_typed_exact_values_and_rejects_missing() {
        let template = serde_json::json!({
            "sql": "SELECT * FROM t WHERE id = {{id}} AND name = '{{name}}'",
            "limit": "{{limit}}"
        });
        let args = serde_json::json!({ "id": 7, "name": "Ada", "limit": 20 });
        let rendered = render_input_template(&template, &args).unwrap();
        assert_eq!(
            rendered["sql"],
            "SELECT * FROM t WHERE id = 7 AND name = 'Ada'"
        );
        assert_eq!(rendered["limit"], 20);
        assert!(render_input_template(&template, &serde_json::json!({ "id": 7 })).is_err());
    }

    #[test]
    fn sql_template_escapes_string_literals_and_safe_literals() {
        let template = serde_json::json!({
            "sql": "SELECT * FROM users WHERE name = '{{name}}' AND note = '{{note}}' AND id = {{id}} AND active = {{active}} AND deleted_at = {{deleted}}"
        });
        let arguments = serde_json::json!({
            "name": "O'Brien",
            "note": "x' OR '1'='1",
            "id": 7,
            "active": true,
            "deleted": null
        });
        let rendered = render_input_template(&template, &arguments).expect("safe template renders");
        assert_eq!(
            rendered["sql"],
            "SELECT * FROM users WHERE name = 'O''Brien' AND note = 'x'' OR ''1''=''1' AND id = 7 AND active = TRUE AND deleted_at = NULL"
        );
        assert!(render_sql_template(
            "SELECT {{id}}",
            &serde_json::json!({ "id": 7 }).as_object().cloned().unwrap(),
            SqlTemplateDialect::Standard,
        )
        .is_ok());
    }

    #[test]
    fn sql_template_rejects_placeholder_after_a_pending_backslash_escape() {
        let template = serde_json::json!({ "sql": "SELECT 'a\\{{name}}'" });
        let arguments = serde_json::json!({ "name": "x' OR '1'='1" });
        assert!(
            render_input_template_with_dialect(
                &template,
                &arguments,
                SqlTemplateDialect::BackslashEscapes,
            )
            .is_err(),
            "a pending backslash would reinterpret the escaped quote"
        );

        // A backslash before the closing quote is ambiguous under MySQL's
        // NO_BACKSLASH_ESCAPES mode and must not be guessed.
        let ambiguous_quote = serde_json::json!({ "sql": "SELECT 'a\\'{{name}}'" });
        assert!(
            render_input_template_with_dialect(
                &ambiguous_quote,
                &arguments,
                SqlTemplateDialect::BackslashEscapes,
            )
            .is_err(),
            "NO_BACKSLASH_ESCAPES would move the placeholder outside the string"
        );

        // An even run is a complete escaped backslash, so the value begins in
        // a normal string state and renders safely.
        let escaped = serde_json::json!({ "sql": "SELECT 'a\\\\{{name}}'" });
        let rendered = render_input_template_with_dialect(
            &escaped,
            &arguments,
            SqlTemplateDialect::BackslashEscapes,
        )
        .expect("a completed escaped backslash renders");
        assert_eq!(rendered["sql"], "SELECT 'a\\\\x'' OR ''1''=''1'");

        // Standard SQL treats the backslash as literal, so no escape is
        // pending and the value is still escaped.
        let standard =
            render_input_template_with_dialect(&template, &arguments, SqlTemplateDialect::Standard)
                .expect("standard SQL renders a literal backslash");
        assert_eq!(standard["sql"], "SELECT 'a\\x'' OR ''1''=''1'");
    }

    #[test]
    fn sql_template_rejects_unsafe_outside_literals_and_token_splicing() {
        for value in [
            serde_json::json!("Ada"),
            serde_json::json!(["Ada"]),
            serde_json::json!({ "name": "Ada" }),
        ] {
            let template = serde_json::json!({ "sql": "SELECT * FROM t WHERE name = {{value}}" });
            let arguments = serde_json::json!({ "value": value });
            assert!(
                render_input_template(&template, &arguments).is_err(),
                "arbitrary value must not become an expression"
            );
        }
        for sql in [
            "SELECT x{{id}} FROM t",
            "SELECT {{id}}x FROM t",
            "SELECT t.{{id}} FROM t",
        ] {
            let template = serde_json::json!({ "sql": sql });
            let arguments = serde_json::json!({ "id": 7 });
            assert!(
                render_input_template(&template, &arguments).is_err(),
                "placeholder must not splice into an identifier: {sql}"
            );
        }
    }

    #[test]
    fn sql_template_fails_closed_in_identifiers_dollar_quotes_and_comments() {
        for sql in [
            "SELECT \"{{name}}\" FROM t",
            "SELECT `{{name}}` FROM t",
            "SELECT [{{name}}] FROM t",
            "SELECT $$ {{name}} $$ FROM t",
            "SELECT $tag$ {{name}} $tag$ FROM t",
            "SELECT 1 -- {{name}}\nFROM t",
            "SELECT /* {{name}} */ 1",
        ] {
            let template = serde_json::json!({ "sql": sql });
            let arguments = serde_json::json!({ "name": "Ada" });
            assert!(
                render_input_template(&template, &arguments).is_err(),
                "placeholder context must fail closed: {sql}"
            );
        }
        for sql in [
            "SELECT '{{name}}",
            "SELECT 'unterminated",
            "SELECT $$ unterminated",
            "SELECT /* unterminated",
        ] {
            let template = serde_json::json!({ "sql": sql });
            let arguments = serde_json::json!({ "name": "Ada" });
            assert!(
                render_input_template(&template, &arguments).is_err(),
                "unterminated SQL must fail closed: {sql}"
            );
        }
    }

    #[test]
    fn sql_template_dialect_controls_backslash_escaping() {
        let template = serde_json::json!({
            "sql": "SELECT * FROM t WHERE p = '{{value}}' AND e = E'{{value}}'"
        });
        let arguments = serde_json::json!({ "value": "a\\b'c" });

        let standard =
            render_input_template_with_dialect(&template, &arguments, SqlTemplateDialect::Standard)
                .unwrap();
        assert_eq!(
            standard["sql"],
            "SELECT * FROM t WHERE p = 'a\\b''c' AND e = E'a\\\\b''c'"
        );

        let mysql = render_input_template_with_dialect(
            &template,
            &arguments,
            SqlTemplateDialect::BackslashEscapes,
        )
        .unwrap();
        assert_eq!(
            mysql["sql"],
            "SELECT * FROM t WHERE p = 'a\\\\b''c' AND e = E'a\\\\b''c'"
        );
        assert_eq!(
            SqlTemplateDialect::from(SqlLiteralDialect::MySql),
            SqlTemplateDialect::BackslashEscapes
        );
        assert_eq!(
            SqlTemplateDialect::from(SqlLiteralDialect::Postgres),
            SqlTemplateDialect::Standard
        );
    }

    #[test]
    fn read_only_classification_fails_closed_for_commands_and_sql() {
        let query = WorkflowDefinition {
            id: "wf".into(),
            name: "wf".into(),
            description: String::new(),
            version: None,
            author: None,
            variables: Vec::new(),
            connection: None,
            database: None,
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            blocks: Vec::new(),
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        assert!(workflow_is_read_only(&query));
        let mut write = query.clone();
        write.steps = vec![WorkflowStep::Query {
            id: "q1".into(),
            sql: "DELETE FROM t".into(),
            connection: None,
            database: None,
            timeout_secs: None,
            on_error: None,
        }];
        assert!(!workflow_is_read_only(&write));
        let mut command = query.clone();
        command.steps = vec![WorkflowStep::Command {
            id: "c1".into(),
            command: "execute".into(),
            connection: None,
            input: serde_json::json!({}),
            timeout_secs: None,
            on_error: None,
        }];
        assert!(!workflow_is_read_only(&command));
        let mut fallback_write = query.clone();
        fallback_write.error_handling = Some(ErrorHandlingConfig {
            strategy: crate::workflow::domain::model::ErrorStrategyKind::Fallback,
            fallback_steps: Some(vec![WorkflowStep::Query {
                id: "fb".into(),
                sql: "DELETE FROM t".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }]),
        });
        assert!(
            !workflow_is_read_only(&fallback_write),
            "workflow-level fallback writes must require approval"
        );
    }

    #[test]
    fn result_truncation_respects_utf8_boundaries() {
        let text = "é".repeat(BRIDGE_RESULT_LIMIT);
        let truncated = truncate_bridge_result(&text);
        assert!(truncated.starts_with("é"));
        assert!(truncated.len() <= BRIDGE_RESULT_LIMIT + 64);
    }
}
