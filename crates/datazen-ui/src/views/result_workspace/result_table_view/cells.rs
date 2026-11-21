//! Pure cell formatting and find/replace helpers for the result grid, ported
//! from `lib/cellTypes` display rules and `lib/resultFindReplace.ts`.

/// A cell's value as the result grid presents it. React's `Value` union is
/// reduced to the renderable shapes (the host keeps the original payload).
#[allow(dead_code)] // VM mirror; consumed by the grid and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GridValue {
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
    pub(crate) fn cell_text(&self) -> String {
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
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            _ => self.cell_text(),
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// React `ResultFindOperator`.
#[allow(dead_code)] // VM mirror; consumed by the find bar and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FindOperator {
    #[default]
    Contains,
    NotContains,
    Prefix,
    Suffix,
}

/// React `ResultFindQuery` / `EMPTY_RESULT_FIND_QUERY`.
#[allow(dead_code)] // VM mirror; consumed by the find bar and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FindQuery {
    pub(crate) needle: String,
    pub(crate) column: Option<String>,
    pub(crate) operator: FindOperator,
    pub(crate) match_case: bool,
}

impl FindQuery {
    #[allow(dead_code)] // fixture helper; consumed by the tests.
    pub(crate) fn empty() -> Self {
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
pub(crate) fn filter_rows(
    rows: &[Vec<GridValue>],
    columns: &[String],
    query: &FindQuery,
) -> Vec<usize> {
    if query.needle.is_empty() {
        return (0..rows.len()).collect();
    }
    rows.iter()
        .enumerate()
        .filter(|(_, row)| {
            columns.iter().enumerate().any(|(index, name)| {
                if query.column.as_deref().is_some_and(|wanted| wanted != name) {
                    return false;
                }
                row.get(index).is_some_and(|value| {
                    cell_matches(value, &query.needle, query.operator, query.match_case)
                })
            })
        })
        .map(|(index, _)| index)
        .collect()
}

/// React `planResultReplacements`: only matching cells change, row-first.
#[allow(dead_code)] // pure helper; consumed by the find bar and tests.
pub(crate) fn plan_replacements(
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
            let original = value.cell_text();
            if cell_matches(value, &query.needle, FindOperator::Contains, query.match_case) {
                plan.push((row_index, name.clone(), replacement.to_owned()));
            } else if query.operator != FindOperator::Contains {
                // Non-contains operators still replace the whole matching cell.
                let matched = cell_matches(value, &query.needle, query.operator, query.match_case);
                if matched {
                    plan.push((row_index, name.clone(), replacement.to_owned()));
                }
            }
            let _ = original;
        }
    }
    plan
}
