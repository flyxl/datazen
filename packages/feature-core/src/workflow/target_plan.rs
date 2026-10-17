//! 执行目标解析（设计 §4）。
//!
//! 四级链：`step > block > workflow 默认 > profile 初始目标`。**调用方参数不在链里**——
//! 调用方当前打开的连接既不是声明，也不能在链走空时顶上来（§4.2 步骤 4）。
//!
//! 回退规则只有一个：最高优先级的**完全未声明**的那一级才可继续向下；一旦某一级
//! 写了东西，就在这一级判定，缺件直接拒绝，不拼接另一级的连接，也不降级到低优先级。
//!
//! 块内目标绑定（§4.3）在这里静态判定：块已经拿到资源，块内 step 的显式目标
//! 只能在块目标内收敛；不一致就整块拒绝，不等运行期再发现。

use super::error::WorkflowError;
use super::executable::{ExecutableStep, TargetDeclaration};
use super::model::BlockKind;

/// 最终目标来自哪一级。用于错误信息与审计，**不参与**目标身份比较。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSource {
    /// L1：step 自己的 `connection` / `database`（command 走 `input.database`）。
    Step,
    /// L2：块声明的目标。
    Block,
    /// L3：workflow 默认目标。
    Workflow,
    /// L4：本次定义绑定 profile 的 `initialTarget`。
    Profile,
}

impl TargetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetSource::Step => "step",
            TargetSource::Block => "block",
            TargetSource::Workflow => "workflow",
            TargetSource::Profile => "profile",
        }
    }
}

/// 解析出的执行目标。只有 `connectionId` + 命名空间，**永远不含 `dbSessionId`**（§4.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTarget {
    pub connection_id: String,
    pub database: Option<String>,
    pub source: TargetSource,
}

/// 本次操作的目标要求，来自 Command definition 的 `targetRequirements`
/// （连接管理 §4.3：不使用全驱动统一 required 列表）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TargetRequirements {
    /// 驱动 namespaceShape / 操作要求里 database 层级是 required。
    pub requires_database: bool,
}

impl TargetRequirements {
    pub const DATABASE: Self = Self {
        requires_database: true,
    };
    pub const NONE: Self = Self {
        requires_database: false,
    };
}

/// 同一 connectionId 内切换命名空间的能力，由驱动 namespaceShape 决定
/// （连接管理 §8：MySQL 可原地 `USE`；PostgreSQL 切 database 必须新连接）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceSwitch {
    /// 同一会话内可切。
    SameSession,
    /// 切库必须新开连接，因此 session/transaction block 不可能覆盖该目标。
    RequiresNewConnection,
}

impl Default for NamespaceSwitch {
    /// 静态层拿不到驱动元数据时按悲观值判定：拒绝优于开一条新连接后失败。
    fn default() -> Self {
        NamespaceSwitch::RequiresNewConnection
    }
}

/// 按 L1 → L2 → L3 → L4 取第一级已声明的目标（§4.2 步骤 1–4）。
///
/// `block_target` 为 `None` 表示该 step 不在块里；`profile_target` 为 `None`
/// 表示这次定义没有绑定 profile（§4.4 的调用方当前连接在任何一级都不出现）。
pub fn resolve(
    step_id: &str,
    step_target: &TargetDeclaration,
    block_target: Option<&TargetDeclaration>,
    workflow_target: &TargetDeclaration,
    profile_target: Option<&TargetDeclaration>,
    requirements: TargetRequirements,
) -> Result<ExecutionTarget, WorkflowError> {
    // 先归一化再判定「是否声明」：模板展开后残留的空白串必须回到未声明，
    // 否则一个空连接名会占住最高优先级并把整条链挡在下面。
    let step_target = step_target.normalized();
    let block_target = block_target.map(TargetDeclaration::normalized);
    let workflow_target = workflow_target.normalized();
    let profile_target = profile_target.map(TargetDeclaration::normalized);
    let levels = [
        (TargetSource::Step, Some(&step_target)),
        (TargetSource::Block, block_target.as_ref()),
        (TargetSource::Workflow, Some(&workflow_target)),
        (TargetSource::Profile, profile_target.as_ref()),
    ];
    let selected = levels.iter().find_map(|(source, declared)| {
        declared
            .filter(|declaration| declaration.is_declared())
            .map(|declaration| (*source, declaration))
    });
    let Some((source, declared)) = selected else {
        return Err(WorkflowError::TargetRequired {
            step_id: step_id.to_string(),
            detail: format!(
                "no target declared at step / block / workflow / profile level; the caller's current connection is never used as a fallback"
            ),
        });
    };

    // 声明了命名空间却没声明连接 = 本级声明不完整。这里拒绝，而不是借用
    // workflow 级的连接去拼一个用户没写过的组合（§4.2 步骤 3）。
    let Some(connection_id) = declared.connection.clone() else {
        return Err(WorkflowError::TargetRequired {
            step_id: step_id.to_string(),
            detail: format!(
                "{} level declares namespace '{}' but no connectionId; a namespace is never joined onto another level's connection",
                source.as_str(),
                declared.database.as_deref().unwrap_or_default()
            ),
        });
    };

    if requirements.requires_database && declared.database.is_none() {
        return Err(WorkflowError::TargetRequired {
            step_id: step_id.to_string(),
            detail: format!(
                "operation requires a database but the {} level target of '{step_id}' declares only connectionId '{connection_id}'",
                source.as_str()
            ),
        });
    }

    Ok(ExecutionTarget {
        connection_id,
        database: declared.database.clone(),
        source,
    })
}

/// 解析块自身持有的资源，并静态拒绝块内与它不一致的 step 目标（§4.2 步骤 6、§4.3）。
///
/// 块内所有 step 跑在**同一条物理资源**上，所以这里的判据是「目标是否收敛于块
/// 目标」，而不是「最后解析出来像不像」。
pub fn resolve_block_targets(
    block_id: &str,
    kind: BlockKind,
    block_target: &TargetDeclaration,
    workflow_target: &TargetDeclaration,
    steps: &[ExecutableStep],
    requirements: TargetRequirements,
    namespace_switch: NamespaceSwitch,
) -> Result<ExecutionTarget, WorkflowError> {
    let block_target = block_target.normalized();
    let workflow_target = workflow_target.normalized();
    // 块有没有把自己的目标写出来：决定最终目标算 L2 还是继承来的 L3。
    let block_declares_target = block_target.is_declared();
    let block = resolve(
        block_id,
        &block_target,
        None,
        &workflow_target,
        None,
        requirements,
    )
    .map_err(|error| match error {
        // 块级缺目标时说清是块，不是某个 step。
        WorkflowError::TargetRequired { detail, .. } => WorkflowError::TargetRequired {
            step_id: format!("block:{block_id}"),
            detail,
        },
        other => other,
    })?;
    // 块自己声明了目标时来源是 L2。上面是复用 resolve 的 step 槽位算出来的，
    // 不改回去审计就会把块目标记成 step 目标；块没声明则保留继承来的来源。
    let mut block = block;
    if block_declares_target {
        block.source = TargetSource::Block;
    }

    for step in steps.iter().filter(|step| step.resolves_target()) {
        let declared = step.target.normalized();
        if !declared.is_declared() {
            continue;
        }
        // A declared step level must be complete even when it repeats only part
        // of the block identity. Never fill missing fields from the block.
        resolve(&step.id, &declared, None, &TargetDeclaration::empty(), None, requirements)?;
        match (&declared.connection, &declared.database) {
            // 跨连接：换资源就是换块，静态拒绝整块。
            (Some(connection), _) if *connection != block.connection_id => {
                return Err(WorkflowError::TargetConflict {
                    step_id: step.id.clone(),
                    detail: format!(
                        "block '{block_id}' is already bound to connection '{}' but step '{}' targets '{connection}'",
                        block.connection_id, step.id
                    ),
                });
            }
            // 同连接、命名空间不同：跨库不是 session block 的能力；若该驱动切库
            // 还要求新连接，则报更具体的 Unsupported（连接管理 §8）。
            (_, Some(database)) if Some(database.as_str()) != block.database.as_deref() => {
                return Err(match namespace_switch {
                    NamespaceSwitch::RequiresNewConnection => WorkflowError::TargetUnsupported {
                        step_id: step.id.clone(),
                        detail: format!(
                            "step '{}' switches to database '{database}' inside block '{block_id}', but this driver needs a new connection for that namespace",
                            step.id
                        ),
                    },
                    NamespaceSwitch::SameSession => WorkflowError::TargetConflict {
                        step_id: step.id.clone(),
                        detail: format!(
                            "step '{}' targets database '{database}' but block '{block_id}' is bound to '{}' — cross-database is not a {} capability",
                            step.id,
                            block.database.as_deref().unwrap_or("(none)"),
                            kind.as_str(),
                        ),
                    },
                });
            }
            _ => {}
        }
    }

    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::{BlockKind, WorkflowStep};

    fn declared(connection: Option<&str>, database: Option<&str>) -> TargetDeclaration {
        TargetDeclaration::from_parts(connection, database)
    }

    fn query(id: &str) -> WorkflowStep {
        WorkflowStep::Query {
            id: id.to_string(),
            sql: "select 1".into(),
            connection: None,
            database: None,
            timeout_secs: None,
            on_error: None,
        }
    }

    fn step(id: &str, connection: Option<&str>, database: Option<&str>) -> ExecutableStep {
        ExecutableStep {
            id: id.to_string(),
            target: declared(connection, database),
            step: query(id),
        }
    }

    fn pick<'a>(
        target: &ExecutionTarget,
        source: TargetSource,
        connection_id: &str,
        database: Option<&str>,
    ) -> bool {
        target.source == source
            && target.connection_id == connection_id
            && target.database.as_deref() == database
    }

    #[test]
    fn the_step_level_wins_over_every_other_declared_level() {
        let resolved = resolve(
            "q1",
            &declared(Some("step-conn"), Some("step-db")),
            None,
            &declared(Some("wf-conn"), Some("wf-db")),
            Some(&declared(Some("profile-conn"), Some("profile-db"))),
            TargetRequirements::DATABASE,
        )
        .expect("the highest declared level wins");
        assert!(pick(
            &resolved,
            TargetSource::Step,
            "step-conn",
            Some("step-db")
        ));
    }

    #[test]
    fn resolution_falls_through_only_when_the_level_declares_nothing() {
        let workflow = declared(Some("wf-conn"), None);
        let profile = declared(Some("profile-conn"), None);

        let by_block = resolve(
            "q1",
            &declared(None, None),
            Some(&declared(Some("block-conn"), None)),
            &workflow,
            Some(&profile),
            TargetRequirements::NONE,
        )
        .expect("block target applies");
        assert!(pick(&by_block, TargetSource::Block, "block-conn", None));

        let by_workflow = resolve(
            "q1",
            &declared(None, None),
            None,
            &workflow,
            Some(&profile),
            TargetRequirements::NONE,
        )
        .expect("workflow default applies");
        assert!(pick(&by_workflow, TargetSource::Workflow, "wf-conn", None));
    }

    #[test]
    fn the_profile_initial_target_is_the_last_declared_level() {
        let profile = declared(Some("profile-conn"), Some("sales"));
        let resolved = resolve(
            "q1",
            &declared(None, None),
            None,
            &declared(None, None),
            Some(&profile),
            TargetRequirements::DATABASE,
        )
        .expect("profile initial target applies");
        assert!(pick(
            &resolved,
            TargetSource::Profile,
            "profile-conn",
            Some("sales")
        ));
    }

    #[test]
    fn an_undeclared_chain_is_refused_instead_of_borrowing_the_callers_current_connection() {
        let error = resolve(
            "q1",
            &declared(None, None),
            None,
            &declared(None, None),
            None,
            TargetRequirements::NONE,
        )
        .expect_err("the caller's current connection is never a fallback level");
        assert!(
            matches!(&error, WorkflowError::TargetRequired { detail, .. } if detail.contains("caller")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn a_namespace_without_a_connection_is_never_joined_onto_another_level() {
        let error = resolve(
            "q1",
            &declared(None, Some("sales")),
            None,
            &declared(Some("wf-conn"), Some("wf-db")),
            None,
            TargetRequirements::DATABASE,
        )
        .expect_err("the step level is declared, so it is chosen, but it has no connectionId");
        match &error {
            WorkflowError::TargetRequired { step_id, detail } => {
                assert_eq!(step_id, "q1");
                assert!(
                    detail.contains("never joined onto another level"),
                    "detail must say why: {detail}"
                );
            }
            other => panic!("expected TargetRequired, got {other:?}"),
        }
    }

    #[test]
    fn a_declared_connection_never_swallows_the_namespace_of_a_lower_priority_level() {
        let error = resolve(
            "q1",
            &declared(Some("step-conn"), None),
            None,
            &declared(Some("wf-conn"), Some("wf-db")),
            None,
            TargetRequirements::DATABASE,
        )
        .expect_err("the required database must not be taken from the workflow default");
        assert!(
            matches!(&error, WorkflowError::TargetRequired { step_id, .. } if step_id == "q1"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn a_multi_database_operation_without_a_database_is_refused_by_the_driver_requirement() {
        let error = resolve(
            "q1",
            &declared(Some("conn-1"), None),
            None,
            &declared(None, None),
            None,
            TargetRequirements::DATABASE,
        )
        .expect_err("the operation needs a namespace the declaration does not carry");
        assert!(
            matches!(&error, WorkflowError::TargetRequired { detail, .. } if detail.contains("requires a database")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn blank_declarations_read_as_undeclared_and_let_the_next_level_win() {
        let resolved = resolve(
            "q1",
            &declared(Some("   "), Some("")),
            Some(&declared(Some(""), None)),
            &declared(Some("wf-conn"), None),
            None,
            TargetRequirements::NONE,
        )
        .expect("blank means undeclared, so resolution continues downwards");
        assert!(pick(&resolved, TargetSource::Workflow, "wf-conn", None));
    }

    #[test]
    fn a_step_targeting_another_connection_inside_a_block_is_a_conflict() {
        let error = resolve_block_targets(
            "b1",
            BlockKind::SessionBlock,
            &declared(Some("block-conn"), None),
            &declared(None, None),
            &[step("q1", Some("other-conn"), None)],
            TargetRequirements::NONE,
            NamespaceSwitch::SameSession,
        )
        .expect_err("one block never holds two physical resources");
        assert!(
            matches!(&error, WorkflowError::TargetConflict { step_id, .. } if step_id == "q1"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn cross_database_inside_a_block_is_refused_even_when_the_driver_switches_in_place() {
        let error = resolve_block_targets(
            "b1",
            BlockKind::SessionBlock,
            &declared(Some("block-conn"), Some("app")),
            &declared(None, None),
            &[step("q1", Some("block-conn"), Some("other"))],
            TargetRequirements::NONE,
            NamespaceSwitch::SameSession,
        )
        .expect_err("cross-database is not a session block capability");
        match &error {
            WorkflowError::TargetConflict { detail, .. } => {
                assert!(detail.contains("cross-database"), "{detail}");
            }
            other => panic!("expected TargetConflict, got {other:?}"),
        }
    }

    #[test]
    fn a_namespace_needing_a_new_connection_is_unsupported_rather_than_a_plain_conflict() {
        let error = resolve_block_targets(
            "b1",
            BlockKind::TransactionBlock,
            &declared(Some("block-conn"), Some("app")),
            &declared(None, None),
            &[step("q1", Some("block-conn"), Some("other"))],
            TargetRequirements::NONE,
            NamespaceSwitch::RequiresNewConnection,
        )
        .expect_err("the driver contract says this namespace needs a new connection");
        assert!(
            matches!(&error, WorkflowError::TargetUnsupported { step_id, .. } if step_id == "q1"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn a_step_repeating_the_block_target_is_accepted() {
        let resolved = resolve_block_targets(
            "b1",
            BlockKind::TransactionBlock,
            &declared(Some("block-conn"), Some("app")),
            &declared(None, None),
            &[
                step("q1", None, None),
                step("q2", Some("block-conn"), Some("app")),
            ],
            TargetRequirements::NONE,
            NamespaceSwitch::RequiresNewConnection,
        )
        .expect("an undeclared step inherits, a repeated declaration is consistent");
        assert!(pick(
            &resolved,
            TargetSource::Block,
            "block-conn",
            Some("app")
        ));
    }

    #[test]
    fn a_block_without_its_own_target_still_binds_from_the_workflow_default() {
        let resolved = resolve_block_targets(
            "b1",
            BlockKind::SessionBlock,
            &declared(None, None),
            &declared(Some("wf-conn"), Some("wf-db")),
            &[step("q1", None, None)],
            TargetRequirements::DATABASE,
            NamespaceSwitch::SameSession,
        )
        .expect("the block binds the workflow default resource");
        assert!(pick(
            &resolved,
            TargetSource::Workflow,
            "wf-conn",
            Some("wf-db")
        ));
    }
}
