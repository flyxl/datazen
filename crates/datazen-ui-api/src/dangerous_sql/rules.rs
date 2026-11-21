use std::collections::HashSet;

use super::types::*;

pub(crate) struct StatementSpan {
    pub range: SqlStatementRange,
    pub raw_from: usize,
    pub raw_to: usize,
}

fn is_word_text(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

fn is_plain_word_text(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(crate) fn split_statements(source: &str, tokens: &[SqlToken]) -> Vec<StatementSpan> {
    let chars: Vec<char> = source.chars().collect();
    let mut raw = Vec::new();
    let mut start = 0usize;
    for token in tokens {
        if token.kind == SqlTokenKind::Semicolon && token.paren_depth == 0 {
            raw.push((start, token.from));
            start = token.to;
        }
    }
    raw.push((start, chars.len()));

    let mut out = Vec::new();
    for (raw_from, raw_to) in raw {
        let mut from = raw_from;
        let mut to = raw_to;
        while from < to && chars[from].is_whitespace() {
            from += 1;
        }
        while to > from && chars[to - 1].is_whitespace() {
            to -= 1;
        }
        if from < to {
            out.push(StatementSpan {
                range: SqlStatementRange { from, to },
                raw_from,
                raw_to,
            });
        }
    }
    out
}

pub(crate) fn build_sig(tokens: &[SqlToken]) -> Vec<SigTok> {
    let mut sig = Vec::new();
    for token in tokens {
        match token.kind {
            SqlTokenKind::OpenParen => sig.push(SigTok {
                kind: SigKind::Open,
                word: None,
                from: token.from,
                to: token.to,
                depth: token.paren_depth,
            }),
            SqlTokenKind::CloseParen => sig.push(SigTok {
                kind: SigKind::Close,
                word: None,
                from: token.from,
                to: token.to,
                depth: token.paren_depth,
            }),
            SqlTokenKind::Other if is_word_text(&token.text) => sig.push(SigTok {
                kind: SigKind::Word,
                word: Some(token.text.to_uppercase()),
                from: token.from,
                to: token.to,
                depth: token.paren_depth,
            }),
            _ => {}
        }
    }
    sig
}

fn is_main_verb(word: &str) -> bool {
    MAIN_VERBS.contains(&word)
}

fn skip_with_clause(sig: &[SigTok]) -> Option<usize> {
    let mut depth = 0i32;
    for index in 1..sig.len() {
        match sig[index].kind {
            SigKind::Open => depth += 1,
            SigKind::Close => depth = (depth - 1).max(0),
            SigKind::Word => {
                if depth == 0 && is_main_verb(sig[index].word.as_deref().unwrap_or("")) {
                    return Some(index);
                }
            }
        }
    }
    None
}

pub(crate) fn extract_main_verb(sig: &[SigTok]) -> Option<SigTok> {
    if sig.is_empty() {
        return None;
    }
    let mut index = 0usize;
    if sig[0].kind == SigKind::Word && sig[0].word.as_deref() == Some("WITH") {
        index = skip_with_clause(sig)?;
    }
    let token = &sig[index];
    if token.kind == SigKind::Word {
        Some(token.clone())
    } else {
        None
    }
}

fn matching_close_paren(sig: &[SigTok], open_index: usize) -> Option<usize> {
    let mut depth = 0i32;
    for index in open_index..sig.len() {
        match sig[index].kind {
            SigKind::Open => depth += 1,
            SigKind::Close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            SigKind::Word => {}
        }
    }
    None
}

pub(crate) fn has_data_modifying_cte(sig: &[SigTok]) -> bool {
    for index in 0..sig.len() {
        if sig[index].kind != SigKind::Word || sig[index].word.as_deref() != Some("AS") {
            continue;
        }
        let mut open = index + 1;
        if sig.get(open).and_then(|tok| tok.word.as_deref()) == Some("NOT")
            && sig.get(open + 1).and_then(|tok| tok.word.as_deref()) == Some("MATERIALIZED")
        {
            open += 2;
        } else if sig.get(open).and_then(|tok| tok.word.as_deref()) == Some("MATERIALIZED") {
            open += 1;
        }
        if sig.get(open).map(|tok| tok.kind) != Some(SigKind::Open) {
            continue;
        }
        let Some(close) = matching_close_paren(sig, open) else {
            continue;
        };
        let body = &sig[open + 1..close];
        if let Some(verb) = extract_main_verb(body) {
            if verb
                .word
                .as_deref()
                .is_some_and(|word| WRITE_VERBS.contains(&word))
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn has_select_into(sig: &[SigTok]) -> bool {
    let mut select_depths = HashSet::new();
    for token in sig {
        if token.kind != SigKind::Word {
            continue;
        }
        match token.word.as_deref() {
            Some("SELECT") => {
                select_depths.insert(token.depth);
            }
            Some("INTO") if select_depths.contains(&token.depth) => return true,
            _ => {}
        }
    }
    false
}

fn is_analyze_keyword(word: Option<&str>) -> bool {
    matches!(word, Some("ANALYZE") | Some("ANALYSE"))
}

fn is_explicit_bool(word: Option<&str>) -> bool {
    word.is_some_and(|value| BOOLEAN_SQL_VALUES.contains(&value))
}

fn is_false_value(word: Option<&str>) -> bool {
    word.is_some_and(|value| FALSE_SQL_VALUES.contains(&value))
}

pub(crate) fn explain_analyze_statement(sig: &[SigTok]) -> Option<Vec<SigTok>> {
    if sig.first().and_then(|tok| tok.word.as_deref()) != Some("EXPLAIN") {
        return None;
    }
    if sig.get(1).map(|tok| tok.kind) == Some(SigKind::Open) {
        let close = matching_close_paren(sig, 1)?;
        let options: Vec<&SigTok> = sig[2..close]
            .iter()
            .filter(|tok| tok.kind == SigKind::Word)
            .collect();
        let analyze_index = options
            .iter()
            .position(|tok| is_analyze_keyword(tok.word.as_deref()))?;
        let next = options
            .get(analyze_index + 1)
            .and_then(|tok| tok.word.as_deref());
        if is_false_value(next) {
            return None;
        }
        return Some(sig[close + 1..].to_vec());
    }
    let mut statement_index = 1usize;
    let mut saw_analyze = false;
    while statement_index < sig.len() {
        let word = sig[statement_index].word.as_deref();
        if is_analyze_keyword(word) {
            saw_analyze = true;
            statement_index += 1;
            let bool_word = sig.get(statement_index).and_then(|tok| tok.word.as_deref());
            if is_explicit_bool(bool_word) {
                if is_false_value(bool_word) {
                    return None;
                }
                statement_index += 1;
            }
        } else if word == Some("VERBOSE") {
            statement_index += 1;
        } else {
            break;
        }
    }
    if !saw_analyze {
        return None;
    }
    Some(sig[statement_index..].to_vec())
}

pub(crate) fn has_top_level_where(tokens: &[SqlToken]) -> bool {
    tokens.iter().any(|token| {
        token.kind == SqlTokenKind::Other
            && token.paren_depth == 0
            && is_plain_word_text(&token.text)
            && token.text.to_uppercase() == "WHERE"
    })
}

pub(crate) fn comment_contains_write_verb(tokens: &[SqlToken]) -> bool {
    for token in tokens {
        if token.kind != SqlTokenKind::BlockComment {
            continue;
        }
        let chars: Vec<char> = token.text.chars().collect();
        if chars.len() < 4 {
            continue;
        }
        let inner: String = chars[2..chars.len() - 2].iter().collect();
        for word in inner.split_whitespace() {
            let upper = word.to_uppercase();
            if !word.is_empty() && WRITE_VERBS.contains(&upper.as_str()) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn is_may_write(verb: Option<&str>, sig: &[SigTok]) -> bool {
    match verb {
        Some("SELECT") => sig.iter().any(|token| {
            token.kind == SigKind::Word && token.word.as_deref() == Some("INTO") && token.depth == 0
        }),
        Some("EXPLAIN") => explain_analyze_statement(sig).is_some(),
        _ => false,
    }
}

pub(crate) fn classify_verb(
    verb: Option<&str>,
    strict_write: bool,
    may_write: bool,
) -> SqlRiskClassification {
    if strict_write || may_write {
        SqlRiskClassification::Mutation
    } else if verb.is_some_and(|value| READ_VERBS.contains(&value)) {
        SqlRiskClassification::Read
    } else {
        SqlRiskClassification::Unknown
    }
}

pub(crate) fn build_findings(
    verb: Option<&str>,
    verb_token: Option<&SigTok>,
    has_where: bool,
    statement_index: usize,
) -> Vec<SqlRiskFinding> {
    let (Some(verb), Some(token)) = (verb, verb_token) else {
        return Vec::new();
    };
    if DESTRUCTIVE_VERBS.contains(&verb) {
        let kind = if verb == "DROP" {
            SqlRiskFindingType::Drop
        } else {
            SqlRiskFindingType::Truncate
        };
        return vec![SqlRiskFinding {
            kind,
            from: token.from,
            to: token.to,
            statement_index,
            verb: verb.to_owned(),
        }];
    }
    if WHERE_NEEDS_VERBS.contains(&verb) && !has_where {
        return vec![SqlRiskFinding {
            kind: SqlRiskFindingType::NoWhere,
            from: token.from,
            to: token.to,
            statement_index,
            verb: verb.to_owned(),
        }];
    }
    Vec::new()
}

pub(crate) fn aggregate_classification(statements: &[SqlStatementRisk]) -> SqlRiskClassification {
    if statements
        .iter()
        .any(|statement| statement.classification == SqlRiskClassification::Mutation)
    {
        SqlRiskClassification::Mutation
    } else if statements
        .iter()
        .any(|statement| statement.classification == SqlRiskClassification::Unknown)
    {
        SqlRiskClassification::Unknown
    } else {
        SqlRiskClassification::Read
    }
}
