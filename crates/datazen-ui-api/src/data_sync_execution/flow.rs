use super::types::*;
use crate::{
    apply_options_to_rows, clear_job_refs, is_paged_comparison, merge_compare_into_mappings,
    selected_rows_from_merged, ClearJobRefsInput, ComparisonTableStatus, DataSyncJobKind,
    DataSyncSyncState,
};

fn plan(effects: Vec<ExecutionEffect>) -> ExecutionPlan {
    ExecutionPlan { effects }
}

fn error(message: String) -> ExecutionEffect {
    ExecutionEffect::Error { message, open: true }
}

pub fn plan_execution(
    input: &ExecutionInput,
    responses: &ExecutionResponses,
    t: &dyn Fn(&str) -> String,
) -> ExecutionPlan {
    let mut effects = Vec::new();
    if input.source_id.is_empty() || input.target_id.is_empty() {
        return plan(effects);
    }
    if input.write_outcome_uncertain {
        effects.push(error(t("sync.executionUnknown")));
        effects.push(ExecutionEffect::State(DataSyncSyncState::Unknown));
        return plan(effects);
    }
    if input.target_read_only {
        effects.push(error(t("sync.targetReadOnly")));
        return plan(effects);
    }
    effects.push(ExecutionEffect::State(DataSyncSyncState::Executing));
    effects.push(ExecutionEffect::Progress(t("sync.executing")));

    let (source_session, target_session) = match &responses.sessions {
        SessionRefresh::Failed(message) => {
            effects.push(error(message.clone()));
            effects.push(ExecutionEffect::State(DataSyncSyncState::Compared));
            effects.push(ExecutionEffect::ClearProgress);
            return plan(effects);
        }
        SessionRefresh::Ready {
            source_session_id,
            target_session_id,
        } => (source_session_id.clone(), target_session_id.clone()),
    };
    let (source_session, target_session) = match (source_session, target_session) {
        (Some(source), Some(target)) => (source, target),
        _ => {
            effects.push(ExecutionEffect::State(DataSyncSyncState::Compared));
            return plan(effects);
        }
    };

    let tables_with_selection: Vec<&crate::ComparisonTableRow> = input
        .mapping_results
        .iter()
        .filter(|row| {
            row.status == ComparisonTableStatus::Matched
                && (input.selected_rows.iter().any(|selected| {
                    selected.source_table == row.source_table
                        && selected.target_table == row.target_table
                }) || input.table_selections.iter().any(|scope| {
                    scope.source_table == row.source_table
                        && scope.target_table == row.target_table
                }))
        })
        .collect();

    if responses.stale || responses.cancel_requested_job {
        return plan(effects);
    }
    if input.selected_rows.is_empty() && input.table_selections.is_empty() {
        effects.push(ExecutionEffect::State(DataSyncSyncState::Compared));
        effects.push(ExecutionEffect::ClearProgress);
        return plan(effects);
    }

    effects.push(ExecutionEffect::Progress(t("sync.executing")));
    effects.push(ExecutionEffect::ExecuteCall {
        source_session: source_session.clone(),
        target_session: target_session.clone(),
        job_id: input.execute_job_id.clone(),
        selected_rows: input.selected_rows.clone(),
        table_selections: input.table_selections.clone(),
    });
    let execute = match &responses.execute {
        Some(result) => result,
        None => return plan(effects),
    };
    effects.push(ExecutionEffect::RefreshJobs);
    let outcome = execute
        .outcome
        .clone()
        .unwrap_or_else(|| if execute.rolled_back { "rolled_back" } else { "committed" }.to_owned());
    effects.push(ExecutionEffect::LastResult(Some(outcome.clone())));
    match outcome.as_str() {
        "not_started" => {
            let message = if execute.error.is_empty() {
                t("sync.executionNotStarted")
            } else {
                execute.error.clone()
            };
            effects.push(error(message));
            effects.push(ExecutionEffect::Uncertain(false));
            effects.push(ExecutionEffect::State(DataSyncSyncState::Compared));
            effects.push(ExecutionEffect::ClearProgress);
            return plan(effects);
        }
        "unknown" => {
            let message = format!("{} {}", t("sync.executionUnknown"), execute.error)
                .trim()
                .to_owned();
            effects.push(error(message));
            effects.push(ExecutionEffect::Uncertain(true));
            effects.push(ExecutionEffect::State(DataSyncSyncState::Unknown));
            effects.push(ExecutionEffect::StepResult);
            effects.push(ExecutionEffect::ClearProgress);
            return plan(effects);
        }
        _ => {}
    }

    effects.push(ExecutionEffect::Progress(t("sync.recomparing")));
    let filters: Vec<(String, String)> = tables_with_selection
        .iter()
        .filter_map(|row| {
            row.source_filter
                .as_ref()
                .map(|filter| (row.source_table.clone(), filter.clone()))
        })
        .collect();
    effects.push(ExecutionEffect::CompareCall {
        job_id: input.recompare_job_id.clone(),
        tables: tables_with_selection
            .iter()
            .map(|row| row.source_table.clone())
            .collect(),
        filters,
    });
    let compare = match &responses.compare {
        Some(result) => result,
        None => return plan(effects),
    };
    let (compared, paged) = match compare {
        CompareResult::Failed(message) => {
            effects.push(error(message.clone()));
            effects.push(ExecutionEffect::Uncertain(false));
            effects.push(ExecutionEffect::State(DataSyncSyncState::Done));
            effects.push(ExecutionEffect::StepResult);
            effects.push(ExecutionEffect::ClearProgress);
            return plan(effects);
        }
        CompareResult::Rows(rows) => (rows.clone(), false),
        CompareResult::Paged {
            tables,
            contract_version,
        } => {
            let paged = is_paged_comparison(Some(contract_version));
            (tables.clone(), paged)
        }
    };
    effects.push(ExecutionEffect::RefreshJobs);
    if paged {
        effects.push(ExecutionEffect::ClearSelection);
    }
    let merged = merge_compare_into_mappings(&input.mapping_results, &compared)
        .into_iter()
        .map(|row| {
            if paged {
                crate::ComparisonTableRow {
                    rows: None,
                    ..row
                }
            } else if let Some(rows) = row.rows.as_ref() {
                crate::ComparisonTableRow {
                    rows: Some(apply_options_to_rows(rows, &input.sync_options)),
                    ..row
                }
            } else {
                row
            }
        })
        .collect::<Vec<_>>();
    effects.push(ExecutionEffect::MergeMappings(merged));
    if !paged {
        effects.push(ExecutionEffect::SelectedRows(selected_rows_from_merged(&compared)));
    }
    effects.push(ExecutionEffect::Uncertain(false));
    effects.push(ExecutionEffect::State(DataSyncSyncState::Done));
    effects.push(ExecutionEffect::StepResult);
    effects.push(ExecutionEffect::ClearProgress);
    effects.push(ExecutionEffect::Status(String::new()));
    plan(effects)
}

pub fn plan_finalize(input: &FinalizeInput) -> FinalizePlan {
    let mut result = FinalizePlan {
        clear_write_in_flight: true,
        ..Default::default()
    };
    result.clear_status = input.cancelling_status.as_deref() == Some(input.execute_job_id.as_str())
        && input.current_status == input.cancelling_message;
    let refs = ClearJobRefsInput {
        job_id: Some(input.execute_job_id.clone()),
        job_kind: Some(DataSyncJobKind::Execute),
        ref_job_id: input.ref_job_id.clone(),
        ref_job_kind: input.ref_job_kind,
    };
    result.clear_execute_refs = clear_job_refs(&refs);
    if let Some(recompare_job_id) = &input.recompare_job_id {
        let compare_refs = ClearJobRefsInput {
            job_id: Some(recompare_job_id.clone()),
            job_kind: Some(DataSyncJobKind::Compare),
            ref_job_id: input.ref_job_id.clone(),
            ref_job_kind: input.ref_job_kind,
        };
        result.clear_compare_refs = clear_job_refs(&compare_refs);
    }
    result.clear_cancel_request =
        input.cancel_requested.as_deref() == Some(input.execute_job_id.as_str());
    result
}
