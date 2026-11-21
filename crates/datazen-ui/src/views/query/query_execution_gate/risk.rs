//! Pure risk assessment ported from `queryExecutionRisk.ts` / `dangerousSql.ts`.
//! The classifier is intentionally lexical (no SQL engine) but keeps the gate
//! decisions the hook test pins: read vs write, hard blocks, and confirm reasons.

use super::sql::statement_ranges;

#[allow(dead_code)] // VM mirror; consumed by the reducer and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RiskClassification {
    #[default]
    Read,
    Mutation,
    Unknown,
}

impl RiskClassification {
    fn rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Unknown => 1,
            Self::Mutation => 2,
        }
    }
}

#[allow(dead_code)] // VM mirror; consumed by confirm i18n and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FindingType {
    Drop,
    Truncate,
    NoWhereUpdate,
    NoWhereDelete,
}

#[allow(dead_code)] // VM mirror; consumed by the reducer and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StatementRisk {
    pub(crate) verb: Option<String>,
    pub(crate) classification: RiskClassification,
    pub(crate) has_top_level_where: bool,
    pub(crate) strict_write: bool,
    pub(crate) comment_hides_write_verb: bool,
    pub(crate) findings: Vec<FindingType>,
}

#[allow(dead_code)] // VM mirror; consumed by the reducer and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RiskMode {
    pub(crate) read_only: bool,
    pub(crate) safe_mode: bool,
    pub(crate) is_production: bool,
    pub(crate) confirm_dangerous: bool,
}

#[allow(dead_code)] // VM mirror; consumed by the reducer and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RiskDecision {
    pub(crate) classification: RiskClassification,
    pub(crate) findings: Vec<FindingType>,
    pub(crate) statements: Vec<StatementRisk>,
    pub(crate) hard_blocked: bool,
    pub(crate) hard_block_reason: Option<&'static str>,
    pub(crate) needs_confirm: bool,
    pub(crate) high_risk: bool,
    pub(crate) confirm_reasons: Vec<&'static str>,
}

const WRITE_VERBS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "REPLACE", "CREATE", "ALTER", "DROP",
    "TRUNCATE", "GRANT", "REVOKE", "RENAME", "COPY", "LOAD", "UNLOAD", "CALL", "EXEC",
    "EXECUTE", "DO", "HANDLER", "OPTIMIZE", "REPAIR", "FLUSH", "RESET", "KILL", "SHUTDOWN",
    "PURGE", "VACUUM", "LOCK", "UNLOCK", "COMMENT",
];
const READ_VERBS: &[&str] = &[
    "SELECT", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "PRAGMA", "USE", "WITH", "VALUES",
];

/// Uppercase code words of one statement (strings/comments excluded).
fn code_words(statement: &str) -> Vec<String> {
    statement
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_uppercase())
        .collect()
}

/// Write verbs appearing only inside comments of the whole script.
fn comment_hides_write(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let comment_start = (bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-'))
            || (bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*'));
        if !comment_start {
            i += 1;
            continue;
        }
        let line = bytes[i] == b'-';
        let end_marker: &[u8] = if line { b"\n" } else { b"*/" };
        let mut end = i + 2;
        while end < bytes.len() && !bytes[end..].starts_with(end_marker) {
            end += 1;
        }
        let body = sql.get(i..end).unwrap_or("");
        let words = code_words(body);
        if words.iter().any(|word| WRITE_VERBS.contains(&word.as_str())) {
            return true;
        }
        i = end.saturating_add(2);
    }
    false
}

fn classify_statement(statement: &str, hidden: bool) -> StatementRisk {
    let words = code_words(statement);
    let verb = words.first().cloned();
    let verb_key = verb.as_deref().unwrap_or("");
    let is_write = WRITE_VERBS.contains(&verb_key);
    let is_read = READ_VERBS.contains(&verb_key);
    let classification = if is_write {
        RiskClassification::Mutation
    } else if is_read {
        RiskClassification::Read
    } else {
        RiskClassification::Unknown
    };
    let has_where = words.iter().any(|word| word == "WHERE");
    let mut findings = Vec::new();
    if verb_key == "DROP" {
        findings.push(FindingType::Drop);
    } else if verb_key == "TRUNCATE" {
        findings.push(FindingType::Truncate);
    } else if verb_key == "UPDATE" && !has_where {
        findings.push(FindingType::NoWhereUpdate);
    } else if verb_key == "DELETE" && !has_where {
        findings.push(FindingType::NoWhereDelete);
    }
    StatementRisk {
        verb,
        classification,
        has_top_level_where: has_where,
        // SELECT INTO and modifying CTEs are beyond this lexical port; the
        // host guard remains the authority for those dialects.
        strict_write: is_write,
        comment_hides_write_verb: hidden,
        findings,
    }
}

/// React `assessExecutionRisk` with the same branch order as the source.
#[allow(dead_code)] // pure gate; consumed by the reducer and tests.
pub(crate) fn assess(sql: &str, mode: &RiskMode) -> RiskDecision {
    let hidden = comment_hides_write(sql);
    let statements: Vec<StatementRisk> = statement_ranges(sql)
        .into_iter()
        .map(|(from, to)| classify_statement(sql.get(from..to).unwrap_or(""), hidden))
        .collect();
    let mut classification = RiskClassification::Read;
    let mut findings = Vec::new();
    for statement in &statements {
        if statement.classification.rank() > classification.rank() {
            classification = statement.classification;
        }
        for finding in &statement.findings {
            if !findings.contains(finding) {
                findings.push(*finding);
            }
        }
    }

    let mut hard_blocked = false;
    let mut hard_block_reason = None;
    for statement in &statements {
        if mode.read_only && statement.strict_write {
            hard_blocked = true;
            hard_block_reason = Some("readOnly");
        } else if mode.safe_mode
            && statement.verb.as_deref().is_some_and(|v| v == "DROP" || v == "TRUNCATE")
        {
            hard_blocked = true;
            hard_block_reason = Some("safeMode");
        } else if mode.safe_mode
            && statement.verb.as_deref().is_some_and(|v| v == "UPDATE" || v == "DELETE")
            && !statement.has_top_level_where
        {
            hard_blocked = true;
            hard_block_reason = Some("safeMode");
        } else if (mode.read_only || mode.safe_mode)
            && statement.classification == RiskClassification::Unknown
            && statement.comment_hides_write_verb
        {
            hard_blocked = true;
            hard_block_reason = Some("safeMode");
        }
        if hard_blocked {
            break;
        }
    }

    let mut confirm_reasons: Vec<&'static str> = Vec::new();
    if mode.confirm_dangerous {
        if mode.is_production && classification != RiskClassification::Read {
            confirm_reasons.push("production");
        }
        if !mode.safe_mode && !findings.is_empty() {
            confirm_reasons.push("high-risk");
        }
    }
    let needs_confirm = !hard_blocked && !confirm_reasons.is_empty();
    let high_risk = !findings.is_empty();
    RiskDecision {
        classification,
        findings,
        statements,
        hard_blocked,
        hard_block_reason,
        needs_confirm,
        high_risk,
        confirm_reasons,
    }
}
