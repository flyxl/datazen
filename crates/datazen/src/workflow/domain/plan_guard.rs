//! transaction block 的计划预校验（设计 §7）：**先证明，后执行**。
//!
//! 校验失败是执行前拒绝：此刻还没申请资源、没发任何语句，所以不产生
//! `ExecutionErrorCode`，`effectOutcome = notStarted`，结果不进入自动重试。
//!
//! 判定规则（§7.4）：
//! 1. 受控 DML Command 声明 `transaction_safe` 且参数 schema 不接受原始 SQL → 保住边界。
//! 2. 任意 SQL 必须由**方言解析器**证明不含显式事务控制 / autocommit 改写 / DDL 与
//!    隐式提交 / 过程内提交路径。
//! 3. 无法证明 → `UnsupportedPlan`。
//! 4. 驱动缺方言解析能力 → `CapabilityUnsupported`，**不允许降级为「看起来安全」**。

use super::error::WorkflowError;

/// 唯一被支持的提交边界形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitBoundary {
    /// 整块在一条 runtime 托管的事务里，要么全成要么全不成。
    SingleTransaction,
}

/// 计划预校验结论。与设计 §7.3 的 `PlanVerdict` 一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanVerdict {
    /// 已证实边界成立，才允许申请资源并 `begin`。
    Proven {
        commit_boundary: CommitBoundary,
        /// 提交边界的作用域：目标 connectionId + 完整命名空间。
        transaction_scope: String,
    },
    Unsupported {
        /// 用于 UI 逐条高亮的 step id。
        offending_steps: Vec<String>,
        detail: String,
    },
    CapabilityUnsupported {
        detail: String,
    },
}

impl PlanVerdict {
    pub fn is_executable(&self) -> bool {
        matches!(self, PlanVerdict::Proven { .. })
    }

    pub fn rejection_error(&self) -> Option<WorkflowError> {
        match self {
            PlanVerdict::Proven { .. } => None,
            PlanVerdict::Unsupported {
                offending_steps,
                detail,
            } => Some(WorkflowError::UnsupportedPlan {
                offending_steps: offending_steps.clone(),
                detail: detail.clone(),
            }),
            PlanVerdict::CapabilityUnsupported { detail } => {
                Some(WorkflowError::CapabilityUnsupported {
                    detail: detail.clone(),
                })
            }
        }
    }
}

/// 块内单个步骤的计划形状（§7.2 校验输入的第 1 项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStep {
    /// 受控 DML Command。`transaction_safe` / `accepts_raw_sql` 来自 Command
    /// definition 的声明，不是运行期观察。
    ControlledCommand {
        step_id: String,
        transaction_safe: bool,
        accepts_raw_sql: bool,
    },
    /// 只读 SQL：只影响快照语义，**不影响**边界证明（§7.2），连解析都不需要。
    ReadOnlySql { step_id: String },
    /// 任意写 SQL：原文必须过方言解析。
    ArbitraryWriteSql { step_id: String, sql: String },
    /// 形状无法归类。不认识的形状按「无法证明」处理，不按「看起来安全」处理。
    Unclassified { step_id: String, detail: String },
}

impl PlanStep {
    pub fn step_id(&self) -> &str {
        match self {
            PlanStep::ControlledCommand { step_id, .. }
            | PlanStep::ReadOnlySql { step_id }
            | PlanStep::ArbitraryWriteSql { step_id, .. }
            | PlanStep::Unclassified { step_id, .. } => step_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryProof {
    PreservesBoundary,
    BreaksBoundary { reason: String },
}

/// 方言解析能力端口。**实现方必须是真正的方言解析器。**
///
/// 设计 §7.5 明确禁止用关键词正则实现它：[runtime::sql_guard::check_sql]
/// （现在做 read-only / safe-mode 语句过滤的那个）解决的是「能不能执行」，本判定
/// 解决的是「能不能保证整块原子性」。把 `check_sql` 接到这里会在
/// `'COMMIT'` 字面量、`commit_time` 列名、`insert into commits(...)` 这类形状上
/// 同时产生漏判与误判，并在 `CALL` / 存储过程 / DDL 隐式提交路径上给出假证明。
pub trait DialectPlanProof {
    fn availability(&self) -> ProofAvailability;
    fn prove(&self, sql: &str) -> BoundaryProof;
}

/// 汇总一条越界理由：越界 step 逐条收集，detail 保留第一条作为主因。
fn record_offender(
    offending: &mut Vec<String>,
    detail: &mut String,
    step_id: &str,
    reason: String,
) {
    offending.push(step_id.to_string());
    if detail.is_empty() {
        *detail = reason;
    } else {
        detail.push_str("; ");
        detail.push_str(&reason);
    }
}

/// 判定整块的提交边界。
///
/// `transaction_scope` 是目标的 connectionId + 完整命名空间，写进结论做审计。
///
/// 先报**确定的** `UnsupportedPlan`，再报 `CapabilityUnsupported`：已经能点名
/// 哪一步越界的计划，不需要让「缺能力」把更具体的结论盖掉。
pub fn check(
    block_id: &str,
    steps: &[PlanStep],
    transaction_scope: &str,
    proof: &dyn DialectPlanProof,
) -> PlanVerdict {
    let mut offending: Vec<String> = Vec::new();
    let mut detail = String::new();

    // 第一遍：不依赖方言解析器就能判定「无法证明」的步骤。
    for step in steps {
        match step {
            // 只读步骤只影响快照语义，不影响边界证明（§7.2），连解析都不需要。
            PlanStep::ReadOnlySql { .. } => {}
            PlanStep::ControlledCommand {
                step_id,
                transaction_safe,
                accepts_raw_sql,
            } => {
                if !transaction_safe || *accepts_raw_sql {
                    record_offender(
                        &mut offending,
                        &mut detail,
                        step_id,
                        format!(
                            "command '{step_id}' in block '{block_id}' is not provably transaction-safe \
                             (transactionSafe={transaction_safe}, acceptsRawSql={accepts_raw_sql})"
                        ),
                    );
                }
            }
            PlanStep::Unclassified {
                step_id,
                detail: reason,
            } => {
                record_offender(
                    &mut offending,
                    &mut detail,
                    step_id,
                    format!("step '{step_id}' in block '{block_id}' has no classifiable plan shape: {reason}"),
                );
            }
            PlanStep::ArbitraryWriteSql { .. } => {}
        }
    }
    if !offending.is_empty() {
        return PlanVerdict::Unsupported {
            offending_steps: offending,
            detail,
        };
    }

    // 第二遍：任意写 SQL 需要方言证明。缺能力时先记住原因，等确定结论出完再决定。
    let unavailable = match proof.availability() {
        ProofAvailability::Available => None,
        ProofAvailability::Unavailable { reason } => Some(reason),
    };
    if let Some(reason) = unavailable {
        if let Some(step) = steps
            .iter()
            .find(|step| matches!(step, PlanStep::ArbitraryWriteSql { .. }))
        {
            // §7.4 规则 4：不允许降级为「看起来安全」，所以这里是拒绝而不是放行。
            return PlanVerdict::CapabilityUnsupported {
                detail: format!(
                    "cannot prove the commit boundary of step '{}' in block '{block_id}': {reason}; an arbitrary SQL the dialect parser never inspected is not treated as safe",
                    step.step_id()
                ),
            };
        }
    }

    for step in steps {
        if let PlanStep::ArbitraryWriteSql { step_id, sql } = step {
            if let BoundaryProof::BreaksBoundary { reason } = proof.prove(sql) {
                record_offender(
                    &mut offending,
                    &mut detail,
                    step_id,
                    format!("step '{step_id}' in block '{block_id}' breaks the commit boundary: {reason}"),
                );
            }
        }
    }
    if !offending.is_empty() {
        return PlanVerdict::Unsupported {
            offending_steps: offending,
            detail,
        };
    }

    PlanVerdict::Proven {
        commit_boundary: CommitBoundary::SingleTransaction,
        transaction_scope: transaction_scope.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只在测试里出现的替身：它把「哪些 SQL 破坏边界」写死，用来验证判定流程，
    /// 不代表任何真实方言解析器。
    struct StubProof {
        availability: ProofAvailability,
        breaks: &'static [&'static str],
    }

    impl DialectPlanProof for StubProof {
        fn availability(&self) -> ProofAvailability {
            self.availability.clone()
        }

        fn prove(&self, sql: &str) -> BoundaryProof {
            if self.breaks.iter().any(|needle| sql.contains(needle)) {
                BoundaryProof::BreaksBoundary {
                    reason: "explicit transaction control or implicit commit".into(),
                }
            } else {
                BoundaryProof::PreservesBoundary
            }
        }
    }

    fn parser_available() -> StubProof {
        StubProof {
            availability: ProofAvailability::Available,
            breaks: &["COMMIT", "CREATE TABLE"],
        }
    }

    fn parser_missing() -> StubProof {
        StubProof {
            availability: ProofAvailability::Unavailable {
                reason: "dialect parser is not registered for this driver".into(),
            },
            breaks: &[],
        }
    }

    #[test]
    fn transaction_safe_controlled_command_proves_the_boundary() {
        let verdict = check(
            "tx1",
            &[PlanStep::ControlledCommand {
                step_id: "c1".into(),
                transaction_safe: true,
                accepts_raw_sql: false,
            }],
            "conn-1/sales",
            &parser_available(),
        );
        assert!(verdict.is_executable());
        assert!(verdict.rejection_error().is_none());
        assert_eq!(
            verdict,
            PlanVerdict::Proven {
                commit_boundary: CommitBoundary::SingleTransaction,
                transaction_scope: "conn-1/sales".into(),
            }
        );
    }

    #[test]
    fn command_that_accepts_raw_sql_is_rejected_before_any_resource_is_taken() {
        let verdict = check(
            "tx1",
            &[PlanStep::ControlledCommand {
                step_id: "c1".into(),
                transaction_safe: true,
                accepts_raw_sql: true,
            }],
            "conn-1/sales",
            &parser_available(),
        );
        assert!(!verdict.is_executable());
        match verdict {
            PlanVerdict::Unsupported {
                ref offending_steps,
                ..
            } => assert_eq!(offending_steps, &["c1".to_string()]),
            other => panic!("expected UnsupportedPlan, got {other:?}"),
        }
        assert!(matches!(
            verdict.rejection_error(),
            Some(WorkflowError::UnsupportedPlan { .. })
        ));
    }

    #[test]
    fn ddl_is_rejected_with_the_offending_step_named() {
        let verdict = check(
            "tx1",
            &[
                PlanStep::ControlledCommand {
                    step_id: "c1".into(),
                    transaction_safe: true,
                    accepts_raw_sql: false,
                },
                PlanStep::ArbitraryWriteSql {
                    step_id: "q2".into(),
                    sql: "CREATE TABLE t (id int)".into(),
                },
            ],
            "conn-1/sales",
            &parser_available(),
        );
        assert!(!verdict.is_executable());
        match verdict {
            PlanVerdict::Unsupported {
                ref offending_steps,
                ref detail,
            } => {
                assert_eq!(offending_steps, &["q2".to_string()]);
                assert!(detail.contains("q2"), "detail must name the step: {detail}");
            }
            other => panic!("expected UnsupportedPlan, got {other:?}"),
        }
    }

    #[test]
    fn missing_dialect_parser_capability_is_never_downgraded_to_looking_safe() {
        let verdict = check(
            "tx1",
            &[PlanStep::ArbitraryWriteSql {
                step_id: "q1".into(),
                sql: "update t set a = 1".into(),
            }],
            "conn-1/sales",
            &parser_missing(),
        );
        assert!(!verdict.is_executable());
        match verdict {
            PlanVerdict::CapabilityUnsupported { ref detail } => {
                assert!(detail.contains("q1"), "detail must name the step: {detail}");
                assert!(detail.contains("not treated as safe"), "{detail}");
            }
            other => panic!("expected CapabilityUnsupported, got {other:?}"),
        }
        assert!(matches!(
            verdict.rejection_error(),
            Some(WorkflowError::CapabilityUnsupported { .. })
        ));
    }

    #[test]
    fn read_only_steps_do_not_need_the_parser_at_all() {
        let verdict = check(
            "tx1",
            &[
                PlanStep::ReadOnlySql {
                    step_id: "r1".into(),
                },
                PlanStep::ReadOnlySql {
                    step_id: "r2".into(),
                },
            ],
            "conn-1/sales",
            &parser_missing(),
        );
        assert!(verdict.is_executable());
    }

    #[test]
    fn definite_offender_is_reported_before_the_missing_capability() {
        let verdict = check(
            "tx1",
            &[
                PlanStep::ArbitraryWriteSql {
                    step_id: "q1".into(),
                    sql: "update t set a = 1".into(),
                },
                PlanStep::Unclassified {
                    step_id: "m2".into(),
                    detail: "dynamic sql assembled at run time".into(),
                },
            ],
            "conn-1/sales",
            &parser_missing(),
        );
        match verdict {
            PlanVerdict::Unsupported {
                ref offending_steps,
                ..
            } => assert_eq!(offending_steps, &["m2".to_string()]),
            other => panic!("expected UnsupportedPlan, got {other:?}"),
        }
    }
}
