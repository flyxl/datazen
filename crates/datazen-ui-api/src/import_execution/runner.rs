use super::sql::plan_import_chunks;
use super::types::{
    AbortSignal, ImportCommitMode, ImportErrorStrategy, ImportFinalStatus, ImportProgress,
    ImportProgressStatus, ImportRowFailure, ImportRunResult, ImportSqlDialect, ImportTransport,
    PlanImportChunksOptions,
};

fn error_message(error: &str) -> String {
    error.to_owned()
}

fn failure_for(row_index: usize, row_origins: &[i64], error: &str) -> ImportRowFailure {
    let origin = row_origins.get(row_index).copied().unwrap_or(0);
    ImportRowFailure {
        row_index,
        source_line: if origin > 0 { Some(origin) } else { None },
        message: error_message(error),
    }
}

fn single_row_sql(
    options: &PlanImportChunksOptions,
    dialect: &ImportSqlDialect,
    row_index: usize,
) -> Option<String> {
    let mut single = options.clone();
    single.rows = options.rows.iter().skip(row_index).take(1).cloned().collect();
    single.commit_mode = ImportCommitMode::Autocommit;
    let planned = plan_import_chunks(&single, dialect);
    planned.first().map(|chunk| chunk.sql.clone())
}

pub fn execute_import(
    options: &PlanImportChunksOptions,
    dialect: &ImportSqlDialect,
    row_origins: &[i64],
    on_error: ImportErrorStrategy,
    signal: &AbortSignal,
    transport: &mut ImportTransport,
    on_progress: Option<&mut dyn FnMut(&ImportProgress)>,
) -> ImportRunResult {
    let chunks = plan_import_chunks(options, dialect);
    let mut committed_rows = 0usize;
    let mut processed_rows = 0usize;
    let mut failures: Vec<ImportRowFailure> = Vec::new();

    let report = |status: ImportProgressStatus,
                  on_progress: &mut Option<&mut dyn FnMut(&ImportProgress)>,
                  processed_rows: usize,
                  committed_rows: usize,
                  failed_rows: usize| {
        if let Some(callback) = on_progress.as_mut() {
            callback(&ImportProgress {
                status,
                processed_rows,
                committed_rows,
                failed_rows,
            });
        }
    };
    let mut progress = on_progress;

    let run_one_row = |row_index: usize,
                       committed_rows: &mut usize,
                       failures: &mut Vec<ImportRowFailure>,
                       transport: &mut ImportTransport| -> bool {
        let Some(sql) = single_row_sql(options, dialect, row_index) else {
            return false;
        };
        match (transport.run_stream)(&sql) {
            Ok(_) => {
                *committed_rows += 1;
                true
            }
            Err(error) => {
                failures.push(failure_for(row_index, row_origins, &error));
                false
            }
        }
    };

    for chunk in chunks {
        if signal.is_aborted() {
            report(ImportProgressStatus::Cancelled, &mut progress, processed_rows, committed_rows, failures.len());
            return ImportRunResult {
                status: ImportFinalStatus::Cancelled,
                committed_rows,
                failed_rows: failures,
                stopped_by: None,
            };
        }
        match (transport.run_stream)(&chunk.sql) {
            Ok(_) => {
                committed_rows += chunk.row_count;
                processed_rows += chunk.row_count;
                report(ImportProgressStatus::Running, &mut progress, processed_rows, committed_rows, failures.len());
            }
            Err(error) => {
                if signal.is_aborted() {
                    report(ImportProgressStatus::Cancelled, &mut progress, processed_rows, committed_rows, failures.len());
                    return ImportRunResult {
                        status: ImportFinalStatus::Cancelled,
                        committed_rows,
                        failed_rows: failures,
                        stopped_by: None,
                    };
                }
                if on_error == ImportErrorStrategy::Stop {
                    let stopped_by = failure_for(chunk.row_start, row_origins, &error);
                    processed_rows += chunk.row_count;
                    report(ImportProgressStatus::Stopped, &mut progress, processed_rows, committed_rows, failures.len());
                    return ImportRunResult {
                        status: ImportFinalStatus::Stopped,
                        committed_rows,
                        failed_rows: failures,
                        stopped_by: Some(stopped_by),
                    };
                }
                for row_index in chunk.row_start..chunk.row_start + chunk.row_count {
                    if signal.is_aborted() {
                        break;
                    }
                    run_one_row(row_index, &mut committed_rows, &mut failures, transport);
                    processed_rows += 1;
                }
                if signal.is_aborted() {
                    report(ImportProgressStatus::Cancelled, &mut progress, processed_rows, committed_rows, failures.len());
                    return ImportRunResult {
                        status: ImportFinalStatus::Cancelled,
                        committed_rows,
                        failed_rows: failures,
                        stopped_by: None,
                    };
                }
                report(ImportProgressStatus::Running, &mut progress, processed_rows, committed_rows, failures.len());
            }
        }
    }

    report(ImportProgressStatus::Completed, &mut progress, processed_rows, committed_rows, failures.len());
    ImportRunResult {
        status: ImportFinalStatus::Completed,
        committed_rows,
        failed_rows: failures,
        stopped_by: None,
    }
}
