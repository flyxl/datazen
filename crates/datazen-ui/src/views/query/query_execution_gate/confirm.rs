//! Pure resolution of the risk-confirm dialog copy from `useQueryExecutionGate`.
//! Keys are returned verbatim so the host owns translation and tests can pin
//! the exact locale keys without an invented string.

use super::risk::{FindingType, RiskClassification};

/// The i18n keys React passes to `confirmDangerous` for one confirm decision.
#[allow(dead_code)] // host-facing value; consumed by the reducer and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmSpec {
    pub title_key: &'static str,
    pub message_key: &'static str,
    pub badge_key: Option<&'static str>,
    pub confirm_label_key: &'static str,
    pub finding_keys: Vec<&'static str>,
    pub code_preview: String,
}

/// React `resolveConfirmI18n` branch order: combined → production → high-risk → chat-write → dangerous.
#[allow(dead_code)] // pure helper; consumed by the spec builder and tests.
pub fn confirm_keys(reasons: &[&str]) -> (&'static str, &'static str, Option<&'static str>) {
    let production = reasons.contains(&"production");
    let high_risk = reasons.contains(&"high-risk");
    if production && high_risk {
        (
            "query.editor.executionConfirm.combinedTitle",
            "query.editor.executionConfirm.combinedMessage",
            Some("query.editor.executionConfirm.badgeProduction"),
        )
    } else if production {
        (
            "query.editor.executionConfirm.productionTitle",
            "query.editor.executionConfirm.productionMessage",
            Some("query.editor.executionConfirm.badgeProduction"),
        )
    } else if high_risk {
        (
            "query.editor.executionConfirm.highRiskTitle",
            "query.editor.executionConfirm.highRiskMessage",
            Some("query.editor.executionConfirm.badgeHighRisk"),
        )
    } else if reasons.contains(&"chat-write") {
        (
            "query.editor.executionConfirm.chatWriteTitle",
            "query.editor.executionConfirm.chatWriteMessage",
            None,
        )
    } else {
        ("query.dangerousSqlTitle", "query.dangerousSqlConfirm", None)
    }
}

/// React `resolveFindingDescriptions` for the findings the gate produces.
#[allow(dead_code)] // pure helper; consumed by the spec builder and tests.
pub fn finding_keys(
    findings: &[FindingType],
    classification: RiskClassification,
) -> Vec<&'static str> {
    let mut keys = Vec::new();
    for finding in findings {
        let key = match finding {
            FindingType::Drop => "query.editor.executionConfirm.findingDrop",
            FindingType::Truncate => "query.editor.executionConfirm.findingTruncate",
            FindingType::NoWhereUpdate => "query.editor.executionConfirm.findingUpdateNoWhere",
            FindingType::NoWhereDelete => "query.editor.executionConfirm.findingDeleteNoWhere",
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if classification == RiskClassification::Unknown && keys.is_empty() {
        keys.push("query.editor.executionConfirm.findingUnknown");
    }
    keys
}

/// Build the full confirm spec for a decision.
#[allow(dead_code)] // pure helper; consumed by the reducer and tests.
pub fn confirm_spec(
    reasons: &[&str],
    findings: &[FindingType],
    classification: RiskClassification,
    sql: &str,
) -> ConfirmSpec {
    let (title_key, message_key, badge_key) = confirm_keys(reasons);
    ConfirmSpec {
        title_key,
        message_key,
        badge_key,
        confirm_label_key: "query.editor.executionConfirm.confirmExecute",
        finding_keys: finding_keys(findings, classification),
        code_preview: sql.to_owned(),
    }
}
