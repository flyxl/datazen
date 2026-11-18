//! The T24 reviewable write-proposal family — React `types/ai.ts` lines
//! 203–267.
//!
//! One statement inside a write proposal, the static impact preview (no
//! statement is executed to build it), the reviewable exact-text proposal
//! itself (`proposalId` doubles as the one-time apply token), the apply
//! receipt, and the lifecycle audit row (which never contains SQL or
//! credentials). The audit `decision` is React's one *open* string union
//! (`'proposal' | 'applied' | 'denied' | string`) — mirrored as a closed
//! enum plus a catch-all variant that round-trips any other string.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// React `AiWriteStatement` — T24: one statement inside a write proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteStatement {
    /// Statement position inside the proposal (0-based).
    pub index: usize,
    /// Normalized verb (`INSERT`, `UPDATE`, …).
    pub verb: String,
    /// Target table when statically resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub sql_hash: String,
}

/// React `AiWriteImpact` — T24: static impact preview; no statement is
/// executed to build it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteImpact {
    pub statement_count: usize,
    pub tables: Vec<String>,
    pub has_where: bool,
    pub destructive: bool,
    pub writes_data: bool,
    pub schema_changes: bool,
    pub summary: String,
}

/// React `AiWriteProposal` — T24: a reviewable exact-text write proposal.
/// `proposal_id` doubles as the one-time apply token; the UI must send it
/// back with the exact SQL and target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteProposal {
    pub proposal_id: String,
    pub token: String,
    pub connection_id: String,
    pub database: String,
    pub sql: String,
    pub sql_hash: String,
    pub created_at: String,
    pub expires_at: String,
    /// Optional unified diff preview of the schema changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub statements: Vec<AiWriteStatement>,
    pub impact: AiWriteImpact,
    pub production_protected: bool,
    pub apply_allowed: bool,
}

/// React `AiWriteApplyReceipt` — T24: result of one successful apply.
/// `rowCount` is required-but-nullable (`number | null`): some verbs
/// report no row count, which serializes as an explicit `null`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteApplyReceipt {
    pub proposal_id: String,
    pub connection_id: String,
    pub database: String,
    pub sql_hash: String,
    pub row_count: Option<u64>,
    /// React `unknown` — whatever the driver reported, opaque here.
    pub result: Value,
}

/// The closed half of React's audit `decision` union.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiWriteAuditDecisionKind {
    Proposal,
    Applied,
    Denied,
}

impl AiWriteAuditDecisionKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposal => "proposal",
            Self::Applied => "applied",
            Self::Denied => "denied",
        }
    }
}

/// React `AiWriteAuditRecord.decision = 'proposal' | 'applied' | 'denied'
/// | string` — an *open* string union: the three lifecycle spellings are
/// known, any other string is carried verbatim (future audit producers).
/// The untagged shape keeps the wire a bare string in both directions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AiWriteAuditDecision {
    Known(AiWriteAuditDecisionKind),
    Other(String),
}

impl AiWriteAuditDecision {
    /// The wire spelling, whichever side of the union this is on.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(kind) => kind.as_str(),
            Self::Other(other) => other.as_str(),
        }
    }
}

/// React `AiWriteAuditRecord` — T24: proposal lifecycle audit row; never
/// contains SQL or credentials. `proposalId`/`connectionId`/
/// `databaseName`/`rowCount` are required-but-nullable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteAuditRecord {
    pub id: String,
    pub organization_id: String,
    pub principal_id: String,
    pub proposal_id: Option<String>,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub sql_hash: String,
    pub decision: AiWriteAuditDecision,
    pub row_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    /// The full proposal round-trips: statements, impact, the optional
    /// diff, and the review flags.
    #[test]
    fn a_write_proposal_round_trips() {
        let wire = serde_json::json!({
            "proposalId": "p-1",
            "token": "tok",
            "connectionId": "conn-1",
            "database": "shop",
            "sql": "update orders set status = 'x' where id = 1",
            "sqlHash": "sha-1",
            "createdAt": "2026-01-01T00:00:00Z",
            "expiresAt": "2026-01-01T00:05:00Z",
            "diff": "--- a\n+++ b\n",
            "statements": [{
                "index": 0,
                "verb": "UPDATE",
                "table": "orders",
                "hasWhere": true,
                "destructive": false,
                "sqlHash": "sha-1"
            }],
            "impact": {
                "statementCount": 1,
                "tables": ["orders"],
                "hasWhere": true,
                "destructive": false,
                "writesData": true,
                "schemaChanges": false,
                "summary": "updates rows in orders"
            },
            "productionProtected": true,
            "applyAllowed": false
        });
        let proposal: AiWriteProposal = decode(wire.clone());
        assert_eq!(proposal.statements[0].index, 0);
        assert_eq!(proposal.statements[0].table.as_deref(), Some("orders"));
        assert_eq!(proposal.impact.writes_data, true);
        assert_eq!(proposal.impact.schema_changes, false);
        assert_eq!(proposal.diff.as_deref(), Some("--- a\n+++ b\n"));
        assert_eq!(serde_json::to_value(&proposal).ok(), Some(wire));

        // The diff is optional: absent stays absent.
        let bare: AiWriteProposal = decode(serde_json::json!({
            "proposalId": "p-2", "token": "t", "connectionId": "c", "database": "d",
            "sql": "delete from t", "sqlHash": "h",
            "createdAt": "2026-01-01T00:00:00Z", "expiresAt": "2026-01-01T00:05:00Z",
            "statements": [], "impact": {
                "statementCount": 0, "tables": [], "hasWhere": false,
                "destructive": true, "writesData": true, "schemaChanges": false,
                "summary": ""
            },
            "productionProtected": false, "applyAllowed": true
        }));
        assert_eq!(bare.diff, None);
        assert!(bare.statements.is_empty());
    }

    /// The apply receipt keeps `rowCount` as an explicit null (a DELETE on
    /// some drivers reports none) and `result` opaque.
    #[test]
    fn an_apply_receipt_round_trips_its_null_row_count() {
        let wire = serde_json::json!({
            "proposalId": "p-1",
            "connectionId": "conn-1",
            "database": "shop",
            "sqlHash": "sha-1",
            "rowCount": null,
            "result": { "rowsAffected": 3 }
        });
        let receipt: AiWriteApplyReceipt = decode(wire.clone());
        assert_eq!(receipt.row_count, None);
        assert_eq!(receipt.result.get("rowsAffected"), Some(&serde_json::json!(3)));
        assert_eq!(serde_json::to_value(&receipt).ok(), Some(wire));

        let counted: AiWriteApplyReceipt = decode(serde_json::json!({
            "proposalId": "p-1", "connectionId": "c", "database": "d",
            "sqlHash": "h", "rowCount": 3, "result": null
        }));
        assert_eq!(counted.row_count, Some(3));
        assert_eq!(counted.result, serde_json::Value::Null);
    }

    /// The audit decision: the three known spellings round-trip through
    /// the closed enum, any other string through the catch-all — the wire
    /// stays a bare string.
    #[test]
    fn audit_decisions_round_trip_the_open_union() {
        let known = [
            (AiWriteAuditDecisionKind::Proposal, "proposal"),
            (AiWriteAuditDecisionKind::Applied, "applied"),
            (AiWriteAuditDecisionKind::Denied, "denied"),
        ];
        for (kind, literal) in known {
            assert_eq!(kind.as_str(), literal);
            let parsed: AiWriteAuditDecisionKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, kind);
            let decision = AiWriteAuditDecision::Known(kind);
            assert_eq!(decision.as_str(), literal);
            assert_eq!(
                serde_json::to_value(&decision).ok(),
                Some(serde_json::json!(literal))
            );
        }
        let other = decode::<AiWriteAuditDecision>(serde_json::json!("expired"));
        assert_eq!(other, AiWriteAuditDecision::Other("expired".to_string()));
        assert_eq!(other.as_str(), "expired");
        assert_eq!(
            serde_json::to_value(&other).ok(),
            Some(serde_json::json!("expired"))
        );
    }

    /// The audit row round-trips with its nullable keys kept present.
    #[test]
    fn an_audit_record_round_trips() {
        let wire = serde_json::json!({
            "id": "a-1",
            "organizationId": "org-1",
            "principalId": "user-1",
            "proposalId": "p-1",
            "connectionId": null,
            "databaseName": "shop",
            "sqlHash": "sha-1",
            "decision": "applied",
            "rowCount": 3,
            "reason": "operator approved",
            "createdAt": "2026-01-01T00:00:00Z"
        });
        let record: AiWriteAuditRecord = decode(wire.clone());
        assert_eq!(
            record.decision,
            AiWriteAuditDecision::Known(AiWriteAuditDecisionKind::Applied)
        );
        assert_eq!(record.connection_id, None);
        assert_eq!(record.row_count, Some(3));
        assert_eq!(serde_json::to_value(&record).ok(), Some(wire));

        // A denial without a reason omits the key entirely.
        let denied: AiWriteAuditRecord = decode(serde_json::json!({
            "id": "a-2", "organizationId": "org-1", "principalId": "user-1",
            "proposalId": null, "connectionId": null, "databaseName": null,
            "sqlHash": "sha-2", "decision": "denied", "rowCount": null,
            "createdAt": "2026-01-01T00:00:00Z"
        }));
        assert_eq!(denied.reason, None);
        assert_eq!(denied.row_count, None);
        let denied_wire = serde_json::to_value(&denied).ok();
        assert_eq!(denied_wire.as_ref().and_then(|wire| wire.get("reason").cloned()), None);
        // The required-but-nullable keys stay present as nulls.
        assert_eq!(
            denied_wire.as_ref().and_then(|wire| wire.get("proposalId").cloned()),
            Some(serde_json::Value::Null)
        );
    }
}
