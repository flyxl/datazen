//! Pure cell formatting and find/replace helpers for the result grid, ported
//! from `lib/cellTypes` display rules and `lib/resultFindReplace.ts`.

/// A cell's value as the result grid presents it. React's `Value` union is
/// reduced to the renderable shapes (the host keeps the original payload).
#[allow(dead_code)] // VM mirror; consumed by the grid and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GridValue {
    Null,
    Text(String),
    Number(String),
    Bool(bool),
    /// An object/array whose compact JSON text is displayed.
    Json(String),
    /// A byte blob rendered as a hex preview.
    Blob(Vec<u8>),
}

impl GridValue {
    /// The text the grid cell shows. NULL is rendered by the caller as an
    /// italic `NULL` marker; here it is the empty string (React's cellText).
    #[allow(dead_code)] // pure helper; consumed by the grid and tests.
    pub fn cell_text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Text(value) | Self::Number(value) | Self::Json(value) => value.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Blob(bytes) => bytes
                .iter()
                .take(16)
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }

    /// The emphasized cell text used for the grid (NULL keeps a visible token).
    #[allow(dead_code)] // pure helper; consumed by the grid and tests.
    pub fn display(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            _ => self.cell_text(),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// React `ResultFindOperator`.
#[allow(dead_code)] // VM mirror; consumed by the find bar and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FindOperator {
    #[default]
    Contains,
    NotContains,
    Prefix,
    Suffix,
}

/// React `ResultFindQuery` / `EMPTY_RESULT_FIND_QUERY`.
#[allow(dead_code)] // VM mirror; consumed by the find bar and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindQuery {
    pub needle: String,
    pub column: Option<String>,
    pub operator: FindOperator,
    pub match_case: bool,
}

impl FindQuery {
    #[allow(dead_code)] // fixture helper; consumed by the tests.
    pub fn empty() -> Self {
        Self::default()
    }
}

fn cell_matches(value: &GridValue, needle: &str, operator: FindOperator, match_case: bool) -> bool {
    let text = value.cell_text();
    let (text, needle) = if match_case {
        (text, needle.to_owned())
    } else {
        (text.to_lowercase(), needle.to_lowercase())
    };
    match operator {
        FindOperator::Contains => text.contains(&needle),
        FindOperator::NotContains => !text.contains(&needle),
        FindOperator::Prefix => text.starts_with(&needle),
        FindOperator::Suffix => text.ends_with(&needle),
    }
}

/// React `filterResultRows`: original row indexes that match the query.
#[allow(dead_code)] // pure helper; consumed by the grid and tests.
pub fn filter_rows(
    rows: &[Vec<GridValue>],
    columns: &[String],
    query: &FindQuery,
) -> Vec<usize> {
    if query.needle.is_empty() {
        return (0..rows.len()).collect();
    }
    // Any-column notContains means the ROW holds no matching cell, so every
    // column must fail — React `rowMatches` uses `every` here, not `some`.
    let every_column = query.column.is_none() && query.operator == FindOperator::NotContains;
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            let matches = |(index, name): (usize, &String)| {
                if query.column.as_deref().is_some_and(|wanted| wanted != name) {
                    return false;
                }
                row.get(index).is_some_and(|value| {
                    cell_matches(value, &query.needle, query.operator, query.match_case)
                })
            };
            if every_column {
                columns.iter().enumerate().all(matches)
            } else {
                columns.iter().enumerate().any(matches)
            }
        })
        .map(|(index, _)| index)
        .collect()
}

/// React `replaceText`: replace every occurrence of the needle inside the
/// cell text. Case-sensitive folds neither side; case-insensitive folds both
/// for the search but splices at the original offsets.
#[allow(dead_code)] // pure helper; consumed by the plan builder and tests.
pub fn replace_text(text: &str, query: &FindQuery, replacement: &str) -> String {
    if query.match_case {
        return text.split(query.needle.as_str()).collect::<Vec<_>>().join(replacement);
    }
    let needle = query.needle.to_lowercase();
    if needle.is_empty() {
        return text.to_owned();
    }
    let mut result = String::new();
    let mut rest = text.to_owned();
    loop {
        let Some(at) = rest.to_lowercase().find(&needle) else { break };
        let end = at + needle.len();
        result.push_str(&rest[..at]);
        result.push_str(replacement);
        rest = rest[end..].to_owned();
    }
    result.push_str(&rest);
    result
}

/// React `planResultReplacements`: only matching cells change, row-first.
#[allow(dead_code)] // pure helper; consumed by the find bar and tests.
pub fn plan_replacements(
    rows: &[Vec<GridValue>],
    columns: &[String],
    query: &FindQuery,
    replacement: &str,
) -> Vec<(usize, String, String)> {
    if query.needle.is_empty() || query.needle == replacement {
        return Vec::new();
    }
    let mut plan = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, name) in columns.iter().enumerate() {
            if query.column.as_deref().is_some_and(|wanted| wanted != name) {
                continue;
            }
            let Some(value) = row.get(column_index) else { continue };
            if !cell_matches(value, &query.needle, query.operator, query.match_case) {
                continue;
            }
            let original = value.cell_text();
            plan.push((
                row_index,
                name.clone(),
                replace_text(&original, query, replacement),
            ));
        }
    }
    plan
}
