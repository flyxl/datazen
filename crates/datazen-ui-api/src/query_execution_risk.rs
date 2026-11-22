#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRiskClassification {
    Read,
    Mutation,
    Unknown,
}

impl SqlRiskClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlRiskClassification::Read => "read",
            SqlRiskClassification::Mutation => "mutation",
            SqlRiskClassification::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlRiskFindingType {
    Drop,
    Truncate,
    NoWhere,
}

impl SqlRiskFindingType {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlRiskFindingType::Drop => "drop",
            SqlRiskFindingType::Truncate => "truncate",
            SqlRiskFindingType::NoWhere => "no-where",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRiskFinding {
    pub finding_type: SqlRiskFindingType,
    pub from: usize,
    pub to: usize,
    pub statement_index: usize,
    pub verb: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlStatementRisk {
    pub index: usize,
    pub verb: Option<String>,
    pub classification: SqlRiskClassification,
    pub findings: Vec<SqlRiskFinding>,
    pub comment_hides_write_verb: bool,
    pub has_top_level_where: bool,
    pub strict_write: bool,
    pub range_from: usize,
    pub range_to: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRiskAssessment {
    pub statements: Vec<SqlStatementRisk>,
    pub classification: SqlRiskClassification,
    pub findings: Vec<SqlRiskFinding>,
    pub has_high_risk: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostGuardMode {
    pub read_only: bool,
    pub safe_mode: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionRiskMode {
    pub read_only: bool,
    pub safe_mode: bool,
    pub is_production: bool,
    pub confirm_dangerous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardBlockReason {
    ReadOnly,
    SafeMode,
}

impl HardBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            HardBlockReason::ReadOnly => "readOnly",
            HardBlockReason::SafeMode => "safeMode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRiskDecision {
    pub classification: SqlRiskClassification,
    pub findings: Vec<SqlRiskFinding>,
    pub statements: Vec<SqlStatementRisk>,
    pub hard_blocked: bool,
    pub hard_block_reason: Option<HardBlockReason>,
    pub needs_confirm: bool,
    pub high_risk: bool,
    pub confirm_reasons: Vec<String>,
}

pub const QUERY_EXECUTION_RISK_GAP: [&str; 2] = [
    "classifyRisk lexer and shared S2-A token stream",
    "Host check_sql enforcement parity fixture",
];

fn statement_block_reason(statement: &SqlStatementRisk, mode: HostGuardMode) -> Option<HardBlockReason> {
    let verb = statement.verb.as_deref().unwrap_or("");
    if mode.read_only && statement.strict_write {
        return Some(HardBlockReason::ReadOnly);
    }
    if mode.safe_mode && (verb == "DROP" || verb == "TRUNCATE") {
        return Some(HardBlockReason::SafeMode);
    }
    if mode.safe_mode
        && (verb == "UPDATE" || verb == "DELETE")
        && !statement.has_top_level_where
    {
        return Some(HardBlockReason::SafeMode);
    }
    if (mode.read_only || mode.safe_mode)
        && statement.classification == SqlRiskClassification::Unknown
        && statement.comment_hides_write_verb
    {
        return Some(HardBlockReason::SafeMode);
    }
    None
}

pub fn host_guard_would_block(assessment: &SqlRiskAssessment, mode: HostGuardMode) -> bool {
    assessment
        .statements
        .iter()
        .any(|statement| statement_block_reason(statement, mode).is_some())
}

pub fn assess_execution_risk(
    assessment: &SqlRiskAssessment,
    mode: ExecutionRiskMode,
) -> ExecutionRiskDecision {
    let mut hard_blocked = false;
    let mut hard_block_reason = None;
    for statement in &assessment.statements {
        if hard_blocked {
            break;
        }
        if let Some(reason) = statement_block_reason(
            statement,
            HostGuardMode {
                read_only: mode.read_only,
                safe_mode: mode.safe_mode,
            },
        ) {
            hard_blocked = true;
            hard_block_reason = Some(reason);
        }
    }

    let mut confirm_reasons = Vec::new();
    if mode.confirm_dangerous {
        if mode.is_production && assessment.classification != SqlRiskClassification::Read {
            confirm_reasons.push("production".to_owned());
        }
        if !mode.safe_mode && assessment.has_high_risk {
            confirm_reasons.push("high-risk".to_owned());
        }
    }
    let needs_confirm = !hard_blocked && !confirm_reasons.is_empty();

    ExecutionRiskDecision {
        classification: assessment.classification,
        findings: assessment.findings.clone(),
        statements: assessment.statements.clone(),
        hard_blocked,
        hard_block_reason,
        needs_confirm,
        high_risk: assessment.has_high_risk,
        confirm_reasons,
    }
}
