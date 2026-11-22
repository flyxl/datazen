use super::{QueryCancelState, QueryExecState, QueryExecTable, QueryTerminalState};

#[derive(Clone, Debug, PartialEq)]
pub enum QueryExecutionTransition {
    Start,
    CancelRequested,
    CancelFailed { error: String },
    Succeeded,
    Failed { error: String },
    Cancelled,
    OutcomeUnknown { error: Option<String> },
}

pub fn reduce_query_execution_state(
    exec: &QueryExecState,
    transition: &QueryExecutionTransition,
) -> QueryExecState {
    match transition {
        QueryExecutionTransition::Start => QueryExecState {
            running: true,
            published_artifacts: None,
            error: None,
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: None,
            execution_id: None,
            ..exec.clone()
        },
        QueryExecutionTransition::CancelRequested => {
            if exec.running {
                QueryExecState {
                    cancel_state: QueryCancelState::Requested,
                    cancel_error: None,
                    ..exec.clone()
                }
            } else {
                exec.clone()
            }
        }
        QueryExecutionTransition::CancelFailed { error } => {
            if exec.running {
                QueryExecState {
                    cancel_state: QueryCancelState::Failed,
                    cancel_error: Some(error.clone()),
                    ..exec.clone()
                }
            } else {
                exec.clone()
            }
        }
        QueryExecutionTransition::Succeeded => QueryExecState {
            running: false,
            error: None,
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: Some(QueryTerminalState::Succeeded),
            execution_id: None,
            ..exec.clone()
        },
        QueryExecutionTransition::Failed { error } => QueryExecState {
            running: false,
            error: Some(error.clone()),
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: Some(QueryTerminalState::Failed),
            execution_id: None,
            ..exec.clone()
        },
        QueryExecutionTransition::Cancelled => QueryExecState {
            running: false,
            error: None,
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: Some(QueryTerminalState::Cancelled),
            execution_id: None,
            ..exec.clone()
        },
        QueryExecutionTransition::OutcomeUnknown { error } => QueryExecState {
            running: false,
            error: error.clone().or_else(|| exec.error.clone()),
            cancel_state: QueryCancelState::Idle,
            terminal_state: Some(QueryTerminalState::Unknown),
            execution_id: None,
            ..exec.clone()
        },
    }
}

fn contains_user_request(lower: &str) -> bool {
    let mut offset = 0usize;
    while let Some(index) = lower[offset..].find("user") {
        let start = offset + index;
        let after = &lower[start + 4..];
        if let Some(request) = after.find("request") {
            let gap = &after[..request];
            if gap.len() <= 1 {
                return true;
            }
        }
        offset = start + 4;
    }
    false
}

pub fn is_cancellation_error(message: Option<&str>) -> bool {
    let message = match message {
        Some(message) if !message.is_empty() => message,
        _ => return false,
    };
    let lower = message.to_lowercase();
    lower.contains("cancel")
        || contains_user_request(&lower)
        || lower.contains("abort")
        || lower.contains("interrupt")
}

pub fn query_error_transition(exec: &QueryExecState, message: &str) -> QueryExecutionTransition {
    if exec.cancel_state == QueryCancelState::Requested && is_cancellation_error(Some(message)) {
        return QueryExecutionTransition::Cancelled;
    }
    if exec.cancel_state == QueryCancelState::Failed && is_cancellation_error(Some(message)) {
        return QueryExecutionTransition::OutcomeUnknown {
            error: Some(message.to_owned()),
        };
    }
    QueryExecutionTransition::Failed {
        error: message.to_owned(),
    }
}

pub fn patch_exec<F>(current: &QueryExecTable, key: &str, patch: F) -> QueryExecTable
where
    F: FnOnce(&mut QueryExecState),
{
    let mut next = current.clone();
    let mut state = next.get(key).cloned().unwrap_or_else(QueryExecState::empty);
    patch(&mut state);
    next.insert(key.to_owned(), state);
    next
}

pub fn transition_exec(
    current: &QueryExecTable,
    key: &str,
    transition: &QueryExecutionTransition,
) -> QueryExecTable {
    let exec = match current.get(key) {
        Some(exec) => exec,
        None => return current.clone(),
    };
    let reduced = reduce_query_execution_state(exec, transition);
    patch_exec(current, key, |state| *state = reduced)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> QueryExecState {
        QueryExecState {
            running: true,
            execution_id: Some("e1".to_owned()),
            ..QueryExecState::empty()
        }
    }

    #[test]
    fn start_clears_every_stale_execution_field() {
        let exec = QueryExecState {
            error: Some("old".to_owned()),
            cancel_state: QueryCancelState::Requested,
            cancel_error: Some("old".to_owned()),
            terminal_state: Some(QueryTerminalState::Failed),
            ..running()
        };
        let next = reduce_query_execution_state(&exec, &QueryExecutionTransition::Start);
        assert!(next.running);
        assert_eq!(next.error, None);
        assert_eq!(next.cancel_state, QueryCancelState::Idle);
        assert_eq!(next.cancel_error, None);
        assert_eq!(next.terminal_state, None);
        assert_eq!(next.execution_id, None);
    }

    #[test]
    fn cancel_transitions_are_ignored_when_not_running() {
        let exec = QueryExecState::empty();
        assert_eq!(
            reduce_query_execution_state(&exec, &QueryExecutionTransition::CancelRequested),
            exec
        );
        assert_eq!(
            reduce_query_execution_state(
                &exec,
                &QueryExecutionTransition::CancelFailed {
                    error: "x".to_owned()
                }
            ),
            exec
        );
    }

    #[test]
    fn terminal_transitions_clear_running_and_execution_id() {
        for transition in [
            QueryExecutionTransition::Succeeded,
            QueryExecutionTransition::Failed {
                error: "boom".to_owned(),
            },
            QueryExecutionTransition::Cancelled,
            QueryExecutionTransition::OutcomeUnknown { error: None },
        ] {
            let next = reduce_query_execution_state(&running(), &transition);
            assert!(!next.running);
            assert_eq!(next.execution_id, None);
            assert_eq!(next.cancel_state, QueryCancelState::Idle);
        }
        let failed = reduce_query_execution_state(
            &running(),
            &QueryExecutionTransition::Failed {
                error: "boom".to_owned(),
            },
        );
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert_eq!(failed.terminal_state, Some(QueryTerminalState::Failed));
        let unknown = reduce_query_execution_state(
            &QueryExecState {
                error: Some("prior".to_owned()),
                ..running()
            },
            &QueryExecutionTransition::OutcomeUnknown { error: None },
        );
        assert_eq!(unknown.error.as_deref(), Some("prior"));
        assert_eq!(unknown.terminal_state, Some(QueryTerminalState::Unknown));
    }

    #[test]
    fn cancellation_error_patterns_are_recognized_case_insensitively() {
        for message in [
            "CANCELLED",
            "user request",
            "user-requested",
            "Aborted",
            "INTERRUPTED",
        ] {
            assert!(is_cancellation_error(Some(message)), "{message}");
        }
        assert!(!is_cancellation_error(None));
        assert!(!is_cancellation_error(Some("")));
        assert!(!is_cancellation_error(Some("syntax error")));
        assert!(!is_cancellation_error(Some(
            "userland request too far apart"
        )));
    }

    #[test]
    fn error_transition_depends_on_the_cancel_state() {
        let requested = QueryExecState {
            cancel_state: QueryCancelState::Requested,
            ..running()
        };
        assert_eq!(
            query_error_transition(&requested, "User cancelled"),
            QueryExecutionTransition::Cancelled
        );
        let failed_cancel = QueryExecState {
            cancel_state: QueryCancelState::Failed,
            ..running()
        };
        assert_eq!(
            query_error_transition(&failed_cancel, "aborted"),
            QueryExecutionTransition::OutcomeUnknown {
                error: Some("aborted".to_owned())
            }
        );
        assert_eq!(
            query_error_transition(&running(), "boom"),
            QueryExecutionTransition::Failed {
                error: "boom".to_owned()
            }
        );
    }

    #[test]
    fn patch_and_transition_preserve_other_panes() {
        let mut table = QueryExecTable::new();
        table.insert("p1".to_owned(), running());
        table.insert("p2".to_owned(), QueryExecState::empty());
        let patched = patch_exec(&table, "p1", |state| state.sql = "select 1".to_owned());
        assert_eq!(patched["p1"].sql, "select 1");
        assert_eq!(patched["p2"], table["p2"]);
        let transitioned = transition_exec(&table, "p1", &QueryExecutionTransition::Succeeded);
        assert!(!transitioned["p1"].running);
        assert_eq!(
            transition_exec(&table, "missing", &QueryExecutionTransition::Start),
            table
        );
        let fresh = patch_exec(&table, "new", |state| state.running = true);
        assert!(fresh["new"].running);
    }
}
