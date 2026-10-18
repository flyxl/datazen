//! Bounded durable projection: Runtime IDs/codes and proof digests only.
use super::transaction_proof::Proof;
use datazen_feature_core::workflow::WorkflowCommandStep;
use datazen_platform_api::{
    dto::{
        execution::{EffectOutcome, ExecutionState, ExecutionView},
        job::{CommitBoundary, JobDomainResult, JobResultItem},
    },
    id::{ExecutionId, StageId, Timestamp},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Evidence {
    records: Vec<Record>,
    verified: BTreeSet<String>,
    pub overflow: bool,
    sessions: BTreeMap<String, String>,
}
struct Record {
    id: ExecutionId,
    block: String,
    phase: &'static str,
    terminal: Option<(ExecutionState, EffectOutcome)>,
}
impl Evidence {
    pub fn opened(
        &mut self,
        handle: &datazen_platform_api::dto::session::SessionHandle,
        block: &str,
    ) {
        self.sessions
            .insert(handle.db_session_id.as_str().into(), block.into());
    }
    pub fn block_for(
        &self,
        handle: &datazen_platform_api::dto::session::SessionHandle,
    ) -> Option<String> {
        self.sessions.get(handle.db_session_id.as_str()).cloned()
    }
    pub fn is_verified(&self, block: &str) -> bool {
        self.verified.contains(block)
    }

    pub fn accepted(&mut self, step: &WorkflowCommandStep, id: &ExecutionId) {
        if self.records.len() >= 1024 {
            self.overflow = true;
            return;
        }
        let (block, phase) = if let Some(block) = step.id.strip_suffix("__transaction_control") {
            (
                block.to_owned(),
                match step.input["sql"].as_str() {
                    Some("BEGIN") => "begin",
                    Some("COMMIT") => "commit",
                    Some("ROLLBACK") => "rollback",
                    _ => "statement",
                },
            )
        } else if let Some((block, _)) = step.id.split_once("__proof_").filter(|_| {
            step.input["sql"].as_str().is_some_and(|sql| {
                sql.starts_with("LOCK TABLE ONLY ")
                    || sql.contains("FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n")
            })
        }) {
            (block.into(), "proof")
        } else {
            (String::new(), "statement")
        };
        self.records.push(Record {
            id: id.clone(),
            block,
            phase,
            terminal: None,
        });
    }
    pub fn finished(&mut self, view: &ExecutionView) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.id == view.execution_id)
        {
            record.terminal = Some((view.state, view.effect_outcome));
        }
    }
    pub fn verified(&mut self, block: &str) {
        self.verified.insert(block.into());
    }
    pub fn ids(&self) -> Vec<ExecutionId> {
        self.records
            .iter()
            .map(|record| record.id.clone())
            .collect()
    }
    pub fn boundaries(
        &self,
        stage: &StageId,
        proofs: &BTreeMap<String, Proof>,
    ) -> Vec<CommitBoundary> {
        self.records
            .iter()
            .filter(|record| {
                record.phase == "commit"
                    && self.verified.contains(&record.block)
                    && record.terminal
                        == Some((ExecutionState::Succeeded, EffectOutcome::Completed))
            })
            .filter_map(|commit| {
                let proof = proofs.get(&commit.block)?;
                let begin = self.records.iter().find(|record| {
                    record.block == commit.block
                        && record.phase == "begin"
                        && record.terminal.is_some_and(|(state, effects)| {
                            state == ExecutionState::Succeeded
                                && matches!(
                                    effects,
                                    EffectOutcome::Completed | EffectOutcome::PartiallyApplied
                                )
                        })
                })?;
                let now = Timestamp::new(chrono::Utc::now().to_rfc3339());
                Some(CommitBoundary {
                    stage_id: stage.clone(),
                    stable_target_fingerprint: proof.fingerprint.clone(),
                    committed_at: now.clone(),
                    operation_id: Some(proof.fingerprint.clone()),
                    batch_id: None,
                    payload_digest: Some(proof.fingerprint.clone()),
                    evidence: vec![
                        format!("begin:{}", begin.id.as_str()),
                        format!("commit:{}", commit.id.as_str()),
                    ],
                    verified_at: Some(now),
                })
            })
            .collect()
    }
    pub fn durable(&self, stage: &StageId, proofs: &BTreeMap<String, Proof>) -> JobDomainResult {
        let mut result = JobDomainResult {
            stage_id: stage.clone(),
            result_code: "workflowExecutionEvidenceV1".into(),
            outcome_code: if self.overflow {
                "evidenceOverflow"
            } else {
                "runtimeObservations"
            }
            .into(),
            counters: vec![],
            items: self
                .records
                .iter()
                .map(|record| JobResultItem {
                    item_id: record.id.as_str().into(),
                    outcome_code: match &record.terminal {
                        Some((state, effects)) => format!("{}:{state:?}:{effects:?}", record.phase),
                        None => format!("{}:Accepted:Unknown", record.phase),
                    },
                    reason_code: None,
                })
                .collect(),
            artifact_ids: vec![],
        };
        for (block, proof) in proofs {
            let confirmed = |phase| {
                self.records.iter().any(|record| {
                    record.block == *block
                        && record.phase == phase
                        && record.terminal
                            == Some((ExecutionState::Succeeded, EffectOutcome::Completed))
                })
            };
            let outcome = if confirmed("commit") && self.verified.contains(block) {
                "committed"
            } else if confirmed("rollback") {
                if self.verified.contains(block) {
                    "rolledBack"
                } else {
                    "notStarted"
                }
            } else {
                "unknown"
            };
            result.items.push(JobResultItem {
                item_id: format!("proof.{}", proof.fingerprint),
                outcome_code: outcome.into(),
                reason_code: None,
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proof() -> Proof {
        Proof {
            connection: "profile".into(),
            revision: 1,
            namespace: datazen_driver_api::namespace::NamespaceTarget::empty()
                .with_database("app")
                .with_schema("approved"),
            source: vec![],
            recipe: datazen_driver_api::workflow_transaction::WorkflowTransactionPlan {
                proof_profile: "pgLockedPlainHeapLiteralDmlV1:test".into(),
                statements: vec![],
                lock_sql: vec![],
                metadata_probe_sql: String::new(),
            },
            fingerprint: "aabbcc".into(),
        }
    }
    fn record(
        id: &str,
        phase: &'static str,
        terminal: Option<(ExecutionState, EffectOutcome)>,
    ) -> Record {
        Record {
            id: ExecutionId::new(id),
            block: "block".into(),
            phase,
            terminal,
        }
    }
    #[test]
    fn only_verified_begin_and_confirmed_commit_produce_a_boundary() {
        let stage = StageId::new("workflow");
        let proofs = BTreeMap::from([("block".into(), proof())]);
        let completed = Some((ExecutionState::Succeeded, EffectOutcome::Completed));
        let mut evidence = Evidence::default();
        evidence.records = vec![
            record("exec-begin", "begin", completed),
            record("exec-write", "statement", completed),
            record("exec-commit", "commit", None),
        ];
        evidence.verified("block");
        assert!(
            evidence.boundaries(&stage, &proofs).is_empty(),
            "lost commit receipt cannot be promoted to committed"
        );
        evidence.records[2].terminal = completed;
        assert_eq!(evidence.boundaries(&stage, &proofs).len(), 1);
        evidence.verified.clear();
        assert!(
            evidence.boundaries(&stage, &proofs).is_empty(),
            "SQL success alone is not proof"
        );
    }
    #[test]
    fn rollback_receipts_and_unknown_outcomes_are_durable_without_sql_or_live_identity() {
        let mut evidence = Evidence::default();
        evidence.records = vec![
            record(
                "exec-begin",
                "begin",
                Some((ExecutionState::Succeeded, EffectOutcome::Completed)),
            ),
            record(
                "exec-write",
                "statement",
                Some((ExecutionState::Failed, EffectOutcome::Unknown)),
            ),
            record("exec-rollback", "rollback", None),
        ];
        let stage = StageId::new("workflow");
        assert!(evidence
            .boundaries(&stage, &BTreeMap::from([("block".into(), proof())]))
            .is_empty());
        let pending = evidence.durable(&stage, &BTreeMap::new());
        assert_eq!(pending.items[2].outcome_code, "rollback:Accepted:Unknown");
        evidence.records[2].terminal = Some((ExecutionState::Succeeded, EffectOutcome::Completed));
        let durable = evidence.durable(&stage, &BTreeMap::new());
        assert_eq!(
            durable.items[2].outcome_code,
            "rollback:Succeeded:Completed"
        );
        assert!(
            datazen_runtime::job::runtime_repository::validate_job_domain_result(&durable).is_ok()
        );
        let encoded = serde_json::to_string(&durable).unwrap();
        for private in [
            "SELECT",
            "INSERT",
            "dbSessionId",
            "runtimeEpoch",
            "password",
            "block",
        ] {
            assert!(!encoded.contains(private));
        }
    }
}
