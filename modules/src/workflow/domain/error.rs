//! Structured workflow errors — IPC boundaries still stringify for compatibility.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Required variable '{0}' is missing")]
    MissingVariable(String),

    #[error("Global timeout ({0}s) exceeded")]
    GlobalTimeout(u64),

    #[error("Command step '{step_id}' requires a database connection")]
    MissingConnection { step_id: String },

    /// Neither the step, the workflow, nor the connection names a database.
    /// Without this the driver silently falls back to its own default
    /// (PostgreSQL hardcodes `"postgres"`) and the failure surfaces much later
    /// as a misleading `relation "..." does not exist`.
    #[error(
        "Query step '{step_id}' has no database: set `database` on the step, \
         or a workflow-level `database`, or a default database on connection \
         '{connection_id}' (otherwise the driver falls back to its built-in \
         default and the SQL fails against the wrong database)"
    )]
    MissingDatabase {
        step_id: String,
        connection_id: String,
    },

    #[error("Failed to connect '{connection_id}': {message}")]
    ConnectionFailed {
        connection_id: String,
        message: String,
    },

    #[error("Unsupported driver command '{command}' for connection '{connection_id}'")]
    UnsupportedCommand {
        command: String,
        connection_id: String,
    },

    #[error("Command '{command}' is not available in workflows")]
    CommandNotInWorkflow { command: String },

    #[error("{0}")]
    Driver(String),

    #[error(
        "Workflow SQL execution {execution_id}: state={state}, effects={effects}, code={code}"
    )]
    SqlExecution {
        execution_id: String,
        state: String,
        effects: String,
        code: String,
    },

    /// The execution may still be live. Error strategies must fail closed.
    #[error("Workflow SQL execution {execution_id} terminal state unconfirmed: state={state}, effects={effects}, code={code}")]
    SqlUnconfirmed {
        execution_id: String,
        state: String,
        effects: String,
        code: String,
    },

    #[error("Workflow SQL result unavailable for execution {execution_id}: effects={effects}, code={code}")]
    SqlResultRead {
        execution_id: String,
        effects: String,
        code: String,
    },

    /// 定义版本无法分派。**不猜测**：非整数与高于上限的整数都拒绝执行，
    /// 前者在历史版本里被当作「未声明定义版本」而静默按 v1 跑。
    #[error("Workflow definition version '{declared}' cannot be dispatched (highest supported: {highest_supported}); declare a supported integer definition version or upgrade DataZen")]
    UnsupportedDefinitionVersion {
        declared: String,
        highest_supported: u32,
    },

    /// 目标声明互相矛盾——静态拒绝，不在运行期「就近选一个」。
    #[error("Target conflict for step '{step_id}': {detail}")]
    TargetConflict { step_id: String, detail: String },

    /// 目标缺失。**不回退到 driver 内置默认**：那会让失败以「relation 不存在」
    /// 的形式在更晚、更误导的位置才浮现。
    #[error("Target required for step '{step_id}': {detail}")]
    TargetRequired { step_id: String, detail: String },

    /// 目标已解析，但该目标不被此操作支持。
    #[error("Target unsupported for step '{step_id}': {detail}")]
    TargetUnsupported { step_id: String, detail: String },

    /// 事务块计划预校验未通过。`effectOutcome = notStarted`：
    /// 不占资源、不建连接、不执行任何一步，也不自动重试。
    #[error("Unsupported plan (steps: {offending_steps:?}): {detail}")]
    UnsupportedPlan {
        offending_steps: Vec<String>,
        detail: String,
    },

    /// 驱动不具备判定该计划的能力。与「判定为不安全」是两回事——
    /// 后者是 UnsupportedPlan，前者是能力缺口，同样拒绝执行。
    #[error("Capability unsupported: {detail}")]
    CapabilityUnsupported { detail: String },

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Template(String),

    #[error("{0}")]
    Step(String),

    #[error("{0}")]
    Ai(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for WorkflowError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}

impl From<&str> for WorkflowError {
    fn from(value: &str) -> Self {
        Self::Other(value.to_string())
    }
}

impl WorkflowError {
    pub fn into_ipc_string(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_missing_variable() {
        let err = WorkflowError::MissingVariable("env".into());
        assert_eq!(err.to_string(), "Required variable 'env' is missing");
    }

    #[test]
    fn from_string_preserves_message() {
        let err: WorkflowError = "step failed".into();
        assert_eq!(err.to_string(), "step failed");
    }

    #[test]
    fn test_tester_into_ipc_string_matches_display() {
        let err = WorkflowError::GlobalTimeout(42);
        assert_eq!(err.into_ipc_string(), "Global timeout (42s) exceeded");
    }

    #[test]
    fn test_tester_missing_connection_formats_step_id() {
        let err = WorkflowError::MissingConnection {
            step_id: "fetch-users".into(),
        };
        assert_eq!(
            err.to_string(),
            "Command step 'fetch-users' requires a database connection"
        );
    }

    #[test]
    fn test_tester_unsupported_command_includes_ids() {
        let err = WorkflowError::UnsupportedCommand {
            command: "backup".into(),
            connection_id: "cfg-1".into(),
        };
        assert!(err.to_string().contains("backup"));
        assert!(err.to_string().contains("cfg-1"));
    }
}

#[cfg(test)]
mod missing_database_tests {
    use super::WorkflowError;

    #[test]
    fn message_names_step_connection_and_all_three_fixes() {
        let err = WorkflowError::MissingDatabase {
            step_id: "orders".into(),
            connection_id: "PG-Local".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("'orders'"), "{msg}");
        assert!(msg.contains("'PG-Local'"), "{msg}");
        // The message must not read like a driver error, since the whole point
        // is to replace the misleading `relation "..." does not exist`.
        assert!(!msg.contains("does not exist"), "{msg}");
        // It should tell the user all three places a database can be set.
        assert!(msg.contains("on the step"), "{msg}");
        assert!(msg.contains("workflow-level"), "{msg}");
        assert!(msg.contains("on connection"), "{msg}");
    }
}
