use crate::{ExecMap, Panel, PanelType};

use super::{
    pane_keys::pane_id_of_pane_key, RunningCloseDecision, RunningExecutionRequest, RunningPane,
    DEFAULT_PANE_ID,
};

pub fn running_panes_of<K, S>(
    panel: &Panel,
    exec: &ExecMap,
    pane_keys_of: K,
    sql_of: S,
) -> Vec<RunningPane>
where
    K: Fn(&[String], &str) -> Vec<String>,
    S: Fn(&str) -> String,
{
    if panel.panel_type != PanelType::Query {
        return Vec::new();
    }
    let mut panes = Vec::new();
    for key in pane_keys_of(&exec.keys(), &panel.id) {
        if let Some(state) = exec.get(&key) {
            if state.running {
                panes.push(RunningPane {
                    pane_id: pane_id_of_pane_key(&key),
                    exec: state.clone(),
                    sql: sql_of(&key),
                });
            }
        }
    }
    panes
}

pub fn stoppable_panes<S>(
    panes: &[RunningPane],
    can_cancel: bool,
    has_session: S,
) -> Vec<RunningPane>
where
    S: Fn(&str) -> bool,
{
    if !can_cancel {
        return Vec::new();
    }
    panes
        .iter()
        .filter(|pane| has_session(&pane.pane_id))
        .cloned()
        .collect()
}

pub fn running_close_request(
    panel: &Panel,
    stoppable: &[RunningPane],
    title: &str,
) -> RunningExecutionRequest {
    let first = stoppable.first();
    RunningExecutionRequest {
        panel_id: panel.id.clone(),
        title: title.to_owned(),
        pane_id: first
            .map(|pane| pane.pane_id.clone())
            .unwrap_or_else(|| DEFAULT_PANE_ID.to_owned()),
        sql: first.map(|pane| pane.sql.clone()).unwrap_or_default(),
        running_panes: stoppable.len(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardDecision {
    ImmediateProceed,
    AsyncConfirm { running_panes: usize },
}

pub fn guard_decision(stoppable: &[RunningPane], pane_filter: Option<&str>) -> GuardDecision {
    let count = match pane_filter {
        Some(pane_id) => stoppable
            .iter()
            .filter(|pane| pane.pane_id == pane_id)
            .count(),
        None => stoppable.len(),
    };
    if count == 0 {
        GuardDecision::ImmediateProceed
    } else {
        GuardDecision::AsyncConfirm {
            running_panes: count,
        }
    }
}

pub fn guard_result(decision: &RunningCloseDecision) -> (bool, Option<String>) {
    match decision {
        RunningCloseDecision::Proceed => (true, None),
        RunningCloseDecision::Declined => (false, None),
        RunningCloseDecision::Unconfirmed { pending } => (
            false,
            Some(
                pending
                    .first()
                    .cloned()
                    .unwrap_or_else(|| DEFAULT_PANE_ID.to_owned()),
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::{pane_keys_of_panel, QueryExecState};

    use super::*;

    fn pane(id: &str) -> RunningPane {
        RunningPane {
            pane_id: id.to_owned(),
            exec: QueryExecState::running("e1"),
            sql: format!("select from {id}"),
        }
    }

    #[test]
    fn only_query_panels_with_running_states_are_collected() {
        let exec = ExecMap::from_entries(vec![
            ("p1".to_owned(), QueryExecState::running("e1")),
            ("p1::left".to_owned(), QueryExecState::running("e2")),
            (
                "p2".to_owned(),
                QueryExecState {
                    running: false,
                    ..QueryExecState::running("e3")
                },
            ),
        ]);
        let panes = running_panes_of(
            &Panel::query("p1", "c1"),
            &exec,
            pane_keys_of_panel,
            |key| format!("sql:{key}"),
        );
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].pane_id, DEFAULT_PANE_ID);
        assert_eq!(panes[0].sql, "sql:p1");
        assert_eq!(panes[1].pane_id, "left");

        let other = running_panes_of(&Panel::other("p1", "c1"), &exec, pane_keys_of_panel, |_| {
            String::new()
        });
        assert!(other.is_empty());
    }

    #[test]
    fn stoppable_requires_both_driver_support_and_a_session() {
        let panes = vec![pane("main"), pane("left")];
        assert!(stoppable_panes(&panes, false, |_| true).is_empty());
        let kept = stoppable_panes(&panes, true, |id| id == "left");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].pane_id, "left");
    }

    #[test]
    fn request_uses_first_pane_and_reports_the_running_count() {
        let request = running_close_request(
            &Panel::query("p1", "c1"),
            &[pane("left"), pane("right")],
            "Orders",
        );
        assert_eq!(request.panel_id, "p1");
        assert_eq!(request.title, "Orders");
        assert_eq!(request.pane_id, "left");
        assert_eq!(request.sql, "select from left");
        assert_eq!(request.running_panes, 2);

        let empty = running_close_request(&Panel::query("p1", "c1"), &[], "Orders");
        assert_eq!(empty.pane_id, DEFAULT_PANE_ID);
        assert_eq!(empty.sql, "");
        assert_eq!(empty.running_panes, 0);
    }

    #[test]
    fn guard_proceeds_immediately_when_nothing_is_stoppable_in_scope() {
        let panes = vec![pane("main"), pane("left")];
        assert_eq!(
            guard_decision(&panes, None),
            GuardDecision::AsyncConfirm { running_panes: 2 }
        );
        assert_eq!(
            guard_decision(&panes, Some("left")),
            GuardDecision::AsyncConfirm { running_panes: 1 }
        );
        assert_eq!(
            guard_decision(&panes, Some("missing")),
            GuardDecision::ImmediateProceed
        );
        assert_eq!(guard_decision(&[], None), GuardDecision::ImmediateProceed);
    }

    #[test]
    fn guard_result_maps_decisions_and_falls_back_to_default_pane() {
        assert_eq!(guard_result(&RunningCloseDecision::Proceed), (true, None));
        assert_eq!(guard_result(&RunningCloseDecision::Declined), (false, None));
        assert_eq!(
            guard_result(&RunningCloseDecision::Unconfirmed {
                pending: vec!["left".to_owned()]
            }),
            (false, Some("left".to_owned()))
        );
        assert_eq!(
            guard_result(&RunningCloseDecision::Unconfirmed {
                pending: Vec::new()
            }),
            (false, Some(DEFAULT_PANE_ID.to_owned()))
        );
    }
}
