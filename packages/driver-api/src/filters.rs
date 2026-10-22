//! Shared filter DTOs for table data APIs.
//!
//! These shapes are part of the IPC contract; they live in driver-api so the
//! migration domain crates can validate filters without host services.

use crate::Value;

/// How one filter condition joins the condition before it.
///
/// The connector hangs on the *current* condition: concatenation runs left to
/// right, so the first condition has no predecessor and its own connector is
/// meaningless — renderers must ignore it. `None` means "fall back to the
/// request-wide logical operator", which is exactly the behaviour every filter
/// list had before per-condition connectors existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterConnector {
    And,
    Or,
}

/// Single filter for table data APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub value: Value,
    /// Connector to the **previous** condition. `None` (the default) falls back
    /// to the request-wide logical operator, so payloads written before
    /// per-condition connectors existed keep behaving identically.
    ///
    /// An absent connector is **not written**: the field is skipped rather than
    /// serialized as `null`, so a stored record carries the connector only
    /// where the user actually set one (08 §5.2.1, "缺省不写"). A `null` on the
    /// wire would make the TypeScript side carry a second spelling of "no
    /// connector" next to the absent key it already has to handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<FilterConnector>,
}

/// The filter operators the product understands.
///
/// The first ten are the operators that already shipped; their spelling and
/// their rendered SQL are frozen. The next nine are shared extensions every
/// ANSI-ish dialect can render. The last three are dialect-specific and can
/// only be rendered by the driver that declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    // ── Existing ten: names, order and wire spelling never change ──
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
    // ── Shared extensions (nine): rendered by the host for every driver ──
    NotIn,
    Between,
    NotBetween,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    IsEmpty,
    IsNotEmpty,
    // ── Dialect-specific (three): rendered by the driver that declares them ──
    Regex,
    NotRegex,
    JsonContains,
}

/// The ten operators that shipped before the extension.
///
/// This set has exactly one purpose: it is the regression baseline for
/// "the existing operators render byte-identical SQL". No runtime default
/// returns it — [`SHARED_FILTER_OPERATORS`] is the default whitelist.
pub const EXISTING_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Eq,
    FilterOperator::Ne,
    FilterOperator::Gt,
    FilterOperator::Lt,
    FilterOperator::Gte,
    FilterOperator::Lte,
    FilterOperator::Like,
    FilterOperator::In,
    FilterOperator::IsNull,
    FilterOperator::IsNotNull,
];

/// The operators the host renders itself: the existing ten plus the nine
/// shared extensions.
///
/// This is the default value of `DatabaseDriver::supported_filter_operators()`.
/// The dialect-specific three are deliberately absent — a driver has to opt in
/// to them explicitly.
pub const SHARED_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Eq,
    FilterOperator::Ne,
    FilterOperator::Gt,
    FilterOperator::Lt,
    FilterOperator::Gte,
    FilterOperator::Lte,
    FilterOperator::Like,
    FilterOperator::In,
    FilterOperator::IsNull,
    FilterOperator::IsNotNull,
    FilterOperator::NotIn,
    FilterOperator::Between,
    FilterOperator::NotBetween,
    FilterOperator::Contains,
    FilterOperator::NotContains,
    FilterOperator::StartsWith,
    FilterOperator::EndsWith,
    FilterOperator::IsEmpty,
    FilterOperator::IsNotEmpty,
];

/// The operators only a driver can render, through `filter_operator_sql`.
pub const DIALECT_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Regex,
    FilterOperator::NotRegex,
    FilterOperator::JsonContains,
];

/// How many literals an operator needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralArity {
    /// The operator carries no value at all (`IS NULL`, `IS NOT NULL`, and the
    /// empty-string family).
    None,
    /// Exactly one literal.
    One,
    /// Exactly two literals (`BETWEEN` and its negation).
    Two,
    /// An open number of literals (`IN` and its negation).
    Many,
}

impl FilterOperator {
    /// The stable camelCase spelling used on the wire and in messages.
    ///
    /// It matches the `#[serde(rename_all = "camelCase")]` form exactly, so
    /// error messages and test assertions never hand-write the spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FilterOperator::Eq => "eq",
            FilterOperator::Ne => "ne",
            FilterOperator::Gt => "gt",
            FilterOperator::Lt => "lt",
            FilterOperator::Gte => "gte",
            FilterOperator::Lte => "lte",
            FilterOperator::Like => "like",
            FilterOperator::In => "in",
            FilterOperator::IsNull => "isNull",
            FilterOperator::IsNotNull => "isNotNull",
            FilterOperator::NotIn => "notIn",
            FilterOperator::Between => "between",
            FilterOperator::NotBetween => "notBetween",
            FilterOperator::Contains => "contains",
            FilterOperator::NotContains => "notContains",
            FilterOperator::StartsWith => "startsWith",
            FilterOperator::EndsWith => "endsWith",
            FilterOperator::IsEmpty => "isEmpty",
            FilterOperator::IsNotEmpty => "isNotEmpty",
            FilterOperator::Regex => "regex",
            FilterOperator::NotRegex => "notRegex",
            FilterOperator::JsonContains => "jsonContains",
        }
    }

    /// Whether the operator belongs to the dialect-specific group.
    pub fn is_dialect_specific(self) -> bool {
        DIALECT_FILTER_OPERATORS.contains(&self)
    }

    /// How many literals the operator's value must carry.
    pub fn literal_arity(self) -> LiteralArity {
        match self {
            FilterOperator::IsNull
            | FilterOperator::IsNotNull
            | FilterOperator::IsEmpty
            | FilterOperator::IsNotEmpty => LiteralArity::None,
            FilterOperator::Between | FilterOperator::NotBetween => LiteralArity::Two,
            FilterOperator::In | FilterOperator::NotIn => LiteralArity::Many,
            _ => LiteralArity::One,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_spelling_is_stable_for_all_operators() {
        let expected = [
            ("eq", FilterOperator::Eq),
            ("ne", FilterOperator::Ne),
            ("gt", FilterOperator::Gt),
            ("lt", FilterOperator::Lt),
            ("gte", FilterOperator::Gte),
            ("lte", FilterOperator::Lte),
            ("like", FilterOperator::Like),
            ("in", FilterOperator::In),
            ("isNull", FilterOperator::IsNull),
            ("isNotNull", FilterOperator::IsNotNull),
            ("notIn", FilterOperator::NotIn),
            ("between", FilterOperator::Between),
            ("notBetween", FilterOperator::NotBetween),
            ("contains", FilterOperator::Contains),
            ("notContains", FilterOperator::NotContains),
            ("startsWith", FilterOperator::StartsWith),
            ("endsWith", FilterOperator::EndsWith),
            ("isEmpty", FilterOperator::IsEmpty),
            ("isNotEmpty", FilterOperator::IsNotEmpty),
            ("regex", FilterOperator::Regex),
            ("notRegex", FilterOperator::NotRegex),
            ("jsonContains", FilterOperator::JsonContains),
        ];
        for (spelling, operator) in expected {
            assert_eq!(
                serde_json::to_string(&operator).unwrap(),
                format!("\"{spelling}\""),
                "serde spelling of {spelling} drifted"
            );
            assert_eq!(operator.as_str(), spelling);
        }
    }

    #[test]
    fn operator_groups_are_disjoint_and_complete() {
        assert_eq!(EXISTING_FILTER_OPERATORS.len(), 10);
        assert_eq!(SHARED_FILTER_OPERATORS.len(), 19);
        assert_eq!(DIALECT_FILTER_OPERATORS.len(), 3);

        let existing: Vec<&str> = EXISTING_FILTER_OPERATORS
            .iter()
            .map(|operator| operator.as_str())
            .collect();
        assert_eq!(
            existing,
            vec![
                "eq",
                "ne",
                "gt",
                "lt",
                "gte",
                "lte",
                "like",
                "in",
                "isNull",
                "isNotNull"
            ]
        );

        for group in [
            EXISTING_FILTER_OPERATORS,
            SHARED_FILTER_OPERATORS,
            DIALECT_FILTER_OPERATORS,
        ] {
            let mut sorted = group.to_vec();
            sorted.sort_by_key(|operator| operator.as_str());
            sorted.dedup();
            assert_eq!(sorted.len(), group.len(), "group contains duplicates");
        }
        for operator in SHARED_FILTER_OPERATORS {
            assert!(
                !DIALECT_FILTER_OPERATORS.contains(operator),
                "shared and dialect groups overlap on {operator:?}"
            );
        }
        for operator in EXISTING_FILTER_OPERATORS {
            assert!(
                SHARED_FILTER_OPERATORS.contains(operator),
                "the existing ten must stay inside the shared set"
            );
        }
        assert_eq!(
            SHARED_FILTER_OPERATORS.len() + DIALECT_FILTER_OPERATORS.len(),
            22
        );
    }

    #[test]
    fn literal_arity_matches_render_matrix() {
        assert_eq!(FilterOperator::IsNull.literal_arity(), LiteralArity::None);
        assert_eq!(
            FilterOperator::IsNotNull.literal_arity(),
            LiteralArity::None
        );
        assert_eq!(FilterOperator::IsEmpty.literal_arity(), LiteralArity::None);
        assert_eq!(
            FilterOperator::IsNotEmpty.literal_arity(),
            LiteralArity::None
        );
        assert_eq!(FilterOperator::Between.literal_arity(), LiteralArity::Two);
        assert_eq!(
            FilterOperator::NotBetween.literal_arity(),
            LiteralArity::Two
        );
        assert_eq!(FilterOperator::In.literal_arity(), LiteralArity::Many);
        assert_eq!(FilterOperator::NotIn.literal_arity(), LiteralArity::Many);
        assert_eq!(FilterOperator::Eq.literal_arity(), LiteralArity::One);
        assert_eq!(FilterOperator::Like.literal_arity(), LiteralArity::One);
    }

    #[test]
    fn dialect_specific_flag_matches_the_dialect_group() {
        for operator in DIALECT_FILTER_OPERATORS {
            assert!(operator.is_dialect_specific());
        }
        for operator in SHARED_FILTER_OPERATORS {
            assert!(!operator.is_dialect_specific());
        }
    }

    #[test]
    fn condition_connector_defaults_to_none_after_deserialize() {
        let condition: FilterCondition =
            serde_json::from_str(r#"{"column":"id","operator":"eq","value":1}"#).unwrap();
        assert_eq!(condition.connector, None);
    }

    #[test]
    fn condition_connector_accepts_camel_case() {
        let condition: FilterCondition =
            serde_json::from_str(r#"{"column":"id","operator":"eq","value":1,"connector":"or"}"#)
                .unwrap();
        assert_eq!(condition.connector, Some(FilterConnector::Or));

        let condition: FilterCondition =
            serde_json::from_str(r#"{"column":"id","operator":"eq","value":1,"connector":"and"}"#)
                .unwrap();
        assert_eq!(condition.connector, Some(FilterConnector::And));
    }

    #[test]
    fn condition_connector_rejects_other_spellings() {
        // Only the camelCase spelling is on the wire; "Or"/"OR" must not be
        // silently accepted, otherwise two spellings of one value could reach
        // the renderer.
        assert!(serde_json::from_str::<FilterCondition>(
            r#"{"column":"id","operator":"eq","value":1,"connector":"Or"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<FilterCondition>(
            r#"{"column":"id","operator":"eq","value":1,"connector":"OR"}"#
        )
        .is_err());
    }
}
