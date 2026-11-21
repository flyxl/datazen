use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub type SqlParamStableId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlParamKind {
    Named,
    Positional,
}

impl SqlParamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlParamKind::Named => "named",
            SqlParamKind::Positional => "positional",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlParamSyntax {
    Colon,
    At,
    DollarPositional,
    DollarNamed,
    Question,
    Template,
}

impl SqlParamSyntax {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlParamSyntax::Colon => "colon",
            SqlParamSyntax::At => "at",
            SqlParamSyntax::DollarPositional => "dollar-positional",
            SqlParamSyntax::DollarNamed => "dollar-named",
            SqlParamSyntax::Question => "question",
            SqlParamSyntax::Template => "template",
        }
    }

    pub fn is_positional(self) -> bool {
        matches!(
            self,
            SqlParamSyntax::DollarPositional | SqlParamSyntax::Question
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlParam {
    pub name: String,
    pub kind: SqlParamKind,
    pub syntax: SqlParamSyntax,
    pub stable_id: SqlParamStableId,
    pub ordinal: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlParamOccurrence {
    pub from: usize,
    pub to: usize,
    pub id: SqlParamStableId,
    pub token: String,
    pub syntax: SqlParamSyntax,
    pub name: String,
    pub ordinal: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlParamOccurrenceV2 {
    pub from: usize,
    pub to: usize,
    pub id: SqlParamStableId,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlBindPayloadV2 {
    pub version: u32,
    pub values: BTreeMap<String, ParamValue>,
    pub occurrences: Vec<SqlParamOccurrenceV2>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
}

pub const DEFAULT_ENABLE_AT: bool = true;
pub const DEFAULT_ENABLE_QUESTION: bool = true;
pub const DEFAULT_ENABLE_DOLLAR_POSITIONAL: bool = true;
pub const DEFAULT_ENABLE_DOLLAR_NAMED: bool = false;
pub const DEFAULT_ENABLE_TEMPLATE: bool = true;

pub type SqlParamFilter = dyn Fn(&str, &[SqlParamOccurrence]) -> Vec<SqlParamOccurrence>;

#[derive(Default)]
pub struct SqlParamDialectPolicy {
    pub enable_at: Option<bool>,
    pub enable_question: Option<bool>,
    pub enable_dollar_positional: Option<bool>,
    pub enable_dollar_named: Option<bool>,
    pub enable_template: Option<bool>,
    pub filter_occurrences: Option<Box<SqlParamFilter>>,
}

pub fn default_dialect_policy() -> SqlParamDialectPolicy {
    SqlParamDialectPolicy {
        enable_at: Some(DEFAULT_ENABLE_AT),
        enable_question: Some(DEFAULT_ENABLE_QUESTION),
        enable_dollar_positional: Some(DEFAULT_ENABLE_DOLLAR_POSITIONAL),
        enable_dollar_named: Some(DEFAULT_ENABLE_DOLLAR_NAMED),
        enable_template: Some(DEFAULT_ENABLE_TEMPLATE),
        filter_occurrences: None,
    }
}

pub(crate) fn is_decimal_number(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i = 1;
    }
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return false;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let frac = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == frac {
            return false;
        }
    }
    i == bytes.len()
}

fn coerce_str(s: &str) -> ParamValue {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return ParamValue::Null;
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return ParamValue::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return ParamValue::Bool(false);
    }
    if is_decimal_number(trimmed) {
        if let Ok(n) = trimmed.parse::<f64>() {
            return ParamValue::Number(n);
        }
    }
    ParamValue::Str(s.to_owned())
}

pub fn coerce_param_value(raw: &ParamValue) -> ParamValue {
    match raw {
        ParamValue::Str(s) => coerce_str(s),
        other => other.clone(),
    }
}

pub fn get_param_label(param: &SqlParam) -> String {
    match param.syntax {
        SqlParamSyntax::Colon => format!(":{}", param.name),
        SqlParamSyntax::At => format!("@{}", param.name),
        SqlParamSyntax::DollarPositional | SqlParamSyntax::DollarNamed => {
            format!("${}", param.name)
        }
        SqlParamSyntax::Question => "?".to_owned(),
        SqlParamSyntax::Template => format!("${{{}}}", param.name),
    }
}

pub fn param_fingerprint(params: &[SqlParam]) -> String {
    params
        .iter()
        .map(|p| p.stable_id.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn sensitive_param_names() -> &'static BTreeSet<&'static str> {
    static NAMES: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        [
            "password",
            "passwd",
            "token",
            "secret",
            "key",
            "credential",
            "api_key",
            "apikey",
            "access_token",
            "auth",
        ]
        .into_iter()
        .collect()
    })
}

pub fn is_sensitive_param_name(name: &str) -> bool {
    sensitive_param_names().contains(name)
}
