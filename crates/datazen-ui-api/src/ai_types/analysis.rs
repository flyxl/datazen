//! Diagnosis and explain-plan analysis results — React `types/ai.ts`
//! lines 156–183.
//!
//! These are the shapes the AI-assisted query helpers return
//! (`commands/ai.ts` diagnosis/explain surface; the kernel has
//! field-for-field twins in `datazen::ai::domain::types` — see the module
//! docs for the cfg-gated mirror trade).

use serde::{Deserialize, Serialize};

/// React `DiagnosisResult` — what the diagnosis helper reports about a
/// failing statement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisResult {
    pub explanation: String,
    /// `string | null` (required-but-nullable): no suggestion serializes
    /// as an explicit `null`, and the key stays present.
    pub suggested_sql: Option<String>,
    pub changes: Vec<String>,
}

/// React `Bottleneck.severity = 'high' | 'medium' | 'low'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BottleneckSeverity {
    High,
    Medium,
    Low,
}

impl BottleneckSeverity {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// React `Bottleneck` — one slow node of an explain plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bottleneck {
    pub node: String,
    pub description: String,
    pub severity: BottleneckSeverity,
}

/// React `ExplainSuggestion` — one actionable improvement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainSuggestion {
    pub description: String,
    /// `string | null` (required-but-nullable): an index suggestion has
    /// SQL, a plan-shape one may not.
    pub sql: Option<String>,
    pub impact: String,
}

/// React `ExplainAnalysis` — the explain helper's full report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainAnalysis {
    pub summary: String,
    pub bottlenecks: Vec<Bottleneck>,
    pub suggestions: Vec<ExplainSuggestion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn bottleneck_severities_keep_the_react_literals() {
        let table = [
            (BottleneckSeverity::High, "high"),
            (BottleneckSeverity::Medium, "medium"),
            (BottleneckSeverity::Low, "low"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: BottleneckSeverity = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    /// A diagnosis with a suggestion and one without: the null stays an
    /// explicit key.
    #[test]
    fn a_diagnosis_round_trips_its_null_suggestion() {
        let wire = serde_json::json!({
            "explanation": "The statement full-scans orders.",
            "suggestedSql": "create index idx_orders_id on orders(id)",
            "changes": ["add an index on orders.id"]
        });
        let result: DiagnosisResult = decode(wire.clone());
        assert_eq!(
            result.suggested_sql.as_deref(),
            Some("create index idx_orders_id on orders(id)")
        );
        assert_eq!(serde_json::to_value(&result).ok(), Some(wire));

        let bare: DiagnosisResult = decode(serde_json::json!({
            "explanation": "The plan is already optimal.",
            "suggestedSql": null,
            "changes": []
        }));
        assert_eq!(bare.suggested_sql, None);
        assert_eq!(
            serde_json::to_value(&bare).ok(),
            Some(serde_json::json!({
                "explanation": "The plan is already optimal.",
                "suggestedSql": null,
                "changes": []
            }))
        );
    }

    /// A full explain analysis round-trips: bottlenecks with severities,
    /// suggestions with and without SQL.
    #[test]
    fn an_explain_analysis_round_trips() {
        let wire = serde_json::json!({
            "summary": "Sort spills to disk.",
            "bottlenecks": [
                { "node": "Sort", "description": "spills 12MB", "severity": "high" },
                { "node": "Seq Scan", "description": "full scan of orders", "severity": "medium" }
            ],
            "suggestions": [
                {
                    "description": "Index the sort key",
                    "sql": "create index on orders(created_at)",
                    "impact": "removes the spill"
                },
                {
                    "description": "Raise work_mem for this session",
                    "sql": null,
                    "impact": "marginal"
                }
            ]
        });
        let analysis: ExplainAnalysis = decode(wire.clone());
        assert_eq!(analysis.bottlenecks[0].severity, BottleneckSeverity::High);
        assert_eq!(analysis.bottlenecks[1].severity, BottleneckSeverity::Medium);
        assert_eq!(
            analysis.suggestions[0].sql.as_deref(),
            Some("create index on orders(created_at)")
        );
        assert_eq!(analysis.suggestions[1].sql, None);
        assert_eq!(serde_json::to_value(&analysis).ok(), Some(wire));
    }
}
