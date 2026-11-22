use super::*;

fn statement(index: usize, verb: &str, classification: SqlRiskClassification) -> SqlStatementRisk {
    SqlStatementRisk {
        index,
        verb: Some(verb.to_owned()),
        classification,
        findings: Vec::new(),
        comment_hides_write_verb: false,
        has_top_level_where: true,
        strict_write: false,
        range_from: 0,
        range_to: 10,
    }
}

fn assessment(statements: Vec<SqlStatementRisk>) -> SqlRiskAssessment {
    SqlRiskAssessment {
        classification: SqlRiskClassification::Read,
        statements,
        findings: Vec::new(),
        has_high_risk: false,
    }
}

fn mode() -> ExecutionRiskMode {
    ExecutionRiskMode {
        read_only: false,
        safe_mode: false,
        is_production: false,
        confirm_dangerous: true,
    }
}

#[test]
fn literals_and_gap_are_pinned() {
    assert_eq!(SqlRiskClassification::Read.as_str(), "read");
    assert_eq!(SqlRiskClassification::Mutation.as_str(), "mutation");
    assert_eq!(SqlRiskClassification::Unknown.as_str(), "unknown");
    assert_eq!(SqlRiskFindingType::Drop.as_str(), "drop");
    assert_eq!(SqlRiskFindingType::Truncate.as_str(), "truncate");
    assert_eq!(SqlRiskFindingType::NoWhere.as_str(), "no-where");
    assert_eq!(HardBlockReason::ReadOnly.as_str(), "readOnly");
    assert_eq!(HardBlockReason::SafeMode.as_str(), "safeMode");
    assert_eq!(QUERY_EXECUTION_RISK_GAP.len(), 2);
}

#[test]
fn read_only_blocks_strict_writes_only() {
    let read = assessment(vec![statement(0, "SELECT", SqlRiskClassification::Read)]);
    let mut strict = statement(0, "SELECT", SqlRiskClassification::Read);
    strict.strict_write = true;
    let writes = assessment(vec![strict]);
    let host = HostGuardMode {
        read_only: true,
        safe_mode: false,
    };
    assert!(!host_guard_would_block(&read, host));
    assert!(host_guard_would_block(&writes, host));
}

#[test]
fn safe_mode_blocks_ddl_and_where_less_writes() {
    let drop = assessment(vec![statement(0, "DROP", SqlRiskClassification::Mutation)]);
    let truncate = assessment(vec![statement(0, "TRUNCATE", SqlRiskClassification::Mutation)]);
    let mut update = statement(0, "UPDATE", SqlRiskClassification::Mutation);
    update.has_top_level_where = false;
    let no_where = assessment(vec![update.clone()]);
    let host = HostGuardMode {
        read_only: false,
        safe_mode: true,
    };
    assert!(host_guard_would_block(&drop, host));
    assert!(host_guard_would_block(&truncate, host));
    assert!(host_guard_would_block(&no_where, host));
    update.has_top_level_where = true;
    assert!(!host_guard_would_block(&assessment(vec![update]), host));
    let delete = assessment(vec![statement(0, "DELETE", SqlRiskClassification::Mutation)]);
    let mut no_where_delete = statement(0, "DELETE", SqlRiskClassification::Mutation);
    no_where_delete.has_top_level_where = false;
    assert!(!host_guard_would_block(&delete, host));
    assert!(host_guard_would_block(&assessment(vec![no_where_delete]), host));
}

#[test]
fn a_hidden_write_verb_is_blocked_under_either_guard() {
    let mut hidden = statement(0, "", SqlRiskClassification::Unknown);
    hidden.comment_hides_write_verb = true;
    let risk = assessment(vec![hidden]);
    assert!(host_guard_would_block(
        &risk,
        HostGuardMode {
            read_only: true,
            safe_mode: false,
        }
    ));
    assert!(host_guard_would_block(
        &risk,
        HostGuardMode {
            read_only: false,
            safe_mode: true,
        }
    ));
    assert!(!host_guard_would_block(
        &risk,
        HostGuardMode::default()
    ));
}

#[test]
fn hard_block_takes_priority_over_confirmation() {
    let mut strict = statement(0, "SELECT", SqlRiskClassification::Mutation);
    strict.strict_write = true;
    let mut risky = assessment(vec![strict]);
    risky.classification = SqlRiskClassification::Mutation;
    risky.has_high_risk = true;
    let decision = assess_execution_risk(
        &risky,
        ExecutionRiskMode {
            read_only: true,
            is_production: true,
            ..mode()
        },
    );
    assert!(decision.hard_blocked);
    assert_eq!(decision.hard_block_reason, Some(HardBlockReason::ReadOnly));
    assert!(!decision.needs_confirm);
    assert_eq!(decision.confirm_reasons, vec!["production", "high-risk"]);
    assert!(decision.high_risk);
}

#[test]
fn confirmation_reasons_follow_production_and_high_risk() {
    let mut risky = assessment(vec![statement(0, "UPDATE", SqlRiskClassification::Mutation)]);
    risky.classification = SqlRiskClassification::Mutation;
    risky.has_high_risk = true;
    let decision = assess_execution_risk(
        &risky,
        ExecutionRiskMode {
            is_production: true,
            ..mode()
        },
    );
    assert!(!decision.hard_blocked);
    assert_eq!(decision.hard_block_reason, None);
    assert_eq!(decision.confirm_reasons, vec!["production", "high-risk"]);
    assert!(decision.needs_confirm);

    let safe_mode_off_high_risk = assess_execution_risk(
        &risky,
        ExecutionRiskMode {
            safe_mode: false,
            ..mode()
        },
    );
    assert_eq!(safe_mode_off_high_risk.confirm_reasons, vec!["high-risk"]);

    let safe_mode_on = assess_execution_risk(
        &risky,
        ExecutionRiskMode {
            safe_mode: true,
            ..mode()
        },
    );
    assert!(safe_mode_on.confirm_reasons.is_empty());
    assert!(!safe_mode_on.needs_confirm);

    let disabled = assess_execution_risk(
        &risky,
        ExecutionRiskMode {
            confirm_dangerous: false,
            is_production: true,
            ..mode()
        },
    );
    assert!(disabled.confirm_reasons.is_empty());
}

#[test]
fn read_statements_in_production_need_no_confirmation() {
    let read = assessment(vec![statement(0, "SELECT", SqlRiskClassification::Read)]);
    let decision = assess_execution_risk(
        &read,
        ExecutionRiskMode {
            is_production: true,
            ..mode()
        },
    );
    assert!(decision.confirm_reasons.is_empty());
    assert!(!decision.needs_confirm);
    assert_eq!(decision.classification, SqlRiskClassification::Read);
    assert_eq!(decision.statements.len(), 1);
}

#[test]
fn crate_root_reexports_the_risk_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::SqlRiskAssessment>();
    resolves::<crate::SqlStatementRisk>();
    resolves::<crate::SqlRiskFinding>();
    resolves::<crate::ExecutionRiskDecision>();
    assert_eq!(crate::SqlRiskClassification::Mutation.as_str(), "mutation");
    assert_eq!(crate::QUERY_EXECUTION_RISK_GAP.len(), 2);
}
