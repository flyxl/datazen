use std::collections::BTreeSet;

use crate::QueryExecState;

use super::{pane_keys::pane_key, RunningPane};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettleState {
    pub unsettled: BTreeSet<String>,
    pub delivered: BTreeSet<String>,
    pub next_attempt_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettleOutcome {
    Settled,
    Pending(Vec<String>),
    Waiting,
}

fn still_running<F>(pane: &RunningPane, panel_id: &str, read_exec: &F) -> bool
where
    F: Fn(&str) -> Option<QueryExecState>,
{
    let key = pane_key(panel_id, Some(&pane.pane_id));
    read_exec(&key).map_or(false, |state| state.running)
}

pub fn initial_settle_state<F>(panes: &[RunningPane], panel_id: &str, read_exec: F) -> SettleState
where
    F: Fn(&str) -> Option<QueryExecState>,
{
    let unsettled = panes
        .iter()
        .filter(|pane| still_running(pane, panel_id, &read_exec))
        .map(|pane| pane.pane_id.clone())
        .collect();
    SettleState {
        unsettled,
        delivered: BTreeSet::new(),
        next_attempt_at: 0,
    }
}

pub fn deadline_from(start_ms: i64, timeout_ms: i64) -> i64 {
    start_ms.saturating_add(timeout_ms)
}

pub fn effective_retry_ms(poll_ms: i64, retry_ms: i64) -> i64 {
    poll_ms.max(retry_ms)
}

pub fn settle_running_panes_step<F, C>(
    state: &mut SettleState,
    panes: &[RunningPane],
    panel_id: &str,
    now_ms: i64,
    deadline_ms: i64,
    retry_ms: i64,
    read_exec: F,
    request_cancel: C,
) -> SettleOutcome
where
    F: Fn(&str) -> Option<QueryExecState>,
    C: Fn(&str) -> Result<bool, ()>,
{
    let running: Vec<&RunningPane> = panes
        .iter()
        .filter(|pane| {
            state.unsettled.contains(&pane.pane_id) && still_running(pane, panel_id, &read_exec)
        })
        .collect();
    for pane in panes {
        if !still_running(pane, panel_id, &read_exec) {
            state.unsettled.remove(&pane.pane_id);
        }
    }
    if state.unsettled.is_empty() {
        return SettleOutcome::Settled;
    }
    if now_ms >= deadline_ms {
        return SettleOutcome::Pending(state.unsettled.iter().cloned().collect());
    }

    if now_ms >= state.next_attempt_at {
        state.next_attempt_at = now_ms.saturating_add(retry_ms);
        for pane in running {
            if state.delivered.contains(&pane.pane_id) {
                continue;
            }
            if matches!(request_cancel(&pane.pane_id), Ok(true)) {
                state.delivered.insert(pane.pane_id.clone());
            }
        }
    }
    SettleOutcome::Waiting
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QueryCancelState;

    fn pane(id: &str) -> RunningPane {
        RunningPane {
            pane_id: id.to_owned(),
            exec: QueryExecState::running("e1"),
            sql: String::new(),
        }
    }

    #[test]
    fn finished_and_missing_entries_are_not_unsettled() {
        let state = initial_settle_state(&[pane("main"), pane("left")], "p1", |key| {
            if key == "p1::left" {
                Some(QueryExecState {
                    running: false,
                    ..QueryExecState::running("e")
                })
            } else {
                None
            }
        });
        assert!(state.unsettled.is_empty());

        let state = initial_settle_state(&[pane("main")], "p1", |_| {
            Some(QueryExecState::running("e"))
        });
        assert_eq!(state.unsettled, BTreeSet::from(["main".to_owned()]));
    }

    #[test]
    fn delivered_cancel_and_finished_statement_settle() {
        let panes = [pane("main")];
        let mut state = initial_settle_state(&panes, "p1", |_| Some(QueryExecState::running("e")));
        let first = settle_running_panes_step(
            &mut state,
            &panes,
            "p1",
            100,
            15_100,
            250,
            |_| Some(QueryExecState::running("e")),
            |_| Ok(true),
        );
        assert_eq!(first, SettleOutcome::Waiting);
        assert!(state.delivered.contains("main"));
        assert_eq!(state.next_attempt_at, 350);

        let second = settle_running_panes_step(
            &mut state,
            &panes,
            "p1",
            150,
            15_100,
            250,
            |_| {
                Some(QueryExecState {
                    running: false,
                    cancel_state: QueryCancelState::Requested,
                    ..QueryExecState::running("e")
                })
            },
            |_| Ok(false),
        );
        assert_eq!(second, SettleOutcome::Settled);
    }

    #[test]
    fn undelivered_and_throwing_cancels_are_retried_until_the_deadline() {
        let panes = [pane("left")];
        let mut state = initial_settle_state(&panes, "p1", |_| Some(QueryExecState::running("e")));
        let first = settle_running_panes_step(
            &mut state,
            &panes,
            "p1",
            0,
            1_000,
            250,
            |_| Some(QueryExecState::running("e")),
            |_| Err(()),
        );
        assert_eq!(first, SettleOutcome::Waiting);
        assert!(state.delivered.is_empty());

        let second = settle_running_panes_step(
            &mut state,
            &panes,
            "p1",
            300,
            1_000,
            250,
            |_| Some(QueryExecState::running("e")),
            |_| Ok(false),
        );
        assert_eq!(second, SettleOutcome::Waiting);
        assert_eq!(state.next_attempt_at, 550);

        let deadline = settle_running_panes_step(
            &mut state,
            &panes,
            "p1",
            1_000,
            1_000,
            250,
            |_| Some(QueryExecState::running("e")),
            |_| Ok(true),
        );
        assert_eq!(deadline, SettleOutcome::Pending(vec!["left".to_owned()]));
    }

    #[test]
    fn retry_is_never_shorter_than_the_poll_interval() {
        assert_eq!(effective_retry_ms(50, 250), 250);
        assert_eq!(effective_retry_ms(300, 250), 300);
        assert_eq!(deadline_from(100, 15_000), 15_100);
    }
}
