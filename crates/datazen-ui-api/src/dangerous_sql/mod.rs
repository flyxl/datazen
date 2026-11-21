mod rules;
mod scanner;
mod tests;
mod types;

pub use scanner::{normalize_fullwidth, scan_sql};
pub use types::{
    SigKind, SigTok, SqlRiskAssessment, SqlRiskClassification, SqlRiskFinding, SqlRiskFindingType,
    SqlStatementRange, SqlStatementRisk, SqlToken, SqlTokenKind, DESTRUCTIVE_VERBS,
    FALSE_SQL_VALUES, MAIN_VERBS, READ_VERBS, WHERE_NEEDS_VERBS, WRITE_VERBS,
};

use rules::{
    aggregate_classification, build_findings, build_sig, classify_verb,
    comment_contains_write_verb, explain_analyze_statement, extract_main_verb,
    has_data_modifying_cte, has_select_into, has_top_level_where, is_may_write, split_statements,
};
pub fn classify_risk(sql: &str) -> SqlRiskAssessment {
    let normalized = normalize_fullwidth(sql);
    let tokens = scan_sql(&normalized);
    let statements: Vec<SqlStatementRisk> = split_statements(&normalized, &tokens)
        .into_iter()
        .enumerate()
        .map(|(index, statement)| {
            let statement_tokens: Vec<SqlToken> = tokens
                .iter()
                .filter(|token| token.from >= statement.raw_from && token.to <= statement.raw_to)
                .cloned()
                .collect();
            let sig = build_sig(&statement_tokens);
            let verb_token = extract_main_verb(&sig);
            let verb = verb_token.as_ref().and_then(|token| token.word.clone());
            let executable_sig = if verb.as_deref() == Some("EXPLAIN") {
                explain_analyze_statement(&sig).unwrap_or_default()
            } else {
                sig.clone()
            };
            let executable_verb = extract_main_verb(&executable_sig).and_then(|token| token.word);
            let strict_write = executable_verb
                .as_deref()
                .is_some_and(|value| WRITE_VERBS.contains(&value))
                || has_data_modifying_cte(&executable_sig)
                || has_select_into(&executable_sig);
            let has_where = has_top_level_where(&statement_tokens);
            let comment_hides_write_verb = comment_contains_write_verb(&statement_tokens);
            let may_write = verb
                .as_deref()
                .is_some_and(|value| is_may_write(Some(value), &sig));
            let classification = classify_verb(verb.as_deref(), strict_write, may_write);
            let findings = build_findings(verb.as_deref(), verb_token.as_ref(), has_where, index);
            SqlStatementRisk {
                index,
                verb,
                classification,
                findings,
                comment_hides_write_verb,
                has_top_level_where: has_where,
                strict_write,
                range: statement.range,
            }
        })
        .collect();
    let findings: Vec<SqlRiskFinding> = statements
        .iter()
        .flat_map(|statement| statement.findings.clone())
        .collect();
    SqlRiskAssessment {
        classification: aggregate_classification(&statements),
        statements,
        has_high_risk: !findings.is_empty(),
        findings,
    }
}

pub fn is_dangerous_write_statement(stmt: &str) -> bool {
    classify_risk(stmt).statements.iter().any(|statement| {
        statement.findings.iter().any(|finding| {
            matches!(
                finding.kind,
                SqlRiskFindingType::Drop | SqlRiskFindingType::Truncate
            )
        })
    })
}

pub fn sql_contains_dangerous_write(sql: &str) -> bool {
    is_dangerous_write_statement(sql)
}
