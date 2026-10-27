//! 定义版本分派：把 YAML 产物映射成与版本无关的可执行形态。
//!
//! 为什么单独一层：执行层只认 [ExecutableWorkflow]。分派发生在解析层，于是
//! 「新 block 格式落到旧 executor」在类型上就不可能，而不只是靠调用方自觉。
//!
//! 版本策略（设计 §10.2 / §10.3）：
//! * 未声明 `version` → v1。存量 YAML 全是这个形状，不报错。
//! * 裸整数且受支持 → 对应版本。
//! * 其余（空串、`"1.0"`、`"v1"`、`0`、负数、高于上限）→ **拒绝执行**。
//!   解析层拒绝属于请求拒绝，对外映射为 `ApiError.code = InvalidArgument`。
//!
//! 本模块是纯函数：不碰资源、连接与 Job。

use std::collections::BTreeSet;

use super::error::WorkflowError;
use super::model::{BlockKind, BlockRetry, WorkflowBlock, WorkflowDefinition, WorkflowStep};

/// runtime 支持的定义版本上限。合法但更高的版本一律拒绝并提示升级。
pub const HIGHEST_SUPPORTED_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowVersion {
    /// 扁平 `steps`，每步独立获取与释放资源。
    V1,
    /// 启用 `blocks`、目标四级链与 Job 包裹。
    V2,
}

impl WorkflowVersion {
    /// 版本缺失按 v1；版本存在则必须严格匹配 ^\d+$。
    ///
    /// 空白串不是「缺失」而是写错了。猜一个低版本比拒绝危险得多：v1 与 v2
    /// 的执行形状完全不同（§10.2 / §10.3）。
    pub fn parse(declared: Option<&str>) -> Result<Self, WorkflowError> {
        let Some(raw) = declared else {
            return Ok(WorkflowVersion::V1);
        };
        let rejected = || WorkflowError::UnsupportedDefinitionVersion {
            declared: raw.to_string(),
            highest_supported: HIGHEST_SUPPORTED_VERSION,
        };
        if raw.is_empty() {
            return Err(rejected());
        }
        // 严格 ^\d+$：u32::from_str 还接受 "+2"，那不是我们声明的格式。
        if !raw.bytes().all(|b| b.is_ascii_digit()) {
            return Err(rejected());
        }
        match raw.parse::<u32>() {
            Ok(1) => Ok(WorkflowVersion::V1),
            Ok(2) => Ok(WorkflowVersion::V2),
            _ => Err(rejected()),
        }
    }
}

/// 一级目标声明（`connectionId` + 命名空间）。
///
/// 只表达**用户写下来的意图**，不表示已解析的最终目标；解析发生在
/// [super::target_plan]。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetDeclaration {
    pub connection: Option<String>,
    pub database: Option<String>,
}

impl TargetDeclaration {
    pub const fn empty() -> Self {
        Self {
            connection: None,
            database: None,
        }
    }

    pub fn from_parts(connection: Option<&str>, database: Option<&str>) -> Self {
        Self {
            connection: connection.map(str::to_string),
            database: database.map(str::to_string),
        }
    }

    /// step 的 L1 声明。`command` 没有顶层 `database`，命名空间走
    /// `input.database`（设计 §4.1 的 L1 来源）。
    pub fn from_step(step: &WorkflowStep) -> Self {
        match step {
            WorkflowStep::Query {
                connection,
                database,
                ..
            } => Self::from_parts(connection.as_deref(), database.as_deref()),
            WorkflowStep::Command {
                connection, input, ..
            } => Self::from_parts(
                connection.as_deref(),
                input.get("database").and_then(serde_json::Value::as_str),
            ),
            _ => Self::empty(),
        }
    }

    /// 归一化：空白与空串按「未声明」处理。模板在 step 开始前展开成字面量，
    /// 变量缺失留下的空串必须回到未声明，而不是被当成一个名字为空的连接。
    pub fn normalized(&self) -> Self {
        Self {
            connection: normalized_value(self.connection.as_deref()),
            database: normalized_value(self.database.as_deref()),
        }
    }

    /// 这一级是否写了东西。只有**整级未声明**才允许向下一级回退：写了
    /// `connection` 却没写命名空间属于声明不完整，直接拒绝，不向下凑。
    pub fn is_declared(&self) -> bool {
        self.connection.is_some() || self.database.is_some()
    }
}

fn normalized_value(value: Option<&str>) -> Option<String> {
    let trimmed = value.map(str::trim).filter(|v| !v.is_empty())?;
    Some(trimmed.to_string())
}

/// 已完成版本分派、目标声明已归一化的一个 step。
#[derive(Debug, Clone)]
pub struct ExecutableStep {
    pub id: String,
    /// L1 声明；为空表示本级未声明，可向 block / workflow / profile 回退。
    pub target: TargetDeclaration,
    pub step: WorkflowStep,
}

impl ExecutableStep {
    fn from_step_owned(step: WorkflowStep) -> Self {
        let target = TargetDeclaration::from_step(&step).normalized();
        Self {
            id: step.step_id().to_string(),
            target,
            step,
        }
    }

    /// 只有 `query` / `command` 走四级链目标解析。其余 step 不吃 step 级目标声明。
    pub fn resolves_target(&self) -> bool {
        matches!(
            &self.step,
            WorkflowStep::Query { .. } | WorkflowStep::Command { .. }
        )
    }
}

/// 一个已通过静态校验的资源块。
#[derive(Debug, Clone)]
pub struct ExecutableBlock {
    /// run 内唯一，出现在 `OwnerRef.blockId` 与检查点里。
    pub id: String,
    pub kind: BlockKind,
    /// L2 声明；为空表示继承 workflow 默认目标（L3）。
    pub target: TargetDeclaration,
    /// 该块可否与**其他**块并发。同一 blockId 在一次 run 内始终只有一个实例。
    pub parallel: bool,
    pub steps: Vec<ExecutableStep>,
    /// 仅 `transactionBlock` 可非零；整块重试，且只在已证实回滚之后才允许。
    pub retry: BlockRetry,
}

impl ExecutableBlock {
    fn from_block(block: WorkflowBlock) -> Result<Self, WorkflowError> {
        let id = block.id.trim().to_string();
        if id.is_empty() {
            return Err(WorkflowError::Validation(
                "workflow block id must not be empty".into(),
            ));
        }
        if block.parallel && !matches!(block.kind, BlockKind::SessionBlock) {
            return Err(WorkflowError::Validation(format!(
                "block '{id}': 'parallel' applies to sessionBlock, not {}",
                block.kind.as_str()
            )));
        }
        if block.retry.is_some() && !block.kind.is_transactional() {
            return Err(WorkflowError::Validation(format!(
                "block '{id}': 'retry' applies to transactionBlock only — a {} never resumes from the middle, so retry is either whole-block or nothing",
                block.kind.as_str()
            )));
        }
        if block.steps.is_empty() {
            return Err(WorkflowError::Validation(format!(
                "block '{id}' declares no steps"
            )));
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for step in &block.steps {
            if !seen.insert(step.step_id()) {
                return Err(WorkflowError::Validation(format!(
                    "block '{id}' has duplicate step id '{}'",
                    step.step_id()
                )));
            }
        }
        Ok(Self {
            id,
            kind: block.kind,
            target: TargetDeclaration::from_parts(
                block.connection.as_deref(),
                block.database.as_deref(),
            )
            .normalized(),
            parallel: block.parallel,
            steps: block
                .steps
                .into_iter()
                .map(ExecutableStep::from_step_owned)
                .collect(),
            retry: block.retry.unwrap_or_default(),
        })
    }
}

/// 执行层唯一接受的形态：与版本无关，没有 `blocks: Option<..>` 这类分支。
#[derive(Debug, Clone)]
pub struct ExecutableWorkflow {
    pub definition_id: String,
    pub name: String,
    pub version: WorkflowVersion,
    /// L3 workflow 默认目标。
    pub default_target: TargetDeclaration,
    /// 独立 step 序列（v1 的全部 step，以及 v2 顶层 `steps`）。
    pub independent_steps: Vec<ExecutableStep>,
    pub blocks: Vec<ExecutableBlock>,
}

impl WorkflowDefinition {
    /// 解析层入口：版本分派 + 静态校验。消费 `self`，让执行层无法绕过分派
    /// 直接读到原始定义。
    pub fn into_executable(self) -> Result<ExecutableWorkflow, WorkflowError> {
        let version = WorkflowVersion::parse(self.version.as_deref())?;
        let default_target =
            TargetDeclaration::from_parts(self.connection.as_deref(), self.database.as_deref())
                .normalized();
        let independent_steps = self
            .steps
            .into_iter()
            .map(ExecutableStep::from_step_owned)
            .collect::<Vec<_>>();

        let blocks = match version {
            // v1 是「全部独立 step」的别名，而不是「一个装满所有 step 的块」。
            // 带 block 的定义按 v1 解析等于把新格式喂给旧 executor，直接拒绝。
            WorkflowVersion::V1 => {
                if !self.blocks.is_empty() {
                    return Err(WorkflowError::Validation(format!(
                        "definition '{}' declares {} resource block(s) but has no v2 version; blocks require version '2'",
                        self.id,
                        self.blocks.len()
                    )));
                }
                Vec::new()
            }
            WorkflowVersion::V2 => {
                let mut seen: BTreeSet<String> = BTreeSet::new();
                let mut mapped = Vec::with_capacity(self.blocks.len());
                for block in self.blocks {
                    if !seen.insert(block.id.trim().to_string()) {
                        return Err(WorkflowError::Validation(format!(
                            "definition '{}' has duplicate block id '{}'",
                            self.id, block.id
                        )));
                    }
                    mapped.push(ExecutableBlock::from_block(block)?);
                }
                mapped
            }
        };

        if independent_steps.is_empty() && blocks.is_empty() {
            return Err(WorkflowError::Validation(format!(
                "definition '{}' declares neither steps nor blocks",
                self.id
            )));
        }

        Ok(ExecutableWorkflow {
            definition_id: self.id,
            name: self.name,
            version,
            default_target,
            independent_steps,
            blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::domain::model::WorkflowStep;

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

    fn definition(
        version: Option<&str>,
        steps: Vec<WorkflowStep>,
        blocks: Vec<WorkflowBlock>,
    ) -> WorkflowDefinition {
        WorkflowDefinition {
            id: "wf".into(),
            name: "wf".into(),
            description: String::new(),
            version: version.map(str::to_string),
            author: None,
            variables: Vec::new(),
            connection: None,
            database: None,
            steps,
            blocks,
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        }
    }

    fn session_block(id: &str) -> WorkflowBlock {
        WorkflowBlock {
            id: id.to_string(),
            kind: BlockKind::SessionBlock,
            connection: None,
            database: None,
            parallel: false,
            steps: vec![query("s1")],
            retry: None,
        }
    }

    #[test]
    fn undeclared_version_reads_as_v1_and_keeps_every_step_independent() {
        let executable = definition(None, vec![query("a"), query("b")], Vec::new())
            .into_executable()
            .expect("a version-less definition is valid v1");
        assert_eq!(executable.version, WorkflowVersion::V1);
        assert_eq!(executable.independent_steps.len(), 2);
        assert!(executable.blocks.is_empty());
    }

    #[test]
    fn resource_blocks_never_reach_the_v1_execution_shape() {
        let error = definition(None, vec![query("a")], vec![session_block("b1")])
            .into_executable()
            .expect_err("blocks without a v2 version must be refused");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("version '2'")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn non_integer_version_is_rejected_instead_of_guessed() {
        for declared in ["", " ", "1.0", "v1", "V2", "+2", "-1", "0", "２", "1 "] {
            assert!(
                WorkflowVersion::parse(Some(declared)).is_err(),
                "version {declared:?} must not be resolved by guessing"
            );
        }
    }

    #[test]
    fn version_above_the_supported_highest_is_refused_with_an_upgrade_hint() {
        match WorkflowVersion::parse(Some("3")) {
            Err(WorkflowError::UnsupportedDefinitionVersion {
                declared,
                highest_supported,
            }) => {
                assert_eq!(declared, "3");
                assert_eq!(highest_supported, HIGHEST_SUPPORTED_VERSION);
            }
            other => panic!("expected UnsupportedDefinitionVersion, got {other:?}"),
        }
    }

    #[test]
    fn a_v2_definition_maps_blocks_to_blocks_and_leaves_top_level_steps_independent() {
        let executable = definition(Some("2"), vec![query("a")], vec![session_block("b1")])
            .into_executable()
            .expect("version 2 with blocks is valid");
        assert_eq!(executable.version, WorkflowVersion::V2);
        assert_eq!(executable.independent_steps.len(), 1);
        assert_eq!(executable.blocks.len(), 1);
        assert_eq!(executable.blocks[0].steps[0].id, "s1");
        assert!(!executable.blocks[0].parallel);
    }

    #[test]
    fn retry_is_refused_on_a_session_block_because_it_never_resumes_from_the_middle() {
        let mut block = session_block("b1");
        block.retry = Some(BlockRetry { block: 1 });
        let error = definition(Some("2"), Vec::new(), vec![block])
            .into_executable()
            .expect_err("retry applies to transactionBlock only");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("transactionBlock")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn parallel_is_refused_on_a_transaction_block() {
        let mut block = session_block("b1");
        block.kind = BlockKind::TransactionBlock;
        block.parallel = true;
        let error = definition(Some("2"), Vec::new(), vec![block])
            .into_executable()
            .expect_err("parallel applies to sessionBlock only");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("sessionBlock")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn a_block_without_steps_is_rejected_like_the_editor_does() {
        let mut block = session_block("b1");
        block.steps.clear();
        let error = definition(Some("2"), vec![query("a")], vec![block])
            .into_executable()
            .expect_err("an empty block has no execution unit");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("no steps")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn duplicate_step_ids_inside_one_block_are_rejected() {
        let mut block = session_block("b1");
        block.steps.push(query("s1"));
        let error = definition(Some("2"), Vec::new(), vec![block])
            .into_executable()
            .expect_err("step ids address block outputs and must be unique");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("duplicate step id")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn a_definition_with_neither_steps_nor_blocks_is_rejected() {
        let error = definition(Some("2"), Vec::new(), Vec::new())
            .into_executable()
            .expect_err("an empty workflow has nothing to run");
        assert!(
            matches!(&error, WorkflowError::Validation(detail) if detail.contains("neither steps nor blocks")),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn whitespace_only_target_declarations_fall_back_to_the_next_level() {
        let step = WorkflowStep::Query {
            id: "q1".into(),
            sql: "select 1".into(),
            connection: Some("   ".into()),
            database: Some(String::new()),
            timeout_secs: None,
            on_error: None,
        };
        let executable_step = ExecutableStep::from_step_owned(step);
        assert!(
            !executable_step.target.is_declared(),
            "a blank declaration must read as undeclared, not as a connection named \"\""
        );
    }

    #[test]
    fn command_namespace_is_read_from_input_because_there_is_no_top_level_database() {
        let step = WorkflowStep::Command {
            id: "c1".into(),
            command: "insert_row".into(),
            connection: Some("conn-1".into()),
            input: serde_json::json!({ "database": "sales" }),
            timeout_secs: None,
            on_error: None,
        };
        let target = TargetDeclaration::from_step(&step).normalized();
        assert_eq!(target.connection.as_deref(), Some("conn-1"));
        assert_eq!(target.database.as_deref(), Some("sales"));
        assert!(ExecutableStep::from_step_owned(step).resolves_target());
    }
}
