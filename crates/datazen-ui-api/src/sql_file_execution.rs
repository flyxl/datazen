use serde_json::Value;

use crate::{
    format_restore_progress, BackupDumpPhase, BackupProgressPayload, ProgressLogPump, Translate,
    KEY_RESTORE_SUCCESS, KEY_RESTORING,
};

pub const RESTORE_SQL_FILE_COMMAND: &str = "restore_sql_file";
pub const RESTORE_PROGRESS_EVENT: &str = "restore-progress";
pub const OVERWRITE_OPTION: &str = "overwrite";

pub const SQL_FILE_EXECUTION_GAP: [&str; 6] = [
    "native .sql file picker",
    "legacyInvoke('restore_sql_file')",
    "requirePlatform().events.listenCrossWindow('restore-progress')",
    "schemaClient.listCatalog",
    "Promise<boolean> confirmations",
    "80ms progress-log timer",
];

#[derive(Clone, Debug, PartialEq)]
pub enum ConfirmationMode {
    None,
    BeforeExecute { confirmed: bool },
    Overwrite { table_count: usize, confirmed: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionFailure {
    ErrorMessage(String),
    RestoreOutcome { code: String, message: Value },
    Raw(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub enum InvokeOutcome {
    Executed,
    NotExecuted,
    Failed(ExecutionFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlFileExecutionOutcome {
    Cancelled,
    Completed(bool),
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlFileExecutionPlan {
    pub options: Vec<String>,
    pub status_lines: Vec<(Option<BackupProgressPayload>, String)>,
    pub log_lines: Vec<String>,
    pub error_message: Option<String>,
    pub unlisten_called: bool,
    pub outcome: SqlFileExecutionOutcome,
}

fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(_) | Value::Object(_) => "[object Object]".to_owned(),
    }
}

pub fn execution_error_message(failure: &ExecutionFailure) -> String {
    match failure {
        ExecutionFailure::ErrorMessage(message) => message.clone(),
        ExecutionFailure::RestoreOutcome { code, message } => {
            if code == "restoreOutcome" {
                message
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| js_string(message))
            } else {
                "[object Object]".to_owned()
            }
        }
        ExecutionFailure::Raw(value) => js_string(value),
    }
}

fn done_payload() -> BackupProgressPayload {
    BackupProgressPayload {
        current: 0,
        total: 0,
        object_name: String::new(),
        phase: BackupDumpPhase::Done,
    }
}

pub fn sql_file_execution_plan(
    confirmation: &ConfirmationMode,
    progress_events: &[BackupProgressPayload],
    invocation: &InvokeOutcome,
    success_message_key: &str,
    t: &dyn Translate,
) -> SqlFileExecutionPlan {
    let mut options = Vec::new();
    let mut status_lines = Vec::new();
    let mut error_message = None;

    match confirmation {
        ConfirmationMode::None => {}
        ConfirmationMode::BeforeExecute { confirmed } => {
            if !confirmed {
                return SqlFileExecutionPlan {
                    options,
                    status_lines,
                    log_lines: Vec::new(),
                    error_message,
                    unlisten_called: false,
                    outcome: SqlFileExecutionOutcome::Cancelled,
                };
            }
        }
        ConfirmationMode::Overwrite {
            table_count,
            confirmed,
        } => {
            if *table_count > 0 {
                if !confirmed {
                    return SqlFileExecutionPlan {
                        options,
                        status_lines,
                        log_lines: Vec::new(),
                        error_message,
                        unlisten_called: false,
                        outcome: SqlFileExecutionOutcome::Cancelled,
                    };
                }
                options.push(OVERWRITE_OPTION.to_owned());
            }
        }
    }

    let mut pump = ProgressLogPump::new();
    let restoring = t.translate(KEY_RESTORING, &[]);
    pump.reset(vec![restoring.clone()]);
    status_lines.push((None, restoring));

    for payload in progress_events {
        let line = format_restore_progress(payload, t);
        status_lines.push((Some(payload.clone()), line.clone()));
        pump.push(&line, Some(t));
    }

    let outcome = match invocation {
        InvokeOutcome::Executed => {
            let done_line = t.translate(success_message_key, &[]);
            pump.push(&done_line, Some(t));
            pump.flush();
            status_lines.push((Some(done_payload()), done_line));
            SqlFileExecutionOutcome::Completed(true)
        }
        InvokeOutcome::NotExecuted => {
            pump.reset(Vec::new());
            status_lines.push((None, String::new()));
            SqlFileExecutionOutcome::Completed(false)
        }
        InvokeOutcome::Failed(failure) => {
            let message = execution_error_message(failure);
            pump.push(&message, Some(t));
            pump.flush();
            status_lines.push((None, message.clone()));
            error_message = Some(message);
            SqlFileExecutionOutcome::Failed
        }
    };

    SqlFileExecutionPlan {
        options,
        status_lines,
        log_lines: pump.lines().to_vec(),
        error_message,
        unlisten_called: true,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn translator() -> impl Translate {
        |key: &str, _params: &[(&str, String)]| format!("T:{key}")
    }

    fn payload(phase: BackupDumpPhase, name: &str) -> BackupProgressPayload {
        BackupProgressPayload {
            current: 1,
            total: 2,
            object_name: name.to_owned(),
            phase,
        }
    }

    #[test]
    fn gap_and_contract_literals_are_pinned() {
        assert_eq!(RESTORE_SQL_FILE_COMMAND, "restore_sql_file");
        assert_eq!(RESTORE_PROGRESS_EVENT, "restore-progress");
        assert_eq!(OVERWRITE_OPTION, "overwrite");
        assert_eq!(SQL_FILE_EXECUTION_GAP.len(), 6);
        assert!(SQL_FILE_EXECUTION_GAP.contains(&"native .sql file picker"));
        assert_eq!(crate::SQL_FILE_EXECUTION_GAP.len(), 6);
    }

    #[test]
    fn error_message_extraction_follows_the_js_branches() {
        assert_eq!(
            execution_error_message(&ExecutionFailure::ErrorMessage("boom".to_owned())),
            "boom"
        );
        assert_eq!(
            execution_error_message(&ExecutionFailure::RestoreOutcome {
                code: "restoreOutcome".to_owned(),
                message: Value::String("detail".to_owned()),
            }),
            "detail"
        );
        assert_eq!(
            execution_error_message(&ExecutionFailure::RestoreOutcome {
                code: "other".to_owned(),
                message: Value::String("detail".to_owned()),
            }),
            "[object Object]"
        );
        assert_eq!(
            execution_error_message(&ExecutionFailure::Raw(Value::String("raw".to_owned()))),
            "raw"
        );
        assert_eq!(
            execution_error_message(&ExecutionFailure::Raw(Value::Number(42.into()))),
            "42"
        );
    }

    #[test]
    fn before_execute_cancellation_happens_before_listening() {
        let plan = sql_file_execution_plan(
            &ConfirmationMode::BeforeExecute { confirmed: false },
            &[],
            &InvokeOutcome::Executed,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Cancelled);
        assert!(!plan.unlisten_called);
        assert!(plan.status_lines.is_empty());
        assert!(plan.log_lines.is_empty());
    }

    #[test]
    fn overwrite_confirmation_adds_the_option_only_for_nonempty_catalogs() {
        let plan = sql_file_execution_plan(
            &ConfirmationMode::Overwrite {
                table_count: 3,
                confirmed: true,
            },
            &[],
            &InvokeOutcome::Executed,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.options, ["overwrite"]);
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Completed(true));

        let plan = sql_file_execution_plan(
            &ConfirmationMode::Overwrite {
                table_count: 0,
                confirmed: false,
            },
            &[],
            &InvokeOutcome::Executed,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert!(plan.options.is_empty());
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Completed(true));

        let plan = sql_file_execution_plan(
            &ConfirmationMode::Overwrite {
                table_count: 3,
                confirmed: false,
            },
            &[],
            &InvokeOutcome::Executed,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Cancelled);
        assert!(!plan.unlisten_called);
    }

    #[test]
    fn successful_run_emits_restoring_progress_events_and_done_line() {
        let events = [
            payload(BackupDumpPhase::Object, "users"),
            payload(BackupDumpPhase::Writing, "orders"),
        ];
        let plan = sql_file_execution_plan(
            &ConfirmationMode::None,
            &events,
            &InvokeOutcome::Executed,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.status_lines.len(), 4);
        assert_eq!(
            plan.status_lines[0],
            (None, "T:backup.restoring".to_owned())
        );
        assert_eq!(plan.status_lines[1].0, Some(events[0].clone()));
        assert_eq!(plan.status_lines[2].0, Some(events[1].clone()));
        assert_eq!(plan.status_lines[3].1, "T:backup.restoreSuccess");
        assert_eq!(plan.status_lines[3].0, Some(done_payload()));
        assert!(plan.log_lines.contains(&"T:backup.restoring".to_owned()));
        assert!(plan
            .log_lines
            .contains(&"T:backup.restoreSuccess".to_owned()));
        assert!(plan.unlisten_called);
    }

    #[test]
    fn not_executed_clears_the_log_and_reports_an_empty_status_line() {
        let plan = sql_file_execution_plan(
            &ConfirmationMode::None,
            &[payload(BackupDumpPhase::Object, "users")],
            &InvokeOutcome::NotExecuted,
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Completed(false));
        assert!(plan.log_lines.is_empty());
        assert_eq!(
            plan.status_lines.last().map(|entry| entry.1.as_str()),
            Some("")
        );
        assert!(plan.unlisten_called);
    }

    #[test]
    fn failure_reports_the_message_to_all_three_sinks_and_still_unlistens() {
        let plan = sql_file_execution_plan(
            &ConfirmationMode::None,
            &[],
            &InvokeOutcome::Failed(ExecutionFailure::ErrorMessage("nope".to_owned())),
            KEY_RESTORE_SUCCESS,
            &translator(),
        );
        assert_eq!(plan.outcome, SqlFileExecutionOutcome::Failed);
        assert_eq!(plan.error_message.as_deref(), Some("nope"));
        assert_eq!(
            plan.status_lines.last().map(|entry| entry.1.as_str()),
            Some("nope")
        );
        assert!(plan.log_lines.contains(&"nope".to_owned()));
        assert!(plan.unlisten_called);
    }

    #[test]
    fn custom_success_key_is_used_for_the_done_line() {
        let seen = RefCell::new(Vec::new());
        let custom = |key: &str, _params: &[(&str, String)]| {
            seen.borrow_mut().push(key.to_owned());
            format!("custom:{key}")
        };
        let plan = sql_file_execution_plan(
            &ConfirmationMode::None,
            &[],
            &InvokeOutcome::Executed,
            "backup.success",
            &custom,
        );
        assert!(seen.borrow().contains(&"backup.success".to_owned()));
        assert_eq!(
            plan.status_lines.last().map(|entry| entry.1.as_str()),
            Some("custom:backup.success")
        );
    }
}
