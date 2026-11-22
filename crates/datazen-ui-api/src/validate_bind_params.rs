use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamValue {
    Null,
    String(String),
    Number(String),
    Bool(bool),
    Other,
}

pub const VALIDATE_BIND_PARAMS_SURFACE: [&str; 4] = [
    "findMissingParams",
    "parseSqlParams",
    "getParamLabel",
    "SqlParamDialectPolicy",
];

pub const VALIDATE_BIND_PARAMS_GAP: [&str; 2] = [
    "the driver-owned SqlParameterStrategy.filterOccurrences seam",
    "rawValues arriving from the query panel's zustand store",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SqlParamDialectPolicy {
    pub enable_at: bool,
    pub enable_question: bool,
    pub enable_dollar_positional: bool,
    pub enable_dollar_named: bool,
    pub enable_template: bool,
}

impl SqlParamDialectPolicy {
    pub fn host_defaults() -> Self {
        Self {
            enable_at: true,
            enable_question: true,
            enable_dollar_positional: true,
            enable_dollar_named: false,
            enable_template: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlParamKind {
    Named,
    Positional,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlParam {
    pub name: String,
    pub kind: SqlParamKind,
    pub syntax: SqlParamSyntax,
    pub stable_id: String,
    pub ordinal: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingParam {
    pub param: SqlParam,
    pub label: String,
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn read_ident(chars: &[char], start: usize) -> Option<(String, usize)> {
    let first = *chars.get(start)?;
    if !is_ident_start(first) {
        return None;
    }
    let mut end = start + 1;
    while end < chars.len() && is_ident(chars[end]) {
        end += 1;
    }
    Some((chars[start..end].iter().collect(), end))
}

fn read_digits(chars: &[char], start: usize) -> Option<(String, usize)> {
    if !chars.get(start).is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let mut end = start + 1;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    Some((chars[start..end].iter().collect(), end))
}

fn skip_quote(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == quote {
            if quote == '\'' && chars.get(index + 1) == Some(&'\'') {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    chars.len()
}

fn skip_dollar_quote(chars: &[char], start: usize) -> usize {
    let mut tag_end = start + 1;
    while tag_end < chars.len() && is_ident(chars[tag_end]) {
        tag_end += 1;
    }
    if tag_end >= chars.len() || chars[tag_end] != '$' {
        return start;
    }
    let tag: String = chars[start..=tag_end].iter().collect();
    let mut index = tag_end + 1;
    while index + tag.len() <= chars.len() {
        if chars[index..index + tag.len()].iter().collect::<String>() == tag {
            return index + tag.len();
        }
        index += 1;
    }
    chars.len()
}

fn skip_non_code(chars: &[char], index: usize) -> usize {
    let ch = chars[index];
    if ch == '\'' || ch == '"' || ch == '`' {
        return skip_quote(chars, index);
    }
    if ch == '$' {
        return skip_dollar_quote(chars, index);
    }
    if ch == '-' && chars.get(index + 1) == Some(&'-') {
        let mut cursor = index + 2;
        while cursor < chars.len() && chars[cursor] != '\n' {
            cursor += 1;
        }
        return (cursor + 1).min(chars.len());
    }
    if ch == '/' && chars.get(index + 1) == Some(&'*') {
        let mut cursor = index + 2;
        while cursor + 1 < chars.len() {
            if chars[cursor] == '*' && chars[cursor + 1] == '/' {
                return cursor + 2;
            }
            cursor += 1;
        }
        return chars.len();
    }
    index
}

pub fn parse_sql_params(sql: &str, policy: Option<&SqlParamDialectPolicy>) -> Vec<SqlParam> {
    let defaults = SqlParamDialectPolicy::host_defaults();
    let policy = policy.unwrap_or(&defaults);
    let chars: Vec<char> = sql.chars().collect();
    let mut params: Vec<SqlParam> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut question_ordinal = 0u64;
    let mut index = 0usize;

    let mut push = |name: String,
                    kind: SqlParamKind,
                    syntax: SqlParamSyntax,
                    stable_id: String,
                    ordinal: Option<u64>,
                    params: &mut Vec<SqlParam>,
                    seen: &mut std::collections::BTreeSet<String>| {
        if seen.insert(stable_id.clone()) {
            params.push(SqlParam { name, kind, syntax, stable_id, ordinal });
        }
    };

    while index < chars.len() {
        let skipped = skip_non_code(&chars, index);
        if skipped != index {
            index = skipped;
            continue;
        }
        let ch = chars[index];
        if ch == ':' && chars.get(index + 1) != Some(&':') {
            if let Some((name, end)) = read_ident(&chars, index + 1) {
                let id = format!("named:{name}");
                push(
                    name,
                    SqlParamKind::Named,
                    SqlParamSyntax::Colon,
                    id,
                    None,
                    &mut params,
                    &mut seen,
                );
                index = end;
                continue;
            }
        }
        if ch == '@' && policy.enable_at && chars.get(index + 1) != Some(&'@') {
            if let Some((name, end)) = read_ident(&chars, index + 1) {
                let id = format!("named:{name}");
                push(
                    name,
                    SqlParamKind::Named,
                    SqlParamSyntax::At,
                    id,
                    None,
                    &mut params,
                    &mut seen,
                );
                index = end;
                continue;
            }
        }
        if ch == '$' {
            if policy.enable_template && chars.get(index + 1) == Some(&'{') {
                if let Some((name, end)) = read_ident(&chars, index + 2) {
                    if chars.get(end) == Some(&'}') {
                        let id = format!("named:{name}");
                        push(
                            name,
                            SqlParamKind::Named,
                            SqlParamSyntax::Template,
                            id,
                            None,
                            &mut params,
                            &mut seen,
                        );
                        index = end + 1;
                        continue;
                    }
                }
            }
            if policy.enable_dollar_named {
                if let Some((name, end)) = read_ident(&chars, index + 1) {
                    let id = format!("named:{name}");
                    push(
                        name,
                        SqlParamKind::Named,
                        SqlParamSyntax::DollarNamed,
                        id,
                        None,
                        &mut params,
                        &mut seen,
                    );
                    index = end;
                    continue;
                }
            }
            if policy.enable_dollar_positional {
                if let Some((number, end)) = read_digits(&chars, index + 1) {
                    let id = format!("dollar:{number}");
                    push(
                        number,
                        SqlParamKind::Positional,
                        SqlParamSyntax::DollarPositional,
                        id,
                        None,
                        &mut params,
                        &mut seen,
                    );
                    index = end;
                    continue;
                }
            }
        }
        if ch == '?' && policy.enable_question {
            question_ordinal += 1;
            let id = format!("question:{question_ordinal}");
            push(
                question_ordinal.to_string(),
                SqlParamKind::Positional,
                SqlParamSyntax::Question,
                id,
                Some(question_ordinal),
                &mut params,
                &mut seen,
            );
        }
        index += 1;
    }
    params
}

pub fn get_param_label(param: &SqlParam) -> String {
    match param.syntax {
        SqlParamSyntax::Colon => format!(":{}", param.name),
        SqlParamSyntax::At => format!("@{}", param.name),
        SqlParamSyntax::DollarPositional | SqlParamSyntax::DollarNamed => format!("${}", param.name),
        SqlParamSyntax::Question => "?".to_owned(),
        SqlParamSyntax::Template => format!("${{{}}}", param.name),
    }
}

fn lookup<'a>(values: &'a BTreeMap<String, ParamValue>, param: &SqlParam) -> Option<&'a ParamValue> {
    values
        .get(&param.stable_id)
        .or_else(|| values.get(&param.name))
        .or_else(|| param.ordinal.and_then(|ordinal| values.get(&ordinal.to_string())))
        .or_else(|| values.get(param.name.trim_start_matches(|ch| ch == '$' || ch == '@' || ch == ':')))
}

fn is_missing(value: Option<&ParamValue>) -> bool {
    match value {
        None => true,
        Some(ParamValue::Null) => true,
        Some(ParamValue::String(text)) => text.trim().is_empty(),
        _ => false,
    }
}

pub fn find_missing_params(
    sql: &str,
    raw_values: Option<&BTreeMap<String, ParamValue>>,
    policy: Option<&SqlParamDialectPolicy>,
) -> Vec<MissingParam> {
    let params = parse_sql_params(sql, policy);
    if params.is_empty() {
        return Vec::new();
    }
    let empty = BTreeMap::new();
    let values = raw_values.unwrap_or(&empty);
    params
        .into_iter()
        .filter(|param| {
            is_missing(lookup(values, param))
        })
        .map(|param| {
            let label = get_param_label(&param);
            MissingParam { param, label }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_at_template_dollar_and_question_syntaxes_are_recognized() {
        let sql = "SELECT :name, @other, ${tpl}, $1, $2, ? FROM t WHERE id = ?";
        let params = parse_sql_params(sql, None);
        let labels: Vec<String> = params.iter().map(get_param_label).collect();
        assert_eq!(
            labels,
            [":name", "@other", "${tpl}", "$1", "$2", "?", "?"]
        );
        let question = &params[6];
        assert_eq!(question.ordinal, Some(2));
        assert_eq!(question.stable_id, "question:2");
        assert_eq!(params[0].kind, SqlParamKind::Named);
        assert_eq!(params[3].kind, SqlParamKind::Positional);
    }

    #[test]
    fn duplicates_keep_the_first_occurrence_and_quoted_text_is_ignored() {
        let sql = "SELECT :id, :id, 'ignored :x', \"col :y\", -- :z\n $1, $1";
        let params = parse_sql_params(sql, None);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "id");
        assert_eq!(params[1].name, "1");
    }

    #[test]
    fn policy_toggles_disable_their_syntaxes() {
        let mut policy = SqlParamDialectPolicy::host_defaults();
        policy.enable_at = false;
        policy.enable_question = false;
        policy.enable_template = false;
        policy.enable_dollar_positional = false;
        let params = parse_sql_params("SELECT @a, ?, ${b}, $1, :c", Some(&policy));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "c");

        let mut named = SqlParamDialectPolicy::host_defaults();
        named.enable_dollar_named = true;
        let params = parse_sql_params("SELECT $name", Some(&named));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].syntax, SqlParamSyntax::DollarNamed);
    }

    #[test]
    fn missing_params_follow_the_lookup_chain_and_empty_rule() {
        let sql = "SELECT :name, @other, ?, $2";
        let all = find_missing_params(sql, None, None);
        assert_eq!(all.len(), 4, "no values means every parameter is missing");
        assert_eq!(all[0].label, ":name");
        assert_eq!(all[3].label, "$2");

        let values = BTreeMap::from([
            ("named:name".to_owned(), ParamValue::String("Ada".to_owned())),
            ("other".to_owned(), ParamValue::String(String::new())),
            ("2".to_owned(), ParamValue::Number("7".to_owned())),
        ]);
        let missing = find_missing_params(sql, Some(&values), None);
        assert_eq!(missing.len(), 2, "name and $1 are present; other is blank and ? is absent");
        assert_eq!(missing[0].label, "@other");
        assert_eq!(missing[1].label, "?");
    }

    #[test]
    fn null_and_whitespace_values_count_as_missing() {
        let sql = "SELECT :a, :b, :c";
        let values = BTreeMap::from([
            ("a".to_owned(), ParamValue::Null),
            ("b".to_owned(), ParamValue::String("   ".to_owned())),
            ("c".to_owned(), ParamValue::Bool(false)),
        ]);
        let missing = find_missing_params(sql, Some(&values), None);
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].param.name, "a");
        assert_eq!(missing[1].param.name, "b");
    }

    #[test]
    fn surface_and_gap_are_pinned() {
        assert_eq!(VALIDATE_BIND_PARAMS_SURFACE.len(), 4);
        assert_eq!(VALIDATE_BIND_PARAMS_GAP.len(), 2);
        assert_eq!(crate::VALIDATE_BIND_PARAMS_SURFACE.len(), 4);
        assert_eq!(crate::VALIDATE_BIND_PARAMS_GAP.len(), 2);
    }

}
