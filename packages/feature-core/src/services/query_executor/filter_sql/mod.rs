//! Filter SQL construction and the rules that decide whether a filter may run.
//!
//! Everything here is pure text work: it turns filter conditions into a `WHERE`
//! fragment and it refuses the conditions that must not reach the database.
//! The host owns no filter judgement of its own — it asks this module and maps
//! the answer onto its own error type.
//!
//! Two rules shape the whole module:
//!
//! * **A filter is never silently dropped.** A condition that cannot be
//!   rendered is an error, not an empty string. The old behaviour skipped
//!   incomplete conditions, which meant the query ran against a wider data set
//!   than the user asked for.
//! * **The dialect never guesses.** Which operators exist, how a pattern is
//!   escaped and how a dialect-only operator is spelled are all declared by
//!   the driver; this module only reads those declarations.

use datazen_driver_api::dialect::SqlDialect;
use datazen_driver_api::filters::{FilterCondition, FilterConnector, FilterOperator, LiteralArity};
use datazen_driver_api::Value;

#[cfg(test)]
mod tests;

/// The longest pattern a `regex` / `notRegex` condition may carry.
///
/// A regular expression is handed to the database engine verbatim, so a
/// pathological pattern can pin a whole backend connection. The limit belongs
/// here, in the shared layer, because both the host's pre-check and the
/// service-level fallback have to apply the same number.
pub const MAX_REGEX_PATTERN_LENGTH: usize = 1000;

/// Why a filter list may not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterRejection {
    /// The operator is not in the driver's whitelist, or the driver cannot
    /// render it.
    UnsupportedOperator {
        column: String,
        operator: FilterOperator,
    },
    /// The condition is missing something it needs to be rendered at all.
    Incomplete {
        column: String,
        operator: FilterOperator,
        reason: &'static str,
    },
    /// The driver declares a `LIKE` escape character but no `ESCAPE` clause.
    ///
    /// This is a driver configuration mistake, not a user mistake, but it has
    /// to be refused where the filter is rendered: escaping without the
    /// clause means the escaped `%` degrades back into a wildcard, so the
    /// query would succeed and return the wrong rows.
    InvalidEscapeConfiguration { escape_char: char },
    /// The driver could not render a literal for this value.
    ///
    /// The driver's own message travels along unchanged: it is not a filter
    /// problem, so it carries no contract prefix.
    Driver { message: String },
}

impl FilterRejection {
    /// The message the host hands to the user.
    ///
    /// The two filter reasons carry the contract prefixes
    /// `grid.filter.unsupportedOperator` and `grid.filter.incomplete`; the
    /// driver variant passes the driver's own message through unchanged.
    pub fn contract_message(&self) -> String {
        match self {
            Self::UnsupportedOperator { column, operator } => format!(
                "grid.filter.unsupportedOperator: this database does not support the '{}' filter on column \"{}\"; {}",
                operator.as_str(),
                column,
                alternative_for(*operator)
            ),
            Self::Incomplete {
                column,
                operator,
                reason,
            } => format!(
                "grid.filter.incomplete: column \"{}\" {} (operator '{}')",
                column,
                reason,
                operator.as_str()
            ),
            Self::InvalidEscapeConfiguration { escape_char } => format!(
                "grid.filter.unsupportedOperator: this database declares a LIKE escape character ('{escape_char}') but does not accept an ESCAPE clause, so escaped patterns cannot be rendered"
            ),
            Self::Driver { message } => message.clone(),
        }
    }
}

/// The operators a caller may offer when the driver supports a narrower set.
fn alternative_for(operator: FilterOperator) -> &'static str {
    match operator {
        FilterOperator::Regex | FilterOperator::NotRegex => "try 'like' or 'contains' instead",
        FilterOperator::JsonContains => "try 'eq' or 'contains' instead",
        _ => "use one of the operators this database supports",
    }
}

/// The `LIKE` escaping a dialect declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LikeEscapeConfig {
    /// The escape character; `None` means "do not escape at all".
    pub escape_char: Option<char>,
    /// The `ESCAPE '…'` clause; `None` means "do not emit one".
    pub clause: Option<&'static str>,
}

/// Read the dialect's `LIKE` escaping configuration.
///
/// The two halves have to agree: an escape character without a matching
/// `ESCAPE` clause means the dialect treats that character literally, so an
/// escaped `%` degrades back into a wildcard. That is worse than not escaping
/// at all, because the query still succeeds and returns the wrong rows — so
/// the half-configured case is refused here rather than papered over.
pub fn like_escape_config(dialect: &dyn SqlDialect) -> Result<LikeEscapeConfig, FilterRejection> {
    let escape_char = dialect.like_pattern_escape();
    let clause = escape_char.and_then(|escape| dialect.like_escape_clause(escape));
    match (escape_char, clause) {
        (Some(escape_char), None) => {
            Err(FilterRejection::InvalidEscapeConfiguration { escape_char })
        }
        _ => Ok(LikeEscapeConfig {
            escape_char,
            clause,
        }),
    }
}

/// Escape the wildcards in a `LIKE` pattern body.
///
/// The escape character itself is escaped too, otherwise a user typing the
/// escape character would change the meaning of their own pattern. The body
/// is walked by `char`, so multi-byte text survives untouched.
pub fn escape_like_pattern(raw: &str, escape: char) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for character in raw.chars() {
        if character == escape || character == '%' || character == '_' {
            escaped.push(escape);
        }
        escaped.push(character);
    }
    escaped
}

/// Build the pattern body of a `LIKE` condition.
///
/// `leading` / `trailing` add the wildcards the operator asks for. `Contains`
/// adds both, `StartsWith` only the trailing one, `EndsWith` only the leading
/// one.
fn like_pattern(body: &str, escape: Option<char>, leading: bool, trailing: bool) -> String {
    let mut pattern = String::new();
    if leading {
        pattern.push('%');
    }
    match escape {
        Some(escape) => pattern.push_str(&escape_like_pattern(body, escape)),
        None => pattern.push_str(body),
    }
    if trailing {
        pattern.push('%');
    }
    pattern
}

/// The plain text a value contributes to a `LIKE` pattern.
fn pattern_body(value: &Value) -> String {
    match value {
        Value::String(text) | Value::Timestamp(text) => text.clone(),
        Value::Integer(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Json(json) => json.to_string(),
        Value::Null | Value::Bytes(_) => String::new(),
    }
}

/// The elements of a set-valued condition (`in` / `notIn`).
///
/// A JSON array is taken as it is. A comma separated string keeps the split
/// behaviour the original implementation had, so a stored filter keeps
/// producing the same statement.
fn set_elements(value: &Value) -> Vec<Value> {
    match value {
        Value::Json(serde_json::Value::Array(items)) => {
            items.iter().map(|item| Value::Json(item.clone())).collect()
        }
        Value::String(text) => text
            .split(',')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| Value::String(part.to_string()))
            .collect(),
        other => vec![other.clone()],
    }
}

/// The two bounds of a range condition (`between` / `notBetween`).
///
/// A JSON array must hold exactly two elements. A comma separated string is
/// split at its **first** comma, so a bound may itself contain a comma; a
/// value that names more than two comma separated parts is refused rather
/// than guessed at.
fn between_bounds(value: &Value) -> Result<(Value, Value), &'static str> {
    match value {
        Value::Json(serde_json::Value::Array(items)) if items.len() == 2 => {
            Ok((Value::Json(items[0].clone()), Value::Json(items[1].clone())))
        }
        Value::String(text) => match text.split_once(',') {
            Some((lower, upper)) => {
                let lower = lower.trim();
                let upper = upper.trim();
                if lower.is_empty() || upper.is_empty() || upper.contains(',') {
                    Err("needs exactly two values")
                } else {
                    Ok((
                        Value::String(lower.to_string()),
                        Value::String(upper.to_string()),
                    ))
                }
            }
            None => Err("needs exactly two values"),
        },
        _ => Err("needs exactly two values"),
    }
}

/// Whether the condition carries everything it needs to be rendered.
///
/// The predicate itself is the one the implementation always had; what changed
/// is what happens when it is false. An incomplete condition used to be
/// skipped, now it is refused.
pub fn filter_is_complete(condition: &FilterCondition) -> bool {
    incomplete_reason(condition).is_none()
}

/// Why the condition cannot be rendered, or `None` when it can.
///
/// The reason names the missing piece, so the refusal can tell the user which
/// condition to fix instead of dropping it.
pub fn incomplete_reason(condition: &FilterCondition) -> Option<&'static str> {
    if condition.column.is_empty() {
        return Some("has no column");
    }
    match condition.operator.literal_arity() {
        LiteralArity::None => None,
        LiteralArity::One => {
            if matches!(
                condition.operator,
                FilterOperator::Regex | FilterOperator::NotRegex
            ) && pattern_body(&condition.value).chars().count() > MAX_REGEX_PATTERN_LENGTH
            {
                return Some("carries a pattern that is too long");
            }
            match &condition.value {
                Value::Null => Some("needs a value"),
                Value::String(text) if text.is_empty() => Some("needs a value"),
                _ => None,
            }
        }
        LiteralArity::Two => between_bounds(&condition.value).err(),
        LiteralArity::Many => match &condition.value {
            Value::Json(serde_json::Value::Array(items)) => {
                if items.is_empty() {
                    Some("needs at least one value")
                } else {
                    None
                }
            }
            Value::String(text) => {
                if text.split(',').any(|part| !part.trim().is_empty()) {
                    None
                } else {
                    Some("needs at least one value")
                }
            }
            Value::Null => Some("needs at least one value"),
            _ => None,
        },
    }
}

/// Check a whole filter list against the driver's capabilities.
///
/// The operator check comes first: an operator the database cannot serve is
/// the more actionable answer, and it does not depend on the value.
pub fn validate_filters(
    supported: &[FilterOperator],
    filters: &[FilterCondition],
) -> Result<(), FilterRejection> {
    for condition in filters {
        if !supported.contains(&condition.operator) {
            return Err(FilterRejection::UnsupportedOperator {
                column: condition.column.clone(),
                operator: condition.operator,
            });
        }
        if let Some(reason) = incomplete_reason(condition) {
            return Err(FilterRejection::Incomplete {
                column: condition.column.clone(),
                operator: condition.operator,
                reason,
            });
        }
    }
    Ok(())
}

/// Render one condition as a SQL fragment.
///
/// An `Err` means the condition must not run. The caller has to turn it into
/// a user visible error; turning it into an empty string is exactly the
/// behaviour this module exists to remove.
pub fn format_condition(
    condition: &FilterCondition,
    dialect: &dyn SqlDialect,
) -> Result<String, FilterRejection> {
    if let Some(reason) = incomplete_reason(condition) {
        return Err(FilterRejection::Incomplete {
            column: condition.column.clone(),
            operator: condition.operator,
            reason,
        });
    }

    let column = dialect.quote_ident(&condition.column);
    let literal = |value: &Value| -> Result<String, FilterRejection> {
        dialect
            .try_format_sql_literal(&Some(value.clone()))
            .map_err(|error| FilterRejection::Driver {
                message: error.to_string(),
            })
    };

    let sql = match condition.operator {
        FilterOperator::Eq => format!("{column} = {}", literal(&condition.value)?),
        FilterOperator::Ne => format!("{column} != {}", literal(&condition.value)?),
        FilterOperator::Gt => format!("{column} > {}", literal(&condition.value)?),
        FilterOperator::Lt => format!("{column} < {}", literal(&condition.value)?),
        FilterOperator::Gte => format!("{column} >= {}", literal(&condition.value)?),
        FilterOperator::Lte => format!("{column} <= {}", literal(&condition.value)?),
        // `like` is a *pattern* operator: a `%` the user typed is a wildcard.
        // The literal operators below are the opposite and do escape.
        FilterOperator::Like => format!("{column} LIKE {}", literal(&condition.value)?),
        FilterOperator::In | FilterOperator::NotIn => {
            let elements = set_elements(&condition.value);
            let rendered = elements
                .iter()
                .map(|element| literal(element))
                .collect::<Result<Vec<_>, _>>()?;
            let keyword = if condition.operator == FilterOperator::In {
                "IN"
            } else {
                "NOT IN"
            };
            format!("{column} {keyword} ({})", rendered.join(", "))
        }
        FilterOperator::IsNull => format!("{column} IS NULL"),
        FilterOperator::IsNotNull => format!("{column} IS NOT NULL"),
        // The empty-string family says "empty string", never "empty": a NULL
        // row is not an empty string, and it has its own operator.
        FilterOperator::IsEmpty => {
            format!("{column} = {}", literal(&Value::String(String::new()))?)
        }
        FilterOperator::IsNotEmpty => {
            format!("{column} <> {}", literal(&Value::String(String::new()))?)
        }
        FilterOperator::Between | FilterOperator::NotBetween => {
            let (lower, upper) =
                between_bounds(&condition.value).map_err(|reason| FilterRejection::Incomplete {
                    column: condition.column.clone(),
                    operator: condition.operator,
                    reason,
                })?;
            let keyword = if condition.operator == FilterOperator::Between {
                "BETWEEN"
            } else {
                "NOT BETWEEN"
            };
            format!(
                "{column} {keyword} {} AND {}",
                literal(&lower)?,
                literal(&upper)?
            )
        }
        FilterOperator::Contains
        | FilterOperator::NotContains
        | FilterOperator::StartsWith
        | FilterOperator::EndsWith => {
            let config = like_escape_config(dialect)?;
            let (leading, trailing, keyword) = match condition.operator {
                FilterOperator::Contains => (true, true, "LIKE"),
                FilterOperator::NotContains => (true, true, "NOT LIKE"),
                FilterOperator::StartsWith => (false, true, "LIKE"),
                _ => (true, false, "LIKE"),
            };
            let body = pattern_body(&condition.value);
            let pattern = like_pattern(&body, config.escape_char, leading, trailing);
            let clause = config.clause.unwrap_or_default();
            format!(
                "{column} {keyword} {}{clause}",
                literal(&Value::String(pattern))?
            )
        }
        // The dialect-specific operators are spelled by the driver. The host
        // trusts the fragment it gets back and only checks that there is one.
        FilterOperator::Regex | FilterOperator::NotRegex | FilterOperator::JsonContains => {
            let value = literal(&condition.value)?;
            match dialect.filter_operator_sql(condition.operator, &column, &value) {
                Some(fragment) => fragment,
                None => {
                    return Err(FilterRejection::UnsupportedOperator {
                        column: condition.column.clone(),
                        operator: condition.operator,
                    })
                }
            }
        }
    };
    Ok(sql)
}

/// Render every condition and keep each fragment paired with its connector.
///
/// The pair matters: a fragment and its connector have to travel together, or
/// a dropped fragment shifts every connector after it by one and the query
/// silently asks for the wrong thing.
pub fn render_filter_parts(
    conditions: &[FilterCondition],
    dialect: &dyn SqlDialect,
) -> Result<Vec<(String, Option<FilterConnector>)>, FilterRejection> {
    let mut parts = Vec::with_capacity(conditions.len());
    for condition in conditions {
        let fragment = format_condition(condition, dialect)?;
        if fragment.is_empty() {
            // A driver may answer with an empty fragment; it means "this
            // condition contributes nothing". The connector goes with the
            // fragment, so the rest of the list stays aligned.
            continue;
        }
        parts.push((fragment, condition.connector));
    }
    Ok(parts)
}

/// The global logical operator, and the fallback for conditions that do not
/// carry one of their own.
pub fn filter_join(logic: Option<&str>) -> &'static str {
    match logic {
        Some(text) if text.eq_ignore_ascii_case("or") => " OR ",
        _ => " AND ",
    }
}

/// One node of the filter expression the connectors describe.
enum Node {
    Fragment(String),
    Binary {
        operator: &'static str,
        left: Box<Node>,
        right: Box<Node>,
    },
}

/// Join rendered fragments into a `WHERE` fragment.
///
/// Every fragment may carry its own connector, which joins it to the fragment
/// before it. A condition without one falls back to the request wide
/// operator, which is what every filter list did before per-condition
/// connectors existed.
///
/// The list is read **left to right**, the way the rows are shown: the first
/// row, then "OR B", then "AND C" means `(A OR B) AND C`. SQL gives `AND` the
/// stronger precedence, so a mixed list has to be parenthesised explicitly —
/// without the parentheses the database reads `A OR B AND C` as
/// `A OR (B AND C)`, which is a different question than the one that was
/// asked, and it fails silently.
pub fn join_filters(
    parts: &[(String, Option<FilterConnector>)],
    filter_logic: Option<&str>,
) -> String {
    let global = filter_join(filter_logic);
    let Some((first, _)) = parts.first() else {
        return String::new();
    };

    let mut node = Node::Fragment(first.clone());
    for (fragment, connector) in parts.iter().skip(1) {
        let operator = match connector {
            Some(FilterConnector::Or) => " OR ",
            Some(FilterConnector::And) => " AND ",
            None => global,
        };
        node = Node::Binary {
            operator,
            left: Box::new(node),
            right: Box::new(Node::Fragment(fragment.clone())),
        };
    }

    render_node(&node)
}

/// Whether the node is an `OR` group.
fn is_or_node(node: &Node) -> bool {
    matches!(node, Node::Binary { operator, .. } if *operator == " OR ")
}

/// Render one node of the left to right expression.
///
/// An `OR` group that ends up on the left of an `AND` is wrapped in
/// parentheses, because SQL binds `AND` first and would otherwise answer a
/// different question. Everything else is left alone, so a list that uses one
/// operator throughout renders exactly as it always did.
fn render_node(node: &Node) -> String {
    match node {
        Node::Fragment(fragment) => fragment.clone(),
        Node::Binary {
            operator,
            left,
            right,
        } => {
            let mut left_text = render_node(left);
            if *operator == " AND " && is_or_node(left) {
                left_text = format!("({left_text})");
            }
            let mut right_text = render_node(right);
            if *operator == " AND " && is_or_node(right) {
                right_text = format!("({right_text})");
            }
            format!("{left_text}{operator}{right_text}")
        }
    }
}
