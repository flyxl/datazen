use super::{column_index, value_as_ddl_text};
use crate::types::{DriverError, QueryResult};
use sqlparser::ast::{
    Expr, GranteeName, ObjectName, ObjectNamePart, Query, SelectItem,
    SelectItemQualifiedWildcardKind, SetExpr, VisitMut, VisitorMut,
};
use std::ops::ControlFlow;

pub(super) fn required_view_metadata_field(
    result: &QueryResult,
    name: &str,
) -> Result<String, DriverError> {
    let index = column_index(&result.columns, &[name]).ok_or_else(|| {
        DriverError::QueryFailed(format!("MySQL view metadata missing `{name}` column"))
    })?;
    result
        .rows
        .first()
        .and_then(|row| value_as_ddl_text(row.get(index).and_then(Option::as_ref)))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DriverError::QueryFailed(format!("MySQL view metadata `{name}` is unavailable"))
        })
}

pub(super) fn missing_show_view_metadata(field: &str) -> DriverError {
    DriverError::QueryFailed(format!(
        "MySQL SHOW CREATE VIEW omitted `{field}` metadata; refusing an incomplete view snapshot"
    ))
}

pub(super) fn mysql_view_definer_identity(definer: &GranteeName) -> Result<String, DriverError> {
    match definer {
        GranteeName::UserHost { user, host } => Ok(format!("{}@{}", user.value, host.value)),
        GranteeName::ObjectName(_) => Err(DriverError::QueryFailed(
            "MySQL SHOW CREATE VIEW returned an unsupported DEFINER identity".into(),
        )),
    }
}

/// Compare the catalog body and SHOW body while ignoring only an explicit
/// qualifier that names this exact view database. External database names and
/// all table identifiers remain part of the equality check.
pub(super) fn mysql_view_query_bodies_match(
    catalog: &Query,
    show_create: &Query,
    source_database: &str,
) -> bool {
    if source_database.trim().is_empty() {
        return false;
    }
    let mut catalog = catalog.clone();
    let mut show_create = show_create.clone();
    let mut normalizer = OwnDatabaseQualifier { source_database };
    let _ = catalog.visit(&mut normalizer);
    let _ = show_create.visit(&mut normalizer);
    catalog == show_create
}

struct OwnDatabaseQualifier<'a> {
    source_database: &'a str,
}

impl VisitorMut for OwnDatabaseQualifier<'_> {
    type Break = std::convert::Infallible;

    fn pre_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        normalize_query_wildcards(query, self.source_database);
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::CompoundIdentifier(parts) => {
                let has_own_database_qualifier = parts.len() == 3
                    && parts
                        .first()
                        .is_some_and(|database| database.value == self.source_database);
                if has_own_database_qualifier {
                    parts.remove(0);
                }
            }
            Expr::QualifiedWildcard(prefix, _) => {
                let has_own_database_qualifier = prefix.0.len() == 2
                    && prefix
                        .0
                        .first()
                        .and_then(ObjectNamePart::as_ident)
                        .is_some_and(|database| database.value == self.source_database);
                if has_own_database_qualifier {
                    prefix.0.remove(0);
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &mut ObjectName) -> ControlFlow<Self::Break> {
        let has_own_database_qualifier = relation.0.len() == 2
            && relation
                .0
                .first()
                .and_then(ObjectNamePart::as_ident)
                .is_some_and(|database| database.value == self.source_database);
        if has_own_database_qualifier {
            relation.0.remove(0);
        }
        ControlFlow::Continue(())
    }
}

fn normalize_query_wildcards(query: &mut Query, source_database: &str) {
    fn normalize_set_expr(expression: &mut SetExpr, source_database: &str) {
        match expression {
            SetExpr::Select(select) => {
                for projection in &mut select.projection {
                    let SelectItem::QualifiedWildcard(
                        SelectItemQualifiedWildcardKind::ObjectName(prefix),
                        _,
                    ) = projection
                    else {
                        continue;
                    };
                    let has_own_database_qualifier = prefix.0.len() == 2
                        && prefix
                            .0
                            .first()
                            .and_then(ObjectNamePart::as_ident)
                            .is_some_and(|database| database.value == source_database);
                    if has_own_database_qualifier {
                        prefix.0.remove(0);
                    }
                }
            }
            SetExpr::Query(query) => normalize_query_wildcards(query, source_database),
            SetExpr::SetOperation { left, right, .. } => {
                normalize_set_expr(left, source_database);
                normalize_set_expr(right, source_database);
            }
            SetExpr::Values(_)
            | SetExpr::Insert(_)
            | SetExpr::Update(_)
            | SetExpr::Delete(_)
            | SetExpr::Merge(_)
            | SetExpr::Table(_) => {}
        }
    }

    normalize_set_expr(&mut query.body, source_database);
}

pub(super) fn metadata_value_matches(
    show_create: &str,
    catalog: &str,
    case_insensitive: bool,
) -> bool {
    let show_create = show_create.trim();
    let catalog = catalog.trim();
    if case_insensitive {
        show_create.eq_ignore_ascii_case(catalog)
    } else {
        show_create == catalog
    }
}

/// Read the trailing CHECK OPTION from SHOW CREATE VIEW. MySQL may emit it as
/// a plain clause or a versioned-comment suffix; other comments and literals
/// are ignored so phrases in query text cannot become creation metadata.
#[cfg(test)]
pub(super) fn mysql_show_create_view_check_option(sql: &str) -> Result<String, DriverError> {
    mysql_view_check_option_suffix(sql).map(|(check_option, _)| check_option)
}

/// Return a parser-compatible CREATE VIEW definition and its separately
/// validated CHECK OPTION. sqlparser does not accept MySQL's trailing clause;
/// remove only that recognized suffix for AST parsing, while preserving the
/// extracted creation semantics for the metadata snapshot.
pub(super) fn mysql_show_create_view_for_parser(
    sql: &str,
) -> Result<(String, String), DriverError> {
    let (check_option, suffix) = mysql_view_check_option_suffix(sql)?;
    let Some((start, end)) = suffix else {
        return Ok((sql.to_owned(), check_option));
    };
    let mut parser_sql = String::with_capacity(sql.len() - (end - start) + 1);
    parser_sql.push_str(&sql[..start]);
    parser_sql.push(' ');
    parser_sql.push_str(&sql[end..]);
    Ok((parser_sql, check_option))
}

fn mysql_view_check_option_suffix(
    sql: &str,
) -> Result<(String, Option<(usize, usize)>), DriverError> {
    let tokens = mysql_view_metadata_tokens(sql)?;
    let patterns: &[(&[&str], &str)] = &[
        (&["WITH", "CASCADED", "CHECK", "OPTION"], "CASCADED"),
        (&["WITH", "LOCAL", "CHECK", "OPTION"], "LOCAL"),
        (&["WITH", "CHECK", "OPTION"], "CASCADED"),
    ];
    let mut found: Option<&str> = None;
    let mut suffix: Option<(usize, usize)> = None;
    for (pattern, value) in patterns {
        let starts = tokens
            .windows(pattern.len())
            .enumerate()
            .filter_map(|(index, window)| {
                window
                    .iter()
                    .zip(pattern.iter())
                    .all(|(token, expected)| token.text.eq_ignore_ascii_case(expected))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        for start in starts {
            if start + pattern.len() != tokens.len() {
                return Err(DriverError::QueryFailed(
                    "MySQL SHOW CREATE VIEW contains an unrecognized CHECK OPTION placement".into(),
                ));
            }
            if found.is_some_and(|previous| previous != *value) {
                return Err(DriverError::QueryFailed(
                    "MySQL SHOW CREATE VIEW contains conflicting CHECK OPTION metadata".into(),
                ));
            }
            let suffix_tokens = &tokens[start..start + pattern.len()];
            let executable_comment = suffix_tokens[0].executable_comment;
            if suffix_tokens
                .iter()
                .any(|token| token.executable_comment != executable_comment)
            {
                return Err(DriverError::QueryFailed(
                    "MySQL SHOW CREATE VIEW contains a split CHECK OPTION suffix".into(),
                ));
            }
            let range = executable_comment
                .unwrap_or((suffix_tokens[0].start, suffix_tokens[pattern.len() - 1].end));
            if let Some((comment_start, comment_end)) = executable_comment {
                let comment_tokens = tokens
                    .iter()
                    .filter(|token| token.executable_comment == executable_comment)
                    .count();
                let prefix = &sql[comment_start + 3..suffix_tokens[0].start];
                let clause_end = suffix_tokens[pattern.len() - 1].end;
                let comment_body_end = comment_end.checked_sub(2).ok_or_else(|| {
                    DriverError::QueryFailed(
                        "MySQL SHOW CREATE VIEW contains malformed executable comment bounds"
                            .into(),
                    )
                })?;
                let trailing = &sql[clause_end..comment_body_end];
                if comment_tokens != pattern.len()
                    || !prefix
                        .bytes()
                        .all(|byte| byte.is_ascii_whitespace() || byte.is_ascii_digit())
                    || !trailing.bytes().all(|byte| byte.is_ascii_whitespace())
                {
                    return Err(DriverError::QueryFailed(
                        "MySQL SHOW CREATE VIEW executable comment contains unsupported CHECK OPTION syntax".into(),
                    ));
                }
            }
            if found.is_some() && suffix != Some(range) {
                return Err(DriverError::QueryFailed(
                    "MySQL SHOW CREATE VIEW contains multiple CHECK OPTION suffixes".into(),
                ));
            }
            found = Some(value);
            suffix = Some(range);
        }
    }
    Ok((found.unwrap_or("NONE").into(), suffix))
}

#[derive(Clone)]
struct MysqlViewMetadataToken {
    text: String,
    start: usize,
    end: usize,
    executable_comment: Option<(usize, usize)>,
}

fn mysql_view_metadata_tokens(sql: &str) -> Result<Vec<MysqlViewMetadataToken>, DriverError> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut executable_comment = None;
    while index < bytes.len() {
        if let Some((_, end)) = executable_comment {
            if bytes[index..].starts_with(b"*/") {
                index += 2;
                executable_comment = None;
                if index > end {
                    return Err(DriverError::QueryFailed(
                        "MySQL SHOW CREATE VIEW contains malformed executable comment bounds"
                            .into(),
                    ));
                }
                continue;
            }
        }
        if let Some(quote) = bytes
            .get(index)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"' | b'`'))
        {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'#'
            || (bytes[index] == b'-'
                && bytes.get(index + 1) == Some(&b'-')
                && bytes
                    .get(index + 2)
                    .is_some_and(|byte| byte.is_ascii_whitespace()))
        {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*!") {
            let start = index;
            let end = bytes[index..]
                .windows(2)
                .position(|window| window == b"*/")
                .map(|offset| index + offset + 2)
                .ok_or_else(|| {
                    DriverError::QueryFailed(
                        "MySQL SHOW CREATE VIEW contains an unterminated executable comment".into(),
                    )
                })?;
            executable_comment = Some((start, end));
            index += 3;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(MysqlViewMetadataToken {
                text: sql[start..index].to_ascii_uppercase(),
                start,
                end: index,
                executable_comment,
            });
            continue;
        }
        index += 1;
    }
    Ok(tokens)
}
