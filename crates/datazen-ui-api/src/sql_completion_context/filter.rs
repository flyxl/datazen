use super::keywords::SqlCompletionKind;

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionOption {
    pub label: String,
    pub option_type: Option<String>,
    pub boost: Option<f64>,
}

impl CompletionOption {
    pub fn new(label: &str, option_type: Option<&str>, boost: Option<f64>) -> Self {
        Self {
            label: label.to_owned(),
            option_type: option_type.map(str::to_owned),
            boost,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionResult {
    pub options: Vec<CompletionOption>,
    pub span: Option<(usize, usize)>,
}

impl CompletionResult {
    pub fn new(options: Vec<CompletionOption>) -> Self {
        Self { options, span: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionContextInput {
    pub text_before_cursor: String,
}

pub enum CompletionOutcome {
    Direct(Option<CompletionResult>),
    Deferred(Box<dyn FnOnce() -> Option<CompletionResult>>),
}

pub type CompletionSourceFn = Box<dyn Fn(&CompletionContextInput) -> CompletionOutcome>;

pub const TABLE_CONTEXT_ALLOWED_KEYWORDS: [&str; 8] = [
    "select", "values", "lateral", "unnest", "only", "final", "json_table", "xmltable",
];

pub const COLUMN_CONTEXT_ALLOWED_KEYWORDS: [&str; 50] = [
    "and", "or", "not", "is", "null", "true", "false", "in", "between", "like", "ilike",
    "similar", "escape", "exists", "case", "when", "then", "else", "end", "distinct", "any",
    "all", "some", "select", "from", "as", "into", "where", "join", "left", "right", "inner",
    "full", "cross", "on", "order", "group", "having", "limit", "offset", "fetch", "union",
    "intersect", "except", "returning", "window", "for", "interval", "array", "collate",
];

pub fn filter_completions_by_kind(
    options: &[CompletionOption],
    kind: SqlCompletionKind,
) -> Vec<CompletionOption> {
    match kind {
        SqlCompletionKind::Column => options
            .iter()
            .filter(|option| option.option_type.as_deref() == Some("property"))
            .map(|option| match option.boost {
                Some(_) => option.clone(),
                None => CompletionOption { boost: Some(5.0), ..option.clone() },
            })
            .collect(),
        SqlCompletionKind::Table => options
            .iter()
            .filter(|option| option.option_type.as_deref() == Some("type"))
            .map(|option| match option.boost {
                Some(_) => option.clone(),
                None => CompletionOption { boost: Some(10.0), ..option.clone() },
            })
            .collect(),
        SqlCompletionKind::Any => options.to_vec(),
    }
}

pub fn filter_keywords_by_kind(
    options: &[CompletionOption],
    kind: SqlCompletionKind,
) -> Vec<CompletionOption> {
    match kind {
        SqlCompletionKind::Table => options
            .iter()
            .filter(|option| TABLE_CONTEXT_ALLOWED_KEYWORDS.contains(&option.label.to_lowercase().as_str()))
            .map(|option| CompletionOption { boost: Some(option.boost.unwrap_or(0.0) - 2.0), ..option.clone() })
            .collect(),
        SqlCompletionKind::Column => options
            .iter()
            .filter(|option| COLUMN_CONTEXT_ALLOWED_KEYWORDS.contains(&option.label.to_lowercase().as_str()))
            .map(|option| CompletionOption { boost: Some(option.boost.unwrap_or(0.0) - 2.0), ..option.clone() })
            .collect(),
        SqlCompletionKind::Any => options.to_vec(),
    }
}
